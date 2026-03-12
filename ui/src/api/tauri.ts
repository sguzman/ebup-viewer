import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  BootstrapState,
  BridgeError,
  CalibreBook,
  CalibreLoadEvent,
  LogLevelEvent,
  OpenSourceResult,
  PdfTranscriptionEvent,
  ReaderPlaybackStateEvent,
  ReaderStateEvent,
  ReaderSettingsPatch,
  ReaderSnapshot,
  RecentBook,
  SessionStateEvent,
  SessionState,
  SourceOpenEvent,
  TtsStateEvent
} from "../types";

export interface BrowsrHealth {
  ok: boolean;
  extension_connected: boolean;
  now: string | null;
}

export interface BrowserWindowInfo {
  id: number;
  focused: boolean;
  height: number | null;
  incognito: boolean | null;
  left: number | null;
  state: string | null;
  top: number | null;
  type: string | null;
  width: number | null;
}

export interface BrowserTabInfo {
  id: number;
  windowId: number;
  index: number | null;
  active: boolean | null;
  audible: boolean | null;
  pinned: boolean | null;
  status: string | null;
  title: string;
  url: string;
  favIconUrl: string | null;
  lastAccessed: number | null;
}

export interface PdfSentenceRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface PdfSentenceLocation {
  sentence_idx: number;
  page_idx: number | null;
  rects: PdfSentenceRect[];
  line_rects: PdfSentenceRect[];
  block_rects: PdfSentenceRect[];
  confidence: string;
  reason: string;
  score: number;
}

export interface PdfRenderPrecomputedState {
  version: number;
  page_texts: string[];
  sentence_page_hints: Array<number | null>;
  source: string;
}

const MAX_RECENT_LIMIT = 512;
const DEFAULT_RECENT_LIMIT = 64;

const isTauriRuntime = (): boolean => {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
};

function normalizeRecentLimit(limit?: number): number {
  const candidate = Number.isFinite(limit) ? Number(limit) : DEFAULT_RECENT_LIMIT;
  return Math.min(MAX_RECENT_LIMIT, Math.max(1, Math.floor(candidate)));
}

function bridgeErrorFromUnknown(error: unknown): BridgeError {
  if (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    "message" in error &&
    typeof (error as { code: unknown }).code === "string" &&
    typeof (error as { message: unknown }).message === "string"
  ) {
    const structured = error as BridgeError;
    return {
      code: structured.code,
      message: structured.message
    };
  }

  if (error instanceof Error) {
    return {
      code: "unknown_error",
      message: error.message
    };
  }

  return {
    code: "unknown_error",
    message: String(error)
  };
}

async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    const normalized = bridgeErrorFromUnknown(error);
    throw {
      code: normalized.code,
      message: `[invoke:${command}] ${normalized.message}`
    } satisfies BridgeError;
  }
}

type MockBackendState = {
  bootstrap: BootstrapState;
  session: SessionState;
  recents: RecentBook[];
  calibreBooks: CalibreBook[];
  reader: ReaderSnapshot | null;
  pages: Array<{ text: string; sentences: string[]; html?: string | null; markdown?: string | null }>;
  logLevel: string;
};

const MOCK_PAGES: Array<{ text: string; sentences: string[]; html?: string | null; markdown?: string | null }> = [
  {
    text: "This is the mock reader content on page one.",
    sentences: ["This is the mock reader content on page one."],
    html: null,
    markdown: null
  },
  {
    text: "This is the mock reader content on page two.",
    sentences: ["This is the mock reader content on page two."],
    html: null,
    markdown: null
  },
  {
    text: "This is the mock reader content on page three.",
    sentences: ["This is the mock reader content on page three."],
    html: null,
    markdown: null
  }
];

type MockScenario = {
  sourcePath: string;
  sourceName: string;
  prettyKind: ReaderSnapshot["pretty_kind"];
  pages: MockBackendState["pages"];
  images: ReaderSnapshot["images"];
};

function buildHtmlParagraphs(label: string, count: number, includeImages = false): string {
  const parts: string[] = [];
  for (let index = 0; index < count; index += 1) {
    parts.push(
      `<p>${label} paragraph ${index + 1}. ${label} sentence ${index + 1} continues with profiling content.</p>`
    );
    if (includeImages && index % 4 === 0) {
      parts.push(
        `<figure><img src="mock-image-${index + 1}.jpg" alt="${label} image ${index + 1}" /><figcaption>${label} image ${index + 1}</figcaption></figure>`
      );
    }
  }
  return `<article>${parts.join("")}</article>`;
}

function buildSentences(label: string, count: number): string[] {
  return Array.from({ length: count }, (_, index) => `${label} sentence ${index + 1}.`);
}

function buildTextFromSentences(sentences: string[]): string {
  return sentences.join(" ");
}

