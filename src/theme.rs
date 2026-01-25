//! Custom theme for the EPUB viewer.

use iced::Theme as IcedTheme;

/// Custom theme for the EPUB viewer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::Light
    }
}

impl From<crate::config::ThemeMode> for Theme {
    fn from(mode: crate::config::ThemeMode) -> Self {
        match mode {
            crate::config::ThemeMode::Night => Theme::Dark,
            crate::config::ThemeMode::Day => Theme::Light,
        }
    }
}

impl From<Theme> for IcedTheme {
    fn from(theme: Theme) -> Self {
        match theme {
            Theme::Light => IcedTheme::Light,
            Theme::Dark => IcedTheme::Dark,
        }
    }
}
