mod bookmark;
mod constants;
mod reader;
mod tts;
mod ui;

use crate::cache::{Bookmark, list_recent_books, save_epub_config};
use crate::calibre::{CalibreColumn, CalibreConfig};
use crate::config::{AppConfig, FontFamily, FontWeight, HighlightColor, ThemeMode};
use crate::epub_loader::LoadedBook;
use crate::normalizer::TextNormalizer;
use crate::pagination::{MAX_LINES_PER_PAGE, MIN_LINES_PER_PAGE, paginate};
use crate::text_utils::split_sentences;
use crate::tts::TtsEngine;
use iced::font::{Family, Weight};
use iced::widget::scrollable::RelativeOffset;
use iced::{Color, Font, Task};
use regex::Regex;
use std::path::PathBuf;
use std::time::Instant;

use super::messages::{Component, Message, NumericSetting};

pub(in crate::app) use bookmark::{BookmarkState, TextOnlyPreview};
pub(crate) use constants::*;
pub(in crate::app) use reader::ReaderState;
pub(crate) use tts::TtsLifecycle;
pub(in crate::app) use tts::{PendingAppendBatch, TtsState};
pub(in crate::app) use ui::{CalibreState, RecentState, SearchState};

fn tts_engine_from_config(config: &AppConfig) -> Option<TtsEngine> {
    TtsEngine::new(
        config.tts_model_path.clone().into(),
        config.tts_espeak_path.clone().into(),
    )
    .ok()
}
/// Core application state composed of sub-models.
pub struct App {
    pub(super) starter_mode: bool,
    pub(super) show_stats: bool,
    pub(super) active_numeric_setting: Option<NumericSetting>,
    pub(super) numeric_setting_input: String,
    pub(super) reader: ReaderState,
    pub(super) tts: TtsState,
    pub(super) bookmark: BookmarkState,
    pub(super) config: AppConfig,
    pub(super) epub_path: PathBuf,
    pub(super) normalizer: TextNormalizer,
    pub(super) text_only_mode: bool,
    pub(super) text_only_preview: Option<TextOnlyPreview>,
    pub(super) search: SearchState,
    pub(super) recent: RecentState,
    pub(super) calibre: CalibreState,
    pub(super) open_path_input: String,
    pub(super) book_loading: bool,
    pub(super) book_loading_error: Option<String>,
    pub(super) pending_window_resize: bool,
    pub(super) pending_window_move: bool,
    pub(super) window_geometry_changed_at: Option<Instant>,
}

impl App {
    /// Re-run pagination after a state change (e.g., font size).
    pub(super) fn repaginate(&mut self) {
        self.reader.pages = paginate(
            &self.reader.full_text,
            self.config.font_size,
            self.config.lines_per_page,
        );
        self.text_only_preview = None;
        if self.reader.pages.is_empty() {
            self.reader
                .pages
                .push(String::from("This EPUB appears to contain no text."));
        }
        self.reader.set_page_clamped(self.reader.current_page);
        self.reader.page_sentences = self
            .reader
            .pages
            .iter()
            .map(|page| split_sentences(page))
            .collect();
        self.reader.page_sentence_counts =
            self.reader.page_sentences.iter().map(Vec::len).collect();
        tracing::debug!(
            pages = self.reader.pages.len(),
            font_size = self.config.font_size,
            lines_per_page = self.config.lines_per_page,
            "Repaginated content"
        );
    }

    pub(super) fn stop_playback(&mut self) {
        if let Some(engine) = &self.tts.engine {
            engine.cancel_preparation();
        }
        if let Some(playback) = self.tts.playback.take() {
            playback.stop();
        }
        self.tts.lifecycle = TtsLifecycle::Idle;
        self.tts.started_at = None;
        self.tts.total_sources = 0;
        self.tts.pending_append = false;
        self.tts.pending_append_batch = None;
    }

