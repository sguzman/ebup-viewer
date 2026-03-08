import { createStore } from "zustand/vanilla";
import { describe, expect, it } from "vitest";

import type { BackendApi } from "../src/api/tauri";
import { createAppStoreState, type AppStore } from "../src/store/appStore";
import type {
  BootstrapState,
  CalibreBook,
  CalibreLoadEvent,
  OpenSourceResult,
  ReaderPlaybackStateEvent,
  ReaderSettingsPatch,
  ReaderSnapshot,
  ReaderStateEvent,
  RecentBook,
  LogLevelEvent,
  PdfTranscriptionEvent,
  SessionState,
  SessionStateEvent,
  SourceOpenEvent,
  TtsStateEvent
} from "../src/types";

function makeBootstrapState(): BootstrapState {
  return {
    app_name: "LanternLeaf",
    mode: "test",
    config: {
      theme: "day",
      font_family: "lexend",
      font_weight: "bold",
      day_highlight: { r: 0.2, g: 0.4, b: 0.7, a: 0.15 },
      night_highlight: { r: 0.8, g: 0.8, b: 0.5, a: 0.2 },
      log_level: "debug",
      default_font_size: 22,
      default_lines_per_page: 700,
      default_tts_speed: 2.5,
      default_pause_after_sentence: 0.06,
      key_toggle_play_pause: "space",
      key_next_sentence: "f",
      key_prev_sentence: "s",
      key_repeat_sentence: "r",
      key_toggle_search: "ctrl+f",
      key_safe_quit: "q",
      key_toggle_settings: "ctrl+t",
      key_toggle_stats: "ctrl+g",
      key_toggle_tts: "ctrl+y",
      browser_tabs_enabled: true,
      close_browser_tab_on_recent_delete: true
    }
  };
}

function makeSessionState(mode: SessionState["mode"]): SessionState {
  return {
    mode,
    active_source_path: mode === "reader" ? "/tmp/book.epub" : null,
    open_in_flight: false,
    panels: {
      show_settings: true,
      show_stats: false,
      show_tts: true
    }
  };
}

function makeReaderSnapshot(sourcePath: string, sentence: string): ReaderSnapshot {
  return {
    source_path: sourcePath,
    source_name: sourcePath.split("/").pop() ?? "book.epub",
    current_page: 0,
    total_pages: 1,
    text_only_mode: false,
    page_text: sentence,
    sentences: [sentence],
    highlighted_sentence_idx: 0,
    search_query: "",
    search_matches: [],
    selected_search_match: null,
    settings: {
      theme: "day",
      font_family: "lexend",
      font_weight: "bold",
      day_highlight: { r: 0.2, g: 0.4, b: 0.7, a: 0.15 },
      night_highlight: { r: 0.8, g: 0.8, b: 0.5, a: 0.2 },
      font_size: 22,
      line_spacing: 1.2,
      word_spacing: 0,
      letter_spacing: 0,
      margin_horizontal: 100,
      margin_vertical: 12,
      lines_per_page: 700,
      pause_after_sentence: 0.06,
      auto_scroll_tts: true,
      center_spoken_sentence: true,
      tts_speed: 2.5,
      tts_volume: 1
    },
    tts: {
      state: "idle",
      current_sentence_idx: 0,
      sentence_count: 1,
      can_seek_prev: false,
      can_seek_next: false,
      progress_pct: 100
    },
    stats: {
      page_index: 1,
      total_pages: 1,
      tts_progress_pct: 100,
      page_time_remaining_secs: 0,
      book_time_remaining_secs: 0,
      page_word_count: 1,
      page_sentence_count: 1,
      page_start_percent: 0,
      page_end_percent: 100,
      words_read_up_to_page_start: 0,
      sentences_read_up_to_page_start: 0,
      words_read_up_to_page_end: 1,
      sentences_read_up_to_page_end: 1,
      words_read_up_to_current_position: 1,
      sentences_read_up_to_current_position: 1
    },
    panels: {
      show_settings: true,
      show_stats: false,
      show_tts: true
    }
  };
}

