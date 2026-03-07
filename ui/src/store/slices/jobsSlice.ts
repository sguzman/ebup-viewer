import {
  reduceCalibreLoadEvent,
  reduceLogLevelEvent,
  reducePdfTranscriptionEvent,
  reduceReaderStateEvent,
  reduceSessionStateEvent,
  reduceSourceOpenEvent,
  reduceTtsStateEvent
} from "./eventIngestion";
import type { SliceContext } from "./types";

export async function ensureJobSubscriptions({ set, get, backend }: SliceContext): Promise<void> {
  if (!get().sourceOpenSubscribed) {
    await backend.onSourceOpen((event) => {
      set((current) => reduceSourceOpenEvent(current, event));
    });
    set({ sourceOpenSubscribed: true });
  }

  if (!get().calibreSubscribed) {
    await backend.onCalibreLoad((event) => {
      set((current) => reduceCalibreLoadEvent(current, event));
    });
    set({ calibreSubscribed: true });
  }

  if (!get().ttsStateSubscribed) {
    await backend.onTtsState((event) => {
      set((current) => reduceTtsStateEvent(current, event));
    });
    set({ ttsStateSubscribed: true });
  }

  if (!get().pdfTranscriptionSubscribed) {
    await backend.onPdfTranscription((event) => {
      set((current) => reducePdfTranscriptionEvent(current, event));
    });
    set({ pdfTranscriptionSubscribed: true });
  }

  if (!get().logLevelSubscribed) {
    await backend.onLogLevel((event) => {
      set((current) => reduceLogLevelEvent(current, event));
    });
    set({ logLevelSubscribed: true });
  }

  if (!get().sessionStateSubscribed) {
    await backend.onSessionState((event) => {
      set((current) => reduceSessionStateEvent(current, event));
    });
    set({ sessionStateSubscribed: true });
  }

  if (!get().readerStateSubscribed) {
    await backend.onReaderState((event) => {
      set((current) => reduceReaderStateEvent(current, event));
    });
    set({ readerStateSubscribed: true });
  }
}
