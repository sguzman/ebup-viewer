// @vitest-environment jsdom
import fs from 'node:fs';
import path from 'node:path';
import { StrictMode, act } from 'react';
import { createRoot } from 'react-dom/client';
import { describe, expect, it } from 'vitest';
import { ReaderShell } from '../src/components/ReaderShell';
import { ReaderQuickActionsDock } from '../src/components/ReaderQuickActionsDock';
import { useAppStore } from '../src/store/appStore';
import type { ReaderSnapshot } from '../src/types';

(globalThis as any).IS_REACT_ACT_ENVIRONMENT = true;
(globalThis as any).ResizeObserver = class { observe() {} disconnect() {} unobserve() {} };

function makeReader(html: string, text: string): ReaderSnapshot {
  const sentences = text.split(/(?<=[.!?])\s+/).filter(Boolean).slice(0, 200);
  return {
    source_path: '/tmp/browser-tab.lltab', source_name: 'Browser Tab', current_page: 0, total_pages: 1,
    text_only_mode: false, has_structured_markdown: true, pretty_kind: 'html', pdf_geometry_mode: null, pdf_sync_strategy: null, images: [],
    tts_text_page: text, reading_markdown_page: null, reading_html_page: html, page_text: text,
    sentences, canonical_sentences: sentences, sentence_anchor_map: sentences.map((_, i) => i), highlighted_sentence_idx: 0,
    search_query: '', search_matches: [], selected_search_match: null,
    settings: { theme: 'day', font_family: 'lexend', font_weight: 'bold', day_highlight: { r: 0.2, g: 0.4, b: 0.7, a: 0.15 }, night_highlight: { r: 0.8, g: 0.8, b: 0.5, a: 0.2 }, font_size: 22, line_spacing: 1.2, word_spacing: 0, letter_spacing: 0, margin_horizontal: 100, margin_vertical: 12, lines_per_page: 700, pause_after_sentence: 0.06, auto_scroll_tts: true, center_spoken_sentence: true, time_remaining_display: 'adaptive', tts_speed: 2.5, tts_volume: 1 },
    tts: { state: 'idle', current_sentence_idx: 0, sentence_count: Math.max(1, sentences.length), can_seek_prev: false, can_seek_next: true, progress_pct: 0 },
    stats: { page_index: 1, total_pages: 1, tts_progress_pct: 0, global_progress_pct: 0, page_time_remaining_secs: 0, book_time_remaining_secs: 0, page_word_count: text.split(/\s+/).filter(Boolean).length, page_sentence_count: sentences.length, page_start_percent: 0, page_end_percent: 100, words_read_up_to_page_start: 0, sentences_read_up_to_page_start: 0, words_read_up_to_page_end: text.split(/\s+/).filter(Boolean).length, sentences_read_up_to_page_end: sentences.length, words_read_up_to_current_position: 0, sentences_read_up_to_current_position: 0 },
    panels: { show_settings: true, show_stats: false, show_tts: true },
  };
}

describe('tmp browser tab loop repro', () => {
  it('mounts reader and quick actions in strict mode', async () => {
    const dirs = fs.readdirSync(path.resolve('.cache/lantern-leaf/browser-tabs')).sort();
    const dir = path.resolve('.cache/lantern-leaf/browser-tabs', dirs[dirs.length - 1]);
    const html = fs.readFileSync(path.join(dir, 'snapshot.html'), 'utf8');
    const text = fs.readFileSync(path.join(dir, 'snapshot.txt'), 'utf8');
    const reader = makeReader(html, text);
    useAppStore.setState({ reader, busy: false, session: { mode: 'reader', active_source_path: reader.source_path, open_in_flight: false, panels: reader.panels } });
    const container = document.createElement('div');
    document.body.appendChild(container);
    const root = createRoot(container);
    await act(async () => {
      root.render(
        <StrictMode>
          <div>
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
            <ReaderQuickActionsDock />
          </div>
        </StrictMode>
      );
    });
    expect(container.innerHTML.length).toBeGreaterThan(0);
    await act(async () => { root.unmount(); });
    container.remove();
  });
});