    pub(super) fn current_font(&self) -> Font {
        let family = match self.config.font_family {
            FontFamily::Sans => Family::SansSerif,
            FontFamily::Serif => Family::Serif,
            FontFamily::Monospace => Family::Monospace,
            FontFamily::Lexend => Family::Name("Lexend"),
            FontFamily::FiraCode => Family::Name("Fira Code"),
            FontFamily::AtkinsonHyperlegible => Family::Name("Atkinson Hyperlegible"),
            FontFamily::AtkinsonHyperlegibleNext => Family::Name("Atkinson Hyperlegible Next"),
            FontFamily::LexicaUltralegible => Family::Name("Lexica Ultralegible"),
            FontFamily::Courier => Family::Name("Courier"),
            FontFamily::FrankGothic => Family::Name("Frank Gothic"),
            FontFamily::Hermit => Family::Name("Hermit"),
            FontFamily::Hasklug => Family::Name("Hasklug"),
            FontFamily::NotoSans => Family::Name("Noto Sans"),
        };

        Font {
            family,
            weight: self.config.font_weight.to_weight(),
            ..Font::DEFAULT
        }
    }

    pub(super) fn formatted_page_content(&self) -> String {
        let base = self
            .reader
            .pages
            .get(self.reader.current_page)
            .map(String::as_str)
            .unwrap_or("")
            .to_string();

        if self.config.word_spacing == 0 && self.config.letter_spacing == 0 {
            return base;
        }

        let word_gap = " ".repeat((self.config.word_spacing as usize).saturating_add(1));
        let letter_gap = " ".repeat(self.config.letter_spacing as usize);

        let mut output = String::with_capacity(base.len() + 16);

        for ch in base.chars() {
            Self::push_formatted_char(ch, &word_gap, &letter_gap, &mut output);
        }

        output
    }

    pub(super) fn format_sentence_for_display(&self, sentence: &str) -> String {
        if self.config.word_spacing == 0 && self.config.letter_spacing == 0 {
            return sentence.to_string();
        }

        let word_gap = " ".repeat((self.config.word_spacing as usize).saturating_add(1));
        let letter_gap = " ".repeat(self.config.letter_spacing as usize);
        let mut output = String::with_capacity(sentence.len() + 8);

        for ch in sentence.chars() {
            Self::push_formatted_char(ch, &word_gap, &letter_gap, &mut output);
        }

        output
    }

    pub(super) fn raw_sentences_for_page(&self, page: usize) -> Vec<String> {
        self.reader
            .page_sentences
            .get(page)
            .cloned()
            .unwrap_or_default()
    }

    pub(super) fn find_audio_start_for_display_sentence(
        &self,
        display_idx: usize,
    ) -> Option<usize> {
        if self.tts.display_to_audio.is_empty() {
            return None;
        }
        let clamped = display_idx.min(self.tts.display_to_audio.len().saturating_sub(1));
        self.tts
            .display_to_audio
            .iter()
            .skip(clamped)
            .find_map(|mapped| *mapped)
            .or_else(|| {
                self.tts
                    .display_to_audio
                    .iter()
                    .take(clamped + 1)
                    .rev()
                    .find_map(|mapped| *mapped)
            })
    }

    pub(super) fn display_index_for_audio_sentence(&self, audio_idx: usize) -> Option<usize> {
        self.tts.audio_to_display.get(audio_idx).copied()
    }

    pub(super) fn display_sentences_for_current_page(&self) -> Vec<String> {
        if self.config.word_spacing == 0 && self.config.letter_spacing == 0 {
            return self.raw_sentences_for_page(self.reader.current_page);
        }
        self.raw_sentences_for_page(self.reader.current_page)
            .into_iter()
            .map(|sentence| self.format_sentence_for_display(&sentence))
            .collect()
    }

