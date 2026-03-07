import type { AppStore } from "../appStore";
import {
  buildToast,
  finishTelemetry,
  setOperationBusy,
  toBridgeError,
  toReaderPlaybackState,
  withBusy
} from "./shared";
import type { SliceContext } from "./types";

export function createCalibreSliceActions({ set, get, backend }: SliceContext): Pick<
  AppStore,
  "loadCalibreBooks" | "openCalibreBook"
> {
  return {
    loadCalibreBooks: async (forceRefresh) => {
      const startedAt = Date.now();
      set({ loadingCalibre: true, error: null });
      setOperationBusy(set, get, "calibreLoad", true);
      try {
        const calibreBooks = await backend.calibreLoadBooks(forceRefresh);
        set({ calibreBooks });
        finishTelemetry(set, get, "loadCalibreBooks", startedAt, true, null);
      } catch (error) {
        const bridgeError = toBridgeError(error);
        set({
          error: bridgeError.message,
          toast: buildToast("error", bridgeError.message)
        });
        finishTelemetry(set, get, "loadCalibreBooks", startedAt, false, bridgeError.message);
      } finally {
        set({ loadingCalibre: false });
        setOperationBusy(set, get, "calibreLoad", false);
      }
    },

    openCalibreBook: async (bookId) => {
      await withBusy(set, get, "openCalibreBook", "sourceOpen", async () => {
        try {
          const result = await backend.calibreOpenBook(bookId);
          const recents = await backend.recentList();
          set({
            session: result.session,
            reader: result.reader,
            readerPlayback: toReaderPlaybackState(result.reader),
            recents,
            toast: buildToast("success", "Book opened from calibre")
          });
        } catch (error) {
          const bridgeError = toBridgeError(error);
          set({
            error: bridgeError.message,
            toast: buildToast("error", bridgeError.message)
          });
          throw bridgeError;
        }
      });
    }
  };
}