function createBackend(overrides: Partial<BackendApi> = {}) {
  const hooks: {
    source?: (event: SourceOpenEvent) => void;
    calibre?: (event: CalibreLoadEvent) => void;
    session?: (event: SessionStateEvent) => void;
    reader?: (event: ReaderStateEvent) => void;
    readerPlayback?: (event: ReaderPlaybackStateEvent) => void;
    tts?: (event: TtsStateEvent) => void;
    pdf?: (event: PdfTranscriptionEvent) => void;
    logLevel?: (event: LogLevelEvent) => void;
  } = {};

  const defaultReader = makeReaderSnapshot("/tmp/default.epub", "Default");
  const defaultOpenResult: OpenSourceResult = {
    session: makeSessionState("reader"),
    reader: defaultReader
  };

  const defaultBackend: BackendApi = {
    appSafeQuit: async () => {},
    sessionGetBootstrap: async () => makeBootstrapState(),
    sessionGetState: async () => makeSessionState("starter"),
    sessionReturnToStarter: async () => makeSessionState("starter"),
    panelToggleSettings: async () => makeSessionState("reader"),
    panelToggleStats: async () => makeSessionState("reader"),
    panelToggleTts: async () => makeSessionState("reader"),
    recentList: async () => [] as RecentBook[],
    recentDelete: async () => {},
    recentCloseBrowserTab: async () => {},
    browserTabsHealth: async () => ({ ok: true, extension_connected: true, now: null }),
    browserTabsListWindows: async () => [],
    browserTabsListTabs: async () => [],
    sourceOpenPath: async () => defaultOpenResult,
    sourceOpenClipboard: async () => defaultOpenResult,
    sourceOpenClipboardText: async () => defaultOpenResult,
    sourceOpenBrowserTab: async () => defaultOpenResult,
    sourceRefreshBrowserTab: async () => defaultOpenResult,
    readerGetSnapshot: async () => defaultReader,
    readerNextPage: async () => defaultReader,
    readerPrevPage: async () => defaultReader,
    readerSetPage: async () => defaultReader,
    readerSentenceClick: async () => defaultReader,
    readerNextSentence: async () => defaultReader,
    readerPrevSentence: async () => defaultReader,
    readerToggleTextOnly: async () => defaultReader,
    readerApplySettings: async (_patch: ReaderSettingsPatch) => defaultReader,
    readerSearchSetQuery: async () => defaultReader,
    readerSearchNext: async () => defaultReader,
    readerSearchPrev: async () => defaultReader,
    readerTtsPlay: async () => defaultReader,
    readerTtsPause: async () => defaultReader,
    readerTtsTogglePlayPause: async () => defaultReader,
    readerTtsPlayFromPageStart: async () => defaultReader,
    readerTtsPlayFromHighlight: async () => defaultReader,
    readerTtsSeekNext: async () => defaultReader,
    readerTtsSeekPrev: async () => defaultReader,
    readerTtsRepeatSentence: async () => defaultReader,
    readerTtsPrecomputePage: async () => defaultReader,
    readerCloseSession: async () => makeSessionState("starter"),
    loggingSetLevel: async () => "debug",
    calibreLoadBooks: async () => [] as CalibreBook[],
    calibreLoadCachedBooks: async () => [] as CalibreBook[],
    calibreOpenBook: async () => defaultOpenResult,
    calibreEnsureThumbnail: async () => null,
    onSourceOpen: async (handler) => {
      hooks.source = handler;
      return async () => {};
    },
    onCalibreLoad: async (handler) => {
      hooks.calibre = handler;
      return async () => {};
    },
    onSessionState: async (handler) => {
      hooks.session = handler;
      return async () => {};
    },
    onReaderState: async (handler) => {
      hooks.reader = handler;
      return async () => {};
    },
    onReaderPlaybackState: async (handler) => {
      hooks.readerPlayback = handler;
      return async () => {};
    },
    onTtsState: async (handler) => {
      hooks.tts = handler;
      return async () => {};
    },
    onPdfTranscription: async (handler) => {
      hooks.pdf = handler;
      return async () => {};
    },
    onLogLevel: async (handler) => {
      hooks.logLevel = handler;
      return async () => {};
    }
  };

  return {
    backend: {
      ...defaultBackend,
      ...overrides
    } satisfies BackendApi,
    hooks
  };
}

