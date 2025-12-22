//! Configuration loading for the EPUB viewer.
//!
//! All user-tunable settings are centralized here and loaded from
//! `conf/config.toml` if present. Any missing or invalid entries fall back to
//! sensible defaults so the UI can still launch.

mod defaults;
mod io;
mod models;
mod tables;

pub use io::{load_config, parse_config, serialize_config};
pub use models::{AppConfig, FontFamily, FontWeight, HighlightColor, LogLevel, ThemeMode};
