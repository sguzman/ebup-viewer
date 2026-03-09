import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ReaderShell } from "../src/components/ReaderShell";
import type { ReaderSnapshot } from "../src/types";

function makeReaderSnapshot(overrides: Partial<ReaderSnapshot> = {}): ReaderSnapshot {
  return {
    source_path: "/tmp/book.html",
    source_name: "book.html",
    current_page: 0,
    total_pages: 3,
    text_only_mode: true,
    has_structured_markdown: false,
    pretty_kind: "none",
    pdf_geometry_mode: null,
    pdf_sync_strategy: null,
    images: [],
    tts_text_page: "Alpha. Beta.",
    reading_markdown_page: null,
    reading_html_page: null,
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
    },
    ...overrides
  };
}

function renderReader(reader: ReaderSnapshot): string {
  return renderToStaticMarkup(
    <ReaderShell
      reader={reader}
      busy={false}
      onCloseSession={async () => {}}
      onPrevPage={async () => {}}
      onNextPage={async () => {}}
      onPrevSentence={async () => {}}
      onNextSentence={async () => {}}
      onSetPage={async () => {}}
      onSentenceClick={async () => {}}
      onToggleTextOnly={async () => {}}
      onSearchQuery={async () => {}}
      onSearchNext={async () => {}}
      onSearchPrev={async () => {}}
      onToggleTheme={async () => {}}
      onToggleSettingsPanel={async () => {}}
      onToggleStatsPanel={async () => {}}
      onToggleTtsPanel={async () => {}}
      onTtsPlay={async () => {}}
      onTtsPause={async () => {}}
      onTtsTogglePlayPause={async () => {}}
      onTtsPlayFromPageStart={async () => {}}
      onTtsPlayFromHighlight={async () => {}}
      onTtsSeekNext={async () => {}}
      onTtsSeekPrev={async () => {}}
      onTtsRepeatSentence={async () => {}}
      onTtsPrecomputePage={async () => {}}
      onApplySettings={async () => {}}
      ttsStateEvent={null}
    />
  );
}

describe("ReaderShell TTS player integration", () => {
  it("renders the player widget in text-only mode", () => {
    const html = renderReader(makeReaderSnapshot({ text_only_mode: true }));
    expect(html).toContain('data-testid="reader-tts-player-widget"');
    expect(html).not.toContain('data-testid="reader-tts-control-row"');
  });

  it("renders the player widget in pretty-text mode", () => {
    const html = renderReader(
      makeReaderSnapshot({
        text_only_mode: false,
        pretty_kind: "markdown",
        reading_markdown_page: "# Heading\n\nAlpha.\n\nBeta."
      })
    );
    expect(html).toContain('data-testid="reader-tts-player-widget"');
    expect(html).toContain('data-testid="reader-pretty-markdown"');
  });

  it("preserves playback labels across text-only and native-pdf pretty-text views", () => {
    const pdfReader = makeReaderSnapshot({
      source_path: "/tmp/book.pdf",
      source_name: "book.pdf",
      text_only_mode: false,
      pretty_kind: "pdf",
      pdf_geometry_mode: "high_text_trust",
      pdf_sync_strategy: "sentence_spans",
      highlighted_sentence_idx: 1,
      tts: {
        state: "playing",
        current_sentence_idx: 1,
        sentence_count: 2,
        can_seek_prev: true,
        can_seek_next: false,
        progress_pct: 50
      }
    });
    const textOnlyReader = {
      ...pdfReader,
      text_only_mode: true,
      pretty_kind: "none" as const
    };

    const prettyHtml = renderReader(pdfReader);
    const textOnlyHtml = renderReader(textOnlyReader);

    expect(prettyHtml).toContain('data-testid="reader-pretty-pdf"');
    expect(prettyHtml).toContain("Sentence 2/2");
    expect(prettyHtml).toContain("Progress 50.000% | playing");
    expect(textOnlyHtml).toContain("Sentence 2/2");
    expect(textOnlyHtml).toContain("Progress 50.000% | playing");
  });

  it("hides the player widget when TTS controls are disabled", () => {
    const html = renderReader(
      makeReaderSnapshot({
        panels: {
          show_settings: false,
          show_stats: true,
          show_tts: false
        }
      })
    );
    expect(html).not.toContain('data-testid="reader-tts-player-widget"');
  });

  it("disables PDF TTS controls when OCR text is unavailable", () => {
    const html = renderReader(
      makeReaderSnapshot({
        source_path: "/tmp/scan.pdf",
        source_name: "scan.pdf",
        text_only_mode: false,
        pretty_kind: "pdf",
        pdf_geometry_mode: "ocr_required",
        pdf_sync_strategy: "render_only",
        page_text: "",
        tts_text_page: "",
        sentences: [],
        panels: {
          show_settings: false,
          show_stats: false,
          show_tts: true
        }
      })
    );

    expect(html).toContain('data-testid="reader-pretty-pdf"');
    expect(html).toContain('data-testid="reader-pretty-pdf-degraded"');
    expect(html).toContain('data-testid="reader-tts-player-disabled-reason"');
    expect(html).toContain("This PDF is renderable now, but precise highlight sync and text playback will stay gated until OCR produces usable text.");
    expect(html).toContain("TTS is disabled for this PDF until OCR produces usable text.");
    expect(html).not.toContain('data-testid="reader-sentence-0"');
  });

  it("shows an explicit text-only unavailable notice for scanned PDFs with no extracted text", () => {
    const html = renderReader(
      makeReaderSnapshot({
        source_path: "/tmp/scan.pdf",
        source_name: "scan.pdf",
        text_only_mode: true,
        pretty_kind: "pdf",
        pdf_geometry_mode: "ocr_required",
        pdf_sync_strategy: "render_only",
        page_text: "",
        tts_text_page: "",
        sentences: [],
        panels: {
          show_settings: false,
          show_stats: false,
          show_tts: true
        }
      })
    );

    expect(html).toContain('data-testid="reader-pdf-text-unavailable"');
    expect(html).toContain("Text-only content is unavailable for this PDF until OCR produces usable text.");
    expect(html).toContain('data-testid="reader-tts-player-disabled-reason"');
  });
});
