//! UI layer for the EPUB viewer.
//!
//! This module owns all GUI state and messages. It expects the caller to
//! provide the already-loaded plain text (see `epub_loader`) and relies on
//! `pagination` to break that text into pages based on the current font size.

use crate::cache::save_last_page;
use crate::config::{
    AppConfig, FontFamily, FontWeight, HighlightColor, Justification, LogLevel, ThemeMode,
};
use crate::pagination::{
    MAX_FONT_SIZE, MAX_LINES_PER_PAGE, MIN_FONT_SIZE, MIN_LINES_PER_PAGE, paginate,
};
use crate::text_utils::split_sentences;
use crate::tts::{TtsEngine, TtsPlayback};
use iced::Color;
use iced::Subscription;
use iced::alignment::{Horizontal, Vertical};
use iced::font::{Family, Weight};
use iced::time;
use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{
    Column, Row, button, column, container, pick_list, row, scrollable, slider, text,
};
use iced::{Element, Font, Length, Task, Theme};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Limits and defaults for reader controls.
const MAX_MARGIN: u16 = 48;
const MAX_WORD_SPACING: u32 = 5;
const MAX_LETTER_SPACING: u32 = 3;
const MIN_TTS_SPEED: f32 = 0.1;
const MAX_TTS_SPEED: f32 = 3.0;
const HIGHLIGHT_LEAD_MS: u64 = 30;
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
const FONT_WEIGHTS: [FontWeight; 3] = [FontWeight::Light, FontWeight::Normal, FontWeight::Bold];
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
    LinesPerPageChanged(u32),
    ToggleTtsControls,
    DayHighlightChanged(Component, f32),
    NightHighlightChanged(Component, f32),
    Play,
    Pause,
    PlayFromPageStart,
    PlayFromCursor(usize),
    SetTtsSpeed(f32),
    SeekForward,
    SeekBackward,
    TtsPrepared {
        page: usize,
        start_idx: usize,
        request_id: u64,
        files: Vec<(PathBuf, Duration)>,
    },
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
    lines_per_page: usize,
    epub_path: std::path::PathBuf,
    tts_request_id: u64,
    // TTS
    tts_engine: Option<TtsEngine>,
    tts_playback: Option<TtsPlayback>,
    tts_open: bool,
    tts_speed: f32,
    tts_threads: usize,
    last_sentences: Vec<String>,
    current_sentence_idx: Option<usize>,
    tts_track: Vec<(PathBuf, Duration)>,
    tts_deadline: Option<Instant>,
    tts_running: bool,
    day_highlight: HighlightColor,
    night_highlight: HighlightColor,
    tts_model_path: String,
    tts_espeak_path: String,
    log_level: LogLevel,
}

impl App {
    fn stop_playback(&mut self) {
        if let Some(playback) = self.tts_playback.take() {
            playback.stop();
        }
        self.tts_running = false;
        self.tts_deadline = None;
    }

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
        self.pages = paginate(&self.full_text, self.font_size, self.lines_per_page);
        if self.pages.is_empty() {
            self.pages
                .push(String::from("This EPUB appears to contain no text."));
        }
        if self.current_page >= self.pages.len() {
            self.current_page = self.pages.len() - 1;
        }
        debug!(
            pages = self.pages.len(),
            font_size = self.font_size,
            lines_per_page = self.lines_per_page,
            "Repaginated content"
        );
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let mut page_changed = false;
        let mut tasks: Vec<Task<Message>> = Vec::new();