    pub(super) fn text_only_preview_for_current_page(&self) -> Option<&TextOnlyPreview> {
        self.text_only_preview
            .as_ref()
            .filter(|preview| preview.page == self.reader.current_page)
    }

    pub(super) fn text_only_highlight_audio_idx_for_current_page(&self) -> Option<usize> {
        let display_idx = self.tts.current_sentence_idx?;
        let preview = self.text_only_preview_for_current_page()?;
        Self::nearest_audio_idx_for_display(display_idx, &preview.display_to_audio)
            .map(|idx| idx.min(preview.audio_sentences.len().saturating_sub(1)))
    }

    pub(super) fn text_only_display_idx_for_audio_idx(&self, audio_idx: usize) -> Option<usize> {
        self.text_only_preview_for_current_page()
            .and_then(|preview| preview.audio_to_display.get(audio_idx).copied())
    }

    pub(super) fn search_sentences_for_current_page(&self) -> Vec<String> {
        if self.text_only_mode {
            return self
                .text_only_preview_for_current_page()
                .map(|preview| preview.audio_sentences.clone())
                .unwrap_or_default();
        }
        self.display_sentences_for_current_page()
    }

    pub(super) fn refresh_recent_books(&mut self) {
        self.recent.books = list_recent_books(64);
    }

    pub(super) fn ensure_text_only_preview_for_page(&mut self, page: usize) {
        if self
            .text_only_preview
            .as_ref()
            .map(|preview| preview.page == page)
            .unwrap_or(false)
        {
            return;
        }

        let display_sentences = self.raw_sentences_for_page(page);
        let preview = if display_sentences.is_empty() {
            TextOnlyPreview {
                page,
                audio_sentences: vec!["No textual content on this page.".to_string()],
                display_to_audio: Vec::new(),
                audio_to_display: Vec::new(),
            }
        } else {
            let plan = self
                .normalizer
                .plan_page_cached(&self.epub_path, page, &display_sentences);
            if plan.audio_sentences.is_empty() {
                TextOnlyPreview {
                    page,
                    audio_sentences: vec![
                        "No speakable text remains on this page after normalization.".to_string(),
                    ],
                    display_to_audio: plan.display_to_audio,
                    audio_to_display: plan.audio_to_display,
                }
            } else {
                TextOnlyPreview {
                    page,
                    audio_sentences: plan.audio_sentences,
                    display_to_audio: plan.display_to_audio,
                    audio_to_display: plan.audio_to_display,
                }
            }
        };
        self.text_only_preview = Some(preview);
    }

    pub(super) fn image_assigned_page(&self, image_idx: usize) -> usize {
        if self.reader.pages.is_empty() || self.reader.images.is_empty() {
            return 0;
        }
        let total_pages = self.reader.pages.len();
        let image_count = self.reader.images.len();
        let page = image_idx.saturating_mul(total_pages) / image_count;
        page.min(total_pages.saturating_sub(1))
    }

    pub(super) fn sentence_count_for_page(&self, page: usize) -> usize {
        self.reader
            .page_sentence_counts
            .get(page)
            .copied()
            .unwrap_or_else(|| {
                self.reader
                    .pages
                    .get(page)
                    .map(|p| split_sentences(p).len())
                    .unwrap_or(0)
            })
    }

    pub(super) fn highlight_color(&self) -> Color {
        let base = if matches!(self.config.theme, ThemeMode::Night) {
            self.config.night_highlight
        } else {
            self.config.day_highlight
        };
        Color {
            r: base.r,
            g: base.g,
            b: base.b,
            a: base.a,
        }
    }

    fn push_formatted_char(ch: char, word_gap: &str, letter_gap: &str, output: &mut String) {
        match ch {
            ' ' => output.push_str(word_gap),
            '\n' => output.push('\n'),
            _ => {
                output.push(ch);
                if !letter_gap.is_empty() {
                    output.push_str(letter_gap);
                }
            }
        }
    }