function mockScenarioForPath(path: string): MockScenario {
  if (path.endsWith("mock-large.epub")) {
    const pages = Array.from({ length: 12 }, (_, index) => {
      const sentences = buildSentences(`Large EPUB page ${index + 1}`, 18);
      return {
        text: buildTextFromSentences(sentences),
        sentences,
        html: buildHtmlParagraphs(`Large EPUB page ${index + 1}`, 18, index % 3 === 0),
        markdown: null
      };
    });
    return {
      sourcePath: path,
      sourceName: "mock-large.epub",
      prettyKind: "html",
      pages,
      images: Array.from({ length: 8 }, (_, index) => ({
        raw_path: `large-epub-image-${index + 1}.jpg`,
        local_path: `.cache/mock-images/large-epub-image-${index + 1}.jpg`
      }))
    };
  }

  if (path.endsWith("browser-tab.lltab")) {
    const pages = Array.from({ length: 6 }, (_, index) => {
      const sentences = buildSentences(`Imported browser tab page ${index + 1}`, 22);
      return {
        text: buildTextFromSentences(sentences),
        sentences,
        html: buildHtmlParagraphs(`Imported browser tab page ${index + 1}`, 22, true),
        markdown: null
      };
    });
    return {
      sourcePath: path,
      sourceName: "browser-tab.lltab",
      prettyKind: "html",
      pages,
      images: Array.from({ length: 10 }, (_, index) => ({
        raw_path: `browser-tab-image-${index + 1}.png`,
        local_path: `.cache/mock-images/browser-tab-image-${index + 1}.png`
      }))
    };
  }

  if (path.endsWith("mock-image-heavy.html")) {
    const pages = Array.from({ length: 8 }, (_, index) => {
      const sentences = buildSentences(`Image-heavy HTML page ${index + 1}`, 16);
      return {
        text: buildTextFromSentences(sentences),
        sentences,
        html: buildHtmlParagraphs(`Image-heavy HTML page ${index + 1}`, 16, true),
        markdown: null
      };
    });
    return {
      sourcePath: path,
      sourceName: "mock-image-heavy.html",
      prettyKind: "html",
      pages,
      images: Array.from({ length: 18 }, (_, index) => ({
        raw_path: `image-heavy-${index + 1}.webp`,
        local_path: `.cache/mock-images/image-heavy-${index + 1}.webp`
      }))
    };
  }

  return {
    sourcePath: path,
    sourceName: path.split(/[\\/]/).pop() ?? path,
    prettyKind: "none",
    pages: MOCK_PAGES,
    images: []
  };
}

const MOCK_BROWSER_WINDOWS: BrowserWindowInfo[] = [
  {
    id: 1001,
    focused: true,
    height: 1080,
    incognito: false,
    left: 0,
    state: "maximized",
    top: 0,
    type: "normal",
    width: 1920
  }
];

const MOCK_BROWSER_TABS: BrowserTabInfo[] = [
  {
    id: 2001,
    windowId: 1001,
    index: 0,
    active: true,
    audible: false,
    pinned: false,
    status: "complete",
    title: "Mock Article",
    url: "https://example.com/mock-article",
    favIconUrl: "https://example.com/favicon.ico",
    lastAccessed: Date.now()
  }
];

const mockReaderSnapshot = (): ReaderSnapshot => ({
  source_path: ".cache/clipboard/mock.txt",
  source_name: "mock.txt",
  current_page: 0,
  total_pages: MOCK_PAGES.length,
  text_only_mode: false,
  has_structured_markdown: false,
  pretty_kind: "none",
  pdf_geometry_mode: null,
  pdf_sync_strategy: null,
  pdf_classification: null,
  pdf_runtime_policy: null,
  pdf_ocr_alignment: null,
  pdf_ocr_pipeline: null,
  images: [],
  tts_text_page: MOCK_PAGES[0].text,
  reading_markdown_page: null,
  reading_html_page: null,
  page_text: MOCK_PAGES[0].text,
  sentences: MOCK_PAGES[0].sentences,
  canonical_sentences: MOCK_PAGES.flatMap((page) => page.sentences),
  page_sentence_counts: MOCK_PAGES.map((page) => page.sentences.length),
  sentence_anchor_map: [0],
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
    text_only_show_original_text: false,
    time_remaining_display: "adaptive",
    tts_speed: 2.5,
    tts_volume: 1.0
  },
  stats: {
    page_index: 1,
    total_pages: MOCK_PAGES.length,
    tts_progress_pct: 100,
    global_progress_pct: 33.333,
    page_time_remaining_secs: 0,
    book_time_remaining_secs: 0,
    page_word_count: MOCK_PAGES[0].text.split(/\s+/).length,
    page_sentence_count: 1,
    page_start_percent: 0,
    page_end_percent: 33.333,
    words_read_up_to_page_start: 0,
    sentences_read_up_to_page_start: 0,
    words_read_up_to_page_end: MOCK_PAGES[0].text.split(/\s+/).length,
    sentences_read_up_to_page_end: 1,
    words_read_up_to_current_position: MOCK_PAGES[0].text.split(/\s+/).length,
    sentences_read_up_to_current_position: 1
  },
  tts: {
    state: "idle",
    current_sentence_idx: 0,
    sentence_count: 1,
    can_seek_prev: false,
    can_seek_next: false,
    progress_pct: 100
  },
  panels: {
    show_settings: true,
    show_stats: false,
    show_tts: true
  }
});

const mockState: MockBackendState = {
  bootstrap: {
    app_name: "LanternLeaf",
    mode: "mock",
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
  },
  session: {
    mode: "starter",
    active_source_path: null,
    open_in_flight: false,
    panels: {
      show_settings: true,
      show_stats: false,
      show_tts: true
    }
  },
  recents: [],
  calibreBooks: [],
  reader: null,
  pages: MOCK_PAGES,
  logLevel: "debug"
};

function ensureMockReader(): ReaderSnapshot {
  if (!mockState.reader) {
    mockState.reader = mockReaderSnapshot();
  }
  return mockState.reader;
}

