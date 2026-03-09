// @vitest-environment jsdom
import fs from "node:fs";
import path from "node:path";
import { StrictMode, act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";

import App from "../src/App";
import { useAppStore } from "../src/store/appStore";
import type { BootstrapState, ReaderSnapshot, SessionState } from "../src/types";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean })
  .IS_REACT_ACT_ENVIRONMENT = true;
(globalThis as typeof globalThis & { ResizeObserver?: typeof ResizeObserver }).ResizeObserver =
  class {
    observe() {}
    disconnect() {}
    unobserve() {}
  } as typeof ResizeObserver;

vi.mock("../src/api/tauri", async () => {
  const actual = await vi.importActual("../src/api/tauri");
  return actual;
});

function makeBootstrap(): BootstrapState {
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

function makeReader(html: string, text: string): ReaderSnapshot {
  const words = text.split(/\s+/).filter(Boolean);
  const sentences = text.split(/(?<=[.!?])\s+/).filter(Boolean).slice(0, 200);
  return {
    source_path: "/tmp/browser-tab.lltab",
    source_name: "Browser Tab",
    current_page: 0,
    total_pages: 1,
    text_only_mode: false,
    has_structured_markdown: true,
    pretty_kind: "html",
    pdf_geometry_mode: null,
    pdf_sync_strategy: null,
    images: [],
    tts_text_page: text,
    reading_markdown_page: null,
    reading_html_page: html,
    page_text: text,
    sentences,
    canonical_sentences: sentences,
    sentence_anchor_map: sentences.map((_, idx) => idx),
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
      sentence_count: Math.max(1, sentences.length),
      can_seek_prev: false,
      can_seek_next: true,
      progress_pct: 0
    },
    stats: {
      page_index: 1,
      total_pages: 1,
      tts_progress_pct: 0,
      global_progress_pct: 0,
      page_time_remaining_secs: 0,
      book_time_remaining_secs: 0,
      page_word_count: words.length,
      page_sentence_count: sentences.length,
      page_start_percent: 0,
      page_end_percent: 100,
      words_read_up_to_page_start: 0,
      sentences_read_up_to_page_start: 0,
      words_read_up_to_page_end: words.length,
      sentences_read_up_to_page_end: sentences.length,
      words_read_up_to_current_position: 0,
      sentences_read_up_to_current_position: 0
    },
    panels: {
      show_settings: true,
      show_stats: false,
      show_tts: true
    }
  };
}

describe("App browser-tab transition", () => {
  it("transitions from starter to browser-tab reader without external-store loop", async () => {
    const dirs = fs.readdirSync(path.resolve(".cache/lantern-leaf/browser-tabs")).sort();
    const dir = path.resolve(".cache/lantern-leaf/browser-tabs", dirs[dirs.length - 1]);
    const html = fs.readFileSync(path.join(dir, "snapshot.html"), "utf8");
    const text = fs.readFileSync(path.join(dir, "snapshot.txt"), "utf8");
    const reader = makeReader(html, text);
    const starterSession: SessionState = {
      mode: "starter",
      active_source_path: null,
      open_in_flight: false,
      panels: {
        show_settings: true,
        show_stats: false,
        show_tts: true
      }
    };

    useAppStore.setState({
      bootstrapState: makeBootstrap(),
      session: starterSession,
      reader: null,
      loadingBootstrap: false,
      busy: false,
      error: null,
      toast: null
    });

    const container = document.createElement("div");
    document.body.appendChild(container);
    const root = createRoot(container);

    await act(async () => {
      root.render(
        <StrictMode>
          <App />
        </StrictMode>
      );
    });

    await act(async () => {
      useAppStore.setState({
        session: {
          mode: "reader",
          active_source_path: reader.source_path,
          open_in_flight: false,
          panels: reader.panels
        },
        reader
      });
    });

    expect(container.innerHTML).toContain("reader-tts-player-widget");

    await act(async () => {
      root.unmount();
    });
    container.remove();
  });
});