function createTestStore(backend: BackendApi) {
  return createStore<AppStore>(createAppStoreState(backend));
}

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("appStore event handling", () => {
  it("applies the newest reader event and ignores stale reader events", async () => {
    const { backend, hooks } = createBackend();
    const store = createTestStore(backend);
    await store.getState().bootstrap();

    const latest = makeReaderSnapshot("/tmp/latest.epub", "Latest sentence");
    const stale = makeReaderSnapshot("/tmp/stale.epub", "Stale sentence");
    hooks.reader?.({ request_id: 10, action: "reader_state", reader: latest });
    hooks.reader?.({ request_id: 9, action: "reader_state", reader: stale });

    const state = store.getState();
    expect(state.reader?.source_path).toBe("/tmp/latest.epub");
    expect(state.reader?.sentences[0]).toBe("Latest sentence");
    expect(state.lastReaderEventRequestId).toBe(10);
  });

  it("clears reader state when a newer starter session event arrives", async () => {
    const { backend, hooks } = createBackend();
    const store = createTestStore(backend);
    await store.getState().bootstrap();

    const reader = makeReaderSnapshot("/tmp/live.epub", "Live");
    hooks.reader?.({ request_id: 5, action: "reader_state", reader });
    hooks.session?.({
      request_id: 6,
      action: "session_state",
      session: makeSessionState("starter")
    });

    const state = store.getState();
    expect(state.session?.mode).toBe("starter");
    expect(state.reader).toBeNull();
    expect(state.lastSessionEventRequestId).toBe(6);
    expect(state.lastReaderEventRequestId).toBe(6);
  });

  it("preserves session object identity on reader events when derived session fields are unchanged", async () => {
    const { backend, hooks } = createBackend();
    const store = createTestStore(backend);
    await store.getState().bootstrap();

    hooks.reader?.({
      request_id: 7,
      action: "reader_state",
      reader: makeReaderSnapshot("/tmp/book.epub", "First reader event")
    });
    const before = store.getState().session;
    hooks.reader?.({
      request_id: 8,
      action: "reader_state",
      reader: makeReaderSnapshot("/tmp/book.epub", "Same session projection")
    });

    const after = store.getState().session;
    expect(after).toBe(before);
  });

  it("preserves reader object identity on repeated reader events with no visible change", async () => {
    const { backend, hooks } = createBackend();
    const store = createTestStore(backend);
    await store.getState().bootstrap();

    hooks.reader?.({
      request_id: 9,
      action: "reader_state",
      reader: makeReaderSnapshot("/tmp/book.epub", "Stable reader event")
    });
    const before = store.getState().reader;

    hooks.reader?.({
      request_id: 10,
      action: "reader_state",
      reader: makeReaderSnapshot("/tmp/book.epub", "Stable reader event")
    });

    const after = store.getState().reader;
    expect(after).toBe(before);
  });

  it("treats open_cancelled as info without setting app error", async () => {
    const { backend } = createBackend({
      sourceOpenPath: async () => {
        throw {
          code: "open_cancelled",
          message: "Source open request was superseded or cancelled"
        };
      }
    });
    const store = createTestStore(backend);

    await store.getState().openSourcePath("/tmp/book.epub");

    const state = store.getState();
    expect(state.error).toBeNull();
    expect(state.toast?.severity).toBe("info");
    expect(state.toast?.message).toContain("superseded");
    expect(state.busy).toBe(false);
  });

  it("shows informational toast for source-open cancelled events", async () => {
    const { backend, hooks } = createBackend();
    const store = createTestStore(backend);
    await store.getState().bootstrap();

    hooks.source?.({
      request_id: 11,
      phase: "cancelled",
      source_path: "/tmp/book.pdf",
      message: "Source open request cancelled by session close"
    });

    const state = store.getState();
    expect(state.toast?.severity).toBe("info");
    expect(state.toast?.message).toContain("session close");
    expect(state.error).toBeNull();
  });

  it("tracks tts/pdf/log-level events from bridge subscriptions", async () => {
    const { backend, hooks } = createBackend();
    const store = createTestStore(backend);
    await store.getState().bootstrap();

    hooks.tts?.({
      request_id: 21,
      action: "reader_tts_play",
      tts: makeReaderSnapshot("/tmp/tts.epub", "TTS").tts
    });
    hooks.pdf?.({
      request_id: 22,
      phase: "started",
      source_path: "/tmp/book.pdf",
      message: null
    });
    hooks.logLevel?.({
      request_id: 23,
      level: "warn"
    });

    const state = store.getState();
    expect(state.ttsStateEvent?.request_id).toBe(21);
    expect(state.pdfTranscriptionEvent?.source_path).toBe("/tmp/book.pdf");
    expect(state.runtimeLogLevel).toBe("warn");
  });

  it("preserves event object identity when repeated bridge events carry the same visible payload", async () => {
    const { backend, hooks } = createBackend();
    const store = createTestStore(backend);
    await store.getState().bootstrap();

    hooks.tts?.({
      request_id: 21,
      action: "reader_tts_play",
      tts: makeReaderSnapshot("/tmp/tts.epub", "TTS").tts
    });
    const ttsBefore = store.getState().ttsStateEvent;
    hooks.tts?.({
      request_id: 22,
      action: "reader_tts_play",
      tts: makeReaderSnapshot("/tmp/tts.epub", "TTS").tts
    });

    hooks.logLevel?.({
      request_id: 23,
      level: "warn"
    });
    const logBefore = store.getState().logLevelEvent;
    hooks.logLevel?.({
      request_id: 24,
      level: "warn"
    });

    expect(store.getState().ttsStateEvent).toBe(ttsBefore);
    expect(store.getState().logLevelEvent).toBe(logBefore);
    expect(store.getState().lastTtsEventRequestId).toBe(22);
    expect(store.getState().lastLogLevelEventRequestId).toBe(24);
  });

  it("updates runtime log level via backend command", async () => {
    const { backend } = createBackend({
      loggingSetLevel: async (level) => level.toLowerCase()
    });
    const store = createTestStore(backend);

    await store.getState().setRuntimeLogLevel("ERROR");

    const state = store.getState();
    expect(state.runtimeLogLevel).toBe("error");
    expect(state.toast?.message).toContain("error");
  });

  it("tracks scoped source-open and reader-tts operation flags independently", async () => {
    const openGate = deferred<OpenSourceResult>();
    const ttsGate = deferred<ReaderSnapshot>();
    const { backend } = createBackend({
      sourceOpenPath: async () => openGate.promise,
      readerTtsPlay: async () => ttsGate.promise
    });
    const store = createTestStore(backend);

    const openPromise = store.getState().openSourcePath("/tmp/book.epub");
    expect(store.getState().operations.sourceOpen).toBe(true);
    expect(store.getState().operations.readerTts).toBe(false);

    const ttsPromise = store.getState().readerTtsPlay();
    expect(store.getState().operations.sourceOpen).toBe(true);
    expect(store.getState().operations.readerTts).toBe(true);
    expect(store.getState().busy).toBe(true);

    openGate.resolve({
      session: makeSessionState("reader"),
      reader: makeReaderSnapshot("/tmp/book.epub", "Opened")
    });
    await openPromise;

    expect(store.getState().operations.sourceOpen).toBe(false);
    expect(store.getState().operations.readerTts).toBe(true);
    expect(store.getState().busy).toBe(true);

    ttsGate.resolve(makeReaderSnapshot("/tmp/book.epub", "Playing"));
    await ttsPromise;

    expect(store.getState().operations.sourceOpen).toBe(false);
    expect(store.getState().operations.readerTts).toBe(false);
    expect(store.getState().busy).toBe(false);
  });

  it("ignores stale source/calibre/pdf/tts/log events by request id", async () => {
    const { backend, hooks } = createBackend();
    const store = createTestStore(backend);
    await store.getState().bootstrap();

    hooks.source?.({
      request_id: 50,
      phase: "started",
      source_path: "/tmp/new.epub",
      message: null
    });
    hooks.source?.({
      request_id: 49,
      phase: "failed",
      source_path: "/tmp/old.epub",
      message: "stale"
    });

    hooks.calibre?.({
      request_id: 40,
      phase: "finished",
      count: 100,
      message: null
    });
    hooks.calibre?.({
      request_id: 39,
      phase: "failed",
      count: null,
      message: "stale"
    });

    hooks.tts?.({
      request_id: 30,
      action: "reader_tts_play",
      tts: makeReaderSnapshot("/tmp/new.epub", "New").tts
    });
    hooks.tts?.({
      request_id: 29,
      action: "reader_tts_pause",
      tts: makeReaderSnapshot("/tmp/old.epub", "Old").tts
    });

    hooks.pdf?.({
      request_id: 20,
      phase: "started",
      source_path: "/tmp/new.pdf",
      message: null
    });
    hooks.pdf?.({
      request_id: 19,
      phase: "failed",
      source_path: "/tmp/old.pdf",
      message: "stale"
    });

    hooks.logLevel?.({
      request_id: 10,
      level: "warn"
    });
    hooks.logLevel?.({
      request_id: 9,
      level: "debug"
    });

    const state = store.getState();
    expect(state.sourceOpenEvent?.request_id).toBe(50);
    expect(state.lastSourceOpenEventRequestId).toBe(50);
    expect(state.calibreLoadEvent?.request_id).toBe(40);
    expect(state.lastCalibreEventRequestId).toBe(40);
    expect(state.ttsStateEvent?.request_id).toBe(30);
    expect(state.lastTtsEventRequestId).toBe(30);
    expect(state.pdfTranscriptionEvent?.request_id).toBe(20);
    expect(state.lastPdfEventRequestId).toBe(20);
    expect(state.runtimeLogLevel).toBe("warn");
    expect(state.lastLogLevelEventRequestId).toBe(10);
  });
});