function applyMockPage(reader: ReaderSnapshot, page: number): void {
  const pages = mockState.pages;
  const clampedPage = Math.max(0, Math.min(pages.length - 1, Math.floor(page)));
  const pageData = pages[clampedPage];
  const totalWords = pages.reduce((sum, item) => sum + item.text.split(/\s+/).length, 0);
  const wordsBeforePage = pages.slice(0, clampedPage).reduce(
    (sum, item) => sum + item.text.split(/\s+/).length,
    0
  );
  const wordsOnPage = pageData.text.split(/\s+/).length;

  reader.current_page = clampedPage;
  reader.total_pages = pages.length;
  reader.tts_text_page = pageData.text;
  reader.reading_markdown_page = pageData.markdown ?? null;
  reader.reading_html_page = pageData.html ?? null;
  reader.pdf_geometry_mode = reader.source_path.toLowerCase().endsWith(".pdf") ? "high_text_trust" : null;
  reader.pdf_sync_strategy = reader.source_path.toLowerCase().endsWith(".pdf") ? "sentence_spans" : null;
  reader.pdf_classification = reader.source_path.toLowerCase().endsWith(".pdf")
    ? {
        document_class: "embedded_clean",
        confidence: 0.92,
        ocr_recommendation: "not_needed",
        reasons: ["mock_pdf_classifier"],
        feature_summary: {
          sampled_pages: 3,
          text_page_ratio: 1,
          empty_text_page_ratio: 0,
          sparse_text_page_ratio: 0,
          noisy_text_page_ratio: 0,
          repeated_header_ratio: 0,
          repeated_footer_ratio: 0,
          image_page_ratio: 0,
          mixed_text_image_page_ratio: 0,
          full_page_raster_page_ratio: 0,
          hidden_text_layer_page_ratio: 0,
          invisible_text_layer_page_ratio: 0,
          duplicate_text_page_ratio: 0,
          stacked_duplicate_text_page_ratio: 0,
          avg_chars_per_page: 1400,
          garbage_ratio: 0.01,
          whitespace_ratio: 0.18
        },
        trust_diagnostics: {
          block_coherence: 0.92,
          coordinate_sanity: 0.91,
          reading_order_stability: 0.9,
          duplicate_text_suppression_needed: false,
          hidden_text_layer_suspected: false,
          invisible_text_suspected: false,
          stacked_duplicate_text_suspected: false,
          full_page_raster_ratio: 0,
          mixed_text_image_ratio: 0,
          ocr_replace_confidence: 0,
          ocr_augment_confidence: 0,
          ocr_confidence_threshold_met: false,
          rationale: ["mock_pdf_classifier"]
        },
        page_classes: [],
        class_distribution: [{ class: "embedded_clean", count: 3 }]
      }
    : null;
  reader.pdf_runtime_policy = reader.source_path.toLowerCase().endsWith(".pdf")
    ? {
        text_only_policy: "full_text",
        sentence_highlight_policy: "exact_sentence",
        search_policy: "full_text",
        bookmark_policy: "canonical_text",
        tts_allowed: true,
        pretty_sync_enabled: true,
        exact_sentence_sync: true,
        explanation: "Exact sentence sync is enabled for this PDF.",
        degraded_reasons: []
      }
    : null;
  reader.pdf_ocr_alignment = reader.source_path.toLowerCase().endsWith(".pdf")
    ? {
        quality_class: "ocr_high_trust",
        source_kind: "embedded_text",
        sentence_count: MOCK_PAGES.flatMap((page) => page.sentences).length,
        mapped_sentence_count: MOCK_PAGES.flatMap((page) => page.sentences).length,
        rect_mapped_sentence_count: MOCK_PAGES.flatMap((page) => page.sentences).length,
        line_mapped_sentence_count: 0,
        block_mapped_sentence_count: 0,
        page_only_sentence_count: 0,
        unmappable_sentence_count: 0,
        highlightable_sentence_count: MOCK_PAGES.flatMap((page) => page.sentences).length,
        token_lineage_available: false,
        deterministic: true,
        coverage_ratio: 1,
        reused_alignment_count: 0,
        rebuilt_alignment_count: MOCK_PAGES.flatMap((page) => page.sentences).length,
        cached_page_bucket_count: 3,
        alignment_build_ms: 3,
        geometry_block_count: 3,
        geometry_line_count: 3,
        geometry_token_count: MOCK_PAGES.flatMap((page) => page.sentences).length * 3,
        page_timing_count: 3,
        chunk_timing_count: 1,
        max_page_build_ms: 2,
        max_chunk_build_ms: 3,
        cross_column_alignment_count: 0,
        cross_column_confident_alignment_count: 0,
        exact_sentence_rate: 1,
        degraded_fallback_rate: 0,
        page_only_rate: 0,
        unmappable_rate: 0,
        degraded_reasons: [],
        explanation: "Mock OCR alignment artifact indicates stable sentence-level geometry."
      }
    : null;
  reader.pdf_ocr_pipeline = reader.source_path.toLowerCase().endsWith(".pdf")
    ? {
        engine_policy: "embedded_text_only",
        fallback_decisions: [],
        ocr_enabled: false,
        page_count: 3,
        sampled_pages: 3,
        chunk_count: 1,
        reading_order_mode: "embedded_text_order",
        normalization_summary: {
          canonical_text_derived_from_ocr: false,
          page_sentence_provenance_available: true,
          token_trail_available: false,
          broken_line_join_count: 0,
          hyphen_recovery_count: 0,
          ligature_replacement_count: 0,
          unicode_normalization_count: 0,
          repeated_header_suppression_count: 0,
          repeated_footer_suppression_count: 0,
          margin_sidenote_suppression_count: 0,
          table_cell_normalization_count: 0,
          footnote_marker_adjustment_count: 0,
          punctuation_repair_count: 0,
          dropped_noise_line_count: 0,
          merged_line_count: 0,
          trace_notes: []
        },
        page_reading_order: [
          {
            page_index: 1,
            layout_class: "single_column",
            confidence: 0.9,
            reasons: ["mock_simple_body_order"]
          }
        ],
        fallback_strategy_labels: []
      }
    : null;
  reader.page_text = pageData.text;
  reader.sentences = pageData.sentences;
  reader.sentence_anchor_map = pageData.sentences.map((_, idx) => idx);
  reader.highlighted_sentence_idx = 0;
  reader.tts.current_sentence_idx = 0;
  reader.tts.sentence_count = pageData.sentences.length;
  reader.stats.page_index = clampedPage + 1;
  reader.stats.total_pages = pages.length;
  reader.stats.page_word_count = wordsOnPage;
  reader.stats.page_sentence_count = pageData.sentences.length;
  reader.stats.tts_progress_pct = 100;
  reader.stats.page_start_percent = Number(((wordsBeforePage / totalWords) * 100).toFixed(3));
  reader.stats.page_end_percent = Number(
    (((wordsBeforePage + wordsOnPage) / totalWords) * 100).toFixed(3)
  );
  reader.stats.words_read_up_to_page_start = wordsBeforePage;
  reader.stats.sentences_read_up_to_page_start = clampedPage;
  reader.stats.words_read_up_to_page_end = wordsBeforePage + wordsOnPage;
  reader.stats.sentences_read_up_to_page_end = clampedPage + pageData.sentences.length;
  reader.stats.words_read_up_to_current_position = wordsBeforePage + wordsOnPage;
  reader.stats.sentences_read_up_to_current_position = clampedPage + pageData.sentences.length;
  reader.stats.global_progress_pct = Number(
    ((reader.stats.words_read_up_to_current_position / totalWords) * 100).toFixed(3)
  );
}

