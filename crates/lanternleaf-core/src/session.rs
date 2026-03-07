use crate::{
    cancellation::CancellationToken, config, epub_loader, normalizer, pagination, text_utils,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use ts_rs::TS;

mod command_transitions;
mod document_loading;
mod page_navigation;
mod playback;
mod settings;

pub use document_loading::{
    load_session_for_source, load_session_for_source_with_cancel, persist_session_housekeeping,
};

const BASE_WPM: f64 = 170.0;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, TS)]
#[ts(export)]
pub struct PanelState {
    pub show_settings: bool,
    pub show_stats: bool,
    pub show_tts: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TtsPlaybackState {
    #[default]
    Idle,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ReaderSettingsView {
    pub theme: config::ThemeMode,
    pub font_family: config::FontFamily,
    pub font_weight: config::FontWeight,
    pub day_highlight: config::HighlightColor,
    pub night_highlight: config::HighlightColor,
    pub font_size: u32,
    pub line_spacing: f32,
    pub word_spacing: u32,
    pub letter_spacing: u32,
    pub margin_horizontal: u16,
    pub margin_vertical: u16,
    pub lines_per_page: usize,
    pub pause_after_sentence: f32,
    pub auto_scroll_tts: bool,
    pub center_spoken_sentence: bool,
    pub time_remaining_display: config::TimeRemainingDisplay,
    pub tts_speed: f32,
    pub tts_volume: f32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ReaderTtsView {
    pub state: TtsPlaybackState,
    pub current_sentence_idx: Option<usize>,
    pub sentence_count: usize,
    pub can_seek_prev: bool,
    pub can_seek_next: bool,
    pub progress_pct: f64,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct ReaderSettingsPatch {
    #[ts(optional)]
    pub theme: Option<config::ThemeMode>,
    #[ts(optional)]
    pub day_highlight: Option<config::HighlightColor>,
    #[ts(optional)]
    pub night_highlight: Option<config::HighlightColor>,
    #[ts(optional)]
    pub font_family: Option<config::FontFamily>,
    #[ts(optional)]
    pub font_weight: Option<config::FontWeight>,
    #[ts(optional)]
    pub font_size: Option<u32>,
    #[ts(optional)]
    pub line_spacing: Option<f32>,
    #[ts(optional)]
    pub word_spacing: Option<u32>,
    #[ts(optional)]
    pub letter_spacing: Option<u32>,
    #[ts(optional)]
    pub margin_horizontal: Option<u16>,
    #[ts(optional)]
    pub margin_vertical: Option<u16>,
    #[ts(optional)]
    pub lines_per_page: Option<usize>,
    #[ts(optional)]
    pub pause_after_sentence: Option<f32>,
    #[ts(optional)]
    pub auto_scroll_tts: Option<bool>,
    #[ts(optional)]
    pub center_spoken_sentence: Option<bool>,
    #[ts(optional)]
    pub tts_speed: Option<f32>,
    #[ts(optional)]
    pub tts_volume: Option<f32>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ReaderStats {
    pub page_index: usize,
    pub total_pages: usize,
    pub tts_progress_pct: f64,
    pub global_progress_pct: f64,
    pub page_time_remaining_secs: f64,
    pub book_time_remaining_secs: f64,
    pub page_word_count: usize,
    pub page_sentence_count: usize,
    pub page_start_percent: f64,
    pub page_end_percent: f64,
    pub words_read_up_to_page_start: usize,
    pub sentences_read_up_to_page_start: usize,
    pub words_read_up_to_page_end: usize,
    pub sentences_read_up_to_page_end: usize,
    pub words_read_up_to_current_position: usize,
    pub sentences_read_up_to_current_position: usize,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ReaderSnapshot {
    pub source_path: String,
    pub source_name: String,
    pub current_page: usize,
    pub total_pages: usize,
    pub text_only_mode: bool,
    pub has_structured_markdown: bool,
    pub pretty_kind: PrettyKind,
    pub images: Vec<ReaderImageRef>,
    pub tts_text_page: String,
    pub reading_markdown_page: Option<String>,
    pub reading_html_page: Option<String>,
    pub page_text: String,
    pub sentences: Vec<String>,
    pub sentence_anchor_map: Vec<Option<usize>>,
    pub highlighted_sentence_idx: Option<usize>,
    pub search_query: String,
    pub search_matches: Vec<usize>,
    pub selected_search_match: Option<usize>,
    pub settings: ReaderSettingsView,
    pub tts: ReaderTtsView,
    pub stats: ReaderStats,
    pub panels: PanelState,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ReaderImageRef {
    pub raw_path: String,
    pub local_path: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PrettyKind {
    None,
    Markdown,
    Html,
}

#[derive(Debug, Clone)]
pub enum SessionCommand {
    GetSnapshot,
    NextPage,
    PrevPage,
    SetPage { page: usize },
    SentenceClick { sentence_idx: usize },
    NextSentence,
    PrevSentence,
    ToggleTextOnly,
    ApplySettings { patch: ReaderSettingsPatch },
    SearchSetQuery { query: String },
    SearchNext,
    SearchPrev,
    TtsPlay,
    TtsPause,
    TtsTogglePlayPause,
    TtsPlayFromPageStart,
    TtsPlayFromHighlight,
    TtsSeekNext,
    TtsSeekPrev,
    TtsRepeatSentence,
    TtsStop,
}

#[derive(Debug, Clone)]
pub struct SessionEvent {
    pub action: &'static str,
    pub snapshot: ReaderSnapshot,
}

#[derive(Debug, Clone)]
pub struct ReaderSession {
    pub source_path: PathBuf,
    source_name: String,
    tts_text: String,
    reading_markdown: Option<String>,
    reading_html: Option<String>,
    has_structured_markdown: bool,
    images: Vec<SessionImage>,
    pub config: config::AppConfig,
    pages: Vec<String>,
    markdown_pages: Vec<String>,
    raw_page_sentences: Vec<Vec<String>>,
    sentence_anchor_maps: Vec<Vec<Option<usize>>>,
    page_word_counts: Vec<usize>,
    page_sentence_counts: Vec<usize>,
    pub current_page: usize,
    highlighted_display_idx: Option<usize>,
    highlighted_audio_idx: Option<usize>,
    pub text_only_mode: bool,
    search_query: String,
    search_matches: Vec<usize>,
    selected_search_match: Option<usize>,
    tts_state: TtsPlaybackState,
    current_plan_page: Option<usize>,
    current_plan: Option<normalizer::PageNormalization>,
}

#[derive(Debug, Clone)]
struct SessionImage {
    raw_path: String,
    path: String,
}

struct MappingTelemetry {
    lookups: AtomicUsize,
    hits: AtomicUsize,
    fallbacks: AtomicUsize,
    missing: AtomicUsize,
    summaries: AtomicUsize,
}

static MAPPING_TELEMETRY: OnceLock<MappingTelemetry> = OnceLock::new();

fn mapping_telemetry() -> &'static MappingTelemetry {
    MAPPING_TELEMETRY.get_or_init(|| MappingTelemetry {
        lookups: AtomicUsize::new(0),
        hits: AtomicUsize::new(0),
        fallbacks: AtomicUsize::new(0),
        missing: AtomicUsize::new(0),
        summaries: AtomicUsize::new(0),
    })
}

fn maybe_log_mapping_summary(path: &Path) {
    const SUMMARY_EVERY: usize = 128;
    let telemetry = mapping_telemetry();
    let lookups = telemetry.lookups.load(Ordering::Relaxed);
    if lookups == 0 || !lookups.is_multiple_of(SUMMARY_EVERY) {
        return;
    }
    let summary_idx = lookups / SUMMARY_EVERY;
    let last = telemetry.summaries.load(Ordering::Relaxed);
    if summary_idx <= last {
        return;
    }
    if telemetry
        .summaries
        .compare_exchange(last, summary_idx, Ordering::Relaxed, Ordering::Relaxed)
        .is_err()
    {
        return;
    }
    let hits = telemetry.hits.load(Ordering::Relaxed);
    let fallbacks = telemetry.fallbacks.load(Ordering::Relaxed);
    let missing = telemetry.missing.load(Ordering::Relaxed);
    let fallback_rate = if lookups == 0 {
        0.0
    } else {
        (fallbacks as f64 / lookups as f64) * 100.0
    };
    tracing::info!(
        path = %path.display(),
        lookups,
        hits,
        fallbacks,
        missing,
        fallback_rate_pct = (fallback_rate * 100.0).round() / 100.0,
        "Sentence mapping telemetry summary"
    );
}

impl ReaderSession {
    pub fn snapshot(
        &mut self,
        panels: PanelState,
        normalizer: &normalizer::TextNormalizer,
    ) -> ReaderSnapshot {
        let sentences = self.current_sentences(normalizer);
        let sentence_anchor_map = self.current_sentence_anchor_map();
        let anchor_hits = sentence_anchor_map
            .iter()
            .filter(|value| value.is_some())
            .count();
        let anchor_missing = sentence_anchor_map.len().saturating_sub(anchor_hits);
        let highlighted_sentence_idx = self.current_highlight_idx();
        let stats = self.stats(normalizer);
        let tts = self.tts_view(normalizer, stats.tts_progress_pct);
        let tts_text_page = self
            .pages
            .get(self.current_page)
            .cloned()
            .unwrap_or_else(String::new);
        let reading_markdown_page = self
            .markdown_pages
            .get(self.current_page)
            .cloned()
            .filter(|value| !value.trim().is_empty());
        let reading_html_page = if self.config.native_html_pretty_enabled {
            self.reading_html
                .as_ref()
                .cloned()
                .filter(|value| !value.trim().is_empty())
        } else {
            None
        };
        let pretty_kind = if reading_html_page.is_some() {
            PrettyKind::Html
        } else if reading_markdown_page.is_some() {
            PrettyKind::Markdown
        } else {
            PrettyKind::None
        };
        tracing::debug!(
            path = %self.source_path.display(),
            page = self.current_page + 1,
            total_pages = self.pages.len(),
            text_only = self.text_only_mode,
            pretty_kind = ?pretty_kind,
            native_html_pretty_enabled = self.config.native_html_pretty_enabled,
            native_html_pagination_mode = ?self.config.native_html_pagination_mode,
            anchor_hits,
            anchor_missing,
            has_markdown = self.reading_markdown.is_some(),
            has_html = self.reading_html.is_some(),
            "Prepared reader snapshot payload"
        );
        ReaderSnapshot {
            source_path: self.source_path_str(),
            source_name: self.source_name.clone(),
            current_page: self.current_page,
            total_pages: self.pages.len(),
            text_only_mode: self.text_only_mode,
            has_structured_markdown: self.has_structured_markdown,
            pretty_kind,
            images: self.current_page_images(),
            tts_text_page: tts_text_page.clone(),
            reading_markdown_page,
            reading_html_page,
            page_text: tts_text_page,
            sentences,
            sentence_anchor_map,
            highlighted_sentence_idx,
            search_query: self.search_query.clone(),
            search_matches: self.search_matches.clone(),
            selected_search_match: self.selected_search_match,
            settings: self.settings_view(),
            tts,
            stats,
            panels,
        }
    }

    pub fn set_search_query(&mut self, query: String, normalizer: &normalizer::TextNormalizer) {
        self.search_query = query;
        self.update_search_matches(normalizer);
        self.apply_selected_match_as_highlight(normalizer);
    }

    pub fn search_next(&mut self, normalizer: &normalizer::TextNormalizer) {
        if self.search_matches.is_empty() {
            self.selected_search_match = None;
            return;
        }
        self.selected_search_match = Some(match self.selected_search_match {
            Some(current) => (current + 1) % self.search_matches.len(),
            None => 0,
        });
        self.apply_selected_match_as_highlight(normalizer);
    }

    pub fn search_prev(&mut self, normalizer: &normalizer::TextNormalizer) {
        if self.search_matches.is_empty() {
            self.selected_search_match = None;
            return;
        }
        self.selected_search_match = Some(match self.selected_search_match {
            Some(0) | None => self.search_matches.len().saturating_sub(1),
            Some(current) => current.saturating_sub(1),
        });
        self.apply_selected_match_as_highlight(normalizer);
    }

    fn current_sentences(&mut self, normalizer: &normalizer::TextNormalizer) -> Vec<String> {
        if self.text_only_mode {
            return self.ensure_current_plan(normalizer).audio_sentences;
        }
        self.raw_page_sentences
            .get(self.current_page)
            .cloned()
            .unwrap_or_default()
    }

    fn current_sentence_anchor_map(&self) -> Vec<Option<usize>> {
        if self.text_only_mode {
            let count = self
                .raw_page_sentences
                .get(self.current_page)
                .map(|v| v.len())
                .unwrap_or(0);
            return (0..count).map(Some).collect();
        }
        self.sentence_anchor_maps
            .get(self.current_page)
            .cloned()
            .or_else(|| {
                crate::cache::load_sentence_anchor_map(&self.source_path, self.current_page)
            })
            .unwrap_or_else(|| {
                let count = self
                    .raw_page_sentences
                    .get(self.current_page)
                    .map(|v| v.len())
                    .unwrap_or(0);
                (0..count).map(Some).collect()
            })
    }

    fn build_sentence_anchor_map_for_page(
        &self,
        page_idx: usize,
        sentence_count: usize,
    ) -> Vec<Option<usize>> {
        if sentence_count == 0 {
            return Vec::new();
        }
        let has_native_html = self.config.native_html_pretty_enabled && self.reading_html.is_some();
        if !has_native_html
            && let Some(cached) =
                crate::cache::load_sentence_anchor_map(&self.source_path, page_idx)
            && cached.len() == sentence_count
        {
            return cached;
        }
        let Some(markdown_page) = self.markdown_pages.get(page_idx) else {
            if self.config.native_html_pretty_enabled
                && let Some(reading_html) = self.reading_html.as_ref()
            {
                let anchor_count = count_html_anchors(reading_html);
                if anchor_count == 0 {
                    tracing::debug!(
                        path = %self.source_path.display(),
                        page = page_idx + 1,
                        sentence_count,
                        "Sentence-anchor map fallback: no HTML anchors detected"
                    );
                    return (0..sentence_count).map(Some).collect();
                }
                tracing::debug!(
                    path = %self.source_path.display(),
                    page = page_idx + 1,
                    sentence_count,
                    anchor_count,
                    source = "html",
                    "Built sentence-anchor map from HTML anchors"
                );
                return proportional_html_anchor_map(
                    &self.page_sentence_counts,
                    page_idx,
                    sentence_count,
                    anchor_count,
                );
            }
            tracing::debug!(
                path = %self.source_path.display(),
                page = page_idx + 1,
                sentence_count,
                "Sentence-anchor map fallback: no markdown/html pretty payload"
            );
            return (0..sentence_count).map(Some).collect();
        };
        let anchor_count = count_markdown_anchors(markdown_page);
        if anchor_count == 0 {
            tracing::debug!(
                path = %self.source_path.display(),
                page = page_idx + 1,
                sentence_count,
                "Sentence-anchor map fallback: no markdown anchors detected"
            );
            return (0..sentence_count).map(Some).collect();
        }
        tracing::debug!(
            path = %self.source_path.display(),
            page = page_idx + 1,
            sentence_count,
            anchor_count,
            source = "markdown",
            "Built sentence-anchor map from markdown anchors"
        );
        proportional_anchor_map(sentence_count, anchor_count)
    }

    fn current_highlight_idx(&self) -> Option<usize> {
        if self.text_only_mode {
            self.highlighted_audio_idx
        } else {
            self.highlighted_display_idx
        }
    }

    fn global_display_idx(&self) -> Option<usize> {
        let page_base: usize = self
            .page_sentence_counts
            .iter()
            .take(self.current_page)
            .sum();
        self.highlighted_display_idx.map(|idx| page_base + idx)
    }

    fn current_page_images(&self) -> Vec<ReaderImageRef> {
        if self.images.is_empty() {
            return Vec::new();
        }
        // Expose all extracted assets so pretty view can resolve image/css
        // references even when page char-offset mapping is imperfect.
        self.images
            .iter()
            .map(|image| ReaderImageRef {
                raw_path: image.raw_path.clone(),
                local_path: image.path.clone(),
            })
            .collect()
    }

    fn tts_view(
        &mut self,
        normalizer: &normalizer::TextNormalizer,
        progress_pct: f64,
    ) -> ReaderTtsView {
        let sentence_count = self.current_audio_sentences(normalizer).len();
        let current_sentence_idx = self.current_audio_highlight_idx(normalizer);
        let can_seek_prev = if let Some(idx) = current_sentence_idx {
            idx > 0 || self.has_sentence_before_current_page()
        } else {
            self.has_sentence_before_current_page()
        };
        let can_seek_next = if let Some(idx) = current_sentence_idx {
            idx + 1 < sentence_count || self.has_sentence_after_current_page()
        } else {
            sentence_count > 0 || self.has_sentence_after_current_page()
        };
        ReaderTtsView {
            state: self.tts_state,
            current_sentence_idx,
            sentence_count,
            can_seek_prev,
            can_seek_next,
            progress_pct: (progress_pct * 1000.0).round() / 1000.0,
        }
    }

    fn has_sentence_before_current_page(&self) -> bool {
        self.page_sentence_counts
            .iter()
            .take(self.current_page)
            .any(|count| *count > 0)
    }

    fn has_sentence_after_current_page(&self) -> bool {
        self.page_sentence_counts
            .iter()
            .skip(self.current_page.saturating_add(1))
            .any(|count| *count > 0)
    }

    fn update_search_matches(&mut self, normalizer: &normalizer::TextNormalizer) {
        self.search_matches.clear();
        self.selected_search_match = None;
        let query = self.search_query.trim().to_string();
        if query.is_empty() {
            return;
        }

        let sentences = self.current_sentences(normalizer);
        let regex = Regex::new(&query).ok();
        let query_lower = query.to_ascii_lowercase();
        for (idx, sentence) in sentences.iter().enumerate() {
            let matched = if let Some(regex) = &regex {
                regex.is_match(sentence)
            } else {
                sentence.to_ascii_lowercase().contains(&query_lower)
            };
            if matched {
                self.search_matches.push(idx);
            }
        }
        if !self.search_matches.is_empty() {
            self.selected_search_match = Some(0);
        }
    }

    fn apply_selected_match_as_highlight(&mut self, normalizer: &normalizer::TextNormalizer) {
        let Some(selected_idx) = self.selected_search_match else {
            return;
        };
        let Some(sentence_idx) = self.search_matches.get(selected_idx).copied() else {
            return;
        };
        self.sentence_click(sentence_idx, normalizer);
    }

    fn stats(&mut self, normalizer: &normalizer::TextNormalizer) -> ReaderStats {
        let page_word_count = self
            .page_word_counts
            .get(self.current_page)
            .copied()
            .unwrap_or_default();
        let page_sentence_count = self
            .page_sentence_counts
            .get(self.current_page)
            .copied()
            .unwrap_or_default();
        let words_before_page: usize = self.page_word_counts.iter().take(self.current_page).sum();
        let sentences_before_page: usize = self
            .page_sentence_counts
            .iter()
            .take(self.current_page)
            .sum();
        let words_up_to_page_end = words_before_page + page_word_count;
        let sentences_up_to_page_end = sentences_before_page + page_sentence_count;
        let total_words = self.page_word_counts.iter().sum::<usize>().max(1);

        let (progress_fraction, sentence_progress_count, sentence_progress_total) =
            if self.text_only_mode {
                let plan = self.ensure_current_plan(normalizer);
                let count = plan.audio_sentences.len();
                let idx = self.highlighted_audio_idx.unwrap_or(0);
                let clamped_idx = idx.min(count.saturating_sub(1));
                let fraction = if count == 0 {
                    0.0
                } else {
                    (clamped_idx + 1) as f64 / count as f64
                };
                (fraction, clamped_idx + 1, count)
            } else {
                let count = page_sentence_count;
                let idx = self.highlighted_display_idx.unwrap_or(0);
                let clamped_idx = idx.min(count.saturating_sub(1));
                let fraction = if count == 0 {
                    0.0
                } else {
                    (clamped_idx + 1) as f64 / count as f64
                };
                (fraction, clamped_idx + 1, count)
            };

        let tts_progress_pct = progress_fraction * 100.0;
        let words_up_to_current_position =
            words_before_page + ((page_word_count as f64) * progress_fraction).round() as usize;
        let sentences_up_to_current_position = sentences_before_page
            + (((page_sentence_count as f64) * progress_fraction).round() as usize).min(
                page_sentence_count
                    .max(sentence_progress_count)
                    .min(sentence_progress_total.max(1)),
            );

        let effective_wpm = (BASE_WPM * self.config.tts_speed as f64).max(40.0);
        let page_total_secs = (page_word_count as f64 / effective_wpm) * 60.0;
        let page_time_remaining_secs = page_total_secs * (1.0 - progress_fraction);
        let book_total_secs = (total_words as f64 / effective_wpm) * 60.0;
        let global_word_progress =
            (words_up_to_current_position as f64 / total_words as f64).clamp(0.0, 1.0);
        let book_time_remaining_secs = book_total_secs * (1.0 - global_word_progress);

        let page_start_percent = (words_before_page as f64 / total_words as f64) * 100.0;
        let page_end_percent = (words_up_to_page_end as f64 / total_words as f64) * 100.0;

        ReaderStats {
            page_index: self.current_page + 1,
            total_pages: self.pages.len(),
            tts_progress_pct,
            global_progress_pct: global_word_progress * 100.0,
            page_time_remaining_secs,
            book_time_remaining_secs,
            page_word_count,
            page_sentence_count,
            page_start_percent,
            page_end_percent,
            words_read_up_to_page_start: words_before_page,
            sentences_read_up_to_page_start: sentences_before_page,
            words_read_up_to_page_end: words_up_to_page_end,
            sentences_read_up_to_page_end: sentences_up_to_page_end,
            words_read_up_to_current_position: words_up_to_current_position,
            sentences_read_up_to_current_position: sentences_up_to_current_position,
        }
    }
}

fn count_markdown_anchors(markdown: &str) -> usize {
    markdown
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| {
            line.starts_with('#')
                || line.starts_with("- ")
                || line.starts_with("* ")
                || line
                    .chars()
                    .next()
                    .map(|ch| ch.is_alphanumeric())
                    .unwrap_or(false)
        })
        .count()
}

fn count_html_anchors(html: &str) -> usize {
    const TAGS: [&str; 13] = [
        "<section",
        "<article",
        "<h1",
        "<h2",
        "<h3",
        "<h4",
        "<h5",
        "<h6",
        "<p",
        "<li",
        "<blockquote",
        "<pre",
        "<img",
    ];
    let lower = html.to_ascii_lowercase();
    TAGS.iter()
        .map(|tag| lower.match_indices(tag).count())
        .sum()
}

fn proportional_anchor_map(sentence_count: usize, anchor_count: usize) -> Vec<Option<usize>> {
    if sentence_count == 0 {
        return Vec::new();
    }
    if anchor_count == 0 {
        return (0..sentence_count).map(Some).collect();
    }
    if sentence_count == 1 {
        return vec![Some(0)];
    }
    (0..sentence_count)
        .map(|idx| {
            let mapped = (idx.saturating_mul(anchor_count)) / sentence_count;
            Some(mapped.min(anchor_count.saturating_sub(1)))
        })
        .collect()
}

fn proportional_html_anchor_map(
    page_sentence_counts: &[usize],
    page_idx: usize,
    sentence_count: usize,
    anchor_count: usize,
) -> Vec<Option<usize>> {
    if sentence_count == 0 {
        return Vec::new();
    }
    if anchor_count == 0 {
        return (0..sentence_count).map(Some).collect();
    }
    let total_sentences: usize = page_sentence_counts.iter().sum();
    if total_sentences == 0 {
        return proportional_anchor_map(sentence_count, anchor_count);
    }
    let global_page_start: usize = page_sentence_counts.iter().take(page_idx).sum();
    (0..sentence_count)
        .map(|local_idx| {
            let global_idx = global_page_start.saturating_add(local_idx);
            let mapped = (global_idx.saturating_mul(anchor_count)) / total_sentences;
            Some(mapped.min(anchor_count.saturating_sub(1)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_session(page_sentences: &[&[&str]]) -> ReaderSession {
        let pages: Vec<String> = page_sentences
            .iter()
            .map(|sentences| sentences.join(" "))
            .collect();
        let raw_page_sentences: Vec<Vec<String>> = page_sentences
            .iter()
            .map(|sentences| {
                sentences
                    .iter()
                    .map(|sentence| sentence.to_string())
                    .collect()
            })
            .collect();
        let page_word_counts: Vec<usize> = pages
            .iter()
            .map(|page| page.split_whitespace().count())
            .collect();
        let page_sentence_counts: Vec<usize> = raw_page_sentences.iter().map(Vec::len).collect();

        ReaderSession {
            source_path: PathBuf::from("/tmp/test.epub"),
            source_name: "test.epub".to_string(),
            tts_text: pages.join("\n\n"),
            reading_markdown: None,
            reading_html: None,
            has_structured_markdown: false,
            images: Vec::new(),
            config: config::AppConfig::default(),
            pages,
            markdown_pages: Vec::new(),
            raw_page_sentences,
            sentence_anchor_maps: Vec::new(),
            page_word_counts,
            page_sentence_counts,
            current_page: 0,
            highlighted_display_idx: Some(0),
            highlighted_audio_idx: None,
            text_only_mode: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            selected_search_match: None,
            tts_state: TtsPlaybackState::Paused,
            current_plan_page: None,
            current_plan: None,
        }
    }

    #[test]
    fn paused_state_is_preserved_when_changing_pages() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B."], &["C.", "D."]]);
        session.current_page = 0;
        session.highlighted_display_idx = Some(1);
        session.tts_state = TtsPlaybackState::Paused;

        session.next_page(&normalizer);

        assert_eq!(session.current_page, 1);
        assert_eq!(session.current_highlight_idx(), Some(0));
        assert_eq!(session.tts_state, TtsPlaybackState::Paused);
    }

    #[test]
    fn paused_state_is_preserved_when_seeking_next_sentence() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B.", "C."]]);
        session.highlighted_display_idx = Some(0);
        session.tts_state = TtsPlaybackState::Paused;

        session.tts_seek_next(&normalizer);

        assert_eq!(session.current_highlight_idx(), Some(1));
        assert_eq!(session.tts_state, TtsPlaybackState::Paused);
    }

    #[test]
    fn paused_state_is_preserved_when_seeking_prev_across_page_boundary() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A."], &["B."]]);
        session.current_page = 1;
        session.highlighted_display_idx = Some(0);
        session.tts_state = TtsPlaybackState::Paused;

        session.tts_seek_prev(&normalizer);

        assert_eq!(session.current_page, 0);
        assert_eq!(session.current_highlight_idx(), Some(0));
        assert_eq!(session.tts_state, TtsPlaybackState::Paused);
    }

    #[test]
    fn sentence_click_keeps_paused_state() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B.", "C."]]);
        session.highlighted_display_idx = Some(0);
        session.tts_state = TtsPlaybackState::Paused;

        session.sentence_click(2, &normalizer);

        assert_eq!(session.current_highlight_idx(), Some(2));
        assert_eq!(session.tts_state, TtsPlaybackState::Paused);
    }

    #[test]
    fn text_only_sentence_click_uses_audio_index_mapping() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&[
            r#"In the word lists of Cheshire, Derbyshire, Lancashire and Yorkshire we find the following terms, all of which took root in the Delaware Valley: abide as in cannot abide it, all out for entirely, apple-pie order to mean very good order, bamboozle for deceive, black and white for writing, blather for empty talk, boggle for take fright, brat for child, budge for move, burying for funeral, by golly as an expletive, by gum for another expletive."#,
        ]]);
        session.tts_state = TtsPlaybackState::Paused;
        session.toggle_text_only(&normalizer);
        let audio_count = session.current_sentences(&normalizer).len();
        assert!(
            audio_count > 1,
            "expected long sentence to split into multiple audio chunks"
        );

        let target_audio_idx = audio_count - 1;
        session.sentence_click(target_audio_idx, &normalizer);

        assert_eq!(session.highlighted_audio_idx, Some(target_audio_idx));
        assert_eq!(session.highlighted_display_idx, Some(0));
        assert_eq!(session.current_highlight_idx(), Some(target_audio_idx));
        assert_eq!(session.tts_state, TtsPlaybackState::Paused);
    }

    #[test]
    fn apply_settings_patch_clamps_pause_speed_and_volume() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B."]]);

        session.apply_settings_patch(
            ReaderSettingsPatch {
                theme: None,
                day_highlight: None,
                night_highlight: None,
                font_family: None,
                font_weight: None,
                font_size: None,
                line_spacing: None,
                word_spacing: None,
                letter_spacing: None,
                margin_horizontal: None,
                margin_vertical: None,
                lines_per_page: None,
                pause_after_sentence: Some(0.056),
                auto_scroll_tts: None,
                center_spoken_sentence: None,
                tts_speed: Some(4.9),
                tts_volume: Some(-1.0),
            },
            &normalizer,
        );

        assert!((session.config.pause_after_sentence - 0.06).abs() < f32::EPSILON);
        assert!((session.config.tts_speed - 4.0).abs() < f32::EPSILON);
        assert!((session.config.tts_volume - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn tts_stop_forces_idle_state() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B."]]);
        session.tts_play(&normalizer);
        assert_eq!(session.tts_state, TtsPlaybackState::Playing);

        session.tts_stop();

        assert_eq!(session.tts_state, TtsPlaybackState::Idle);
    }

    #[test]
    fn restore_bookmark_position_preserves_page_and_sentence() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B."], &["C.", "D.", "E."], &["F."]]);

        let bookmark = crate::cache::Bookmark {
            page: 1,
            sentence_idx: Some(2),
            sentence_text: None,
            scroll_y: 0.0,
        };
        session.restore_bookmark_position(&bookmark, &normalizer);

        assert_eq!(session.current_page, 1);
        assert_eq!(session.highlighted_display_idx, Some(2));
    }

    #[test]
    fn session_command_dispatch_emits_expected_action_and_snapshot() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B."], &["C.", "D."]]);

        let event =
            session.apply_command(SessionCommand::NextPage, PanelState::default(), &normalizer);

        assert_eq!(event.action, "reader_next_page");
        assert_eq!(event.snapshot.current_page, 1);
        assert_eq!(event.snapshot.highlighted_sentence_idx, Some(0));
    }

    #[test]
    fn session_command_dispatch_preserves_paused_tts_state() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B."], &["C.", "D."]]);
        session.tts_state = TtsPlaybackState::Paused;

        let event =
            session.apply_command(SessionCommand::NextPage, PanelState::default(), &normalizer);

        assert_eq!(session.tts_state, TtsPlaybackState::Paused);
        assert_eq!(event.snapshot.tts.state, TtsPlaybackState::Paused);
    }

    #[test]
    fn session_command_dispatch_applies_settings_patch_with_rounding() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B."]]);

        let event = session.apply_command(
            SessionCommand::ApplySettings {
                patch: ReaderSettingsPatch {
                    theme: None,
                    day_highlight: None,
                    night_highlight: None,
                    font_family: None,
                    font_weight: None,
                    font_size: None,
                    line_spacing: None,
                    word_spacing: None,
                    letter_spacing: None,
                    margin_horizontal: None,
                    margin_vertical: None,
                    lines_per_page: None,
                    pause_after_sentence: Some(0.056),
                    auto_scroll_tts: None,
                    center_spoken_sentence: None,
                    tts_speed: Some(2.5),
                    tts_volume: Some(1.3),
                },
            },
            PanelState::default(),
            &normalizer,
        );

        assert_eq!(event.action, "reader_apply_settings");
        assert!((event.snapshot.settings.pause_after_sentence - 0.06).abs() < f32::EPSILON);
        assert!((event.snapshot.settings.tts_speed - 2.5).abs() < f32::EPSILON);
        assert!((event.snapshot.settings.tts_volume - 1.3).abs() < f32::EPSILON);
    }