    fn nearest_audio_idx_for_display(
        display_idx: usize,
        display_to_audio: &[Option<usize>],
    ) -> Option<usize> {
        if display_to_audio.is_empty() {
            return None;
        }
        let clamped = display_idx.min(display_to_audio.len().saturating_sub(1));
        display_to_audio
            .iter()
            .skip(clamped)
            .find_map(|mapped| *mapped)
            .or_else(|| {
                display_to_audio
                    .iter()
                    .take(clamped + 1)
                    .rev()
                    .find_map(|mapped| *mapped)
            })
    }

    pub(super) fn save_epub_config(&self) {
        if self.starter_mode {
            return;
        }
        save_epub_config(&self.epub_path, &self.config);
    }

    pub(super) fn apply_loaded_book(
        &mut self,
        book: LoadedBook,
        mut config: AppConfig,
        epub_path: PathBuf,
        bookmark: Option<Bookmark>,
    ) -> Option<RelativeOffset> {
        clamp_config(&mut config);

        self.stop_playback();
        self.starter_mode = false;
        self.book_loading = false;
        self.book_loading_error = None;
        self.pending_window_resize = false;
        self.pending_window_move = false;
        self.window_geometry_changed_at = None;
        self.text_only_mode = false;
        self.text_only_preview = None;
        self.open_path_input.clear();
        self.search.visible = false;
        self.search.query.clear();
        self.search.error = None;
        self.search.matches.clear();
        self.search.selected_match = 0;
        self.recent.visible = false;
        self.calibre.visible = false;
        self.calibre.error = None;
        self.show_stats = false;
        self.active_numeric_setting = None;
        self.numeric_setting_input.clear();
        self.config = config;
        self.epub_path = epub_path;
        self.reader.full_text = book.text;
        self.reader.images = book.images;
        self.reader.set_page_clamped(0);
        self.bookmark.last_scroll_offset = RelativeOffset::START;
        self.bookmark.viewport_fraction = 0.25;
        self.bookmark.pending_sentence_snap = None;
        self.bookmark.defer_sentence_snap_until_scroll = false;
        self.bookmark.last_scroll_bookmark_save_at = None;
        self.tts = TtsState::new(tts_engine_from_config(&self.config));

        self.repaginate();
        let mut initial_scroll: Option<RelativeOffset> = None;
        if let Some(bookmark) = bookmark {
            self.reader.set_page_clamped(bookmark.page);
            let scroll_y = if bookmark.scroll_y.is_finite() {
                bookmark.scroll_y.clamp(0.0, 1.0)
            } else {
                0.0
            };
            self.bookmark.last_scroll_offset = RelativeOffset {
                x: 0.0,
                y: scroll_y,
            };

            self.tts.last_sentences = self.raw_sentences_for_page(self.reader.current_page);
            let restored_idx = bookmark
                .sentence_text
                .as_ref()
                .and_then(|target| self.tts.last_sentences.iter().position(|s| s == target))
                .or(bookmark.sentence_idx)
                .map(|idx| idx.min(self.tts.last_sentences.len().saturating_sub(1)));
            if let Some(idx) = restored_idx {
                self.tts
                    .set_current_sentence_clamped(idx, self.tts.last_sentences.len());
            } else {
                self.tts.current_sentence_idx = None;
            }
            self.bookmark.pending_sentence_snap = self.tts.current_sentence_idx;

            if self.bookmark.last_scroll_offset.y > 0.0 {
                initial_scroll = Some(self.bookmark.last_scroll_offset);
            } else if let Some(idx) = restored_idx {
                if let Some(offset) = self.scroll_offset_for_sentence(idx) {
                    self.bookmark.last_scroll_offset = offset;
                    initial_scroll = Some(offset);
                }
            }

            tracing::info!(
                page = self.reader.current_page + 1,
                sentence_idx = ?self.tts.current_sentence_idx,
                scroll = self.bookmark.last_scroll_offset.y,
                "Restored bookmark from cache"
            );
        } else {
            tracing::info!("Starting from first page");
        }

        tracing::info!(
            path = %self.epub_path.display(),
            font_size = self.config.font_size,
            night_mode = matches!(self.config.theme, ThemeMode::Night),
            "Loaded book into reader state"
        );

        self.update_search_matches();
        initial_scroll
    }