async function mockOpenWithPath(path: string): Promise<OpenSourceResult> {
  const trimmed = path.trim();
  if (!trimmed) {
    throw {
      code: "invalid_input",
      message: "Path cannot be empty"
    } satisfies BridgeError;
  }

  const scenario = mockScenarioForPath(trimmed);
  mockState.pages = scenario.pages;
  const reader = ensureMockReader();
  reader.source_path = scenario.sourcePath;
  reader.source_name = scenario.sourceName;
  reader.pretty_kind = scenario.prettyKind;
  reader.images = scenario.images;
  reader.text_only_mode = false;
  mockState.session.mode = "reader";
  mockState.session.active_source_path = scenario.sourcePath;
  applyMockPage(reader, 0);
  mockState.reader = reader;
  return {
    session: structuredClone(mockState.session),
    reader: structuredClone(reader)
  };
}

async function mockSessionGetBootstrap(): Promise<BootstrapState> {
  return structuredClone(mockState.bootstrap);
}

async function mockSessionToggleTheme(): Promise<BootstrapState> {
  const nextTheme = mockState.bootstrap.config.theme === "night" ? "day" : "night";
  mockState.bootstrap.config.theme = nextTheme;
  if (mockState.reader) {
    mockState.reader.settings.theme = nextTheme;
  }
  return structuredClone(mockState.bootstrap);
}

async function mockSessionGetState(): Promise<SessionState> {
  return structuredClone(mockState.session);
}

async function mockSessionReturnToStarter(): Promise<SessionState> {
  mockState.session.mode = "starter";
  mockState.session.active_source_path = null;
  mockState.reader = null;
  return structuredClone(mockState.session);
}

async function mockAppSafeQuit(): Promise<void> {
  await mockSessionReturnToStarter();
}

async function mockRecentList(limit?: number): Promise<RecentBook[]> {
  return structuredClone(mockState.recents.slice(0, normalizeRecentLimit(limit)));
}

async function mockRecentDelete(path: string): Promise<void> {
  mockState.recents = mockState.recents.filter((book) => book.source_path !== path);
}

async function mockRecentCloseBrowserTab(path: string): Promise<void> {
  const recent = mockState.recents.find((book) => book.source_path === path);
  if (!recent?.browser_tab_id) {
    throw {
      code: "invalid_input",
      message: `Recent source is not an imported browser tab: ${path}`
    } satisfies BridgeError;
  }
}

async function mockSourceOpenPath(path: string): Promise<OpenSourceResult> {
  return mockOpenWithPath(path);
}

async function mockSourceOpenClipboardText(text: string): Promise<OpenSourceResult> {
  const trimmed = text.trim();
  if (!trimmed) {
    throw {
      code: "invalid_input",
      message: "Clipboard text is empty"
    } satisfies BridgeError;
  }
  return mockOpenWithPath(".cache/clipboard/mock.txt");
}

async function mockBrowserTabsHealth(): Promise<BrowsrHealth> {
  return {
    ok: true,
    extension_connected: true,
    now: new Date().toISOString()
  };
}

async function mockBrowserTabsListWindows(): Promise<BrowserWindowInfo[]> {
  return structuredClone(MOCK_BROWSER_WINDOWS);
}

async function mockBrowserTabsListTabs(
  windowId?: number,
  query?: string
): Promise<BrowserTabInfo[]> {
  const needle = (query ?? "").trim().toLowerCase();
  return structuredClone(
    MOCK_BROWSER_TABS.filter((tab) => {
      if (windowId !== undefined && tab.windowId !== windowId) {
        return false;
      }
      if (!needle) {
        return true;
      }
      return (
        tab.title.toLowerCase().includes(needle) || tab.url.toLowerCase().includes(needle)
      );
    })
  );
}

async function mockSourceOpenBrowserTab(tabId: number): Promise<OpenSourceResult> {
  const tab = MOCK_BROWSER_TABS.find((value) => value.id === tabId);
  if (!tab) {
    throw {
      code: "not_found",
      message: `Unknown mock browser tab ${tabId}`
    } satisfies BridgeError;
  }
  return mockOpenWithPath(`.cache/browser-tabs/${tab.id}/browser-tab.lltab`);
}

async function mockSourceOpenBrowserTabBundle(tabId: number): Promise<OpenSourceResult> {
  return mockSourceOpenBrowserTab(tabId);
}

async function mockReaderGetSnapshot(): Promise<ReaderSnapshot> {
  return structuredClone(ensureMockReader());
}

async function mockReaderNextPage(): Promise<ReaderSnapshot> {
  const reader = ensureMockReader();
  applyMockPage(reader, reader.current_page + 1);
  return structuredClone(reader);
}

async function mockReaderPrevPage(): Promise<ReaderSnapshot> {
  const reader = ensureMockReader();
  applyMockPage(reader, reader.current_page - 1);
  return structuredClone(reader);
}

