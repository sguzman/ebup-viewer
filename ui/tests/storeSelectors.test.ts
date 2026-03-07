import { describe, expect, it } from "vitest";

import type { AppStore } from "../src/store/appStore";
import {
  selectCalibreSlice,
  selectJobsSlice,
  selectNotificationsSlice,
  selectReaderSlice,
  selectSessionSlice,
  selectSettingsSlice,
  selectStatsSlice,
  selectTtsSlice
} from "../src/store/selectors";

function makeState(): AppStore {
  return {
    appShell: {
      bootstrapState: null,
      runtimeLogLevel: "info",
      operations: {
        sourceOpen: false,
        starterCommand: false,
        readerCommand: false,
        readerTts: false,
        readerSettings: false,
        browserTabRefresh: false,
        calibreLoad: false,
        runtimeConfig: false
      },
      loadingBootstrap: false,
      busy: false
    },
    sessionDomain: {
      session: null
    },
    readerDocument: {
      snapshot: null
    },
    readerPlaybackDomain: {
      playback: null,
      ttsStateEvent: null,
      playbackEvent: null
    },
    readerUi: {
      sourcePath: null,
      currentPage: null,
      totalPages: null,
      textOnlyMode: false,
      prettyKind: null,
      searchQuery: "",
      searchMatches: [],
      selectedSearchMatch: null,
      panels: null,
      settings: null
    },
    starter: {
      recents: [],
      calibreBooks: [],
      loadingRecents: false,
      loadingCalibre: false
    },
    jobs: {
      sourceOpenEvent: null,
      calibreLoadEvent: null,
      pdfTranscriptionEvent: null,
      logLevelEvent: null,
      sourceOpenSubscribed: false,
      calibreSubscribed: false,
      ttsStateSubscribed: false,
      pdfTranscriptionSubscribed: false,
      logLevelSubscribed: false,
      sessionStateSubscribed: false,
      readerStateSubscribed: false,
      readerPlaybackStateSubscribed: false,
      lastSessionEventRequestId: 0,
      lastReaderEventRequestId: 0,
      lastReaderPlaybackEventRequestId: 0,
      lastSourceOpenEventRequestId: 0,
      lastCalibreEventRequestId: 0,
      lastTtsEventRequestId: 0,
      lastPdfEventRequestId: 0,
      lastLogLevelEventRequestId: 0
    },
    notifications: {
      error: null,
      toast: null,
      telemetry: []
    },
    bootstrapState: null,
    session: null,
    reader: null,
    readerPlayback: null,
    recents: [],
    calibreBooks: [],
    telemetry: [],
    operations: {
      sourceOpen: false,
      starterCommand: false,
      readerCommand: false,
      readerTts: false,
      readerSettings: false,
      browserTabRefresh: false,
      calibreLoad: false,
      runtimeConfig: false
    },
    loadingBootstrap: false,
    loadingRecents: false,
    loadingCalibre: false,
    busy: false,
    error: null,
    toast: null,
    sourceOpenEvent: null,
    calibreLoadEvent: null,
    ttsStateEvent: null,
    readerPlaybackStateEvent: null,
    pdfTranscriptionEvent: null,
    logLevelEvent: null,
    runtimeLogLevel: "info",
    sourceOpenSubscribed: false,
    calibreSubscribed: false,
    ttsStateSubscribed: false,
    pdfTranscriptionSubscribed: false,
    logLevelSubscribed: false,
    sessionStateSubscribed: false,
    readerStateSubscribed: false,
    readerPlaybackStateSubscribed: false,
    lastSessionEventRequestId: 0,
    lastReaderEventRequestId: 0,
    lastReaderPlaybackEventRequestId: 0,
    lastSourceOpenEventRequestId: 0,
    lastCalibreEventRequestId: 0,
    lastTtsEventRequestId: 0,
    lastPdfEventRequestId: 0,
    lastLogLevelEventRequestId: 0,
    appSafeQuit: async () => {},
    bootstrap: async () => {},
    refreshRecents: async () => {},
    openSourcePath: async () => {},
    openClipboardText: async () => {},
    openBrowserTab: async () => {},
    refreshCurrentBrowserTab: async () => {},
    deleteRecent: async () => {},
    returnToStarter: async () => {},
    closeReaderSession: async () => {},
    refreshReaderSnapshot: async () => {},
    readerNextPage: async () => {},
    readerPrevPage: async () => {},
    readerSetPage: async () => {},
    readerSentenceClick: async () => {},
    readerNextSentence: async () => {},
    readerPrevSentence: async () => {},
    readerToggleTextOnly: async () => {},
    readerApplySettings: async () => {},
    readerSearchSetQuery: async () => {},
    readerSearchNext: async () => {},
    readerSearchPrev: async () => {},
    readerTtsPlay: async () => {},
    readerTtsPause: async () => {},
    readerTtsTogglePlayPause: async () => {},
    readerTtsPlayFromPageStart: async () => {},
    readerTtsPlayFromHighlight: async () => {},
    readerTtsSeekNext: async () => {},
    readerTtsSeekPrev: async () => {},
    readerTtsRepeatSentence: async () => {},
    readerTtsPrecomputePage: async () => {},
    toggleSettingsPanel: async () => {},
    toggleStatsPanel: async () => {},
    toggleTtsPanel: async () => {},
    loadCalibreBooks: async () => {},
    openCalibreBook: async () => {},
    setRuntimeLogLevel: async () => {},
    toggleTheme: async () => {},
    clearError: () => {},
    dismissToast: () => {},
    clearTelemetry: () => {}
  };
}

describe("store selectors", () => {
  it("projects stable slices from a base app store", () => {
    const state = makeState();
    state.operations.readerTts = true;
    state.operations.calibreLoad = true;
    state.appShell.operations.readerTts = true;
    state.appShell.operations.calibreLoad = true;

    expect(selectSessionSlice(state).loadingBootstrap).toBe(false);
    expect(selectReaderSlice(state).reader).toBeNull();
    expect(selectReaderSlice(state).busy).toBe(true);
    expect(selectTtsSlice(state).ttsStateEvent).toBeNull();
    expect(selectCalibreSlice(state).calibreBooks).toEqual([]);
    expect(selectCalibreSlice(state).loadingCalibre).toBe(true);
    expect(selectSettingsSlice(state).runtimeLogLevel).toBe("info");
    expect(selectStatsSlice(state).stats).toBeNull();
    expect(selectJobsSlice(state).pdfTranscriptionEvent).toBeNull();
    expect(selectNotificationsSlice(state).error).toBeNull();
  });
});
