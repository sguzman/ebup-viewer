import { useShallow } from "zustand/react/shallow";

import { useAppStore, type AppStore } from "./appStore";

function selectReaderBusy(state: AppStore): boolean {
  return (
    state.operations.readerCommand ||
    state.operations.readerTts ||
    state.operations.readerSettings ||
    state.operations.browserTabRefresh
  );
}

function selectStarterBusy(state: AppStore): boolean {
  return (
    state.operations.sourceOpen ||
    state.operations.starterCommand ||
    state.operations.calibreLoad ||
    state.operations.runtimeConfig
  );
}

export const selectSessionSlice = (state: AppStore) => ({
  bootstrapState: state.bootstrapState,
  session: state.session,
  loadingBootstrap: state.loadingBootstrap,
  loadingRecents: state.loadingRecents,
  appSafeQuit: state.appSafeQuit,
  bootstrap: state.bootstrap,
  refreshRecents: state.refreshRecents,
  openSourcePath: state.openSourcePath,
  openClipboardText: state.openClipboardText,
  openBrowserTab: state.openBrowserTab,
  refreshCurrentBrowserTab: state.refreshCurrentBrowserTab,
  deleteRecent: state.deleteRecent,
  returnToStarter: state.returnToStarter,
  closeReaderSession: state.closeReaderSession
});

export const selectReaderSlice = (state: AppStore) => ({
  reader: state.reader,
  busy: selectReaderBusy(state),
  refreshReaderSnapshot: state.refreshReaderSnapshot,
  readerNextPage: state.readerNextPage,
  readerPrevPage: state.readerPrevPage,
  readerSetPage: state.readerSetPage,
  readerSentenceClick: state.readerSentenceClick,
  readerNextSentence: state.readerNextSentence,
  readerPrevSentence: state.readerPrevSentence,
  readerToggleTextOnly: state.readerToggleTextOnly,
  readerApplySettings: state.readerApplySettings,
  readerSearchSetQuery: state.readerSearchSetQuery,
  readerSearchNext: state.readerSearchNext,
  readerSearchPrev: state.readerSearchPrev
});

export const selectTtsSlice = (state: AppStore) => ({
  reader: state.reader,
  ttsStateEvent: state.ttsStateEvent,
  readerTtsPlay: state.readerTtsPlay,
  readerTtsPause: state.readerTtsPause,
  readerTtsTogglePlayPause: state.readerTtsTogglePlayPause,
  readerTtsPlayFromPageStart: state.readerTtsPlayFromPageStart,
  readerTtsPlayFromHighlight: state.readerTtsPlayFromHighlight,
  readerTtsSeekNext: state.readerTtsSeekNext,
  readerTtsSeekPrev: state.readerTtsSeekPrev,
  readerTtsRepeatSentence: state.readerTtsRepeatSentence,
  readerTtsPrecomputePage: state.readerTtsPrecomputePage
});

export const selectCalibreSlice = (state: AppStore) => ({
  calibreBooks: state.calibreBooks,
  loadingCalibre: state.loadingCalibre || state.operations.calibreLoad,
  loadCalibreBooks: state.loadCalibreBooks,
  openCalibreBook: state.openCalibreBook
});

export const selectSettingsSlice = (state: AppStore) => ({
  runtimeLogLevel: state.runtimeLogLevel,
  toggleSettingsPanel: state.toggleSettingsPanel,
  toggleStatsPanel: state.toggleStatsPanel,
  toggleTtsPanel: state.toggleTtsPanel,
  setRuntimeLogLevel: state.setRuntimeLogLevel,
  toggleTheme: state.toggleTheme
});

export const selectStatsSlice = (state: AppStore) => ({
  stats: state.reader?.stats ?? null
});

export const selectJobsSlice = (state: AppStore) => ({
  sourceOpenEvent: state.sourceOpenEvent,
  calibreLoadEvent: state.calibreLoadEvent,
  pdfTranscriptionEvent: state.pdfTranscriptionEvent,
  ttsStateEvent: state.ttsStateEvent
});

export const selectNotificationsSlice = (state: AppStore) => ({
  error: state.error,
  toast: state.toast,
  clearError: state.clearError,
  dismissToast: state.dismissToast,
  telemetry: state.telemetry,
  clearTelemetry: state.clearTelemetry
});

export function useAppShellState() {
  return useAppStore(
    useShallow((state) => ({
      loadingBootstrap: state.loadingBootstrap,
      error: state.error,
      clearError: state.clearError,
      bootstrap: state.bootstrap
    }))
  );
}

export function useAppThemeState() {
  return useAppStore(
    useShallow((state) => ({
      bootstrapState: state.bootstrapState,
      readerThemeSettings: state.reader?.settings ?? null
    }))
  );
}