async function mockReaderSetPage(page: number): Promise<ReaderSnapshot> {
  const reader = ensureMockReader();
  applyMockPage(reader, page);
  return structuredClone(reader);
}

async function mockReaderApplySettings(patch: ReaderSettingsPatch): Promise<ReaderSnapshot> {
  const reader = ensureMockReader();
  reader.settings = {
    ...reader.settings,
    ...patch
  };
  return structuredClone(reader);
}

async function mockReaderNextSentence(): Promise<ReaderSnapshot> {
  const reader = ensureMockReader();
  const count = reader.sentences.length;
  if (count === 0) {
    reader.highlighted_sentence_idx = null;
    reader.tts.current_sentence_idx = null;
    return structuredClone(reader);
  }
  const current = reader.highlighted_sentence_idx ?? 0;
  reader.highlighted_sentence_idx = Math.min(count - 1, current + 1);
  reader.tts.current_sentence_idx = reader.highlighted_sentence_idx;
  return structuredClone(reader);
}

async function mockReaderPrevSentence(): Promise<ReaderSnapshot> {
  const reader = ensureMockReader();
  const count = reader.sentences.length;
  if (count === 0) {
    reader.highlighted_sentence_idx = null;
    reader.tts.current_sentence_idx = null;
    return structuredClone(reader);
  }
  const current = reader.highlighted_sentence_idx ?? 0;
  reader.highlighted_sentence_idx = Math.max(0, current - 1);
  reader.tts.current_sentence_idx = reader.highlighted_sentence_idx;
  return structuredClone(reader);
}

async function mockReaderTtsPlay(): Promise<ReaderSnapshot> {
  const reader = ensureMockReader();
  if (reader.highlighted_sentence_idx === null) {
    reader.highlighted_sentence_idx = 0;
  }
  reader.tts.current_sentence_idx = reader.highlighted_sentence_idx;
  reader.tts.state = "playing";
  return structuredClone(reader);
}

async function mockReaderTtsPause(): Promise<ReaderSnapshot> {
  const reader = ensureMockReader();
  if (reader.tts.state === "playing") {
    reader.tts.state = "paused";
  }
  return structuredClone(reader);
}

async function mockReaderTtsTogglePlayPause(): Promise<ReaderSnapshot> {
  const reader = ensureMockReader();
  if (reader.tts.state === "playing") {
    reader.tts.state = "paused";
  } else {
    reader.tts.state = "playing";
  }
  return structuredClone(reader);
}

async function mockReaderTtsPlayFromPageStart(): Promise<ReaderSnapshot> {
  const reader = ensureMockReader();
  reader.highlighted_sentence_idx = 0;
  reader.tts.current_sentence_idx = 0;
  reader.tts.state = "playing";
  return structuredClone(reader);
}

async function mockReaderTtsPlayFromHighlight(): Promise<ReaderSnapshot> {
  const reader = ensureMockReader();
  if (reader.highlighted_sentence_idx === null) {
    reader.highlighted_sentence_idx = 0;
  }
  reader.tts.current_sentence_idx = reader.highlighted_sentence_idx;
  reader.tts.state = "playing";
  return structuredClone(reader);
}

async function mockReaderTtsSeekNext(): Promise<ReaderSnapshot> {
  return mockReaderNextSentence();
}

async function mockReaderTtsSeekPrev(): Promise<ReaderSnapshot> {
  return mockReaderPrevSentence();
}

async function mockReaderTtsRepeatSentence(): Promise<ReaderSnapshot> {
  const reader = ensureMockReader();
  if (reader.highlighted_sentence_idx === null) {
    reader.highlighted_sentence_idx = 0;
  }
  reader.tts.current_sentence_idx = reader.highlighted_sentence_idx;
  return structuredClone(reader);
}

async function mockReaderTtsPrecomputePage(): Promise<ReaderSnapshot> {
  return mockReaderGetSnapshot();
}
async function mockCalibreLoadBooks(): Promise<CalibreBook[]> {
  return structuredClone(mockState.calibreBooks);
}

async function mockCalibreOpenBook(): Promise<OpenSourceResult> {
  return mockOpenWithPath(".cache/calibre-downloads/mock.epub");
}

async function mockCalibreEnsureThumbnail(): Promise<string | null> {
  return null;
}

async function mockPanelToggleSettings(): Promise<SessionState> {
  mockState.session.panels.show_settings = !mockState.session.panels.show_settings;
  if (mockState.session.panels.show_settings) {
    mockState.session.panels.show_stats = false;
  }
  return structuredClone(mockState.session);
}

async function mockPanelToggleStats(): Promise<SessionState> {
  mockState.session.panels.show_stats = !mockState.session.panels.show_stats;
  if (mockState.session.panels.show_stats) {
    mockState.session.panels.show_settings = false;
  }
  return structuredClone(mockState.session);
}

async function mockPanelToggleTts(): Promise<SessionState> {
  mockState.session.panels.show_tts = !mockState.session.panels.show_tts;
  return structuredClone(mockState.session);
}

async function mockOnSourceOpen(handler: (event: SourceOpenEvent) => void): Promise<UnlistenFn> {
  queueMicrotask(() =>
    handler({
      request_id: 0,
      phase: "ready",
      source_path: null,
      message: "Using mock backend adapter"
    })
  );
  return () => Promise.resolve();
}

async function mockOnCalibreLoad(handler: (event: CalibreLoadEvent) => void): Promise<UnlistenFn> {
  queueMicrotask(() =>
    handler({
      request_id: 0,
      phase: "ready",
      count: 0,
      message: "Using mock backend adapter"
    })
  );
  return () => Promise.resolve();
}

async function mockOnSessionState(handler: (event: SessionStateEvent) => void): Promise<UnlistenFn> {
  queueMicrotask(() =>
    handler({
      request_id: 0,
      action: "ready",
      session: structuredClone(mockState.session)
    })
  );
  return () => Promise.resolve();
}

