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

/// Helper to launch the app in starter mode (no book path yet).
pub fn run_app_starter(config: AppConfig) -> iced::Result {
    const STARTER_MIN_WIDTH: f32 = 1600.0;
    const STARTER_MIN_HEIGHT: f32 = 780.0;

    let window_settings = window::Settings {
        size: Size::new(
            config.window_width.max(STARTER_MIN_WIDTH),
            config.window_height.max(STARTER_MIN_HEIGHT),
        ),
        min_size: Some(Size::new(STARTER_MIN_WIDTH, STARTER_MIN_HEIGHT)),
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
        .run_with(move || App::bootstrap_starter(config))
}
