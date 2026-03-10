import { useShallow } from "zustand/react/shallow";

import { useAppStore, type AppStore } from "./appStore";

function selectReaderBusy(state: AppStore): boolean {
  return (
    state.appShell.operations.readerCommand ||
    state.appShell.operations.readerTts ||
    state.appShell.operations.readerSettings ||
    state.appShell.operations.browserTabRefresh
  );
}

function selectStarterBusy(state: AppStore): boolean {
  return (
    state.appShell.operations.sourceOpen ||
    state.appShell.operations.starterCommand ||
    state.appShell.operations.calibreLoad ||
    state.appShell.operations.runtimeConfig
  );
}

function selectReaderDocumentKey(state: AppStore): string {
  const reader = state.readerDocument.snapshot;
  if (!reader) {
    return "";
  }
  return [
    reader.source_path,
    reader.current_page,
    reader.text_only_mode ? "text" : "pretty",
    reader.pretty_kind,
    reader.sentences.length,
    reader.reading_markdown_page ?? "",
    reader.reading_html_page ?? ""
  ].join("\n");
}

export const selectSessionSlice = (state: AppStore) => ({
  bootstrapState: state.appShell.bootstrapState,
  session: state.sessionDomain.session,
  loadingBootstrap: state.appShell.loadingBootstrap,
  loadingRecents: state.starter.loadingRecents,
  appSafeQuit: state.appSafeQuit,
  bootstrap: state.bootstrap,
  refreshRecents: state.refreshRecents,
  openSourcePath: state.openSourcePath,
  openClipboardText: state.openClipboardText,
  openBrowserTab: state.openBrowserTab,
  refreshCurrentBrowserTab: state.refreshCurrentBrowserTab,
  deleteRecent: state.deleteRecent,
  closeRecentBrowserTab: state.closeRecentBrowserTab,
  returnToStarter: state.returnToStarter,
  closeReaderSession: state.closeReaderSession
});

export const selectReaderSlice = (state: AppStore) => ({
  reader: state.readerDocument.snapshot,
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
  reader: state.readerDocument.snapshot,
  ttsStateEvent: state.readerPlaybackDomain.ttsStateEvent,
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
  calibreBooks: state.starter.calibreBooks,
  loadingCalibre: state.starter.loadingCalibre || state.appShell.operations.calibreLoad,
  loadCalibreBooks: state.loadCalibreBooks,
  openCalibreBook: state.openCalibreBook
});

export const selectSettingsSlice = (state: AppStore) => ({
  runtimeLogLevel: state.appShell.runtimeLogLevel,
  toggleSettingsPanel: state.toggleSettingsPanel,
  toggleStatsPanel: state.toggleStatsPanel,
  toggleTtsPanel: state.toggleTtsPanel,
  setRuntimeLogLevel: state.setRuntimeLogLevel,
  toggleTheme: state.toggleTheme
});

export const selectStatsSlice = (state: AppStore) => ({
  stats: state.readerPlaybackDomain.playback?.stats ?? null
});

export const selectJobsSlice = (state: AppStore) => ({
  sourceOpenEvent: state.jobs.sourceOpenEvent,
  calibreLoadEvent: state.jobs.calibreLoadEvent,
  pdfTranscriptionEvent: state.jobs.pdfTranscriptionEvent,
  ttsStateEvent: state.readerPlaybackDomain.ttsStateEvent
});

export const selectNotificationsSlice = (state: AppStore) => ({
  error: state.notifications.error,
  toast: state.notifications.toast,
  clearError: state.clearError,
  dismissToast: state.dismissToast,
  telemetry: state.notifications.telemetry,
  clearTelemetry: state.clearTelemetry
});

export function useAppShellState() {
  return useAppStore(
    useShallow((state) => ({
      loadingBootstrap: state.appShell.loadingBootstrap,
      error: state.notifications.error,
      clearError: state.clearError,
      bootstrap: state.bootstrap
    }))
  );
}

export function useAppThemeState() {
  return useAppStore(
    useShallow((state) => ({
      bootstrapState: state.appShell.bootstrapState,
      readerThemeSettings: state.readerUi.settings
    }))
  );
}

export function useAppKeyboardBindings() {
  return useAppStore(
    useShallow((state) => ({
      bootstrapState: state.appShell.bootstrapState,
      sessionMode: state.sessionDomain.session?.mode ?? null,
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
      sessionMode: state.sessionDomain.session?.mode ?? "unknown",
      sourceOpenEvent: state.jobs.sourceOpenEvent,
      pdfTranscriptionEvent: state.jobs.pdfTranscriptionEvent,
      calibreLoadEvent: state.jobs.calibreLoadEvent
    }))
  );
}

export function useAppToastState() {
  return useAppStore(
    useShallow((state) => ({
      toast: state.notifications.toast,
      dismissToast: state.dismissToast
    }))
  );
}

export function useReaderQuickActionsState() {
  return useAppStore(
    useShallow((state) => ({
      busy:
        state.appShell.operations.readerCommand ||
        state.appShell.operations.readerSettings ||
        state.appShell.operations.browserTabRefresh,
      isTextOnly: state.readerUi.textOnlyMode,
      isBrowserTab: state.readerUi.sourcePath?.toLowerCase().endsWith(".lltab") ?? false,
      showSettings: state.readerUi.panels?.show_settings ?? false,
      showStats: state.readerUi.panels?.show_stats ?? false,
      showTts: state.readerUi.panels?.show_tts ?? false,
      onRefreshBrowserTab: state.refreshCurrentBrowserTab,
      onToggleTextOnly: state.readerToggleTextOnly,
      onToggleSettingsPanel: state.toggleSettingsPanel,
      onToggleStatsPanel: state.toggleStatsPanel,
      onToggleTtsPanel: state.toggleTtsPanel
    }))
  );
}