        match message {
            Message::NextPage => {
                if self.current_page + 1 < self.pages.len() {
                    self.current_page += 1;
                    page_changed = true;
                    info!(page = self.current_page + 1, "Navigated to next page");
                    tasks.push(self.start_playback_from(self.current_page, 0));
                }
            }
            Message::PreviousPage => {
                if self.current_page > 0 {
                    self.current_page -= 1;
                    page_changed = true;
                    info!(page = self.current_page + 1, "Navigated to previous page");
                    tasks.push(self.start_playback_from(self.current_page, 0));
                }
            }
            Message::FontSizeChanged(size) => {
                let clamped = size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
                if clamped != self.font_size {
                    debug!(old = self.font_size, new = clamped, "Font size changed");
                    self.font_size = clamped;
                    self.repaginate();
                }
            }
            Message::ToggleTheme => {
                info!(night_mode = !self.night_mode, "Toggled theme");
                self.night_mode = !self.night_mode;
                self.save_epub_config();
            }
            Message::ToggleSettings => {
                debug!("Toggled settings panel");
                self.settings_open = !self.settings_open;
                self.save_epub_config();
            }
            Message::FontFamilyChanged(family) => {
                debug!(?family, "Font family changed");
                self.font_family = family;
                self.save_epub_config();
            }
            Message::FontWeightChanged(weight) => {
                debug!(?weight, "Font weight changed");
                self.font_weight = weight;
                self.save_epub_config();
            }
            Message::LineSpacingChanged(spacing) => {
                self.line_spacing = spacing.clamp(0.8, 2.5);
                debug!(line_spacing = self.line_spacing, "Line spacing changed");
                self.save_epub_config();
            }
            Message::MarginHorizontalChanged(margin) => {
                self.margin_horizontal = margin.min(MAX_MARGIN);
                debug!(
                    margin_horizontal = self.margin_horizontal,
                    "Horizontal margin changed"
                );
                self.save_epub_config();
            }
            Message::MarginVerticalChanged(margin) => {
                self.margin_vertical = margin.min(MAX_MARGIN);
                debug!(
                    margin_vertical = self.margin_vertical,
                    "Vertical margin changed"
                );
                self.save_epub_config();
            }
            Message::JustificationChanged(justification) => {
                debug!(?justification, "Text justification changed");
                self.justification = justification;
                self.save_epub_config();
            }
            Message::WordSpacingChanged(spacing) => {
                self.word_spacing = spacing.min(MAX_WORD_SPACING);
                debug!(word_spacing = self.word_spacing, "Word spacing changed");
                self.save_epub_config();
            }
            Message::LetterSpacingChanged(spacing) => {
                self.letter_spacing = spacing.min(MAX_LETTER_SPACING);
                debug!(
                    letter_spacing = self.letter_spacing,
                    "Letter spacing changed"
                );
                self.save_epub_config();
            }
            Message::LinesPerPageChanged(lines) => {
                let clamped =
                    lines.clamp(MIN_LINES_PER_PAGE as u32, MAX_LINES_PER_PAGE as u32) as usize;
                if clamped != self.lines_per_page {
                    let anchor = self
                        .pages
                        .get(self.current_page)
                        .and_then(|p| split_sentences(p.clone()).into_iter().next());
                    let before = self.current_page;
                    self.lines_per_page = clamped;
                    self.repaginate();
                    if let Some(sentence) = anchor {
                        if let Some(idx) =
                            self.pages.iter().position(|page| page.contains(&sentence))
                        {
                            self.current_page = idx;
                        }
                    }
                    if self.current_page != before {
                        page_changed = true;
                    }
                    debug!(
                        lines_per_page = self.lines_per_page,
                        "Lines per page changed"
                    );
                    self.save_epub_config();
                }
            }
            Message::DayHighlightChanged(component, value) => {
                self.day_highlight = apply_component(self.day_highlight, component, value);
                debug!(?component, value, "Day highlight updated");
                self.save_epub_config();
            }
            Message::NightHighlightChanged(component, value) => {
                self.night_highlight = apply_component(self.night_highlight, component, value);
                debug!(?component, value, "Night highlight updated");
                self.save_epub_config();
            }
            Message::ToggleTtsControls => {
                debug!("Toggled TTS controls");
                self.tts_open = !self.tts_open;
                self.save_epub_config();
            }
            Message::SetTtsSpeed(speed) => {
                let clamped = speed.clamp(MIN_TTS_SPEED, MAX_TTS_SPEED);
                self.tts_speed = clamped;
                info!(speed = self.tts_speed, "Adjusted TTS speed");
                // Restart playback at current position with new synthesis speed to preserve pitch
                if self.tts_playback.is_some() {
                    let idx = self.current_sentence_idx.unwrap_or(0);
                    tasks.push(self.start_playback_from(self.current_page, idx));
                }
                self.save_epub_config();
            }
            Message::Play => {
                if let Some(playback) = &self.tts_playback {
                    info!("Resuming TTS playback");
                    playback.play();
                    self.tts_running = true;
                    if let Some(idx) = self.current_sentence_idx {
                        if let Some((_, dur)) = self.tts_track.get(idx) {
                            let lead = Duration::from_millis(HIGHLIGHT_LEAD_MS);
                            self.tts_deadline = Some(Instant::now() + dur.saturating_sub(lead));
                        }
                    }
                } else {
                    info!("Starting TTS playback from current page");
                    tasks.push(self.start_playback_from(self.current_page, 0));
                }
            }
            Message::PlayFromPageStart => {
                info!("Playing page from start");
                tasks.push(self.start_playback_from(self.current_page, 0));
            }
            Message::PlayFromCursor(idx) => {
                info!(idx, "Playing from cursor");
                tasks.push(self.start_playback_from(self.current_page, idx));
            }
            Message::Pause => {
                if let Some(playback) = &self.tts_playback {
                    info!("Pausing TTS playback");
                    playback.pause();
                }
                self.tts_running = false;
                self.tts_deadline = None;
            }
            Message::SeekForward => {
                let next_idx = self.current_sentence_idx.unwrap_or(0) + 1;
                if next_idx < self.last_sentences.len() {
                    info!(next_idx, "Seeking forward within page");
                    tasks.push(self.start_playback_from(self.current_page, next_idx));
                } else if self.current_page + 1 < self.pages.len() {
                    self.current_page += 1;
                    info!("Seeking forward into next page");
                    tasks.push(self.start_playback_from(self.current_page, 0));
                    page_changed = true;
                    self.save_epub_config();
                }
            }
            Message::SeekBackward => {
                let current_idx = self.current_sentence_idx.unwrap_or(0);
                if current_idx > 0 {
                    info!(
                        previous_idx = current_idx.saturating_sub(1),
                        "Seeking backward within page"
                    );
                    tasks.push(self.start_playback_from(self.current_page, current_idx - 1));
                } else if self.current_page > 0 {
                    self.current_page -= 1;
                    let last_idx = split_sentences(
                        self.pages
                            .get(self.current_page)
                            .map(String::as_str)
                            .unwrap_or("")
                            .to_string(),
                    )
                    .len()
                    .saturating_sub(1);
                    info!("Seeking backward into previous page");
                    tasks.push(self.start_playback_from(self.current_page, last_idx));
                    page_changed = true;
                    self.save_epub_config();
                }
            }
            Message::Tick(now) => {
                if self.tts_running {
                    // If paused, skip ticking.
                    if self
                        .tts_playback
                        .as_ref()
                        .map(|p| p.is_paused())
                        .unwrap_or(false)
                    {
                        return Task::none();
                    }

                    // If the sink finished early, advance to next page or stop.
                    if self
                        .tts_playback
                        .as_ref()
                        .map(|p| p.is_finished())
                        .unwrap_or(true)
                    {
                        self.stop_playback();
                        if self.current_page + 1 < self.pages.len() {
                            self.current_page += 1;
                            info!("Playback finished page, advancing");
                            tasks.push(self.start_playback_from(self.current_page, 0));
                        } else {
                            info!("Playback finished at end of book");
                        }
                        return Task::none();
                    }

                    if let Some(deadline) = self.tts_deadline {
                        if now >= deadline {
                            let next_idx = self.current_sentence_idx.unwrap_or(0) + 1;
                            if next_idx < self.last_sentences.len() {
                                self.current_sentence_idx = Some(next_idx);
                                if let Some((_, dur)) = self.tts_track.get(next_idx) {
                                    let lead = Duration::from_millis(HIGHLIGHT_LEAD_MS);
                                    let next_deadline = Instant::now() + dur.saturating_sub(lead);
                                    self.tts_deadline = Some(next_deadline);
                                } else {
                                    self.tts_running = false;
                                    self.tts_deadline = None;
                                }
                            } else if self.current_page + 1 < self.pages.len() {
                                self.current_page += 1;
                                info!("Reached end of page during Tick, advancing");
                                tasks.push(self.start_playback_from(self.current_page, 0));
                            } else {
                                info!("Reached end of playback during Tick");
                                self.tts_running = false;
                                self.tts_deadline = None;
                            }
                        }
                    }
                }
            }
            Message::TtsPrepared {
                page,
                start_idx,
                request_id,
                files,
            } => {
                if request_id != self.tts_request_id {
                    debug!(
                        request_id,
                        current = self.tts_request_id,
                        "Ignoring stale TTS request"
                    );
                    return Task::none();
                }
                info!(
                    page,
                    start_idx,
                    file_count = files.len(),
                    "Received prepared TTS batch"
                );
                if page != self.current_page {
                    // Stale result; ignore.
                    debug!(
                        page,
                        current = self.current_page,
                        "Ignoring stale TTS batch"
                    );
                    return Task::none();
                }
                if files.is_empty() {
                    warn!("TTS batch was empty; stopping playback");
                    self.stop_playback();
                    self.current_sentence_idx = None;
                    return Task::none();
                }
                self.stop_playback();
                if let Some(engine) = &self.tts_engine {
                    if let Ok(playback) =
                        engine.play_files(&files.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>())
                    {
                        self.tts_playback = Some(playback);
                        self.tts_track = files.clone();
                        self.current_sentence_idx =
                            Some(start_idx.min(files.len().saturating_sub(1)));
                        if let Some((_, dur)) = self.tts_track.first() {
                            let lead = Duration::from_millis(HIGHLIGHT_LEAD_MS);
                            let next_deadline = Instant::now() + dur.saturating_sub(lead);
                            self.tts_deadline = Some(next_deadline);
                            self.tts_running = true;
                            debug!(deadline = ?next_deadline, "Started TTS playback and highlighting");
                        }
                    } else {
                        warn!("Failed to start playback from prepared files");
                    }
                }
            }
        }

