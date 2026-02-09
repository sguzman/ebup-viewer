//! Entry point for the EPUB viewer.
//!
//! Responsibilities here are intentionally minimal:
//! - Parse command-line arguments.
//! - Load the EPUB text via `epub_loader`.
//! - Load user configuration from `conf/config.toml`.
//! - Launch the GUI application with the loaded text and config.

mod app;
mod cache;
mod config;
mod epub_loader;
mod normalizer;
mod pagination;
mod text_utils;
mod tts;
mod tts_worker;

use crate::app::run_app;
use crate::cache::{load_bookmark, load_epub_config};
use crate::config::load_config;
use crate::epub_loader::load_epub_text;
use anyhow::{Context, Result, anyhow};
use std::env;
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};
use tracing_subscriber::{EnvFilter, fmt, prelude::*, reload};

type ReloadHandle = reload::Handle<EnvFilter, tracing_subscriber::Registry>;

fn main() {
    if tts_worker::maybe_run_worker() {
        return;
    }
    let reload_handle = init_tracing();
    if let Err(err) = run(&reload_handle) {
        error!("{err:?}");
        std::process::exit(1);
    }
}

fn run(reload_handle: &ReloadHandle) -> Result<()> {
    let epub_path = parse_args()?;
    let base_config = load_config(Path::new("conf/config.toml"));
    let mut config = base_config.clone();
    if let Some(mut overrides) = load_epub_config(&epub_path) {
        info!("Loaded per-epub overrides from cache");
        // Always honor the base config's log level so user changes take effect.
        overrides.log_level = base_config.log_level;
        // Always honor base TTS worker count to avoid stale cached values.
        overrides.tts_threads = base_config.tts_threads;
        config = overrides;
    }
    set_log_level(reload_handle, config.log_level.as_filter_str());
    info!(
        path = %epub_path.display(),
        level = %config.log_level,
        "Starting EPUB viewer"
    );
    info!(path = %epub_path.display(), "Opening EPUB");
    info!(
        model = %config.tts_model_path,
        espeak = %config.tts_espeak_path,
        threads = config.tts_threads,
        "Active TTS configuration"
    );
    let bookmark = load_bookmark(&epub_path);
    if let Some(bm) = &bookmark {
        info!(page = bm.page, "Resuming from cached page");
    }
    let text = load_epub_text(&epub_path)?;
    run_app(text, config, epub_path, bookmark).context("Failed to start the GUI")?;
    Ok(())
}

fn parse_args() -> Result<PathBuf> {
    let mut args = env::args().skip(1);
    let path = args
        .next()
        .ok_or_else(|| anyhow!("Usage: epub-viewer <path-to-book>"))?;

    let path = PathBuf::from(path);
    if !path.exists() {
        return Err(anyhow!("File not found: {}", path.as_path().display()));
    }
    Ok(path)
}

fn init_tracing() -> ReloadHandle {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"));
    let (filter_layer, handle) = reload::Layer::new(env_filter);
    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_target(true)
                .with_file(true)
                .with_line_number(true)
                .with_filter(filter_layer),
        )
        .init();
    warn!("Logging initialized; override level with config.log_level or RUST_LOG");
    handle
}

fn set_log_level(handle: &ReloadHandle, level: &str) {
    let parsed = EnvFilter::builder()
        .parse(level)
        .unwrap_or_else(|_| EnvFilter::new("debug"));
    if let Err(err) = handle.modify(|filter| *filter = parsed.clone()) {
        warn!(%level, "Failed to update log level from config: {err}");
    } else {
        info!(%level, "Applied log level from config");
    }
}
