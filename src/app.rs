//! UI layer for the EPUB viewer.
//!
//! This module owns all GUI state and messages. It expects the caller to
//! provide the already-loaded plain text (see `epub_loader`) and relies on
//! `pagination` to break that text into pages based on the current font size.

use crate::pagination::{paginate, MAX_FONT_SIZE, MIN_FONT_SIZE};
use iced::widget::{button, column, row, scrollable, slider, text};
use iced::{Alignment, Element, Length, Task, Theme};

/// Default font size used on startup.
const DEFAULT_FONT_SIZE: u32 = 16;

/// Messages emitted by the UI.
#[derive(Debug, Clone)]
pub enum Message {
    NextPage,
    PreviousPage,
    FontSizeChanged(u32),
}

/// Core application state.
pub struct App {
    full_text: String,
    pages: Vec<String>,
    current_page: usize,
    font_size: u32,
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
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let total_pages = self.pages.len().max(1);
        let page_label = format!("Page {} of {}", self.current_page + 1, total_pages);

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

        let controls = row![prev_button, next_button, text(page_label)]
            .spacing(10)
            .align_y(Alignment::Center);

        let font_label = text(format!("Font size: {}", self.font_size));
        let font_slider = slider(
            MIN_FONT_SIZE as f32..=MAX_FONT_SIZE as f32,
            self.font_size as f32,
            |value| Message::FontSizeChanged(value.round() as u32),
        );

        let font_controls = row![font_label, font_slider]
            .spacing(10)
            .align_y(Alignment::Center);

        let page_content = self
            .pages
            .get(self.current_page)
            .map(String::as_str)
            .unwrap_or("");

        let text_view = scrollable(
            text(page_content)
                .size(self.font_size as f32)
                .width(Length::Fill),
        )
        .height(Length::Fill);

        column![controls, font_controls, text_view]
            .padding(16)
            .spacing(12)
            .into()
    }
}

/// Helper to launch the app with the provided text.
pub fn run_app(text: String) -> iced::Result {
    iced::application("EPUB Viewer", App::update, App::view)
        .theme(|_| Theme::Dark)
        .run_with(move || {
            let mut app = App {
                pages: Vec::new(),
                full_text: text,
                current_page: 0,
                font_size: DEFAULT_FONT_SIZE,
            };
            app.repaginate();
            (app, Task::none())
        })
}