async function mockOnReaderState(handler: (event: ReaderStateEvent) => void): Promise<UnlistenFn> {
  queueMicrotask(() => {
    if (!mockState.reader) {
      return;
    }
    handler({
      request_id: 0,
      action: "ready",
      reader: structuredClone(mockState.reader)
    });
  });
  return () => Promise.resolve();
}

async function mockOnTtsState(handler: (event: TtsStateEvent) => void): Promise<UnlistenFn> {
  queueMicrotask(() => {
    const reader = ensureMockReader();
    handler({
      request_id: 0,
      action: "ready",
      tts: structuredClone(reader.tts)
    });
  });
  return () => Promise.resolve();
}

async function mockOnPdfTranscription(
  handler: (event: PdfTranscriptionEvent) => void
): Promise<UnlistenFn> {
  queueMicrotask(() =>
    handler({
      request_id: 0,
      phase: "ready",
      source_path: "",
      message: "Using mock backend adapter"
    })
  );
  return () => Promise.resolve();
}

async function mockOnLogLevel(handler: (event: LogLevelEvent) => void): Promise<UnlistenFn> {
  queueMicrotask(() =>
    handler({
      request_id: 0,
      level: mockState.logLevel
    })
  );
  return () => Promise.resolve();
}

async function mockLoggingSetLevel(level: string): Promise<string> {
  const normalized = level.trim().toLowerCase();
  if (!["trace", "debug", "info", "warn", "error"].includes(normalized)) {
    throw {
      code: "invalid_input",
      message: `Unsupported log level '${level}'`
    } satisfies BridgeError;
  }
  mockState.logLevel = normalized;
  return mockState.logLevel;
}

export interface BackendApi {
  appSafeQuit: () => Promise<void>;
  sessionGetBootstrap: () => Promise<BootstrapState>;
  sessionToggleTheme: () => Promise<BootstrapState>;
  sessionGetState: () => Promise<SessionState>;
  sessionReturnToStarter: () => Promise<SessionState>;
  panelToggleSettings: () => Promise<SessionState>;
  panelToggleStats: () => Promise<SessionState>;
  panelToggleTts: () => Promise<SessionState>;
  recentList: (limit?: number) => Promise<RecentBook[]>;
  recentDelete: (path: string) => Promise<void>;
  recentCloseBrowserTab: (path: string) => Promise<void>;
  browserTabsHealth: () => Promise<BrowsrHealth>;
  browserTabsListWindows: () => Promise<BrowserWindowInfo[]>;
  browserTabsListTabs: (
    windowId?: number,
    query?: string,
    refresh?: boolean
  ) => Promise<BrowserTabInfo[]>;
  sourceOpenPath: (path: string) => Promise<OpenSourceResult>;
  sourceOpenClipboard: () => Promise<OpenSourceResult>;
  sourceOpenClipboardText: (text: string) => Promise<OpenSourceResult>;
  sourceOpenBrowserTab: (tabId: number, windowId?: number) => Promise<OpenSourceResult>;
  sourceOpenBrowserTabBundle: (tabId: number, windowId?: number) => Promise<OpenSourceResult>;
  sourceRefreshBrowserTab: (path: string) => Promise<OpenSourceResult>;
  readerGetSnapshot: () => Promise<ReaderSnapshot>;
  readerNextPage: () => Promise<ReaderSnapshot>;
  readerPrevPage: () => Promise<ReaderSnapshot>;
  readerSetPage: (page: number) => Promise<ReaderSnapshot>;
  readerSentenceClick: (sentenceIdx: number) => Promise<ReaderSnapshot>;
  readerNextSentence: () => Promise<ReaderSnapshot>;
  readerPrevSentence: () => Promise<ReaderSnapshot>;
  readerToggleTextOnly: () => Promise<ReaderSnapshot>;
  readerApplySettings: (patch: ReaderSettingsPatch) => Promise<ReaderSnapshot>;
  readerSearchSetQuery: (query: string) => Promise<ReaderSnapshot>;
  readerSearchNext: () => Promise<ReaderSnapshot>;
  readerSearchPrev: () => Promise<ReaderSnapshot>;
  readerTtsPlay: () => Promise<ReaderSnapshot>;
  readerTtsPause: () => Promise<ReaderSnapshot>;
  readerTtsTogglePlayPause: () => Promise<ReaderSnapshot>;
  readerTtsPlayFromPageStart: () => Promise<ReaderSnapshot>;
  readerTtsPlayFromHighlight: () => Promise<ReaderSnapshot>;
  readerTtsSeekNext: () => Promise<ReaderSnapshot>;
  readerTtsSeekPrev: () => Promise<ReaderSnapshot>;
  readerTtsRepeatSentence: () => Promise<ReaderSnapshot>;
  readerTtsPrecomputePage: () => Promise<ReaderSnapshot>;
  readerLoadPdfBytes: (path: string) => Promise<Uint8Array>;
  readerLoadPdfRenderPrecomputed: (path: string) => Promise<PdfRenderPrecomputedState>;
  readerLoadPdfSyncMap: (path: string) => Promise<PdfSentenceLocation[]>;
  readerPersistPdfSyncMap: (path: string, locations: PdfSentenceLocation[]) => Promise<void>;
  readerCloseSession: () => Promise<SessionState>;
  loggingSetLevel: (level: string) => Promise<string>;
  calibreLoadCachedBooks: () => Promise<CalibreBook[]>;
  calibreLoadBooks: (forceRefresh?: boolean) => Promise<CalibreBook[]>;
  calibreOpenBook: (bookId: number) => Promise<OpenSourceResult>;
  calibreEnsureThumbnail: (bookId: number) => Promise<string | null>;
  onSourceOpen: (handler: (event: SourceOpenEvent) => void) => Promise<UnlistenFn>;
  onCalibreLoad: (handler: (event: CalibreLoadEvent) => void) => Promise<UnlistenFn>;
  onSessionState: (handler: (event: SessionStateEvent) => void) => Promise<UnlistenFn>;
  onReaderState: (handler: (event: ReaderStateEvent) => void) => Promise<UnlistenFn>;
  onReaderPlaybackState: (handler: (event: ReaderPlaybackStateEvent) => void) => Promise<UnlistenFn>;
  onTtsState: (handler: (event: TtsStateEvent) => void) => Promise<UnlistenFn>;
  onPdfTranscription: (handler: (event: PdfTranscriptionEvent) => void) => Promise<UnlistenFn>;
  onLogLevel: (handler: (event: LogLevelEvent) => void) => Promise<UnlistenFn>;
}

