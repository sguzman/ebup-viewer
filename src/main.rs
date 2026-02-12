//! Entry point for the EPUB viewer.
//!
//! Responsibilities here are intentionally minimal:
//! - Parse command-line arguments.
//! - Load the EPUB text via `epub_loader`.
//! - Load user configuration from `conf/config.toml`.
//! - Launch the GUI application with the loaded text and config.

mod app;
mod cache;
mod calibre;
mod config;
mod epub_loader;
mod normalizer;
mod pagination;
mod text_utils;
mod tts;
mod tts_worker;

use crate::app::{run_app, run_app_starter};
use crate::cache::{load_bookmark, load_epub_config, remember_source_path};
use crate::config::load_config;
use crate::epub_loader::load_book_content;
use anyhow::{Context, Result, anyhow};
use std::env;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{error, info, warn};
use tracing_subscriber::{EnvFilter, fmt, prelude::*, reload};

type ReloadHandle = reload::Handle<EnvFilter, tracing_subscriber::Registry>;
static SIGINT_REQUESTED: AtomicBool = AtomicBool::new(false);

fn main() {
    if tts_worker::maybe_run_worker() {
        return;
    }
    install_signal_handlers();
    let reload_handle = init_tracing();
    if let Err(err) = run(&reload_handle) {
        error!("{err:?}");
        std::process::exit(1);
    }
}

pub(crate) fn take_sigint_requested() -> bool {
    SIGINT_REQUESTED.swap(false, Ordering::SeqCst)
}

fn install_signal_handlers() {
    if let Err(err) = ctrlc::set_handler(|| {
        SIGINT_REQUESTED.store(true, Ordering::SeqCst);
    }) {
        eprintln!("Failed to install Ctrl+C handler: {err}");
    }
}

fn run(reload_handle: &ReloadHandle) -> Result<()> {
    let base_config = load_config(Path::new("conf/config.toml"));
    let path_arg = parse_args()?;

    let Some(epub_path) = path_arg else {
        set_log_level(reload_handle, base_config.log_level.as_filter_str());
        info!(
            level = %base_config.log_level,
            "Starting EPUB viewer in starter mode"
        );
        run_app_starter(base_config).context("Failed to start the starter GUI")?;
        return Ok(());
    };

    remember_source_path(&epub_path);
    let mut config = base_config.clone();
    if let Some(mut overrides) = load_epub_config(&epub_path) {
        info!("Loaded per-epub overrides from cache");
        // Always honor the base config's log level so user changes take effect.
        overrides.log_level = base_config.log_level;
        // Always honor base TTS worker count to avoid stale cached values.
        overrides.tts_threads = base_config.tts_threads;
        // Always honor base progress logging cadence for batch generation.
        overrides.tts_progress_log_interval_secs = base_config.tts_progress_log_interval_secs;
        // Always honor base keybinding configuration.
        overrides.key_toggle_play_pause = base_config.key_toggle_play_pause.clone();
        overrides.key_safe_quit = base_config.key_safe_quit.clone();
        overrides.key_next_sentence = base_config.key_next_sentence.clone();
        overrides.key_prev_sentence = base_config.key_prev_sentence.clone();
        overrides.key_repeat_sentence = base_config.key_repeat_sentence.clone();
        overrides.key_toggle_search = base_config.key_toggle_search.clone();
        overrides.key_toggle_settings = base_config.key_toggle_settings.clone();
        overrides.key_toggle_stats = base_config.key_toggle_stats.clone();
        overrides.key_toggle_tts = base_config.key_toggle_tts.clone();
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
        progress_log_interval_secs = config.tts_progress_log_interval_secs,
        "Active TTS configuration"
    );
    let bookmark = load_bookmark(&epub_path);
    if let Some(bm) = &bookmark {
        info!(page = bm.page, "Resuming from cached page");
    }
    let book = load_book_content(&epub_path)?;
    run_app(book, config, epub_path, bookmark).context("Failed to start the GUI")?;
    Ok(())
}

fn parse_args() -> Result<Option<PathBuf>> {
    let mut args = env::args().skip(1);
    let Some(path) = args.next() else {
        return Ok(None);
    };

    let path = PathBuf::from(path);
    if !path.exists() {
        return Err(anyhow!("File not found: {}", path.as_path().display()));
    }
    Ok(Some(path))
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
