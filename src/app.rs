//! UI layer for the EPUB viewer.
//!
//! This module owns all GUI state and messages. It expects the caller to
//! provide the already-loaded plain text (see `epub_loader`) and relies on
//! `pagination` to break that text into pages based on the current font size.

use crate::pagination::{paginate, MAX_FONT_SIZE, MIN_FONT_SIZE};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{
    button, column, container, pick_list, row, scrollable, slider, text, Column, Row,
};
use iced::widget::text::LineHeight;
use iced::{Element, Font, Length, Task, Theme};
use iced::font::{Family, Weight};

/// Default font size used on startup.
const DEFAULT_FONT_SIZE: u32 = 16;
const DEFAULT_LINE_SPACING: f32 = 1.2;
const DEFAULT_MARGIN: u16 = 12;
const MAX_MARGIN: u16 = 48;
const MAX_WORD_SPACING: u32 = 5;
const MAX_LETTER_SPACING: u32 = 3;

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
    MarginChanged(u16),
    JustificationChanged(Justification),
    WordSpacingChanged(u32),
    LetterSpacingChanged(u32),
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
    margin: u16,
    justification: Justification,
    word_spacing: u32,
    letter_spacing: u32,
}

impl App {
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
        match message {
            Message::NextPage => {
                if self.current_page + 1 < self.pages.len() {
                    self.current_page += 1;
                }
            }
            Message::PreviousPage => {
                if self.current_page > 0 {
                    self.current_page -= 1;
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
            Message::MarginChanged(margin) => {
                self.margin = margin.min(MAX_MARGIN);
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

        let text_view = scrollable(
            container(
                text(page_content)
                    .size(self.font_size as f32)
                    .line_height(LineHeight::Relative(self.line_spacing))
                    .width(Length::Fill)
                    .align_x(self.justification.as_alignment())
                    .font(self.current_font()),
            )
            .padding(self.margin as u16),
        )
        .height(Length::Fill);

        let content: Column<'_, Message> = column![controls, font_controls, text_view]
            .padding(16)
            .spacing(12);

        let mut layout: Row<'_, Message> = row![content].spacing(16);

        if self.settings_open {
            layout = layout.push(self.settings_panel());
        }

        layout.into()
    }
}

/// Helper to launch the app with the provided text.
pub fn run_app(text: String) -> iced::Result {
    iced::application("EPUB Viewer", App::update, App::view)
        .theme(|app: &App| if app.night_mode { Theme::Dark } else { Theme::Light })
        .run_with(move || {
            let mut app = App {
                pages: Vec::new(),
                full_text: text,
                current_page: 0,
                font_size: DEFAULT_FONT_SIZE,
                night_mode: true,
                settings_open: false,
                font_family: FontFamily::Sans,
                font_weight: FontWeight::Normal,
                line_spacing: DEFAULT_LINE_SPACING,
                margin: DEFAULT_MARGIN,
                justification: Justification::Left,
                word_spacing: 0,
                letter_spacing: 0,
            };
            app.repaginate();
            (app, Task::none())
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFamily {
    Sans,
    Serif,
    Monospace,
}

impl std::fmt::Display for FontFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FontFamily::Sans => write!(f, "Sans"),
            FontFamily::Serif => write!(f, "Serif"),
            FontFamily::Monospace => write!(f, "Monospace"),
        }
    }
}

impl FontFamily {
    const ALL: [FontFamily; 3] = [FontFamily::Sans, FontFamily::Serif, FontFamily::Monospace];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontWeight {
    Light,
    Normal,
    Bold,
}

impl std::fmt::Display for FontWeight {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FontWeight::Light => write!(f, "Light"),
            FontWeight::Normal => write!(f, "Normal"),
            FontWeight::Bold => write!(f, "Bold"),
        }
    }
}

impl FontWeight {
    const ALL: [FontWeight; 3] = [FontWeight::Light, FontWeight::Normal, FontWeight::Bold];

    fn to_weight(self) -> Weight {
        match self {
            FontWeight::Light => Weight::Light,
            FontWeight::Normal => Weight::Normal,
            FontWeight::Bold => Weight::Bold,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Justification {
    Left,
    Center,
    Right,
}

impl std::fmt::Display for Justification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Justification::Left => write!(f, "Left"),
            Justification::Center => write!(f, "Center"),
            Justification::Right => write!(f, "Right"),
        }
    }
}

impl Justification {
    const ALL: [Justification; 3] =
        [Justification::Left, Justification::Center, Justification::Right];

    fn as_alignment(self) -> Horizontal {
        match self {
            Justification::Left => Horizontal::Left,
            Justification::Center => Horizontal::Center,
            Justification::Right => Horizontal::Right,
        }
    }
}

impl App {
    fn current_font(&self) -> Font {
        let family = match self.font_family {
            FontFamily::Sans => Family::SansSerif,
            FontFamily::Serif => Family::Serif,
            FontFamily::Monospace => Family::Monospace,
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

        if self.word_spacing == 0 && self.letter_spacing == 0 {
            return base.to_string();
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
        let family_picker =
            pick_list(FontFamily::ALL, Some(self.font_family), Message::FontFamilyChanged);
        let weight_picker =
            pick_list(FontWeight::ALL, Some(self.font_weight), Message::FontWeightChanged);
        let justification_picker =
            pick_list(Justification::ALL, Some(self.justification), Message::JustificationChanged);

        let line_spacing_slider = slider(
            0.8..=2.5,
            self.line_spacing,
            Message::LineSpacingChanged,
        );

        let margin_slider = slider(
            0.0..=MAX_MARGIN as f32,
            self.margin as f32,
            |value| Message::MarginChanged(value.round() as u16),
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
            row![text(format!("Margins: {} px", self.margin)), margin_slider]
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
}
