use crate::cache::{Bookmark, save_epub_config};
use crate::config::{AppConfig, FontFamily, FontWeight, HighlightColor, ThemeMode};
use crate::epub_loader::{BookImage, LoadedBook};
use crate::normalizer::TextNormalizer;
use crate::pagination::{MAX_LINES_PER_PAGE, MIN_LINES_PER_PAGE, paginate};
use crate::text_utils::split_sentences;
use crate::tts::{TtsEngine, TtsPlayback};
use iced::font::{Family, Weight};
use iced::widget::scrollable::{Id as ScrollId, RelativeOffset};
use iced::{Color, Font, Task};
use once_cell::sync::Lazy;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::messages::{Component, Message};

/// Limits and defaults for reader controls.
pub(crate) const MAX_MARGIN: u16 = 100;
pub(crate) const MAX_WORD_SPACING: u32 = 5;
pub(crate) const MAX_LETTER_SPACING: u32 = 3;
pub(crate) const MIN_TTS_SPEED: f32 = 0.1;
pub(crate) const MAX_TTS_SPEED: f32 = 3.0;
pub(crate) const MIN_TTS_VOLUME: f32 = 0.0;
pub(crate) const MAX_TTS_VOLUME: f32 = 2.0;
pub(crate) static TEXT_SCROLL_ID: Lazy<ScrollId> = Lazy::new(|| ScrollId::new("text-scroll"));
pub(crate) const FONT_FAMILIES: [FontFamily; 13] = [
    FontFamily::Sans,
    FontFamily::Serif,
    FontFamily::Monospace,
    FontFamily::Lexend,
    FontFamily::FiraCode,
    FontFamily::AtkinsonHyperlegible,
    FontFamily::AtkinsonHyperlegibleNext,
    FontFamily::LexicaUltralegible,
    FontFamily::Courier,
    FontFamily::FrankGothic,
    FontFamily::Hermit,
    FontFamily::Hasklug,
    FontFamily::NotoSans,
];
pub(crate) const FONT_WEIGHTS: [FontWeight; 3] =
    [FontWeight::Light, FontWeight::Normal, FontWeight::Bold];

/// Reader-related model.
pub struct ReaderState {
    pub(super) full_text: String,
    pub(super) pages: Vec<String>,
    pub(super) page_sentences: Vec<Vec<String>>,
    pub(super) page_sentence_counts: Vec<usize>,
    pub(super) images: Vec<BookImage>,
    pub(super) current_page: usize,
}

/// Runtime TTS model (configuration lives in `AppConfig`).
pub struct TtsState {
    pub(super) engine: Option<TtsEngine>,
    pub(super) playback: Option<TtsPlayback>,
    pub(super) last_sentences: Vec<String>,
    pub(super) current_sentence_idx: Option<usize>,
    pub(super) sentence_offset: usize,
    pub(super) track: Vec<(PathBuf, Duration)>,
    pub(super) started_at: Option<Instant>,
    pub(super) elapsed: Duration,
    pub(super) running: bool,
    pub(super) request_id: u64,
    pub(super) sources_per_sentence: usize,
    pub(super) total_sources: usize,
    pub(super) display_to_audio: Vec<Option<usize>>,
    pub(super) audio_to_display: Vec<usize>,
}

/// Bookmark and scroll tracking model.
pub struct BookmarkState {
    pub(super) last_scroll_offset: RelativeOffset,
    pub(super) viewport_fraction: f32,
    pub(super) viewport_width: f32,
    pub(super) viewport_height: f32,
    pub(super) content_width: f32,
    pub(super) content_height: f32,
    pub(super) pending_sentence_snap: Option<usize>,
}

/// Core application state composed of sub-models.
pub struct App {
    pub(super) reader: ReaderState,
    pub(super) tts: TtsState,
    pub(super) bookmark: BookmarkState,
    pub(super) config: AppConfig,
    pub(super) epub_path: PathBuf,
    pub(super) normalizer: TextNormalizer,
    pub(super) text_only_mode: bool,
}

