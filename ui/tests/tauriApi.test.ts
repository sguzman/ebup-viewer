import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn()
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => async () => {})
}));

type TauriModule = typeof import("../src/api/tauri");

function setTauriRuntimeWindow(): void {
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    writable: true,
    value: { __TAURI_INTERNALS__: {} }
  });
}

async function loadTauriApiModule(): Promise<TauriModule> {
  vi.resetModules();
  setTauriRuntimeWindow();
  return import("../src/api/tauri");
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  Reflect.deleteProperty(globalThis, "window");
});

describe("tauri command adapter", () => {
  it("normalizes recent-list limits before invoking the backend", async () => {
    const invokeMock = vi.mocked(invoke);
    invokeMock.mockResolvedValue([]);
    const api = await loadTauriApiModule();

    await api.backendApi.recentList(9999);
    await api.backendApi.recentList(0);
    await api.backendApi.recentList();

    expect(invokeMock).toHaveBeenNthCalledWith(1, "recent_list", { limit: 512 });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "recent_list", { limit: 1 });
    expect(invokeMock).toHaveBeenNthCalledWith(3, "recent_list", { limit: 64 });
  });

  it("propagates structured bridge errors unchanged", async () => {
    const invokeMock = vi.mocked(invoke);
    invokeMock.mockRejectedValue({
      code: "invalid_input",
      message: "Path cannot be empty"
    });
    const api = await loadTauriApiModule();

    await expect(api.backendApi.sourceOpenPath("")).rejects.toEqual({
      code: "invalid_input",
      message: "[invoke:source_open_path] Path cannot be empty"
    });
  });

  it("maps unknown errors to unknown_error bridge payload", async () => {
    const invokeMock = vi.mocked(invoke);
    invokeMock.mockRejectedValue(new Error("boom"));
    const api = await loadTauriApiModule();

    await expect(api.backendApi.readerNextPage()).rejects.toEqual({
      code: "unknown_error",
      message: "[invoke:reader_next_page] boom"
    });
  });

  it("subscribes session-state listener on the expected event channel", async () => {
    const api = await loadTauriApiModule();
    const listenMock = vi.mocked(listen);
    const handler = vi.fn();

    await api.backendApi.onSessionState(handler);

    expect(listenMock).toHaveBeenCalledTimes(1);
    const [eventName, callback] = listenMock.mock.calls[0] as [string, (event: unknown) => void];
    expect(eventName).toBe("session-state");

    callback({
      payload: {
        request_id: 8,
        action: "session_return_to_starter",
        session: {
          mode: "starter",
          active_source_path: null,
          open_in_flight: false,
          panels: {
            show_settings: true,
            show_stats: false,
            show_tts: true
          }
        }
      }
    });

    expect(handler).toHaveBeenCalledWith({
      request_id: 8,
      action: "session_return_to_starter",
      session: {
        mode: "starter",
        active_source_path: null,
        open_in_flight: false,
        panels: {
          show_settings: true,
          show_stats: false,
          show_tts: true
        }
      }
    });
  });

  it("subscribes all bridge progress/state channels with stable names", async () => {
    const api = await loadTauriApiModule();
    const listenMock = vi.mocked(listen);
    const noop = vi.fn();

    await api.backendApi.onSourceOpen(noop);
    await api.backendApi.onCalibreLoad(noop);
    await api.backendApi.onSessionState(noop);
    await api.backendApi.onReaderState(noop);
    await api.backendApi.onTtsState(noop);
    await api.backendApi.onPdfTranscription(noop);
    await api.backendApi.onLogLevel(noop);

    expect(listenMock.mock.calls.map((call) => call[0])).toEqual([
      "source-open",
      "calibre-load",
      "session-state",
      "reader-state",
      "tts-state",
      "pdf-transcription",
      "log-level"
    ]);
  });

  it("invokes critical bridge commands with expected names and argument keys", async () => {
    const invokeMock = vi.mocked(invoke);
    invokeMock.mockResolvedValue({});
    const api = await loadTauriApiModule();

    await api.backendApi.sourceOpenPath("/tmp/book.epub");
    await api.backendApi.sourceOpenBrowserTab(42, 7);
    await api.backendApi.sourceOpenBrowserTabBundle(42, 7);
    await api.backendApi.sourceRefreshBrowserTab("/tmp/browser-tab.lltab");
    await api.backendApi.sourceOpenClipboardText("hello");
    await api.backendApi.readerSentenceClick(7);
    await api.backendApi.readerApplySettings({ pause_after_sentence: 0.06, tts_speed: 2.5 });
    await api.backendApi.readerSetPage(12);
    await api.backendApi.readerSearchSetQuery("needle");
    await api.backendApi.calibreLoadBooks(true);
    await api.backendApi.calibreOpenBook(42);
    await api.backendApi.panelToggleSettings();
    await api.backendApi.panelToggleStats();
    await api.backendApi.panelToggleTts();
    await api.backendApi.loggingSetLevel("warn");

    expect(invokeMock.mock.calls).toEqual([
      ["source_open_path", { path: "/tmp/book.epub" }],
      ["source_open_browser_tab", { tabId: 42, windowId: 7 }],
      ["source_open_browser_tab_bundle", { tabId: 42, windowId: 7 }],
      ["source_refresh_browser_tab", { path: "/tmp/browser-tab.lltab" }],
      ["source_open_clipboard_text", { text: "hello" }],
      ["reader_sentence_click", { sentenceIdx: 7 }],
      [
        "reader_apply_settings",
        {
          patch: {
            pause_after_sentence: 0.06,
            tts_speed: 2.5
          }
        }
      ],
      ["reader_set_page", { page: 12 }],
      ["reader_search_set_query", { query: "needle" }],
      ["calibre_load_books", { forceRefresh: true }],
      ["calibre_open_book", { bookId: 42 }],
      ["panel_toggle_settings", undefined],
      ["panel_toggle_stats", undefined],
      ["panel_toggle_tts", undefined],
      ["logging_set_level", { level: "warn" }]
    ]);
  });
});
