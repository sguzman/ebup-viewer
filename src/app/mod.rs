mod messages;
mod state;
mod update;
mod view;

pub use state::App;

use crate::cache::Bookmark;
use crate::config::AppConfig;
use crate::epub_loader::LoadedBook;
use iced::{Point, Size, Theme, window};

/// Helper to launch the app with the provided text.
pub fn run_app(
    book: LoadedBook,
    config: AppConfig,
    epub_path: std::path::PathBuf,
    bookmark: Option<Bookmark>,
) -> iced::Result {
    let window_settings = window::Settings {
        size: Size::new(config.window_width, config.window_height),
        position: match (config.window_pos_x, config.window_pos_y) {
            (Some(x), Some(y)) if x.is_finite() && y.is_finite() => {
                window::Position::Specific(Point::new(x, y))
            }
            _ => window::Position::Default,
        },
        ..window::Settings::default()
    };

    iced::application("EPUB Viewer", App::update, App::view)
        .window(window_settings)
        .subscription(App::subscription)
        .theme(|app: &App| {
            if matches!(app.config.theme, crate::config::ThemeMode::Night) {
                Theme::Dark
            } else {
                Theme::Light
            }
        })
        .run_with(move || App::bootstrap(book, config, epub_path, bookmark))
}