export function useReaderQuickActionsBusy(): boolean {
  return useAppStore(
    (state) =>
      state.appShell.operations.readerCommand ||
      state.appShell.operations.readerSettings ||
      state.appShell.operations.browserTabRefresh
  );
}

export function useReaderQuickActionsFlags() {
  return useAppStore(
    useShallow((state) => ({
      isBrowserTab: state.readerUi.sourcePath?.toLowerCase().endsWith(".lltab") ?? false,
      isTextOnly: state.readerUi.textOnlyMode,
      showSettings: state.readerUi.panels?.show_settings ?? false,
      showStats: state.readerUi.panels?.show_stats ?? false,
      showTts: state.readerUi.panels?.show_tts ?? false
    }))
  );
}

export function useReaderQuickActionsActions() {
  return useAppStore(
    useShallow((state) => ({
      onRefreshBrowserTab: state.refreshCurrentBrowserTab,
      onToggleTextOnly: state.readerToggleTextOnly,
      onToggleSettingsPanel: state.toggleSettingsPanel,
      onToggleStatsPanel: state.toggleStatsPanel,
      onToggleTtsPanel: state.toggleTtsPanel
    }))
  );
}

export function useReaderDocumentState() {
  const documentKey = useAppStore(selectReaderDocumentKey);
  return useAppStore(
    useShallow((state) => ({
      documentKey,
      reader: state.readerDocument.snapshot
        ? {
            source_path: state.readerDocument.snapshot.source_path,
            current_page: state.readerDocument.snapshot.current_page,
            text_only_mode: state.readerDocument.snapshot.text_only_mode,
            pretty_kind: state.readerDocument.snapshot.pretty_kind,
            reading_markdown_page: state.readerDocument.snapshot.reading_markdown_page,
            reading_html_page: state.readerDocument.snapshot.reading_html_page,
            page_text: state.readerDocument.snapshot.page_text,
            sentences: state.readerDocument.snapshot.sentences,
            sentence_anchor_map: state.readerDocument.snapshot.sentence_anchor_map
          }
        : null
    }))
  );
}

export function useReaderDocumentKey(): string {
  return useAppStore(selectReaderDocumentKey);
}

export function useReaderViewState() {
  return useAppStore(
    useShallow((state) => ({
      reader: state.readerDocument.snapshot,
      busy: selectReaderBusy(state)
    }))
  );
}

export function useReaderViewTuple(): readonly [AppStore["reader"], boolean] {
  return useAppStore(
    useShallow((state) => [state.readerDocument.snapshot, selectReaderBusy(state)] as const)
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
  return useAppStore((state) => state.readerPlaybackDomain.ttsStateEvent);
}

export function useReaderPlaybackState(sourcePath: string, currentPage: number) {
  return useAppStore((state) => {
    const playback = state.readerPlaybackDomain.playback;
    if (!playback) {
      return null;
    }
    if (playback.source_path !== sourcePath || playback.current_page !== currentPage) {
      return null;
    }
    return playback;
  });
}

export function useStarterViewState() {
  return useAppStore(
    useShallow((state) => ({
      bootstrapState: state.appShell.bootstrapState,
      recents: state.starter.recents,
      calibreBooks: state.starter.calibreBooks,
      busy: selectStarterBusy(state),
      loadingRecents: state.starter.loadingRecents,
      loadingCalibre: state.starter.loadingCalibre || state.appShell.operations.calibreLoad,
      sourceOpenEvent: state.jobs.sourceOpenEvent,
      calibreLoadEvent: state.jobs.calibreLoadEvent,
      pdfTranscriptionEvent: state.jobs.pdfTranscriptionEvent,
      runtimeLogLevel: state.appShell.runtimeLogLevel
    }))
  );
}

export function useStarterViewTuple(): readonly [
  AppStore["bootstrapState"],
  AppStore["recents"],
  AppStore["calibreBooks"],
  boolean,
  boolean,
  boolean,
  AppStore["sourceOpenEvent"],
  AppStore["calibreLoadEvent"],
  AppStore["pdfTranscriptionEvent"],
  AppStore["runtimeLogLevel"]
] {
  return useAppStore(
    useShallow((state) =>
      [
        state.appShell.bootstrapState,
        state.starter.recents,
        state.starter.calibreBooks,
        selectStarterBusy(state),
        state.starter.loadingRecents,
        state.starter.loadingCalibre || state.appShell.operations.calibreLoad,
        state.jobs.sourceOpenEvent,
        state.jobs.calibreLoadEvent,
        state.jobs.pdfTranscriptionEvent,
        state.appShell.runtimeLogLevel
      ] as const)
  );
}

export function useStarterActionState() {
  return useAppStore(
    useShallow((state) => ({
      openSourcePath: state.openSourcePath,
      openClipboardText: state.openClipboardText,
      openBrowserTab: state.openBrowserTab,
      openBrowserTabBundle: state.openBrowserTabBundle,
      deleteRecent: state.deleteRecent,
      closeRecentBrowserTab: state.closeRecentBrowserTab,
      refreshRecents: state.refreshRecents,
      loadCalibreBooks: state.loadCalibreBooks,
      openCalibreBook: state.openCalibreBook,
      setRuntimeLogLevel: state.setRuntimeLogLevel,
      toggleTheme: state.toggleTheme
    }))
  );
}

export function useSessionMode(): "starter" | "reader" | null {
  return useAppStore((state) => state.sessionDomain.session?.mode ?? null);
}