    pub(super) fn update_search_matches(&mut self) {
        let query = self.search.query.trim();
        if query.is_empty() {
            self.search.error = None;
            self.search.matches.clear();
            self.search.selected_match = 0;
            return;
        }

        let regex = match Regex::new(query) {
            Ok(regex) => regex,
            Err(err) => {
                self.search.error = Some(err.to_string());
                self.search.matches.clear();
                self.search.selected_match = 0;
                return;
            }
        };

        self.search.error = None;
        let sentences = self.search_sentences_for_current_page();
        self.search.matches = sentences
            .iter()
            .enumerate()
            .filter_map(|(idx, sentence)| regex.is_match(sentence).then_some(idx))
            .collect();
        if self.search.matches.is_empty() {
            self.search.selected_match = 0;
        } else {
            self.search.selected_match = self
                .search
                .selected_match
                .min(self.search.matches.len().saturating_sub(1));
        }
    }

    pub(super) fn display_idx_for_search_sentence_idx(&self, sentence_idx: usize) -> Option<usize> {
        if self.text_only_mode {
            self.text_only_display_idx_for_audio_idx(sentence_idx)
        } else {
            let count = self.sentence_count_for_page(self.reader.current_page);
            if count == 0 {
                None
            } else {
                Some(sentence_idx.min(count.saturating_sub(1)))
            }
        }
    }

    pub(super) fn selected_search_sentence_idx(&self) -> Option<usize> {
        if self.search.matches.is_empty() {
            None
        } else {
            self.search
                .matches
                .get(self.search.selected_match)
                .copied()
                .or_else(|| self.search.matches.first().copied())
        }
    }

