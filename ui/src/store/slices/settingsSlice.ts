import type { AppStore } from "../appStore";
import { buildToast, toBridgeError, togglePanels, withBusy } from "./shared";
import type { SliceContext } from "./types";

export function createSettingsSliceActions({ set, get, backend }: SliceContext): Pick<
  AppStore,
  | "readerApplySettings"
  | "toggleSettingsPanel"
  | "toggleStatsPanel"
  | "toggleTtsPanel"
  | "setRuntimeLogLevel"
  | "toggleTheme"
> {
  let applySettingsRequestSeq = 0;

  return {
    readerApplySettings: async (patch) => {
      const requestSeq = ++applySettingsRequestSeq;
      const previousReader = get().reader;

      if (previousReader) {
        set({
          reader: {
            ...previousReader,
            settings: {
              ...previousReader.settings,
              ...patch
            }
          }
        });
      }

      try {
        const reader = await backend.readerApplySettings(patch);
        if (requestSeq !== applySettingsRequestSeq) {
          return;
        }
        set({ reader });
      } catch (error) {
        if (requestSeq !== applySettingsRequestSeq) {
          return;
        }
        set({
          reader: previousReader,
          error: toBridgeError(error).message
        });
      }
    },

    toggleSettingsPanel: async () => {
      const previousSession = get().session;
      const previousReader = get().reader;
      if (previousSession) {
        const panels = togglePanels(previousSession.panels, "show_settings");
        set({
          session: { ...previousSession, panels },
          reader: previousReader ? { ...previousReader, panels } : previousReader
        });
      }
      try {
        const session = await backend.panelToggleSettings();
        set({ session });
      } catch (error) {
        set({ session: previousSession, reader: previousReader });
        set({ error: toBridgeError(error).message });
      }
    },

    toggleStatsPanel: async () => {
      const previousSession = get().session;
      const previousReader = get().reader;
      if (previousSession) {
        const panels = togglePanels(previousSession.panels, "show_stats");
        set({
          session: { ...previousSession, panels },
          reader: previousReader ? { ...previousReader, panels } : previousReader
        });
      }
      try {
        const session = await backend.panelToggleStats();
        set({ session });
      } catch (error) {
        set({ session: previousSession, reader: previousReader });
        set({ error: toBridgeError(error).message });
      }
    },

    toggleTtsPanel: async () => {
      const previousSession = get().session;
      const previousReader = get().reader;
      if (previousSession) {
        const panels = togglePanels(previousSession.panels, "show_tts");
        set({
          session: { ...previousSession, panels },
          reader: previousReader ? { ...previousReader, panels } : previousReader
        });
      }
      try {
        const session = await backend.panelToggleTts();
        set({ session });
      } catch (error) {
        set({ session: previousSession, reader: previousReader });
        set({ error: toBridgeError(error).message });
      }
    },

    toggleTheme: async () => {
      await withBusy(set, get, "toggleTheme", "readerSettings", async () => {
        const previousBootstrap = get().bootstrapState;
        const previousReader = get().reader;
        const session = get().session;

        // In reader mode, theme must be written through reader settings so future
        // reader snapshots keep the same mode and highlight palette.
        if (session?.mode === "reader" && previousReader) {
          const optimisticTheme = previousReader.settings.theme === "night" ? "day" : "night";

          set({
            reader: {
              ...previousReader,
              settings: {
                ...previousReader.settings,
                theme: optimisticTheme
              }
            },
            bootstrapState: previousBootstrap
              ? {
                  ...previousBootstrap,
                  config: {
                    ...previousBootstrap.config,
                    theme: optimisticTheme
                  }
                }
              : previousBootstrap
          });

          try {
            const reader = await backend.readerApplySettings({ theme: optimisticTheme });
            set({
              reader,
              bootstrapState: previousBootstrap
                ? {
                    ...previousBootstrap,
                    config: {
                      ...previousBootstrap.config,
                      theme: reader.settings.theme
                    }
                  }
                : previousBootstrap
            });
          } catch (error) {
            const bridgeError = toBridgeError(error);
            set({
              bootstrapState: previousBootstrap,
              reader: previousReader,
              error: bridgeError.message
            });
            throw bridgeError;
          }
          return;
        }

        if (previousBootstrap) {
          const optimisticTheme =
            previousBootstrap.config.theme === "night" ? "day" : "night";
          set({
            bootstrapState: {
              ...previousBootstrap,
              config: {
                ...previousBootstrap.config,
                theme: optimisticTheme
              }
            },
            reader: previousReader
              ? {
                  ...previousReader,
                  settings: {
                    ...previousReader.settings,
                    theme: optimisticTheme
                  }
                }
              : previousReader
          });
        }

        try {
          const bootstrapState = await backend.sessionToggleTheme();
          const reader = get().reader;
          set({
            bootstrapState,
            reader: reader
              ? {
                  ...reader,
                  settings: {
                    ...reader.settings,
                    theme: bootstrapState.config.theme
                  }
                }
              : reader
          });
        } catch (error) {
          const bridgeError = toBridgeError(error);
          set({
            bootstrapState: previousBootstrap,
            reader: previousReader,
            error: bridgeError.message
          });
          throw bridgeError;
        }
      });
    },

    setRuntimeLogLevel: async (level) => {
      await withBusy(set, get, "setRuntimeLogLevel", "runtimeConfig", async () => {
        try {
          const normalized = await backend.loggingSetLevel(level);
          set({
            runtimeLogLevel: normalized,
            toast: buildToast("success", `Log level set to ${normalized}`)
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
