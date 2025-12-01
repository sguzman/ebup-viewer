mod messages;
mod state;
mod update;
mod view;

pub use state::App;

use crate::config::AppConfig;
use iced::Theme;

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
        .run_with(move || App::bootstrap(text, config, epub_path, last_page))
}