    pub(super) fn bootstrap(
        book: LoadedBook,
        mut config: AppConfig,
        epub_path: PathBuf,
        bookmark: Option<Bookmark>,
    ) -> (App, Task<Message>) {
        clamp_config(&mut config);
        let mut app = App {
            starter_mode: false,
            show_stats: false,
            active_numeric_setting: None,
            numeric_setting_input: String::new(),
            reader: ReaderState {
                pages: Vec::new(),
                page_sentences: Vec::new(),
                page_sentence_counts: Vec::new(),
                full_text: book.text,
                images: book.images,
                current_page: 0,
            },
            bookmark: BookmarkState {
                last_scroll_offset: RelativeOffset::START,
                viewport_fraction: 0.25,
                viewport_width: 0.0,
                viewport_height: 0.0,
                content_width: 0.0,
                content_height: 0.0,
                pending_sentence_snap: None,
                defer_sentence_snap_until_scroll: false,
                last_scroll_bookmark_save_at: None,
            },
            epub_path,
            tts: TtsState::new(tts_engine_from_config(&config)),
            config,
            normalizer: TextNormalizer::load_default(),
            text_only_mode: false,
            text_only_preview: None,
            search: SearchState {
                visible: false,
                query: String::new(),
                error: None,
                matches: Vec::new(),
                selected_match: 0,
            },
            recent: RecentState {
                visible: false,
                books: list_recent_books(64),
            },
            calibre: CalibreState {
                visible: false,
                loading: false,
                error: None,
                books: Vec::new(),
                search_query: String::new(),
                config: CalibreConfig::load_default(),
                sort_column: CalibreColumn::Title,
                sort_desc: false,
            },
            open_path_input: String::new(),
            book_loading: false,
            book_loading_error: None,
            pending_window_resize: false,
            pending_window_move: false,
            window_geometry_changed_at: None,
        };

        app.repaginate();
        let mut init_task = Task::none();
        match bookmark {
            Some(bookmark) => {
                app.reader.set_page_clamped(bookmark.page);
                let scroll_y = if bookmark.scroll_y.is_finite() {
                    bookmark.scroll_y.clamp(0.0, 1.0)
                } else {
                    0.0
                };
                app.bookmark.last_scroll_offset = RelativeOffset {
                    x: 0.0,
                    y: scroll_y,
                };

                app.tts.last_sentences = app.raw_sentences_for_page(app.reader.current_page);
                let restored_idx = bookmark
                    .sentence_text
                    .as_ref()
                    .and_then(|target| app.tts.last_sentences.iter().position(|s| s == target))
                    .or(bookmark.sentence_idx)
                    .map(|idx| idx.min(app.tts.last_sentences.len().saturating_sub(1)));
                if let Some(idx) = restored_idx {
                    app.tts
                        .set_current_sentence_clamped(idx, app.tts.last_sentences.len());
                } else {
                    app.tts.current_sentence_idx = None;
                }
                app.bookmark.pending_sentence_snap = app.tts.current_sentence_idx;

                if let Some(idx) = app.tts.current_sentence_idx {
                    // Prefer persisted scroll for initial layout, then do a one-time
                    // geometry-aware sentence snap after the first viewport update.
                    if app.bookmark.last_scroll_offset.y > 0.0 {
                        init_task = iced::widget::scrollable::snap_to(
                            TEXT_SCROLL_ID.clone(),
                            app.bookmark.last_scroll_offset,
                        );
                    } else if let Some(offset) = app.scroll_offset_for_sentence(idx) {
                        app.bookmark.last_scroll_offset = offset;
                        init_task =
                            iced::widget::scrollable::snap_to(TEXT_SCROLL_ID.clone(), offset);
                    }
                } else if app.bookmark.last_scroll_offset.y > 0.0 {
                    init_task = iced::widget::scrollable::snap_to(
                        TEXT_SCROLL_ID.clone(),
                        app.bookmark.last_scroll_offset,
                    );
                }
                tracing::info!(
                    page = app.reader.current_page + 1,
                    sentence_idx = ?app.tts.current_sentence_idx,
                    scroll = app.bookmark.last_scroll_offset.y,
                    "Restored bookmark from cache"
                );
            }
            None => {
                tracing::info!("Starting from first page");
            }
        };
        tracing::info!(
            font_size = app.config.font_size,
            night_mode = matches!(app.config.theme, ThemeMode::Night),
            "Initialized app state"
        );

        app.update_search_matches();

        (app, init_task)
    }

    pub(super) fn bootstrap_starter(mut config: AppConfig) -> (App, Task<Message>) {
        clamp_config(&mut config);
        let app = App {
            starter_mode: true,
            show_stats: false,
            active_numeric_setting: None,
            numeric_setting_input: String::new(),
            reader: ReaderState {
                pages: vec![String::new()],
                page_sentences: vec![Vec::new()],
                page_sentence_counts: vec![0],
                full_text: String::new(),
                images: Vec::new(),
                current_page: 0,
            },
            tts: TtsState::new(None),
            bookmark: BookmarkState {
                last_scroll_offset: RelativeOffset::START,
                viewport_fraction: 0.25,
                viewport_width: 0.0,
                viewport_height: 0.0,
                content_width: 0.0,
                content_height: 0.0,
                pending_sentence_snap: None,
                defer_sentence_snap_until_scroll: false,
                last_scroll_bookmark_save_at: None,
            },
            config,
            epub_path: PathBuf::new(),
            normalizer: TextNormalizer::load_default(),
            text_only_mode: false,
            text_only_preview: None,
            search: SearchState {
                visible: false,
                query: String::new(),
                error: None,
                matches: Vec::new(),
                selected_match: 0,
            },
            recent: RecentState {
                visible: true,
                books: list_recent_books(64),
            },
            calibre: CalibreState {
                visible: true,
                loading: false,
                error: None,
                books: Vec::new(),
                search_query: String::new(),
                config: CalibreConfig::load_default(),
                sort_column: CalibreColumn::Title,
                sort_desc: false,
            },
            open_path_input: String::new(),
            book_loading: false,
            book_loading_error: None,
            pending_window_resize: false,
            pending_window_move: false,
            window_geometry_changed_at: None,
        };

        let init_task = if app.calibre.config.enabled {
            Task::done(Message::PrimeCalibreLoad)
        } else {
            Task::none()
        };
        (app, init_task)
    }
}

