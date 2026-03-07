import {
  reduceCalibreLoadEvent,
  reduceLogLevelEvent,
  reducePdfTranscriptionEvent,
  reduceReaderStateEvent,
  reduceSessionStateEvent,
  reduceSourceOpenEvent,
  reduceTtsStateEvent
} from "./eventIngestion";
import { recordEventIngestion } from "../../perf/debug";
import type { SliceContext } from "./types";

export async function ensureJobSubscriptions({ set, get, backend }: SliceContext): Promise<void> {
  const applyReducedEvent = (name: string, reducer: (current: ReturnType<typeof get>, event: any) => object) => (event: any) => {
    recordEventIngestion(name, event);
    const next = reducer(get(), event);
    if (Object.keys(next).length === 0) {
      return;
    }
    set(next);
  };

  if (!get().sourceOpenSubscribed) {
    await backend.onSourceOpen(applyReducedEvent("bridge:source-open", reduceSourceOpenEvent));
    set({ sourceOpenSubscribed: true });
  }

  if (!get().calibreSubscribed) {
    await backend.onCalibreLoad(applyReducedEvent("bridge:calibre-load", reduceCalibreLoadEvent));
    set({ calibreSubscribed: true });
  }

  if (!get().ttsStateSubscribed) {
    await backend.onTtsState(applyReducedEvent("bridge:tts-state", reduceTtsStateEvent));
    set({ ttsStateSubscribed: true });
  }

  if (!get().pdfTranscriptionSubscribed) {
    await backend.onPdfTranscription(
      applyReducedEvent("bridge:pdf-transcription", reducePdfTranscriptionEvent)
    );
    set({ pdfTranscriptionSubscribed: true });
  }

  if (!get().logLevelSubscribed) {
    await backend.onLogLevel(applyReducedEvent("bridge:log-level", reduceLogLevelEvent));
    set({ logLevelSubscribed: true });
  }

  if (!get().sessionStateSubscribed) {
    await backend.onSessionState(applyReducedEvent("bridge:session-state", reduceSessionStateEvent));
    set({ sessionStateSubscribed: true });
  }

  if (!get().readerStateSubscribed) {
    await backend.onReaderState(applyReducedEvent("bridge:reader-state", reduceReaderStateEvent));
    set({ readerStateSubscribed: true });
  }
}
