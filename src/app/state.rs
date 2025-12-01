use crate::cache::save_epub_config;
use crate::config::{
    AppConfig, FontFamily, FontWeight, HighlightColor, Justification, LogLevel, ThemeMode,
};
use crate::pagination::{
    MAX_FONT_SIZE, MAX_LINES_PER_PAGE, MIN_FONT_SIZE, MIN_LINES_PER_PAGE, paginate,
};
use crate::tts::{TtsEngine, TtsPlayback};
use iced::alignment::Horizontal;
use iced::font::{Family, Weight};
use iced::widget::scrollable::Id as ScrollId;
use iced::{Color, Font, Task};
use once_cell::sync::Lazy;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::messages::{Component, Message};

/// Limits and defaults for reader controls.
pub(crate) const MAX_MARGIN: u16 = 48;
pub(crate) const MAX_WORD_SPACING: u32 = 5;
pub(crate) const MAX_LETTER_SPACING: u32 = 3;
pub(crate) const MIN_TTS_SPEED: f32 = 0.1;
pub(crate) const MAX_TTS_SPEED: f32 = 3.0;
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
pub(crate) const JUSTIFICATIONS: [Justification; 4] = [
    Justification::Left,
    Justification::Center,
    Justification::Right,
    Justification::Justified,
];

/// Core application state.
pub struct App {
    pub(super) full_text: String,
    pub(super) pages: Vec<String>,
    pub(super) current_page: usize,
    pub(super) font_size: u32,
    pub(super) night_mode: bool,
    pub(super) settings_open: bool,
    pub(super) font_family: FontFamily,
    pub(super) font_weight: FontWeight,
    pub(super) line_spacing: f32,
    pub(super) margin_horizontal: u16,
    pub(super) margin_vertical: u16,
    pub(super) justification: Justification,
    pub(super) word_spacing: u32,
    pub(super) letter_spacing: u32,
    pub(super) lines_per_page: usize,
    pub(super) epub_path: PathBuf,
    pub(super) tts_engine: Option<TtsEngine>,
    pub(super) tts_playback: Option<TtsPlayback>,
    pub(super) tts_open: bool,
    pub(super) tts_speed: f32,
    pub(super) tts_threads: usize,
    pub(super) last_sentences: Vec<String>,
    pub(super) current_sentence_idx: Option<usize>,
    pub(super) tts_track: Vec<(PathBuf, Duration)>,
    pub(super) tts_started_at: Option<Instant>,
    pub(super) tts_elapsed: Duration,
    pub(super) tts_running: bool,
    pub(super) day_highlight: HighlightColor,
    pub(super) night_highlight: HighlightColor,
    pub(super) tts_model_path: String,
    pub(super) tts_espeak_path: String,
    pub(super) log_level: LogLevel,
    pub(super) tts_request_id: u64,
    pub(super) pause_after_sentence: f32,
}

impl App {
    pub(super) fn justification_alignment(&self) -> Horizontal {
        match self.justification {
            Justification::Left => Horizontal::Left,
            Justification::Center => Horizontal::Center,
            Justification::Right => Horizontal::Right,
            Justification::Justified => Horizontal::Left,
        }
    }

    /// Re-run pagination after a state change (e.g., font size).
    pub(super) fn repaginate(&mut self) {
        self.pages = paginate(&self.full_text, self.font_size, self.lines_per_page);
        if self.pages.is_empty() {
            self.pages
                .push(String::from("This EPUB appears to contain no text."));
        }
        if self.current_page >= self.pages.len() {
            self.current_page = self.pages.len() - 1;
        }
        tracing::debug!(
            pages = self.pages.len(),
            font_size = self.font_size,
            lines_per_page = self.lines_per_page,
            "Repaginated content"
        );
    }

    pub(super) fn stop_playback(&mut self) {
        if let Some(playback) = self.tts_playback.take() {
            playback.stop();
        }
        self.tts_running = false;
        self.tts_started_at = None;
    }

    pub(super) fn current_font(&self) -> Font {
        let family = match self.font_family {
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
            weight: self.font_weight.to_weight(),
            ..Font::DEFAULT
        }
    }