function createTauriBackendApi(): BackendApi {
  return {
    appSafeQuit: () => invokeCommand<void>("app_safe_quit"),
    sessionGetBootstrap: () => invokeCommand<BootstrapState>("session_get_bootstrap"),
    sessionToggleTheme: () => invokeCommand<BootstrapState>("session_toggle_theme"),
    sessionGetState: () => invokeCommand<SessionState>("session_get_state"),
    sessionReturnToStarter: () => invokeCommand<SessionState>("session_return_to_starter"),
    panelToggleSettings: () => invokeCommand<SessionState>("panel_toggle_settings"),
    panelToggleStats: () => invokeCommand<SessionState>("panel_toggle_stats"),
    panelToggleTts: () => invokeCommand<SessionState>("panel_toggle_tts"),
    recentList: (limit) =>
      invokeCommand<RecentBook[]>("recent_list", { limit: normalizeRecentLimit(limit) }),
    recentDelete: (path) => invokeCommand<void>("recent_delete", { path }),
    recentCloseBrowserTab: (path) => invokeCommand<void>("recent_close_browser_tab", { path }),
    browserTabsHealth: () => invokeCommand<BrowsrHealth>("browser_tabs_health"),
    browserTabsListWindows: () =>
      invokeCommand<BrowserWindowInfo[]>("browser_tabs_list_windows"),
    browserTabsListTabs: (windowId, query, refresh) =>
      invokeCommand<BrowserTabInfo[]>("browser_tabs_list_tabs", { windowId, query, refresh }),
    sourceOpenPath: (path) => invokeCommand<OpenSourceResult>("source_open_path", { path }),
    sourceOpenClipboard: () => invokeCommand<OpenSourceResult>("source_open_clipboard"),
    sourceOpenClipboardText: (text) =>
      invokeCommand<OpenSourceResult>("source_open_clipboard_text", { text }),
    sourceOpenBrowserTab: (tabId, windowId) =>
      invokeCommand<OpenSourceResult>("source_open_browser_tab", { tabId, windowId }),
    sourceOpenBrowserTabBundle: (tabId, windowId) =>
      invokeCommand<OpenSourceResult>("source_open_browser_tab_bundle", { tabId, windowId }),
    sourceRefreshBrowserTab: (path) =>
      invokeCommand<OpenSourceResult>("source_refresh_browser_tab", { path }),
    readerGetSnapshot: () => invokeCommand<ReaderSnapshot>("reader_get_snapshot"),
    readerNextPage: () => invokeCommand<ReaderSnapshot>("reader_next_page"),
    readerPrevPage: () => invokeCommand<ReaderSnapshot>("reader_prev_page"),
    readerSetPage: (page) => invokeCommand<ReaderSnapshot>("reader_set_page", { page }),
    readerSentenceClick: (sentenceIdx) =>
      invokeCommand<ReaderSnapshot>("reader_sentence_click", { sentenceIdx }),
    readerNextSentence: () => invokeCommand<ReaderSnapshot>("reader_next_sentence"),
    readerPrevSentence: () => invokeCommand<ReaderSnapshot>("reader_prev_sentence"),
    readerToggleTextOnly: () => invokeCommand<ReaderSnapshot>("reader_toggle_text_only"),
    readerApplySettings: (patch) => invokeCommand<ReaderSnapshot>("reader_apply_settings", { patch }),
    readerSearchSetQuery: (query) =>
      invokeCommand<ReaderSnapshot>("reader_search_set_query", { query }),
    readerSearchNext: () => invokeCommand<ReaderSnapshot>("reader_search_next"),
    readerSearchPrev: () => invokeCommand<ReaderSnapshot>("reader_search_prev"),
    readerTtsPlay: () => invokeCommand<ReaderSnapshot>("reader_tts_play"),
    readerTtsPause: () => invokeCommand<ReaderSnapshot>("reader_tts_pause"),
    readerTtsTogglePlayPause: () => invokeCommand<ReaderSnapshot>("reader_tts_toggle_play_pause"),
    readerTtsPlayFromPageStart: () => invokeCommand<ReaderSnapshot>("reader_tts_play_from_page_start"),
    readerTtsPlayFromHighlight: () => invokeCommand<ReaderSnapshot>("reader_tts_play_from_highlight"),
    readerTtsSeekNext: () => invokeCommand<ReaderSnapshot>("reader_tts_seek_next"),
    readerTtsSeekPrev: () => invokeCommand<ReaderSnapshot>("reader_tts_seek_prev"),
    readerTtsRepeatSentence: () => invokeCommand<ReaderSnapshot>("reader_tts_repeat_sentence"),
    readerTtsPrecomputePage: () => invokeCommand<ReaderSnapshot>("reader_tts_precompute_page"),
    readerLoadPdfBytes: async (path) =>
      Uint8Array.from(await invokeCommand<number[]>("reader_load_pdf_bytes", { path })),
    readerLoadPdfRenderPrecomputed: (path) =>
      invokeCommand<PdfRenderPrecomputedState>("reader_load_pdf_render_precomputed", { path }),
    readerLoadPdfSyncMap: (path) =>
      invokeCommand<PdfSentenceLocation[]>("reader_load_pdf_sync_map", { path }),
    readerPersistPdfSyncMap: (path, locations) =>
      invokeCommand<void>("reader_persist_pdf_sync_map", { path, locations }),
    readerCloseSession: () => invokeCommand<SessionState>("reader_close_session"),
    loggingSetLevel: (level) => invokeCommand<string>("logging_set_level", { level }),
    calibreLoadCachedBooks: () => invokeCommand<CalibreBook[]>("calibre_load_cached_books"),
    calibreLoadBooks: (forceRefresh) =>
      invokeCommand<CalibreBook[]>("calibre_load_books", { forceRefresh }),
    calibreOpenBook: (bookId) => invokeCommand<OpenSourceResult>("calibre_open_book", { bookId }),
    calibreEnsureThumbnail: (bookId) =>
      invokeCommand<string | null>("calibre_ensure_thumbnail", { bookId }),
    onSourceOpen: async (handler) => {
      return listen<SourceOpenEvent>("source-open", (event) => handler(event.payload));
    },
    onCalibreLoad: async (handler) => {
      return listen<CalibreLoadEvent>("calibre-load", (event) => handler(event.payload));
    },
    onSessionState: async (handler) => {
      return listen<SessionStateEvent>("session-state", (event) => handler(event.payload));
    },
    onReaderState: async (handler) => {
      return listen<ReaderStateEvent>("reader-state", (event) => handler(event.payload));
    },
    onReaderPlaybackState: async (handler) => {
      return listen<ReaderPlaybackStateEvent>("reader-playback-state", (event) =>
        handler(event.payload)
      );
    },
    onTtsState: async (handler) => {
      return listen<TtsStateEvent>("tts-state", (event) => handler(event.payload));
    },
    onPdfTranscription: async (handler) => {
      return listen<PdfTranscriptionEvent>("pdf-transcription", (event) => handler(event.payload));
    },
    onLogLevel: async (handler) => {
      return listen<LogLevelEvent>("log-level", (event) => handler(event.payload));
    }
  };
}

