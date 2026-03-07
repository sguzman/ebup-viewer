import type { AppStore } from "../appStore";
import { setOperationBusy, toBridgeError, toReaderPlaybackState } from "./shared";
import type { SliceContext } from "./types";

export function createReaderSliceActions({ set, get, backend }: SliceContext): Pick<
  AppStore,
  | "refreshReaderSnapshot"
  | "readerNextPage"
  | "readerPrevPage"
  | "readerSetPage"
  | "readerSentenceClick"
  | "readerNextSentence"
  | "readerPrevSentence"
  | "readerToggleTextOnly"
  | "readerSearchSetQuery"
  | "readerSearchNext"
  | "readerSearchPrev"
> {
  const syncReader = async (
    fn: () => Promise<Awaited<ReturnType<typeof backend.readerGetSnapshot>>>
  ) => {
    setOperationBusy(set, get, "readerCommand", true);
    try {
      const reader = await fn();
      set({ reader, readerPlayback: toReaderPlaybackState(reader) });
    } catch (error) {
      set({ error: toBridgeError(error).message });
    } finally {
      setOperationBusy(set, get, "readerCommand", false);
    }
  };

  return {
    refreshReaderSnapshot: async () => {
      const session = get().session;
      if (!session || session.mode !== "reader") {
        return;
      }
      await syncReader(() => backend.readerGetSnapshot());
    },

    readerNextPage: async () => syncReader(() => backend.readerNextPage()),
    readerPrevPage: async () => syncReader(() => backend.readerPrevPage()),
    readerSetPage: async (page) => syncReader(() => backend.readerSetPage(page)),
    readerSentenceClick: async (sentenceIdx) =>
      syncReader(() => backend.readerSentenceClick(sentenceIdx)),
    readerNextSentence: async () => syncReader(() => backend.readerNextSentence()),
    readerPrevSentence: async () => syncReader(() => backend.readerPrevSentence()),
    readerToggleTextOnly: async () => syncReader(() => backend.readerToggleTextOnly()),
    readerSearchSetQuery: async (query) => syncReader(() => backend.readerSearchSetQuery(query)),
    readerSearchNext: async () => syncReader(() => backend.readerSearchNext()),
    readerSearchPrev: async () => syncReader(() => backend.readerSearchPrev())
  };
}