    #[test]
    fn tts_highlight_continuity_is_preserved_across_view_toggles() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B.", "C."]]);
        session.highlighted_display_idx = Some(2);

        session.toggle_text_only(&normalizer);
        assert_eq!(session.current_highlight_idx(), Some(2));

        session.toggle_text_only(&normalizer);
        assert_eq!(session.current_highlight_idx(), Some(2));
    }

    #[test]
    fn markdown_anchor_count_detects_blocks() {
        let markdown = "# Title\n\nParagraph one.\n\n- Item one\n- Item two\n\n## Next";
        assert_eq!(count_markdown_anchors(markdown), 5);
    }

    #[test]
    fn html_anchor_count_detects_structural_elements() {
        let html = "<section><h1>A</h1><p>One</p><ul><li>x</li><li>y</li></ul><img src=\"a.png\"/></section>";
        assert_eq!(count_html_anchors(html), 6);
    }

    #[test]
    fn proportional_anchor_map_spreads_sentences_across_anchors() {
        let map = proportional_anchor_map(5, 3);
        assert_eq!(map, vec![Some(0), Some(0), Some(1), Some(1), Some(2)]);
    }

    #[test]
    fn proportional_html_anchor_map_uses_global_sentence_position() {
        let page_sentence_counts = vec![2, 2, 2];
        let page1 = proportional_html_anchor_map(&page_sentence_counts, 0, 2, 6);
        let page2 = proportional_html_anchor_map(&page_sentence_counts, 1, 2, 6);
        let page3 = proportional_html_anchor_map(&page_sentence_counts, 2, 2, 6);
        assert_eq!(page1, vec![Some(0), Some(1)]);
        assert_eq!(page2, vec![Some(2), Some(3)]);
        assert_eq!(page3, vec![Some(4), Some(5)]);
    }

    #[test]
    fn html_sentence_anchor_map_falls_back_to_identity_when_no_pretty_anchors() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B.", "C."]]);
        session.reading_html = Some("<div>   </div>".to_string());
        session.repaginate(&normalizer, None);
        let sentence_count = session
            .raw_page_sentences
            .get(session.current_page)
            .map(|v| v.len())
            .unwrap_or(0);
        assert_eq!(
            session.current_sentence_anchor_map(),
            (0..sentence_count).map(Some).collect::<Vec<_>>()
        );
    }

    #[test]
    fn build_sentence_anchor_map_for_native_html_uses_current_page_sentence_counts() {
        let mut session = build_test_session(&[&["A1.", "A2."], &["B1.", "B2."], &["C1.", "C2."]]);
        session.config.native_html_pretty_enabled = true;
        session.reading_html = Some(
            "<p>A1.</p><p>A2.</p><p>B1.</p><p>B2.</p><p>C1.</p><p>C2.</p>".to_string(),
        );

        assert_eq!(
            session.build_sentence_anchor_map_for_page(0, 2),
            vec![Some(0), Some(1)]
        );
        assert_eq!(
            session.build_sentence_anchor_map_for_page(1, 2),
            vec![Some(2), Some(3)]
        );
        assert_eq!(
            session.build_sentence_anchor_map_for_page(2, 2),
            vec![Some(4), Some(5)]
        );
    }
}