        if page_changed {
            save_last_page(&self.epub_path, self.current_page);
        }

        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let total_pages = self.pages.len().max(1);
        let page_label = format!("Page {} of {}", self.current_page + 1, total_pages);

        let theme_label = if self.night_mode {
            "Day Mode"
        } else {
            "Night Mode"
        };
        let theme_toggle = button(theme_label).on_press(Message::ToggleTheme);
        let settings_toggle = button(if self.settings_open {
            "Hide Settings"
        } else {
            "Show Settings"
        })
        .on_press(Message::ToggleSettings);
        let tts_toggle = button(if self.tts_open {
            "Hide TTS"
        } else {
            "Show TTS"
        })
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

        let text_view_content: Element<'_, Message> =
            if self.tts_playback.is_some() && !self.last_sentences.is_empty() {
                // Inline spans keep natural text flow while allowing highlight of the active sentence.
                let sentences = split_sentences(page_content.clone());
                if sentences.is_empty() {
                    return text(page_content)
                        .size(self.font_size as f32)
                        .line_height(LineHeight::Relative(self.line_spacing))
                        .width(Length::Fill)
                        .wrapping(Wrapping::WordOrGlyph)
                        .align_x(self.justification_alignment())
                        .font(self.current_font())
                        .into();
                }
                let highlight_idx = self
                    .current_sentence_idx
                    .unwrap_or(0)
                    .min(sentences.len().saturating_sub(1));
                let highlight = self.highlight_color();

                let spans: Vec<iced::widget::text::Span<'_, Message>> = sentences
                    .into_iter()
                    .enumerate()
                    .map(|(idx, sentence)| {
                        let mut span: iced::widget::text::Span<'_, Message> =
                            iced::widget::text::Span::new(format!("{sentence} "))
                                .font(self.current_font())
                                .size(self.font_size as f32)
                                .line_height(LineHeight::Relative(self.line_spacing));

                        if idx == highlight_idx {
                            span = span
                                .background(iced::Background::Color(highlight))
                                .padding(iced::Padding::from(2u16));
                        }

                        span
                    })
                    .collect();

                let rich: iced::widget::text::Rich<'_, Message> =
                    iced::widget::text::Rich::with_spans(spans);

                rich.width(Length::Fill)
                    .wrapping(Wrapping::WordOrGlyph)
                    .align_x(self.justification_alignment())
                    .into()
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

        let mut layout: Row<'_, Message> = row![container(content).width(Length::Fill)].spacing(16);

        if self.settings_open {
            layout = layout.push(self.settings_panel());
        }

        layout.into()
    }

