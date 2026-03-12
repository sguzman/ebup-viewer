import type { AppStore } from "../appStore";
import { setOperationBusy, toBridgeError, toReaderPlaybackState } from "./shared";
import type { SliceContext } from "./types";

export function createTtsSliceActions({ set, get, backend }: SliceContext): Pick<
  AppStore,
  | "readerTtsPlay"
  | "readerTtsPause"
  | "readerTtsTogglePlayPause"
  | "readerTtsPlayFromPageStart"
  | "readerTtsPlayFromHighlight"
  | "readerTtsSeekNext"
  | "readerTtsSeekPrev"
  | "readerTtsRepeatSentence"
  | "readerTtsPrecomputePage"
> {
  const shouldApplyPlaybackOnlyUpdate = (
    currentReader: AppStore["reader"],
    nextReader: Awaited<ReturnType<typeof backend.readerGetSnapshot>>
  ): boolean => {
    if (!currentReader) {
      return false;
    }
    return (
      currentReader.source_path === nextReader.source_path &&
      currentReader.current_page === nextReader.current_page &&
      currentReader.total_pages === nextReader.total_pages &&
      currentReader.text_only_mode === nextReader.text_only_mode &&
      currentReader.pretty_kind === nextReader.pretty_kind
    );
  };

  const syncReader = async (
    fn: () => Promise<Awaited<ReturnType<typeof backend.readerGetSnapshot>>>
  ) => {
    setOperationBusy(set, get, "readerTts", true);
    try {
      const reader = await fn();
      set({ reader, readerPlayback: toReaderPlaybackState(reader) });
    } catch (error) {
      set({ error: toBridgeError(error).message });
    } finally {
      setOperationBusy(set, get, "readerTts", false);
    }
  };

  const syncReaderPlaybackFastPath = async (
    fn: () => Promise<Awaited<ReturnType<typeof backend.readerGetSnapshot>>>
  ) => {
    setOperationBusy(set, get, "readerTts", true);
    try {
      const reader = await fn();
      if (shouldApplyPlaybackOnlyUpdate(get().readerDocument.snapshot, reader)) {
        set({ readerPlayback: toReaderPlaybackState(reader) });
      } else {
        set({ reader, readerPlayback: toReaderPlaybackState(reader) });
      }
    } catch (error) {
      set({ error: toBridgeError(error).message });
    } finally {
      setOperationBusy(set, get, "readerTts", false);
    }
  };

  return {
    readerTtsPlay: async () => syncReaderPlaybackFastPath(() => backend.readerTtsPlay()),
    readerTtsPause: async () => syncReaderPlaybackFastPath(() => backend.readerTtsPause()),
    readerTtsTogglePlayPause: async () =>
      syncReaderPlaybackFastPath(() => backend.readerTtsTogglePlayPause()),
    readerTtsPlayFromPageStart: async () =>
      syncReaderPlaybackFastPath(() => backend.readerTtsPlayFromPageStart()),
    readerTtsPlayFromHighlight: async () =>
      syncReaderPlaybackFastPath(() => backend.readerTtsPlayFromHighlight()),
    readerTtsSeekNext: async () => syncReaderPlaybackFastPath(() => backend.readerTtsSeekNext()),
    readerTtsSeekPrev: async () => syncReaderPlaybackFastPath(() => backend.readerTtsSeekPrev()),
    readerTtsRepeatSentence: async () =>
      syncReaderPlaybackFastPath(() => backend.readerTtsRepeatSentence()),
    readerTtsPrecomputePage: async () => syncReader(() => backend.readerTtsPrecomputePage())
  };
}