impl FontWeight {
    pub(super) fn to_weight(self) -> Weight {
        match self {
            FontWeight::Light => Weight::Light,
            FontWeight::Normal => Weight::Normal,
            FontWeight::Bold => Weight::Bold,
        }
    }
}

pub(crate) fn apply_component(
    mut color: HighlightColor,
    component: Component,
    value: f32,
) -> HighlightColor {
    let clamped = value.clamp(0.0, 1.0);
    match component {
        Component::R => color.r = clamped,
        Component::G => color.g = clamped,
        Component::B => color.b = clamped,
        Component::A => color.a = clamped,
    }
    color
}

fn clamp_config(config: &mut AppConfig) {
    use crate::pagination::{MAX_FONT_SIZE, MIN_FONT_SIZE};

    fn normalize_key_binding(value: &mut String, fallback: String) {
        let normalized = value.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            *value = fallback;
        } else {
            *value = normalized;
        }
    }

    config.font_size = config.font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
    config.line_spacing = config.line_spacing.clamp(0.8, 2.5);
    config.margin_horizontal = config.margin_horizontal.min(MAX_HORIZONTAL_MARGIN);
    config.margin_vertical = config.margin_vertical.min(MAX_VERTICAL_MARGIN);
    config.window_width = config.window_width.clamp(320.0, 7680.0);
    config.window_height = config.window_height.clamp(240.0, 4320.0);
    config.window_pos_x = config.window_pos_x.filter(|v| v.is_finite());
    config.window_pos_y = config.window_pos_y.filter(|v| v.is_finite());
    config.word_spacing = config.word_spacing.min(MAX_WORD_SPACING);
    config.letter_spacing = config.letter_spacing.min(MAX_LETTER_SPACING);
    config.lines_per_page = config
        .lines_per_page
        .clamp(MIN_LINES_PER_PAGE, MAX_LINES_PER_PAGE);
    config.pause_after_sentence = config.pause_after_sentence.clamp(0.0, 2.0);
    config.tts_speed = config.tts_speed.clamp(MIN_TTS_SPEED, MAX_TTS_SPEED);
    config.tts_volume = config.tts_volume.clamp(MIN_TTS_VOLUME, MAX_TTS_VOLUME);
    config.tts_threads = config.tts_threads.max(1);
    config.tts_progress_log_interval_secs = config.tts_progress_log_interval_secs.clamp(0.1, 60.0);
    normalize_key_binding(&mut config.key_toggle_play_pause, "space".to_string());
    normalize_key_binding(&mut config.key_safe_quit, "q".to_string());
    normalize_key_binding(&mut config.key_next_sentence, "f".to_string());
    normalize_key_binding(&mut config.key_prev_sentence, "s".to_string());
    normalize_key_binding(&mut config.key_repeat_sentence, "r".to_string());
    normalize_key_binding(&mut config.key_toggle_search, "ctrl+f".to_string());
    normalize_key_binding(&mut config.key_toggle_settings, "ctrl+t".to_string());
    normalize_key_binding(&mut config.key_toggle_stats, "ctrl+g".to_string());
    normalize_key_binding(&mut config.key_toggle_tts, "ctrl+y".to_string());
}
