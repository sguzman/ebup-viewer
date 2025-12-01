//! UI layer for the EPUB viewer.
//!
//! This module owns all GUI state and messages. It expects the caller to
//! provide the already-loaded plain text (see `epub_loader`) and relies on
//! `pagination` to break that text into pages based on the current font size.

use crate::cache::save_last_page;
use crate::config::{AppConfig, FontFamily, FontWeight, Justification, ThemeMode};
use crate::text_utils::split_sentences;
use crate::tts::{TtsEngine, TtsPlayback};
use crate::pagination::{paginate, MAX_FONT_SIZE, MIN_FONT_SIZE};
use iced::Color;
use iced::alignment::{Horizontal, Vertical};
use iced::time;
use iced::Subscription;
use iced::widget::{
    button, column, container, pick_list, row, scrollable, slider, text, Column, Row,
};
use iced::widget::text::{LineHeight, Wrapping};
use iced::{Element, Font, Length, Task, Theme};
use iced::font::{Family, Weight};
use std::time::{Duration, Instant};
use std::path::PathBuf;
use std::fs::File;
use std::io::BufReader;
use rodio::{Decoder, Source};
use iced::widget::container::Style;

/// Limits and defaults for reader controls.
const MAX_MARGIN: u16 = 48;
const MAX_WORD_SPACING: u32 = 5;
const MAX_LETTER_SPACING: u32 = 3;
const MIN_TTS_SPEED: f32 = 0.1;
const MAX_TTS_SPEED: f32 = 2.0;
const FONT_FAMILIES: [FontFamily; 13] = [
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
const FONT_WEIGHTS: [FontWeight; 3] = [
    FontWeight::Light,
    FontWeight::Normal,
    FontWeight::Bold,
];
const JUSTIFICATIONS: [Justification; 4] = [
    Justification::Left,
    Justification::Center,
    Justification::Right,
    Justification::Justified,
];

/// Messages emitted by the UI.
#[derive(Debug, Clone)]
pub enum Message {
    NextPage,
    PreviousPage,
    FontSizeChanged(u32),
    ToggleTheme,
    ToggleSettings,
    FontFamilyChanged(FontFamily),
    FontWeightChanged(FontWeight),
    LineSpacingChanged(f32),
    MarginHorizontalChanged(u16),
    MarginVerticalChanged(u16),
    JustificationChanged(Justification),
    WordSpacingChanged(u32),
    LetterSpacingChanged(u32),
    ToggleTtsControls,
    Play,
    Pause,
    PlayFromPageStart,
    PlayFromCursor(usize),
    SetTtsSpeed(f32),
    SeekForward,
    SeekBackward,
    Tick(Instant),
}

/// Core application state.
pub struct App {
    full_text: String,
    pages: Vec<String>,
    current_page: usize,
    font_size: u32,
    night_mode: bool,
    settings_open: bool,
    font_family: FontFamily,
    font_weight: FontWeight,
    line_spacing: f32,
    margin_horizontal: u16,
    margin_vertical: u16,
    justification: Justification,
    word_spacing: u32,
    letter_spacing: u32,
    epub_path: std::path::PathBuf,
    // TTS
    tts_engine: Option<TtsEngine>,
    tts_playback: Option<TtsPlayback>,
    tts_open: bool,
    tts_speed: f32,
    last_sentences: Vec<String>,
    current_sentence_idx: Option<usize>,
    tts_track: Vec<(PathBuf, Duration)>,
    tts_deadline: Option<Instant>,
    tts_running: bool,
}

impl App {
    fn justification_alignment(&self) -> Horizontal {
        match self.justification {
            Justification::Left => Horizontal::Left,
            Justification::Center => Horizontal::Center,
            Justification::Right => Horizontal::Right,
            Justification::Justified => Horizontal::Left,
        }
    }

    /// Re-run pagination after a state change (e.g., font size).
    fn repaginate(&mut self) {
        self.pages = paginate(&self.full_text, self.font_size);
        if self.pages.is_empty() {
            self.pages.push(String::from("This EPUB appears to contain no text."));
        }
        if self.current_page >= self.pages.len() {
            self.current_page = self.pages.len() - 1;
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let mut page_changed = false;

        match message {
            Message::NextPage => {
                if self.current_page + 1 < self.pages.len() {
                    self.current_page += 1;
                    page_changed = true;
                }
            }
            Message::PreviousPage => {
                if self.current_page > 0 {
                    self.current_page -= 1;
                    page_changed = true;
                }
            }
            Message::FontSizeChanged(size) => {
                let clamped = size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
                if clamped != self.font_size {
                    self.font_size = clamped;
                    self.repaginate();
                }
            }
            Message::ToggleTheme => {
                self.night_mode = !self.night_mode;
            }
            Message::ToggleSettings => {
                self.settings_open = !self.settings_open;
            }
            Message::FontFamilyChanged(family) => {
                self.font_family = family;
            }
            Message::FontWeightChanged(weight) => {
                self.font_weight = weight;
            }
            Message::LineSpacingChanged(spacing) => {
                self.line_spacing = spacing.clamp(0.8, 2.5);
            }
            Message::MarginHorizontalChanged(margin) => {
                self.margin_horizontal = margin.min(MAX_MARGIN);
            }
            Message::MarginVerticalChanged(margin) => {
                self.margin_vertical = margin.min(MAX_MARGIN);
            }
            Message::JustificationChanged(justification) => {
                self.justification = justification;
            }
            Message::WordSpacingChanged(spacing) => {
                self.word_spacing = spacing.min(MAX_WORD_SPACING);
            }
            Message::LetterSpacingChanged(spacing) => {
                self.letter_spacing = spacing.min(MAX_LETTER_SPACING);
            }
            Message::ToggleTtsControls => {
                self.tts_open = !self.tts_open;
            }
            Message::SetTtsSpeed(speed) => {
                let clamped = speed.clamp(MIN_TTS_SPEED, MAX_TTS_SPEED);
                self.tts_speed = clamped;
                if let Some(playback) = &self.tts_playback {
                    playback.set_speed(clamped);
                }
            }
            Message::Play => {
                if let Some(playback) = &self.tts_playback {
                    playback.play();
                } else {
                    self.start_playback_from(self.current_page, 0);
                }
            }
            Message::PlayFromPageStart => {
                self.start_playback_from(self.current_page, 0);
            }
            Message::PlayFromCursor(idx) => {
                self.start_playback_from(self.current_page, idx);
            }
            Message::Pause => {
                if let Some(playback) = &self.tts_playback {
                    playback.pause();
                }
            }
            Message::SeekForward => {
                let next = self.current_page + 1;
                if next < self.pages.len() {
                    self.current_page = next;
                    self.start_playback_from(self.current_page, 0);
                    page_changed = true;
                }
            }
            Message::SeekBackward => {
                if self.current_page > 0 {
                    self.current_page -= 1;
                    self.start_playback_from(self.current_page, 0);
                    page_changed = true;
                }
            }
            Message::Tick(now) => {
                if self.tts_running {
                    if let Some(deadline) = self.tts_deadline {
                        if now >= deadline {
                            let next_idx = self.current_sentence_idx.unwrap_or(0) + 1;
                            if next_idx < self.last_sentences.len() {
                                self.current_sentence_idx = Some(next_idx);
                                if let Some((_, dur)) = self.tts_track.get(next_idx) {
                                    self.tts_deadline = Some(Instant::now() + *dur);
                                } else {
                                    self.tts_running = false;
                                    self.tts_deadline = None;
                                }
                            } else {
                                self.tts_running = false;
                                self.tts_deadline = None;
                            }
                        }
                    }
                }
            }
        }

        if page_changed {
            save_last_page(&self.epub_path, self.current_page);
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let total_pages = self.pages.len().max(1);
        let page_label = format!("Page {} of {}", self.current_page + 1, total_pages);

        let theme_label = if self.night_mode { "Day Mode" } else { "Night Mode" };
        let theme_toggle = button(theme_label).on_press(Message::ToggleTheme);
        let settings_toggle = button(if self.settings_open { "Hide Settings" } else { "Show Settings" })
            .on_press(Message::ToggleSettings);
        let tts_toggle = button(if self.tts_open { "Hide TTS" } else { "Show TTS" })
            .on_press(Message::ToggleTtsControls);

        let prev_button = if self.current_page > 0 {
            button("Previous").on_press(Message::PreviousPage)
        } else {
            button("Previous")
        };

        let next_button = if self.current_page + 1 < total_pages {
            button("Next").on_press(Message::NextPage)
        } else {
            button("Next")
        };

        let controls = row![
            prev_button,
            next_button,
            theme_toggle,
            settings_toggle,
            tts_toggle,
            text(page_label)
        ]
        .spacing(10)
        .align_y(Vertical::Center);

        let font_label = text(format!("Font size: {}", self.font_size));
        let font_slider = slider(
            MIN_FONT_SIZE as f32..=MAX_FONT_SIZE as f32,
            self.font_size as f32,
            |value| Message::FontSizeChanged(value.round() as u32),
        );

        let font_controls = row![font_label, font_slider]
            .spacing(10)
            .align_y(Vertical::Center);

        let page_content = self.formatted_page_content();

        let text_view_content: Element<'_, Message> = if self.tts_playback.is_some()
            && !self.last_sentences.is_empty()
        {
            let sentences = split_sentences(page_content.clone());
            let highlight_idx = self.current_sentence_idx.unwrap_or(0);
            let items: Column<'_, Message> = sentences
                .into_iter()
                .enumerate()
                .fold(column![].spacing(6), |col, (idx, sentence)| {
                    let is_active = idx == highlight_idx;
                    let style = move |_theme: &Theme| {
                        Style {
                            background: if is_active {
                                Some(iced::Background::Color(Color {
                                    r: 0.2,
                                    g: 0.4,
                                    b: 0.7,
                                    a: 0.15,
                                }))
                            } else {
                                None
                            },
                            ..Default::default()
                        }
                    };
                    col.push(
                        container(
                            text(sentence)
                                .size(self.font_size as f32)
                                .line_height(LineHeight::Relative(self.line_spacing))
                                .width(Length::Fill)
                                .wrapping(Wrapping::WordOrGlyph)
                                .align_x(self.justification_alignment())
                                .font(self.current_font()),
                        )
                        .padding(4)
                        .style(style),
                    )
                });
            items.into()
        } else {
            text(page_content)
                .size(self.font_size as f32)
                .line_height(LineHeight::Relative(self.line_spacing))
                .width(Length::Fill)
                .wrapping(Wrapping::WordOrGlyph)
                .align_x(self.justification_alignment())
                .font(self.current_font())
                .into()
        };

        let text_view = scrollable(
            container(text_view_content)
                .width(Length::Fill)
                .padding([self.margin_vertical, self.margin_horizontal]),
        )
        .height(Length::Fill);

        let mut content: Column<'_, Message> = column![controls, font_controls, text_view]
            .padding(16)
            .spacing(12);

        if self.tts_open {
            content = content.push(self.tts_controls());
        }

        let mut layout: Row<'_, Message> =
            row![container(content).width(Length::Fill)].spacing(16);

        if self.settings_open {
            layout = layout.push(self.settings_panel());
        }

        layout.into()
    }

    fn subscription(app: &App) -> Subscription<Message> {
        if app.tts_running {
            time::every(Duration::from_millis(200)).map(Message::Tick)
        } else {
            Subscription::none()
        }
    }
}

/// Helper to launch the app with the provided text.
pub fn run_app(
    text: String,
    config: AppConfig,
    epub_path: std::path::PathBuf,
    last_page: Option<usize>,
) -> iced::Result {
    iced::application("EPUB Viewer", App::update, App::view)
        .subscription(App::subscription)
        .theme(|app: &App| if app.night_mode { Theme::Dark } else { Theme::Light })
        .run_with(move || {
            let font_size = config.font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
            let line_spacing = config.line_spacing.clamp(0.8, 2.5);
            let margin_horizontal = config.margin_horizontal.min(MAX_MARGIN);
            let margin_vertical = config.margin_vertical.min(MAX_MARGIN);
            let word_spacing = config.word_spacing.min(MAX_WORD_SPACING);
            let letter_spacing = config.letter_spacing.min(MAX_LETTER_SPACING);

            let mut app = App {
                pages: Vec::new(),
                full_text: text,
                current_page: 0,
                font_size,
                night_mode: matches!(config.theme, ThemeMode::Night),
                settings_open: false,
                font_family: config.font_family,
                font_weight: config.font_weight,
                line_spacing,
                justification: config.justification,
                word_spacing,
                letter_spacing,
                margin_horizontal,
                margin_vertical,
                epub_path,
                tts_engine: TtsEngine::new(
                    config.tts_model_path.clone().into(),
                    config.tts_espeak_path.clone().into(),
                    config.tts_speed,
                )
                    .ok(),
                tts_playback: None,
                tts_open: false,
                tts_speed: config.tts_speed.clamp(MIN_TTS_SPEED, MAX_TTS_SPEED),
                last_sentences: Vec::new(),
                current_sentence_idx: None,
                tts_track: Vec::new(),
                tts_deadline: None,
                tts_running: false,
            };
            app.repaginate();
            if let Some(last) = last_page {
                app.current_page = last.min(app.pages.len().saturating_sub(1));
            }
            (app, Task::none())
        })
}

impl FontWeight {
    fn to_weight(self) -> Weight {
        match self {
            FontWeight::Light => Weight::Light,
            FontWeight::Normal => Weight::Normal,
            FontWeight::Bold => Weight::Bold,
        }
    }
}

impl App {
    fn current_font(&self) -> Font {
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

    fn formatted_page_content(&self) -> String {
        let base = self
            .pages
            .get(self.current_page)
            .map(String::as_str)
            .unwrap_or("");

        let justified = if self.justification == Justification::Justified {
            justify_text(base, self.font_size, self.margin_horizontal)
        } else {
            base.to_string()
        };

        if self.word_spacing == 0 && self.letter_spacing == 0 {
            return justified;
        }

        let word_gap = " ".repeat((self.word_spacing as usize).saturating_add(1));
        let letter_gap = " ".repeat(self.letter_spacing as usize);

        let mut output = String::with_capacity(justified.len() + 16);

        for ch in justified.chars() {
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

    fn settings_panel(&self) -> Element<'_, Message> {
        let family_picker =
            pick_list(FONT_FAMILIES, Some(self.font_family), Message::FontFamilyChanged);
        let weight_picker =
            pick_list(FONT_WEIGHTS, Some(self.font_weight), Message::FontWeightChanged);
        let justification_picker =
            pick_list(
                JUSTIFICATIONS,
                Some(self.justification),
                Message::JustificationChanged,
            );

        let line_spacing_slider = slider(
            0.8..=2.5,
            self.line_spacing,
            Message::LineSpacingChanged,
        )
        .step(0.05);

        let margin_slider = slider(
            0.0..=MAX_MARGIN as f32,
            self.margin_horizontal as f32,
            |value| Message::MarginHorizontalChanged(value.round() as u16),
        );

        let margin_vertical_slider = slider(
            0.0..=MAX_MARGIN as f32,
            self.margin_vertical as f32,
            |value| Message::MarginVerticalChanged(value.round() as u16),
        );

        let word_spacing_slider = slider(
            0.0..=MAX_WORD_SPACING as f32,
            self.word_spacing as f32,
            |value| Message::WordSpacingChanged(value.round() as u32),
        );

        let letter_spacing_slider = slider(
            0.0..=MAX_LETTER_SPACING as f32,
            self.letter_spacing as f32,
            |value| Message::LetterSpacingChanged(value.round() as u32),
        );

        let panel = column![
            text("Reader Settings").size(20.0),
            row![text("Font family"), family_picker].spacing(8).align_y(Vertical::Center),
            row![text("Font weight"), weight_picker].spacing(8).align_y(Vertical::Center),
            row![text("Line spacing"), line_spacing_slider].spacing(8).align_y(Vertical::Center),
            row![text(format!("Horizontal margin: {} px", self.margin_horizontal)), margin_slider]
                .spacing(8)
                .align_y(Vertical::Center),
            row![text(format!("Vertical margin: {} px", self.margin_vertical)), margin_vertical_slider]
                .spacing(8)
                .align_y(Vertical::Center),
            row![text("Justification"), justification_picker]
                .spacing(8)
                .align_y(Vertical::Center),
            row![text(format!("Word spacing: {}", self.word_spacing)), word_spacing_slider]
                .spacing(8)
                .align_y(Vertical::Center),
            row![text(format!("Letter spacing: {}", self.letter_spacing)), letter_spacing_slider]
                .spacing(8)
                .align_y(Vertical::Center),
        ]
        .spacing(12)
        .width(Length::Fixed(280.0));

        container(panel).padding(12).into()
    }

    fn tts_controls(&self) -> Element<'_, Message> {
        let play_label = if self
            .tts_playback
            .as_ref()
            .map(|p| p.is_paused())
            .unwrap_or(true)
        {
            "Play"
        } else {
            "Pause"
        };

        let play_button = if play_label == "Play" {
            button(play_label).on_press(Message::Play)
        } else {
            button(play_label).on_press(Message::Pause)
        };
        let play_from_start = button("Play Page").on_press(Message::PlayFromPageStart);

        let speed_slider = slider(
            MIN_TTS_SPEED..=MAX_TTS_SPEED,
            self.tts_speed,
            Message::SetTtsSpeed,
        )
        .step(0.05);

        let controls = row![
            button("⏮").on_press(Message::SeekBackward),
            play_button,
            button("⏭").on_press(Message::SeekForward),
            play_from_start,
            text(format!("Speed: {:.2}x", self.tts_speed)),
            speed_slider,
        ]
        .spacing(10)
        .align_y(Vertical::Center);

        container(
            column![text("TTS Controls"), controls]
                .spacing(8)
                .padding(8),
        )
        .into()
    }

    fn start_playback_from(&mut self, page: usize, sentence_idx: usize) {
        let Some(engine) = &self.tts_engine else {
            return;
        };

        let sentences = split_sentences(
            self.pages
                .get(page)
                .map(String::as_str)
                .unwrap_or("")
                .to_string(),
        );
        self.last_sentences = sentences.clone();
        self.current_sentence_idx = Some(sentence_idx.min(sentences.len().saturating_sub(1)));

        let sentence_idx = sentence_idx.min(sentences.len().saturating_sub(1));
        let mut files = Vec::new();
        let mut track = Vec::new();
        for (_i, sent) in sentences.iter().enumerate().skip(sentence_idx) {
            if let Ok(path) = engine.ensure_audio(sent) {
                let dur = sentence_duration(&path, self.tts_speed);
                track.push((path.clone(), dur));
                files.push(path);
            }
        }

        if let Ok(playback) = engine.play_files(&files) {
            playback.set_speed(self.tts_speed);
            self.tts_playback = Some(playback);
            self.tts_track = track;
            self.current_sentence_idx = Some(sentence_idx.min(self.last_sentences.len().saturating_sub(1)));
            if let Some((_, dur)) = self.tts_track.first() {
                self.tts_deadline = Some(Instant::now() + *dur);
                self.tts_running = true;
            }
        }
    }
}

fn sentence_duration(path: &PathBuf, speed: f32) -> Duration {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return Duration::from_secs(1),
    };
    let reader = BufReader::new(file);
    let dur = Decoder::new(reader)
        .ok()
        .and_then(|d| d.total_duration())
        .unwrap_or(Duration::from_secs(1));

    if speed > 0.0 {
        dur.mul_f64((1.0 / speed) as f64)
    } else {
        dur
    }
}

/// Attempt to justify text by distributing spaces between words to hit a target line width.
fn justify_text(content: &str, font_size: u32, horizontal_margin: u16) -> String {
    let mut target_width = approximate_chars_per_line(font_size).max(20);
    // Shrink width a bit based on margins to better approximate the visible area.
    target_width = target_width.saturating_sub((horizontal_margin / 2) as usize);
    target_width = target_width.max(20);
    let mut output = String::new();

    for (pi, paragraph) in content.split("\n\n").enumerate() {
        if pi > 0 {
            output.push_str("\n\n");
        }

        let words: Vec<&str> = paragraph.split_whitespace().collect();
        if words.is_empty() {
            continue;
        }

        let mut line_words: Vec<&str> = Vec::new();
        let mut line_len = 0usize;

        for word in words {
            let additional = if line_words.is_empty() {
                word.len()
            } else {
                line_len + 1 + word.len()
            };

            if !line_words.is_empty() && additional > target_width {
                output.push_str(&justify_line(&line_words, target_width));
                output.push('\n');
                line_words.clear();
                line_len = 0;
            }

            if line_words.is_empty() {
                line_len = word.len();
            } else {
                line_len += 1 + word.len();
            }
            line_words.push(word);
        }

        if !line_words.is_empty() {
            // Last line of paragraph: leave ragged-right.
            output.push_str(&line_words.join(" "));
        }
    }

    output
}

fn justify_line(words: &[&str], target_width: usize) -> String {
    if words.len() <= 1 {
        return words.join(" ");
    }

    let total_chars: usize = words.iter().map(|w| w.len()).sum();
    let gaps = words.len() - 1;
    let base_spaces = 1;
    let mut extra_spaces = target_width.saturating_sub(total_chars + gaps * base_spaces);

    let mut result = String::new();

    for (i, word) in words.iter().enumerate() {
        result.push_str(word);
        if i < gaps {
            let mut spaces = base_spaces;
            if extra_spaces > 0 {
                let add = (extra_spaces + gaps - i - 1) / (gaps - i);
                spaces += add;
                extra_spaces = extra_spaces.saturating_sub(add);
            }
            result.push_str(&" ".repeat(spaces));
        }
    }

    result
}

fn approximate_chars_per_line(font_size: u32) -> usize {
    let normalized = font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE) as f32;
    (80.0 * (16.0 / normalized))
        .round()
        .clamp(30.0, 120.0) as usize
}