function createMockBackendApi(): BackendApi {
  return {
    appSafeQuit: mockAppSafeQuit,
    sessionGetBootstrap: mockSessionGetBootstrap,
    sessionToggleTheme: mockSessionToggleTheme,
    sessionGetState: mockSessionGetState,
    sessionReturnToStarter: mockSessionReturnToStarter,
    panelToggleSettings: mockPanelToggleSettings,
    panelToggleStats: mockPanelToggleStats,
    panelToggleTts: mockPanelToggleTts,
    recentList: mockRecentList,
    recentDelete: mockRecentDelete,
    recentCloseBrowserTab: mockRecentCloseBrowserTab,
    browserTabsHealth: mockBrowserTabsHealth,
    browserTabsListWindows: mockBrowserTabsListWindows,
    browserTabsListTabs: mockBrowserTabsListTabs,
    sourceOpenPath: mockSourceOpenPath,
    sourceOpenClipboard: () => mockSourceOpenClipboardText(""),
    sourceOpenClipboardText: mockSourceOpenClipboardText,
    sourceOpenBrowserTab: mockSourceOpenBrowserTab,
    sourceOpenBrowserTabBundle: mockSourceOpenBrowserTabBundle,
    sourceRefreshBrowserTab: mockOpenWithPath,
    readerGetSnapshot: mockReaderGetSnapshot,
    readerNextPage: mockReaderNextPage,
    readerPrevPage: mockReaderPrevPage,
    readerSetPage: mockReaderSetPage,
    readerSentenceClick: mockReaderGetSnapshot,
    readerNextSentence: mockReaderNextSentence,
    readerPrevSentence: mockReaderPrevSentence,
    readerToggleTextOnly: mockReaderGetSnapshot,
    readerApplySettings: mockReaderApplySettings,
    readerSearchSetQuery: mockReaderGetSnapshot,
    readerSearchNext: mockReaderGetSnapshot,
    readerSearchPrev: mockReaderGetSnapshot,
    readerTtsPlay: mockReaderTtsPlay,
    readerTtsPause: mockReaderTtsPause,
    readerTtsTogglePlayPause: mockReaderTtsTogglePlayPause,
    readerTtsPlayFromPageStart: mockReaderTtsPlayFromPageStart,
    readerTtsPlayFromHighlight: mockReaderTtsPlayFromHighlight,
    readerTtsSeekNext: mockReaderTtsSeekNext,
    readerTtsSeekPrev: mockReaderTtsSeekPrev,
    readerTtsRepeatSentence: mockReaderTtsRepeatSentence,
    readerTtsPrecomputePage: mockReaderTtsPrecomputePage,
    readerLoadPdfBytes: async () => new Uint8Array(),
    readerLoadPdfRenderPrecomputed: async () => ({
      version: 1,
      page_texts: [],
      sentence_page_hints: [],
      source: "mock"
    }),
    readerLoadPdfSyncMap: async () => [],
    readerPersistPdfSyncMap: async () => {},
    readerCloseSession: mockSessionReturnToStarter,
    loggingSetLevel: mockLoggingSetLevel,
    calibreLoadCachedBooks: mockCalibreLoadBooks,
    calibreLoadBooks: mockCalibreLoadBooks,
    calibreOpenBook: mockCalibreOpenBook,
    calibreEnsureThumbnail: mockCalibreEnsureThumbnail,
    onSourceOpen: mockOnSourceOpen,
    onCalibreLoad: mockOnCalibreLoad,
    onSessionState: mockOnSessionState,
    onReaderState: mockOnReaderState,
    onReaderPlaybackState: async () => () => {},
    onTtsState: mockOnTtsState,
    onPdfTranscription: mockOnPdfTranscription,
    onLogLevel: mockOnLogLevel
  };
}

export const backendApi: BackendApi = isTauriRuntime()
  ? createTauriBackendApi()
  : createMockBackendApi();
