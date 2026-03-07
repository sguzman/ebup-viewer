import type { AppStore } from "../appStore";
import { buildToast } from "./shared";
import type { SliceContext } from "./types";

function samePanels(
  left: AppStore["session"] extends infer T
    ? T extends { panels: infer P }
      ? P
      : never
    : never,
  right: AppStore["session"] extends infer T
    ? T extends { panels: infer P }
      ? P
      : never
    : never
): boolean {
  return (
    left.show_settings === right.show_settings &&
    left.show_stats === right.show_stats &&
    left.show_tts === right.show_tts
  );
}

function sameSessionState(left: AppStore["session"], right: AppStore["session"]): boolean {
  if (!left || !right) {
    return left === right;
  }
  return (
    left.mode === right.mode &&
    left.active_source_path === right.active_source_path &&
    left.open_in_flight === right.open_in_flight &&
    samePanels(left.panels, right.panels)
  );
}

export async function ensureJobSubscriptions({ set, get, backend }: SliceContext): Promise<void> {
  if (!get().sourceOpenSubscribed) {
    await backend.onSourceOpen((event) => {
      set((current) => {
        if (event.request_id < current.lastSourceOpenEventRequestId) {
          return {};
        }
        return {
          sourceOpenEvent: event,
          lastSourceOpenEventRequestId: event.request_id
        };
      });
      if (event.phase === "cancelled") {
        const suffix = event.request_id > 0 ? ` (request ${event.request_id})` : "";
        set({
          toast: buildToast("info", `${event.message ?? "Source open cancelled"}${suffix}`)
        });
        return;
      }
      if (event.phase === "failed") {
        const suffix = event.request_id > 0 ? ` (request ${event.request_id})` : "";
        set({
          toast: buildToast("error", `${event.message ?? "Source open failed"}${suffix}`)
        });
      }
    });
    set({ sourceOpenSubscribed: true });
  }

  if (!get().calibreSubscribed) {
    await backend.onCalibreLoad((event) => {
      set((current) => {
        if (event.request_id < current.lastCalibreEventRequestId) {
          return {};
        }
        return {
          calibreLoadEvent: event,
          lastCalibreEventRequestId: event.request_id
        };
      });
      if (event.phase === "failed") {
        const suffix = event.request_id > 0 ? ` (request ${event.request_id})` : "";
        set({
          toast: buildToast("error", `${event.message ?? "Calibre load failed"}${suffix}`)
        });
      }
    });
    set({ calibreSubscribed: true });
  }

  if (!get().ttsStateSubscribed) {
    await backend.onTtsState((event) => {
      set((current) => {
        if (event.request_id < current.lastTtsEventRequestId) {
          return {};
        }
        return {
          ttsStateEvent: event,
          lastTtsEventRequestId: event.request_id
        };
      });
    });
    set({ ttsStateSubscribed: true });
  }

  if (!get().pdfTranscriptionSubscribed) {
    await backend.onPdfTranscription((event) => {
      set((current) => {
        if (event.request_id < current.lastPdfEventRequestId) {
          return {};
        }
        return {
          pdfTranscriptionEvent: event,
          lastPdfEventRequestId: event.request_id
        };
      });
      if (event.phase === "failed") {
        const suffix = event.request_id > 0 ? ` (request ${event.request_id})` : "";
        set({
          toast: buildToast("error", `${event.message ?? "PDF transcription failed"}${suffix}`)
        });
      }
    });
    set({ pdfTranscriptionSubscribed: true });
  }

  if (!get().logLevelSubscribed) {
    await backend.onLogLevel((event) => {
      set((current) => {
        if (event.request_id < current.lastLogLevelEventRequestId) {
          return {};
        }
        return {
          logLevelEvent: event,
          runtimeLogLevel: event.level,
          lastLogLevelEventRequestId: event.request_id
        };
      });
    });
    set({ logLevelSubscribed: true });
  }

  if (!get().sessionStateSubscribed) {
    await backend.onSessionState((event) => {
      set((current) => {
        if (event.request_id < current.lastSessionEventRequestId) {
          return {};
        }
        const next: Partial<AppStore> = {
          lastSessionEventRequestId: event.request_id
        };
        if (!sameSessionState(current.session, event.session)) {
          next.session = event.session;
        }
        if (event.session.mode !== "reader") {
          if (current.reader !== null) {
            next.reader = null;
          }
          next.lastReaderEventRequestId = Math.max(
            current.lastReaderEventRequestId,
            event.request_id
          );
        }
        return next;
      });
    });
    set({ sessionStateSubscribed: true });
  }

  if (!get().readerStateSubscribed) {
    await backend.onReaderState((event) => {
      set((current) => {
        if (event.request_id < current.lastReaderEventRequestId) {
          return {};
        }
        const nextSession = current.session
          ? {
              ...current.session,
              mode: "reader" as const,
              active_source_path: event.reader.source_path,
              open_in_flight: false,
              panels: event.reader.panels
            }
          : {
              mode: "reader" as const,
              active_source_path: event.reader.source_path,
              open_in_flight: false,
              panels: event.reader.panels
            };

        const next: Partial<AppStore> = {
          reader: event.reader,
          lastReaderEventRequestId: event.request_id,
          lastSessionEventRequestId: Math.max(
            current.lastSessionEventRequestId,
            event.request_id
          )
        };
        if (!sameSessionState(current.session, nextSession)) {
          next.session = nextSession;
        }
        return next;
      });
    });
    set({ readerStateSubscribed: true });
  }
}