impl App {
    /// Re-run pagination after a state change (e.g., font size).
    pub(super) fn repaginate(&mut self) {
        self.reader.pages = paginate(
            &self.reader.full_text,
            self.config.font_size,
            self.config.lines_per_page,
        );
        if self.reader.pages.is_empty() {
            self.reader
                .pages
                .push(String::from("This EPUB appears to contain no text."));
        }
        if self.reader.current_page >= self.reader.pages.len() {
            self.reader.current_page = self.reader.pages.len() - 1;
        }
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
        if let Some(playback) = self.tts.playback.take() {
            playback.stop();
        }
        self.tts.running = false;
        self.tts.started_at = None;
        self.tts.total_sources = 0;
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
            match ch {
                ' ' => output.push_str(&word_gap),
                '\n' => output.push('\n'),
                _ => {
                    output.push(ch);
                    if !letter_gap.is_empty() {
                        output.push_str(&letter_gap);
                    }
                }
            }
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
        split_sentences(&self.formatted_page_content())
    }

    pub(super) fn text_only_preview_for_current_page(&self) -> String {
        let display_sentences = self.raw_sentences_for_page(self.reader.current_page);
        if display_sentences.is_empty() {
            return "No textual content on this page.".to_string();
        }
        let plan = self.normalizer.plan_page(&display_sentences);
        if plan.audio_sentences.is_empty() {
            return "No speakable text remains on this page after normalization.".to_string();
        }
        plan.audio_sentences.join("\n\n")
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

    pub(super) fn save_epub_config(&self) {
        save_epub_config(&self.epub_path, &self.config);
    }

    pub(super) fn bootstrap(
        book: LoadedBook,
        mut config: AppConfig,
        epub_path: PathBuf,
        bookmark: Option<Bookmark>,
    ) -> (App, Task<Message>) {
        clamp_config(&mut config);
        let mut app = App {
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
            },
            epub_path,
            tts: TtsState {
                engine: TtsEngine::new(
                    config.tts_model_path.clone().into(),
                    config.tts_espeak_path.clone().into(),
                )
                .ok(),
                playback: None,
                last_sentences: Vec::new(),
                current_sentence_idx: None,
                sentence_offset: 0,
                track: Vec::new(),
                started_at: None,
                elapsed: Duration::ZERO,
                running: false,
                request_id: 0,
                sources_per_sentence: 1,
                total_sources: 0,
                display_to_audio: Vec::new(),
                audio_to_display: Vec::new(),
            },
            config,
            normalizer: TextNormalizer::load_default(),
            text_only_mode: false,
        };

        app.repaginate();
        let mut init_task = Task::none();
        match bookmark {
            Some(bookmark) => {
                let capped_page = bookmark.page.min(app.reader.pages.len().saturating_sub(1));
                app.reader.current_page = capped_page;
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
                app.tts.current_sentence_idx = restored_idx;
                app.bookmark.pending_sentence_snap = restored_idx;

                if let Some(idx) = app.tts.current_sentence_idx {
                    // Prefer persisted scroll for initial layout, then do a one-time
                    // geometry-aware sentence snap after the first viewport update.
                    if app.bookmark.last_scroll_offset.y > 0.0 {
                        init_task = iced::widget::scrollable::snap_to(
                            TEXT_SCROLL_ID.clone(),
                            app.bookmark.last_scroll_offset,
                        );
                    } else if let Some(offset) =
                        app.scroll_offset_for_sentence(idx, app.tts.last_sentences.len())
                    {
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
    config.font_size = config.font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
    config.line_spacing = config.line_spacing.clamp(0.8, 2.5);
    config.margin_horizontal = config.margin_horizontal.min(MAX_MARGIN);
    config.margin_vertical = config.margin_vertical.min(MAX_MARGIN);
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
}