export function useAppKeyboardBindings() {
  return useAppStore(
    useShallow((state) => ({
      bootstrapState: state.bootstrapState,
      sessionMode: state.session?.mode ?? null,
      appSafeQuit: state.appSafeQuit,
      toggleSettingsPanel: state.toggleSettingsPanel,
      toggleStatsPanel: state.toggleStatsPanel,
      toggleTtsPanel: state.toggleTtsPanel,
      readerTtsTogglePlayPause: state.readerTtsTogglePlayPause,
      readerTtsSeekNext: state.readerTtsSeekNext,
      readerTtsSeekPrev: state.readerTtsSeekPrev,
      readerTtsRepeatSentence: state.readerTtsRepeatSentence
    }))
  );
}

export function useAppHiddenStatusState() {
  return useAppStore(
    useShallow((state) => ({
      sessionMode: state.session?.mode ?? "unknown",
      sourceOpenEvent: state.sourceOpenEvent,
      pdfTranscriptionEvent: state.pdfTranscriptionEvent,
      calibreLoadEvent: state.calibreLoadEvent
    }))
  );
}

export function useAppToastState() {
  return useAppStore(
    useShallow((state) => ({
      toast: state.toast,
      dismissToast: state.dismissToast
    }))
  );
}

export function useReaderQuickActionsState() {
  return useAppStore(
    useShallow((state) => ({
      busy:
        state.operations.readerCommand ||
        state.operations.readerSettings ||
        state.operations.browserTabRefresh,
      isTextOnly: state.reader?.text_only_mode ?? false,
      isBrowserTab: state.reader?.source_path.toLowerCase().endsWith(".lltab") ?? false,
      showSettings: state.reader?.panels.show_settings ?? false,
      showStats: state.reader?.panels.show_stats ?? false,
      showTts: state.reader?.panels.show_tts ?? false,
      onRefreshBrowserTab: state.refreshCurrentBrowserTab,
      onToggleTextOnly: state.readerToggleTextOnly,
      onToggleSettingsPanel: state.toggleSettingsPanel,
      onToggleStatsPanel: state.toggleStatsPanel,
      onToggleTtsPanel: state.toggleTtsPanel
    }))
  );
}

export function useReaderViewState() {
  return useAppStore(
    useShallow((state) => ({
      reader: state.reader,
      busy: selectReaderBusy(state)
    }))
  );
}

export function useReaderActionState() {
  return useAppStore(
    useShallow((state) => ({
      closeReaderSession: state.closeReaderSession,
      readerNextPage: state.readerNextPage,
      readerPrevPage: state.readerPrevPage,
      readerSetPage: state.readerSetPage,
      readerSentenceClick: state.readerSentenceClick,
      readerNextSentence: state.readerNextSentence,
      readerPrevSentence: state.readerPrevSentence,
      readerTtsPlay: state.readerTtsPlay,
      readerTtsPause: state.readerTtsPause,
      readerTtsTogglePlayPause: state.readerTtsTogglePlayPause,
      readerTtsPlayFromPageStart: state.readerTtsPlayFromPageStart,
      readerTtsPlayFromHighlight: state.readerTtsPlayFromHighlight,
      readerTtsSeekNext: state.readerTtsSeekNext,
      readerTtsSeekPrev: state.readerTtsSeekPrev,
      readerTtsRepeatSentence: state.readerTtsRepeatSentence,
      readerTtsPrecomputePage: state.readerTtsPrecomputePage,
      readerToggleTextOnly: state.readerToggleTextOnly,
      readerSearchSetQuery: state.readerSearchSetQuery,
      readerSearchNext: state.readerSearchNext,
      readerSearchPrev: state.readerSearchPrev,
      readerApplySettings: state.readerApplySettings,
      toggleTheme: state.toggleTheme,
      toggleSettingsPanel: state.toggleSettingsPanel,
      toggleStatsPanel: state.toggleStatsPanel,
      toggleTtsPanel: state.toggleTtsPanel
    }))
  );
}

export function useReaderTtsMetaState() {
  return useAppStore((state) => state.ttsStateEvent);
}

export function useStarterViewState() {
  return useAppStore(
    useShallow((state) => ({
      bootstrapState: state.bootstrapState,
      recents: state.recents,
      calibreBooks: state.calibreBooks,
      busy: selectStarterBusy(state),
      loadingRecents: state.loadingRecents,
      loadingCalibre: state.loadingCalibre || state.operations.calibreLoad,
      sourceOpenEvent: state.sourceOpenEvent,
      calibreLoadEvent: state.calibreLoadEvent,
      pdfTranscriptionEvent: state.pdfTranscriptionEvent,
      runtimeLogLevel: state.runtimeLogLevel
    }))
  );
}

export function useStarterActionState() {
  return useAppStore(
    useShallow((state) => ({
      openSourcePath: state.openSourcePath,
      openClipboardText: state.openClipboardText,
      openBrowserTab: state.openBrowserTab,
      deleteRecent: state.deleteRecent,
      refreshRecents: state.refreshRecents,
      loadCalibreBooks: state.loadCalibreBooks,
      openCalibreBook: state.openCalibreBook,
      setRuntimeLogLevel: state.setRuntimeLogLevel,
      toggleTheme: state.toggleTheme
    }))
  );
}

export function useSessionMode(): "starter" | "reader" | null {
  return useAppStore((state) => state.session?.mode ?? null);
}