    fn subscription(app: &App) -> Subscription<Message> {
        if app.tts_running {
            time::every(Duration::from_millis(50)).map(Message::Tick)
        } else {
            Subscription::none()
        }
    }

    fn highlight_color(&self) -> Color {
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

    fn save_epub_config(&self) {
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
        };

        crate::cache::save_epub_config(&self.epub_path, &config);
    }
}

fn color_row<'a>(
    label: &'a str,
    color: HighlightColor,
    msg: impl Fn(Component, f32) -> Message + Copy + 'a,
) -> Row<'a, Message> {
    row![
        text(label),
        slider(0.0..=1.0, color.r, move |v| msg(Component::R, v)).step(0.01),
        slider(0.0..=1.0, color.g, move |v| msg(Component::G, v)).step(0.01),
        slider(0.0..=1.0, color.b, move |v| msg(Component::B, v)).step(0.01),
        slider(0.0..=1.0, color.a, move |v| msg(Component::A, v)).step(0.01),
    ]
    .spacing(6)
    .align_y(Vertical::Center)
}

#[derive(Debug, Clone, Copy)]
pub enum Component {
    R,
    G,
    B,
    A,
}

fn apply_component(mut color: HighlightColor, component: Component, value: f32) -> HighlightColor {
    let clamped = value.clamp(0.0, 1.0);
    match component {
        Component::R => color.r = clamped,
        Component::G => color.g = clamped,
        Component::B => color.b = clamped,
        Component::A => color.a = clamped,
    }
    color
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
        .theme(|app: &App| {
            if app.night_mode {
                Theme::Dark
            } else {
                Theme::Light
            }
        })
        .run_with(move || {
            let font_size = config.font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
            let line_spacing = config.line_spacing.clamp(0.8, 2.5);
            let margin_horizontal = config.margin_horizontal.min(MAX_MARGIN);
            let margin_vertical = config.margin_vertical.min(MAX_MARGIN);
            let word_spacing = config.word_spacing.min(MAX_WORD_SPACING);
            let letter_spacing = config.letter_spacing.min(MAX_LETTER_SPACING);
            let lines_per_page = config
                .lines_per_page
                .clamp(MIN_LINES_PER_PAGE, MAX_LINES_PER_PAGE);

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
                tts_deadline: None,
                tts_running: false,
                day_highlight: config.day_highlight,
                night_highlight: config.night_highlight,
                tts_model_path: config.tts_model_path,
                tts_espeak_path: config.tts_espeak_path,
                log_level: config.log_level,
                tts_request_id: 0,
            };
            app.repaginate();
            if let Some(last) = last_page {
                app.current_page = last.min(app.pages.len().saturating_sub(1));
                info!(page = app.current_page + 1, "Restored last page from cache");
            } else {
                info!("Starting from first page");
            }
            info!(
                font_size = app.font_size,
                night_mode = app.night_mode,
                justification = ?app.justification,
                "Initialized app state"
            );
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

    fn settings_panel(&self) -> Element<'_, Message> {
        let family_picker = pick_list(
            FONT_FAMILIES,
            Some(self.font_family),
            Message::FontFamilyChanged,
        );
        let weight_picker = pick_list(
            FONT_WEIGHTS,
            Some(self.font_weight),
            Message::FontWeightChanged,
        );
        let justification_picker = pick_list(
            JUSTIFICATIONS,
            Some(self.justification),
            Message::JustificationChanged,
        );

        let line_spacing_slider =
            slider(0.8..=2.5, self.line_spacing, Message::LineSpacingChanged).step(0.05);
        let lines_per_page_slider = slider(
            MIN_LINES_PER_PAGE as f32..=MAX_LINES_PER_PAGE as f32,
            self.lines_per_page as f32,
            |value| Message::LinesPerPageChanged(value.round() as u32),
        )
        .step(1.0);

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
            row![text("Font family"), family_picker]
                .spacing(8)
                .align_y(Vertical::Center),
            row![text("Font weight"), weight_picker]
                .spacing(8)
                .align_y(Vertical::Center),
            row![text("Line spacing"), line_spacing_slider]
                .spacing(8)
                .align_y(Vertical::Center),
            row![
                text(format!("Lines per page: {}", self.lines_per_page)),
                lines_per_page_slider
            ]
            .spacing(8)
            .align_y(Vertical::Center),
            row![
                text(format!("Horizontal margin: {} px", self.margin_horizontal)),
                margin_slider
            ]
            .spacing(8)
            .align_y(Vertical::Center),
            row![
                text(format!("Vertical margin: {} px", self.margin_vertical)),
                margin_vertical_slider
            ]
            .spacing(8)
            .align_y(Vertical::Center),
            row![text("Justification"), justification_picker]
                .spacing(8)
                .align_y(Vertical::Center),
            row![
                text(format!("Word spacing: {}", self.word_spacing)),
                word_spacing_slider
            ]
            .spacing(8)
            .align_y(Vertical::Center),
            row![
                text(format!("Letter spacing: {}", self.letter_spacing)),
                letter_spacing_slider
            ]
            .spacing(8)
            .align_y(Vertical::Center),
            text("Highlight Colors").size(18.0),
            color_row("Day highlight", self.day_highlight, |c, v| {
                Message::DayHighlightChanged(c, v)
            },),
            color_row("Night highlight", self.night_highlight, |c, v| {
                Message::NightHighlightChanged(c, v)
            },),
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

    fn start_playback_from(&mut self, page: usize, sentence_idx: usize) -> Task<Message> {
        let Some(engine) = self.tts_engine.clone() else {
            return Task::none();
        };

        // Stop any existing playback before starting a new request.
        self.stop_playback();
        self.tts_track.clear();
        self.tts_track.clear();

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
        let cache_root = crate::cache::tts_dir(&self.epub_path);
        let speed = self.tts_speed;
        let threads = self.tts_threads.max(1);
        let page_id = page;
        self.tts_request_id = self.tts_request_id.wrapping_add(1);
        let request_id = self.tts_request_id;
        self.save_epub_config();
        info!(
            page = page + 1,
            sentence_idx, speed, threads, "Preparing playback task"
        );

        Task::perform(
            async move {
                engine
                    .prepare_batch(cache_root, sentences, sentence_idx, speed, threads)
                    .map(|files| Message::TtsPrepared {
                        page: page_id,
                        start_idx: sentence_idx,
                        request_id,
                        files,
                    })
                    .unwrap_or_else(|_| Message::TtsPrepared {
                        page: page_id,
                        start_idx: sentence_idx,
                        request_id,
                        files: Vec::new(),
                    })
            },
            |msg| msg,
        )
    }
}
