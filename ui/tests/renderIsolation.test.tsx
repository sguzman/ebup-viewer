// @vitest-environment jsdom

import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";
import { useShallow } from "zustand/react/shallow";

import { useAppStore } from "../src/store/appStore";
import {
  useReaderDocumentKey,
  useReaderQuickActionsBusy,
  useReaderQuickActionsFlags,
  useStarterViewState
} from "../src/store/selectors";
import type { AppStore } from "../src/store/appStore";
import type { BootstrapState, ReaderSnapshot } from "../src/types";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT =
  true;

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

function makeReaderSnapshot(): ReaderSnapshot {
  return {
    source_path: "/tmp/book.lltab",
    source_name: "book.lltab",
    current_page: 0,
    total_pages: 3,
    text_only_mode: false,
    has_structured_markdown: false,
    pretty_kind: "html",
    images: [],
    tts_text_page: "Alpha. Beta.",
    reading_markdown_page: null,
    reading_html_page: "<p data-ll-html-anchor=\"0\">Alpha.</p><p data-ll-html-anchor=\"1\">Beta.</p>",
    page_text: "Alpha. Beta.",
    sentences: ["Alpha.", "Beta."],
    sentence_anchor_map: [0, 1],
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
      time_remaining_display: "adaptive",
      tts_speed: 2.5,
      tts_volume: 1
    },
    tts: {
      state: "idle",
      current_sentence_idx: 0,
      sentence_count: 2,
      can_seek_prev: false,
      can_seek_next: true,
      progress_pct: 50
    },
    stats: {
      page_index: 1,
      total_pages: 3,
      tts_progress_pct: 50,
      global_progress_pct: 33.333,
      page_time_remaining_secs: 12,
      book_time_remaining_secs: 120,
      page_word_count: 2,
      page_sentence_count: 2,
      page_start_percent: 0,
      page_end_percent: 33.333,
      words_read_up_to_page_start: 0,
      sentences_read_up_to_page_start: 0,
      words_read_up_to_page_end: 2,
      sentences_read_up_to_page_end: 2,
      words_read_up_to_current_position: 1,
      sentences_read_up_to_current_position: 1
    },
    panels: {
      show_settings: false,
      show_stats: false,
      show_tts: true
    }
  };
}

function setBaseState() {
  const reader = makeReaderSnapshot();
  useAppStore.setState((state) => ({
    ...state,
    bootstrapState: makeBootstrapState(),
    recents: [],
    calibreBooks: [],
    reader,
    ttsStateEvent: null,
    operations: {
      ...state.operations,
      readerCommand: false,
      readerSettings: false,
      browserTabRefresh: false
    }
  }));
  return reader;
}

const originalState = useAppStore.getState();

afterEach(() => {
  useAppStore.setState(originalState as AppStore, true);
  document.body.innerHTML = "";
});

function mountProbe(element: React.ReactElement) {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const root = createRoot(container);
  act(() => {
    root.render(element);
  });
  return {
    unmount: () =>
      act(() => {
        root.unmount();
      })
  };
}

describe("render isolation", () => {
  it("playback ticks do not rerender starter subscriptions", () => {
    const reader = setBaseState();
    let renders = 0;

    function StarterProbe() {
      renders += 1;
      const starter = useStarterViewState();
      return <div data-busy={starter.busy ? "1" : "0"} />;
    }

    const mounted = mountProbe(<StarterProbe />);
    expect(renders).toBe(1);

    act(() => {
      useAppStore.setState({
        reader: {
          ...reader,
          highlighted_sentence_idx: 1,
          tts: {
            ...reader.tts,
            current_sentence_idx: 1,
            progress_pct: 100
          },
          stats: {
            ...reader.stats,
            tts_progress_pct: 100,
            words_read_up_to_current_position: 2,
            sentences_read_up_to_current_position: 2
          }
        }
      });
    });

    expect(renders).toBe(1);
    mounted.unmount();
  });

  it("quick actions stay cold on pure playback and tts metadata churn", () => {
    const reader = setBaseState();
    let renders = 0;

    function QuickActionsProbe() {
      renders += 1;
      const busy = useReaderQuickActionsBusy();
      const flags = useReaderQuickActionsFlags();
      return <div data-busy={busy ? "1" : "0"} data-text-only={flags.isTextOnly ? "1" : "0"} />;
    }

    const mounted = mountProbe(<QuickActionsProbe />);
    expect(renders).toBe(1);

    act(() => {
      useAppStore.setState({
        reader: {
          ...reader,
          highlighted_sentence_idx: 1,
          tts: {
            ...reader.tts,
            current_sentence_idx: 1,
            progress_pct: 100
          }
        },
        ttsStateEvent: {
          request_id: 11,
          action: "playback_progress",
          state: "playing",
          sentence_index: 1,
          progress_pct: 100,
          message: null
        }
      });
    });

    expect(renders).toBe(1);
    mounted.unmount();
  });

  it("panel toggles do not invalidate the reader document key", () => {
    const reader = setBaseState();
    let renders = 0;

    function ReaderDocumentProbe() {
      renders += 1;
      const documentKey = useReaderDocumentKey();
      return <div data-document-key={documentKey} />;
    }

    const mounted = mountProbe(<ReaderDocumentProbe />);
    expect(renders).toBe(1);

    act(() => {
      useAppStore.setState({
        reader: {
          ...reader,
          panels: {
            ...reader.panels,
            show_stats: true
          }
        }
      });
    });

    expect(renders).toBe(1);
    mounted.unmount();
  });

  it("panel toggles do not rerender document-only reader subscriptions", () => {
    const reader = setBaseState();
    let renders = 0;

    function ReaderDocumentStateProbe() {
      renders += 1;
      const documentTuple = useAppStore(
        useShallow((state) => [
          state.reader?.source_path ?? "",
          state.reader?.current_page ?? -1,
          state.reader?.pretty_kind ?? "none"
        ] as const)
      );
      return <div data-document-tuple={documentTuple.join("|")} />;
    }

    const mounted = mountProbe(<ReaderDocumentStateProbe />);
    expect(renders).toBe(1);

    act(() => {
      useAppStore.setState({
        reader: {
          ...reader,
          panels: {
            show_settings: true,
            show_stats: true,
            show_tts: false
          }
        }
      });
    });

    expect(renders).toBe(1);
    mounted.unmount();
  });
});