    pub(super) fn formatted_page_content(&self) -> String {
        let base = self
            .pages
            .get(self.current_page)
            .map(String::as_str)
            .unwrap_or("")
            .to_string();

        if self.word_spacing == 0 && self.letter_spacing == 0 {
            return base;
        }

        let word_gap = " ".repeat((self.word_spacing as usize).saturating_add(1));
        let letter_gap = " ".repeat(self.letter_spacing as usize);

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

    pub(super) fn highlight_color(&self) -> Color {
        let base = if self.night_mode {
            self.night_highlight
        } else {
            self.day_highlight
        };
        Color {
            r: base.r,
            g: base.g,
            b: base.b,
            a: base.a,
        }
    }

    pub(super) fn save_epub_config(&self) {
        let config = AppConfig {
            theme: if self.night_mode {
                ThemeMode::Night
            } else {
                ThemeMode::Day
            },
            font_size: self.font_size,
            line_spacing: self.line_spacing,
            margin_horizontal: self.margin_horizontal,
            margin_vertical: self.margin_vertical,
            font_family: self.font_family,
            font_weight: self.font_weight,
            justification: self.justification,
            word_spacing: self.word_spacing,
            letter_spacing: self.letter_spacing,
            lines_per_page: self.lines_per_page,
            tts_model_path: self.tts_model_path.clone(),
            tts_speed: self.tts_speed,
            tts_espeak_path: self.tts_espeak_path.clone(),
            tts_threads: self.tts_threads,
            show_tts: self.tts_open,
            show_settings: self.settings_open,
            day_highlight: self.day_highlight,
            night_highlight: self.night_highlight,
            log_level: self.log_level,
            pause_after_sentence: self.pause_after_sentence,
        };

        save_epub_config(&self.epub_path, &config);
    }

    pub(super) fn bootstrap(
        text: String,
        config: AppConfig,
        epub_path: PathBuf,
        last_page: Option<usize>,
    ) -> (App, Task<Message>) {
        let font_size = config.font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        let line_spacing = config.line_spacing.clamp(0.8, 2.5);
        let margin_horizontal = config.margin_horizontal.min(MAX_MARGIN);
        let margin_vertical = config.margin_vertical.min(MAX_MARGIN);
        let word_spacing = config.word_spacing.min(MAX_WORD_SPACING);
        let letter_spacing = config.letter_spacing.min(MAX_LETTER_SPACING);
        let lines_per_page = config
            .lines_per_page
            .clamp(MIN_LINES_PER_PAGE, MAX_LINES_PER_PAGE);
        let pause_after_sentence = config.pause_after_sentence.clamp(0.0, 2.0);

        let mut app = App {
            pages: Vec::new(),
            full_text: text,
            current_page: 0,
            font_size,
            night_mode: matches!(config.theme, ThemeMode::Night),
            settings_open: config.show_settings,
            font_family: config.font_family,
            font_weight: config.font_weight,
            line_spacing,
            justification: config.justification,
            word_spacing,
            letter_spacing,
            lines_per_page,
            margin_horizontal,
            margin_vertical,
            epub_path,
            tts_engine: TtsEngine::new(
                config.tts_model_path.clone().into(),
                config.tts_espeak_path.clone().into(),
            )
            .ok(),
            tts_playback: None,
            tts_open: config.show_tts,
            tts_speed: config.tts_speed.clamp(MIN_TTS_SPEED, MAX_TTS_SPEED),
            tts_threads: config.tts_threads.max(1),
            last_sentences: Vec::new(),
            current_sentence_idx: None,
            tts_track: Vec::new(),
            tts_started_at: None,
            tts_elapsed: Duration::ZERO,
            tts_running: false,
            day_highlight: config.day_highlight,
            night_highlight: config.night_highlight,
            tts_model_path: config.tts_model_path,
            tts_espeak_path: config.tts_espeak_path,
            log_level: config.log_level,
            tts_request_id: 0,
            pause_after_sentence,
        };

        app.repaginate();
        if let Some(last) = last_page {
            app.current_page = last.min(app.pages.len().saturating_sub(1));
            tracing::info!(page = app.current_page + 1, "Restored last page from cache");
        } else {
            tracing::info!("Starting from first page");
        }
        tracing::info!(
            font_size = app.font_size,
            night_mode = app.night_mode,
            justification = ?app.justification,
            "Initialized app state"
        );

        (app, Task::none())
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
