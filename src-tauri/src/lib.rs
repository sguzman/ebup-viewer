use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_log::{Target, TargetKind, log::LevelFilter};
use tracing::{info, warn};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt};
use ts_rs::TS;

pub use lanternleaf_core::{
    browser_tabs, cache, calibre, config, epub_loader, normalizer, pagination, quack_check,
    text_utils, tts,
};
use lanternleaf_core::{cancellation, session};

const MAX_RECENT_LIMIT: usize = 512;
const DEFAULT_RECENT_LIMIT: usize = 64;
const TTS_PROGRESS_POLL_INTERVAL: Duration = Duration::from_millis(8);
const TTS_PREPARE_SENTENCE_WINDOW: usize = 8;

static TRACING_LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();
static EVENT_EMISSION_TELEMETRY: OnceLock<Mutex<EventEmissionTelemetry>> = OnceLock::new();

#[derive(Debug, Default)]
struct EventEmissionTelemetry {
    last_reader_emit: Option<Instant>,
    last_tts_emit: Option<Instant>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
enum UiMode {
    Starter,
    Reader,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
struct BootstrapConfig {
    theme: config::ThemeMode,
    font_family: config::FontFamily,
    font_weight: config::FontWeight,
    day_highlight: config::HighlightColor,
    night_highlight: config::HighlightColor,
    log_level: String,
    default_font_size: u32,
    default_lines_per_page: usize,
    default_tts_speed: f32,
    default_pause_after_sentence: f32,
    key_toggle_play_pause: String,
    key_next_sentence: String,
    key_prev_sentence: String,
    key_repeat_sentence: String,
    key_toggle_search: String,
    key_safe_quit: String,
    key_toggle_settings: String,
    key_toggle_stats: String,
    key_toggle_tts: String,
    browser_tabs_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
struct BootstrapState {
    app_name: String,
    mode: String,
    config: BootstrapConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
struct SessionState {
    mode: UiMode,
    active_source_path: Option<String>,
    open_in_flight: bool,
    panels: session::PanelState,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
struct OpenSourceResult {
    session: SessionState,
    reader: session::ReaderSnapshot,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
struct RecentBook {
    source_path: String,
    display_title: String,
    snippet: String,
    thumbnail_path: Option<String>,
    #[ts(type = "number")]
    last_opened_unix_secs: u64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
struct CalibreBookDto {
    #[ts(type = "number")]
    id: u64,
    title: String,
    extension: String,
    authors: String,
    year: Option<i32>,
    #[ts(type = "number | null")]
    file_size_bytes: Option<u64>,
    source_path: Option<String>,
    cover_thumbnail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
struct SourceOpenEvent {
    #[ts(type = "number")]
    request_id: u64,
    phase: String,
    source_path: Option<String>,
    message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
struct CalibreLoadEvent {
    #[ts(type = "number")]
    request_id: u64,
    phase: String,
    count: Option<usize>,
    message: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
struct TtsStateEvent {
    #[ts(type = "number")]
    request_id: u64,
    action: String,
    tts: session::ReaderTtsView,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
struct PdfTranscriptionEvent {
    #[ts(type = "number")]
    request_id: u64,
    phase: String,
    source_path: String,
    message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
struct LogLevelEvent {
    #[ts(type = "number")]
    request_id: u64,
    level: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
struct SessionStateEvent {
    #[ts(type = "number")]
    request_id: u64,
    action: String,
    session: SessionState,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
struct ReaderStateEvent {
    #[ts(type = "number")]
    request_id: u64,
    action: String,
    reader: session::ReaderSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
struct BridgeError {
    code: String,
    message: String,
}

#[derive(Debug, Clone)]
struct TtsRequestRuntime {
    request_id: u64,
    cancel_token: cancellation::CancellationToken,
    pause_requested: Arc<AtomicBool>,
}

impl TtsRequestRuntime {
    fn set_paused(&self, paused: bool) {
        self.pause_requested.store(paused, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone)]
struct TtsPlaybackPlan {
    source_path: PathBuf,
    page: usize,
    sentences: Vec<String>,
    start_idx: usize,
    pause_after: Duration,
    speed: f32,
    volume: f32,
    threads: usize,
    progress_log_interval: Duration,
    model_path: PathBuf,
    espeak_path: PathBuf,
}

#[derive(Debug)]
struct BackendState {
    mode: UiMode,
    active_source_path: Option<PathBuf>,
    active_open_source_path: Option<PathBuf>,
    open_in_flight: bool,
    active_open_request: Option<u64>,
    open_cancel_token: Option<cancellation::CancellationToken>,
    calibre_load_request: Option<u64>,
    calibre_cancel_token: Option<cancellation::CancellationToken>,
    tts_request: Option<TtsRequestRuntime>,
    next_request_id: u64,
    panels: session::PanelState,
    base_config: config::AppConfig,
    normalizer: normalizer::TextNormalizer,
    reader: Option<session::ReaderSession>,
    calibre_config: calibre::CalibreConfig,
    calibre_books: Vec<calibre::CalibreBook>,
}

impl BackendState {
    fn new() -> Self {
        let config_path = app_config_path();
        let base_config = config::load_config(&config_path);
        let panels = panels_from_config(&base_config);
        Self {
            mode: UiMode::Starter,
            active_source_path: None,
            active_open_source_path: None,
            open_in_flight: false,
            active_open_request: None,
            open_cancel_token: None,
            calibre_load_request: None,
            calibre_cancel_token: None,
            tts_request: None,
            next_request_id: 1,
            panels,
            base_config,
            normalizer: normalizer::TextNormalizer::load_default(),
            reader: None,
            calibre_config: calibre::CalibreConfig::load_default(),
            calibre_books: Vec::new(),
        }
    }
}

fn panels_from_config(cfg: &config::AppConfig) -> session::PanelState {
    let show_stats = cfg.show_stats;
    let show_settings = if show_stats { false } else { cfg.show_settings };
    session::PanelState {
        show_settings,
        show_stats,
        show_tts: cfg.show_tts,
    }
}

fn to_session_state(state: &BackendState) -> SessionState {
    SessionState {
        mode: state.mode,
        active_source_path: state
            .active_source_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        open_in_flight: state.open_in_flight,
        panels: state.panels,
    }
}

fn bridge_error(code: &str, message: impl Into<String>) -> BridgeError {
    BridgeError {
        code: code.to_string(),
        message: message.into(),
    }
}

fn runtime_mode_label() -> String {
    let tauri_env = std::env::var("TAURI_ENV")
        .ok()
        .map(|value| value.to_ascii_lowercase());
    let tauri_dev = std::env::var("TAURI_DEV")
        .ok()
        .map(|value| value.to_ascii_lowercase());
    let forced_dev = matches!(tauri_dev.as_deref(), Some("1") | Some("true") | Some("yes"))
        || matches!(tauri_env.as_deref(), Some("dev") | Some("development"));

    if cfg!(dev) || forced_dev {
        "dev".to_string()
    } else {
        "release".to_string()
    }
}

fn bootstrap_state_from_backend(guard: &BackendState) -> BootstrapState {
    BootstrapState {
        app_name: "LanternLeaf".to_string(),
        mode: runtime_mode_label(),
        config: BootstrapConfig {
            theme: guard.base_config.theme,
            font_family: guard.base_config.font_family,
            font_weight: guard.base_config.font_weight,
            day_highlight: guard.base_config.day_highlight,
            night_highlight: guard.base_config.night_highlight,
            log_level: guard.base_config.log_level.as_filter_str().to_string(),
            default_font_size: guard.base_config.font_size,
            default_lines_per_page: guard.base_config.lines_per_page,
            default_tts_speed: guard.base_config.tts_speed,
            default_pause_after_sentence: guard.base_config.pause_after_sentence,
            key_toggle_play_pause: guard.base_config.key_toggle_play_pause.clone(),
            key_next_sentence: guard.base_config.key_next_sentence.clone(),
            key_prev_sentence: guard.base_config.key_prev_sentence.clone(),
            key_repeat_sentence: guard.base_config.key_repeat_sentence.clone(),
            key_toggle_search: guard.base_config.key_toggle_search.clone(),
            key_safe_quit: guard.base_config.key_safe_quit.clone(),
            key_toggle_settings: guard.base_config.key_toggle_settings.clone(),
            key_toggle_stats: guard.base_config.key_toggle_stats.clone(),
            key_toggle_tts: guard.base_config.key_toggle_tts.clone(),
            browser_tabs_enabled: guard.base_config.browser_tabs_enabled,
        },
    }
}

fn workspace_root_from_cwd() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    if cwd.file_name().and_then(|name| name.to_str()) == Some("src-tauri") {
        cwd.parent().map(Path::to_path_buf)
    } else {
        Some(cwd)
    }
}

fn configure_cache_dir_from_config(config: &config::AppConfig, config_path: &Path) {
    if std::env::var_os(cache::CACHE_DIR_ENV).is_some() {
        return;
    }

    let configured = config.cache_dir.trim();
    if configured.is_empty() {
        return;
    }

    let candidate = PathBuf::from(configured);
    let workspace_root = workspace_root_from_cwd();
    let resolved = if candidate.is_absolute() {
        candidate
    } else if let Some(root) = workspace_root {
        root.join(candidate)
    } else {
        config_path
            .parent()
            .map(|parent| parent.join(&candidate))
            .unwrap_or(candidate)
    };

    if let Err(err) = fs::create_dir_all(&resolved) {
        warn!(cache_dir = %resolved.display(), "Failed to create configured cache dir: {err}");
        return;
    }

    // SAFETY: startup-time process env initialization before background worker threads are launched.
    unsafe {
        std::env::set_var(cache::CACHE_DIR_ENV, &resolved);
    }

    info!(
        cache_dir = %resolved.display(),
        "Configured cache root from config"
    );
}

fn configure_cache_dir_from_workspace() {
    if std::env::var_os(cache::CACHE_DIR_ENV).is_some() {
        return;
    }

    let Some(root) = workspace_root_from_cwd() else {
        return;
    };

    let cache_candidate = root.join(cache::CACHE_DIR);

    if !cache_candidate.exists() {
        return;
    }

    // SAFETY: startup-time process env initialization before background worker threads are launched.
    unsafe {
        std::env::set_var(cache::CACHE_DIR_ENV, &cache_candidate);
    }

    info!(
        cache_dir = %cache_candidate.display(),
        "Configured cache root from workspace context"
    );
}

fn configure_calibre_config_path_from_workspace() {
    if std::env::var_os("CALIBRE_CONFIG_PATH").is_some() {
        return;
    }

    let Some(root) = workspace_root_from_cwd() else {
        return;
    };

    let calibre_config_path = root.join("conf/calibre.toml");
    if !calibre_config_path.exists() {
        return;
    }

    // SAFETY: startup-time process env initialization before background worker threads are launched.
    unsafe {
        std::env::set_var("CALIBRE_CONFIG_PATH", &calibre_config_path);
    }

    info!(
        path = %calibre_config_path.display(),
        "Configured calibre config path from workspace context"
    );
}

fn configure_normalizer_config_path_from_workspace() {
    if std::env::var_os("LANTERNLEAF_NORMALIZER_CONFIG_PATH").is_some() {
        return;
    }

    let Some(root) = workspace_root_from_cwd() else {
        return;
    };

    let normalizer_config_path = root.join("conf/normalizer.toml");
    if !normalizer_config_path.exists() {
        return;
    }

    // SAFETY: startup-time process env initialization before background worker threads are launched.
    unsafe {
        std::env::set_var(
            "LANTERNLEAF_NORMALIZER_CONFIG_PATH",
            &normalizer_config_path,
        );
    }

    info!(
        path = %normalizer_config_path.display(),
        "Configured normalizer config path from workspace context"
    );
}

fn configure_abbreviations_config_path_from_workspace() {
    if std::env::var_os("LANTERNLEAF_ABBREVIATIONS_CONFIG_PATH").is_some() {
        return;
    }

    let Some(root) = workspace_root_from_cwd() else {
        return;
    };

    let abbreviations_config_path = root.join("conf/abbreviations.toml");
    if !abbreviations_config_path.exists() {
        return;
    }

    // SAFETY: startup-time process env initialization before background worker threads are launched.
    unsafe {
        std::env::set_var(
            "LANTERNLEAF_ABBREVIATIONS_CONFIG_PATH",
            &abbreviations_config_path,
        );
    }

    info!(
        path = %abbreviations_config_path.display(),
        "Configured abbreviations config path from workspace context"
    );
}

fn dev_logs_dir() -> PathBuf {
    if let Some(root) = workspace_root_from_cwd() {
        root.join("logs")
    } else {
        PathBuf::from("logs")
    }
}

fn app_config_path() -> PathBuf {
    let workspace_root = workspace_root_from_cwd();

    if let Some(value) = std::env::var_os("LANTERNLEAF_CONFIG_PATH") {
        let candidate = PathBuf::from(value);
        return if candidate.is_absolute() {
            candidate
        } else if let Some(root) = workspace_root {
            root.join(candidate)
        } else {
            candidate
        };
    }

    if let Some(root) = workspace_root {
        return root.join("conf/config.toml");
    }

    PathBuf::from("conf/config.toml")
}
fn parse_log_level_label(label: &str) -> Option<config::LogLevel> {
    match label.trim().to_ascii_lowercase().as_str() {
        "trace" => Some(config::LogLevel::Trace),
        "debug" => Some(config::LogLevel::Debug),
        "info" => Some(config::LogLevel::Info),
        "warn" | "warning" => Some(config::LogLevel::Warn),
        "error" => Some(config::LogLevel::Error),
        _ => None,
    }
}

fn log_level_to_filter(level: config::LogLevel) -> LevelFilter {
    match level {
        config::LogLevel::Trace => LevelFilter::Trace,
        config::LogLevel::Debug => LevelFilter::Debug,
        config::LogLevel::Info => LevelFilter::Info,
        config::LogLevel::Warn => LevelFilter::Warn,
        config::LogLevel::Error => LevelFilter::Error,
    }
}

fn log_timestamp_slug() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(now) => format!("{}-{:03}", now.as_secs(), now.subsec_millis()),
        Err(_) => "0-000".to_string(),
    }
}

fn init_tracing(config: &config::AppConfig, timestamp_slug: &str) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(config.log_level.as_filter_str()));

    if runtime_mode_label() == "dev" {
        let logs_dir = dev_logs_dir();
        if let Err(err) = fs::create_dir_all(&logs_dir) {
            eprintln!(
                "failed to create tracing logs dir {}: {err}",
                logs_dir.display()
            );
        }

        let tracing_file_name = format!("lanternleaf-dev-{timestamp_slug}.log");
        let file_appender = tracing_appender::rolling::never(&logs_dir, tracing_file_name);
        let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
        let _ = TRACING_LOG_GUARD.set(guard);

        let stderr_layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_target(true)
            .with_file(true)
            .with_line_number(true);
        let file_layer = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(file_writer)
            .with_target(true)
            .with_file(true)
            .with_line_number(true);

        let subscriber = tracing_subscriber::registry()
            .with(filter)
            .with(stderr_layer)
            .with(file_layer);
        let _ = tracing::subscriber::set_global_default(subscriber);
    } else {
        let subscriber = tracing_subscriber::registry().with(filter).with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(true)
                .with_file(true)
                .with_line_number(true),
        );
        let _ = tracing::subscriber::set_global_default(subscriber);
    }
}

fn normalize_recent_limit(limit: Option<usize>) -> usize {
    limit
        .unwrap_or(DEFAULT_RECENT_LIMIT)
        .clamp(1, MAX_RECENT_LIMIT)
}

fn is_supported_source(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase()),
        Some(ext)
            if ext == "epub"
                || ext == "pdf"
                || ext == "txt"
                || ext == "md"
                || ext == "markdown"
                || ext == "html"
                || ext == "doc"
                || ext == "docx"
                || ext == "lltab"
    )
}

fn resolve_source_path(path: &str) -> Result<PathBuf, BridgeError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(bridge_error("invalid_input", "Path cannot be empty"));
    }

    let candidate = PathBuf::from(trimmed);
    if !candidate.exists() {
        return Err(bridge_error(
            "not_found",
            format!("Source path does not exist: {trimmed}"),
        ));
    }

    if !candidate.is_file() {
        return Err(bridge_error(
            "invalid_input",
            format!("Source path is not a file: {trimmed}"),
        ));
    }

    if !is_supported_source(&candidate) {
        let extension = candidate
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .unwrap_or_else(|| "<none>".to_string());
        warn!(
            path = %candidate.display(),
            extension,
            "Rejected unsupported source extension"
        );
        return Err(bridge_error(
            "unsupported_source",
            format!(
                "Unsupported source type for {} (expected .epub/.pdf/.txt/.md/.markdown/.html/.doc/.docx/.lltab)",
                candidate.display()
            ),
        ));
    }

    candidate.canonicalize().map_err(|err| {
        bridge_error(
            "io_error",
            format!(
                "Failed to canonicalize source path {}: {err}",
                candidate.display()
            ),
        )
    })
}

fn browsr_client_from_config(
    cfg: &config::AppConfig,
) -> Result<browser_tabs::BrowsrClient, BridgeError> {
    browser_tabs::BrowsrClient::new(&cfg.browsr_base_url, cfg.browsr_timeout_ms)
        .map_err(|err| bridge_error("browsr_config_error", err.to_string()))
}

fn thumbnail_path_to_data_url(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let encoded = BASE64_STANDARD.encode(bytes);
    let mime = match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
    {
        Some(ext) if ext == "png" => "image/png",
        Some(ext) if ext == "webp" => "image/webp",
        _ => "image/jpeg",
    };
    Some(format!("data:{};base64,{}", mime, encoded))
}

fn map_calibre_book(book: calibre::CalibreBook) -> CalibreBookDto {
    CalibreBookDto {
        id: book.id,
        title: book.title,
        extension: book.extension,
        authors: book.authors,
        year: book.year,
        file_size_bytes: book.file_size_bytes,
        source_path: book.path.map(|path| path.to_string_lossy().to_string()),
        cover_thumbnail: book
            .cover_thumbnail
            .as_deref()
            .and_then(thumbnail_path_to_data_url),
    }
}

fn export_single_type<T: TS + 'static>(out_dir: &Path) -> Result<(), String> {
    T::export_all_to(out_dir).map_err(|err| err.to_string())
}

pub fn export_ts_bindings(out_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(out_dir)
        .map_err(|err| format!("Failed to create {}: {err}", out_dir.display()))?;

    for entry in fs::read_dir(out_dir)
        .map_err(|err| format!("Failed to list {}: {err}", out_dir.display()))?
    {
        let entry = entry.map_err(|err| format!("Failed to read entry: {err}"))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("ts") {
            fs::remove_file(&path)
                .map_err(|err| format!("Failed to remove {}: {err}", path.display()))?;
        }
    }

    export_single_type::<UiMode>(out_dir)?;
    export_single_type::<BootstrapConfig>(out_dir)?;
    export_single_type::<BootstrapState>(out_dir)?;
    export_single_type::<SessionState>(out_dir)?;
    export_single_type::<OpenSourceResult>(out_dir)?;
    export_single_type::<RecentBook>(out_dir)?;
    export_single_type::<CalibreBookDto>(out_dir)?;
    export_single_type::<SourceOpenEvent>(out_dir)?;
    export_single_type::<CalibreLoadEvent>(out_dir)?;
    export_single_type::<TtsStateEvent>(out_dir)?;
    export_single_type::<PdfTranscriptionEvent>(out_dir)?;
    export_single_type::<LogLevelEvent>(out_dir)?;
    export_single_type::<SessionStateEvent>(out_dir)?;
    export_single_type::<ReaderStateEvent>(out_dir)?;
    export_single_type::<BridgeError>(out_dir)?;
    export_single_type::<session::PanelState>(out_dir)?;
    export_single_type::<session::ReaderSettingsView>(out_dir)?;
    export_single_type::<session::ReaderTtsView>(out_dir)?;
    export_single_type::<session::ReaderSettingsPatch>(out_dir)?;
    export_single_type::<session::ReaderStats>(out_dir)?;
    export_single_type::<session::ReaderSnapshot>(out_dir)?;
    export_single_type::<session::PrettyKind>(out_dir)?;
    export_single_type::<session::TtsPlaybackState>(out_dir)?;
    export_single_type::<config::ThemeMode>(out_dir)?;
    export_single_type::<config::FontFamily>(out_dir)?;
    export_single_type::<config::FontWeight>(out_dir)?;
    export_single_type::<config::HighlightColor>(out_dir)?;

    let index_content = r#"export type { UiMode } from "./UiMode";
export type { BootstrapConfig } from "./BootstrapConfig";
export type { BootstrapState } from "./BootstrapState";
export type { SessionState } from "./SessionState";
export type { OpenSourceResult } from "./OpenSourceResult";
export type { RecentBook } from "./RecentBook";
export type { CalibreBookDto } from "./CalibreBookDto";
export type { SourceOpenEvent } from "./SourceOpenEvent";
export type { CalibreLoadEvent } from "./CalibreLoadEvent";
export type { TtsStateEvent } from "./TtsStateEvent";
export type { PdfTranscriptionEvent } from "./PdfTranscriptionEvent";
export type { LogLevelEvent } from "./LogLevelEvent";
export type { SessionStateEvent } from "./SessionStateEvent";
export type { ReaderStateEvent } from "./ReaderStateEvent";
export type { BridgeError } from "./BridgeError";
export type { PanelState } from "./PanelState";
export type { ReaderSettingsView } from "./ReaderSettingsView";
export type { ReaderTtsView } from "./ReaderTtsView";
export type { ReaderSettingsPatch } from "./ReaderSettingsPatch";
export type { ReaderStats } from "./ReaderStats";
export type { ReaderSnapshot } from "./ReaderSnapshot";
export type { PrettyKind } from "./PrettyKind";
export type { TtsPlaybackState } from "./TtsPlaybackState";
export type { ThemeMode } from "./ThemeMode";
export type { FontFamily } from "./FontFamily";
export type { FontWeight } from "./FontWeight";
export type { HighlightColor } from "./HighlightColor";
"#;

    fs::write(out_dir.join("index.ts"), index_content).map_err(|err| {
        format!(
            "Failed to write {}: {err}",
            out_dir.join("index.ts").display()
        )
    })?;

    Ok(())
}

fn persist_active_reader(state: &mut BackendState) {
    if let Some(reader) = &state.reader {
        session::persist_session_housekeeping(reader);
    }
}

fn cancel_tts_request(state: &mut BackendState) {
    if let Some(runtime) = state.tts_request.take() {
        runtime.cancel_token.cancel();
    }
}

fn cleanup_for_shutdown(state: &mut BackendState) -> Option<u64> {
    let cancelled_open_request = if state.open_in_flight {
        state.active_open_request
    } else {
        None
    };
    if let Some(token) = state.open_cancel_token.take() {
        token.cancel();
    }
    if let Some(token) = state.calibre_cancel_token.take() {
        token.cancel();
    }
    cancel_tts_request(state);
    state.calibre_load_request = None;
    if let Some(reader) = state.reader.as_mut() {
        reader.tts_stop();
    }
    persist_active_reader(state);
    state.reader = None;
    state.mode = UiMode::Starter;
    state.active_source_path = None;
    state.active_open_source_path = None;
    state.open_in_flight = false;
    state.active_open_request = None;
    cancelled_open_request
}

fn finalize_shutdown_with_config_path(state: &Mutex<BackendState>, _config_path: &Path) {
    match state.lock() {
        Ok(mut guard) => {
            let _ = cleanup_for_shutdown(&mut guard);
        }
        Err(_) => warn!("Skipping shutdown housekeeping: backend state lock poisoned"),
    }
}

fn finalize_shutdown_from_mutex(state: &Mutex<BackendState>) {
    let config_path = app_config_path();
    finalize_shutdown_with_config_path(state, &config_path);
}

fn allocate_request_id(state: &mut BackendState) -> u64 {
    let request_id = state.next_request_id;
    state.next_request_id = state.next_request_id.wrapping_add(1).max(1);
    request_id
}

fn begin_open_request(
    state: &mut BackendState,
    source_path: &Path,
) -> Result<(u64, cancellation::CancellationToken), BridgeError> {
    if state.open_in_flight {
        return Err(bridge_error(
            "operation_conflict",
            "A book open operation is already in progress",
        ));
    }
    let request_id = allocate_request_id(state);
    let cancel_token = cancellation::CancellationToken::new();
    state.open_in_flight = true;
    state.active_open_request = Some(request_id);
    state.active_open_source_path = Some(source_path.to_path_buf());
    state.open_cancel_token = Some(cancel_token.clone());
    Ok((request_id, cancel_token))
}

fn emit_session_state(
    app: &tauri::AppHandle,
    request_id: u64,
    action: &str,
    session: &SessionState,
) {
    let _ = app.emit(
        "session-state",
        SessionStateEvent {
            request_id,
            action: action.to_string(),
            session: session.clone(),
        },
    );
}

fn event_emission_telemetry() -> &'static Mutex<EventEmissionTelemetry> {
    EVENT_EMISSION_TELEMETRY.get_or_init(|| Mutex::new(EventEmissionTelemetry::default()))
}

fn emission_rate_fields(
    previous: &mut Option<Instant>,
    now: Instant,
) -> (Option<u128>, Option<f64>) {
    let elapsed = previous.map(|last| now.saturating_duration_since(last).as_millis());
    *previous = Some(now);
    let rate_hz = elapsed.and_then(|ms| {
        if ms == 0 {
            None
        } else {
            Some(1000.0 / ms as f64)
        }
    });
    (elapsed, rate_hz)
}

fn classify_reader_action(action: &str) -> &'static str {
    match action {
        "source_open" => "page_load",
        "reader_next_page" | "reader_prev_page" | "reader_set_page" => "page_transition",
        "reader_tts_runtime_step"
        | "reader_sentence_click"
        | "reader_next_sentence"
        | "reader_prev_sentence"
        | "reader_tts_seek_next"
        | "reader_tts_seek_prev"
        | "reader_tts_repeat_sentence"
        | "reader_tts_play_from_page_start"
        | "reader_tts_play_from_highlight" => "cursor_move",
        "reader_apply_settings"
        | "panel_toggle_settings"
        | "panel_toggle_stats"
        | "panel_toggle_tts" => "ui_mutation",
        _ => "session_update",
    }
}

fn emit_reader_state(
    app: &tauri::AppHandle,
    request_id: u64,
    action: &str,
    reader: &session::ReaderSnapshot,
) {
    let now = Instant::now();
    let (since_last_emit_ms, emission_rate_hz) = event_emission_telemetry()
        .lock()
        .ok()
        .map(|mut telemetry| emission_rate_fields(&mut telemetry.last_reader_emit, now))
        .unwrap_or((None, None));
    let snapshot_size_bytes = serde_json::to_vec(reader)
        .map(|payload| payload.len())
        .unwrap_or_default();
    let update_kind = classify_reader_action(action);
    tracing::debug!(
        request_id,
        action,
        update_kind,
        page = reader.current_page + 1,
        total_pages = reader.total_pages,
        highlighted_sentence_idx = reader.highlighted_sentence_idx,
        tts_state = ?reader.tts.state,
        snapshot_size_bytes,
        since_last_emit_ms,
        emission_rate_hz,
        "Emitting reader-state bridge event"
    );
    let _ = app.emit(
        "reader-state",
        ReaderStateEvent {
            request_id,
            action: action.to_string(),
            reader: reader.clone(),
        },
    );
}

fn emit_tts_state(
    app: &tauri::AppHandle,
    request_id: u64,
    action: &str,
    tts: &session::ReaderTtsView,
) {
    let now = Instant::now();
    let (since_last_emit_ms, playback_update_rate_hz) = event_emission_telemetry()
        .lock()
        .ok()
        .map(|mut telemetry| emission_rate_fields(&mut telemetry.last_tts_emit, now))
        .unwrap_or((None, None));
    let payload_size_bytes = serde_json::to_vec(tts)
        .map(|payload| payload.len())
        .unwrap_or_default();
    tracing::debug!(
        request_id,
        action,
        update_kind = classify_reader_action(action),
        tts_state = ?tts.state,
        selected_sentence = tts.current_sentence_idx,
        payload_size_bytes,
        since_last_emit_ms,
        playback_update_rate_hz,
        "Emitting tts-state bridge event"
    );
    let _ = app.emit(
        "tts-state",
        TtsStateEvent {
            request_id,
            action: action.to_string(),
            tts: tts.clone(),
        },
    );
}

fn build_tts_playback_plan(state: &mut BackendState) -> Option<TtsPlaybackPlan> {
    let normalizer = state.normalizer.clone();
    let panels = state.panels;
    let reader = state.reader.as_mut()?;
    let snapshot = reader.snapshot(panels, &normalizer);
    if snapshot.tts.state != session::TtsPlaybackState::Playing {
        return None;
    }
    let (audio_sentences, start_idx) = reader.current_tts_audio_slice(&normalizer);
    if audio_sentences.is_empty() {
        return None;
    }
    tracing::debug!(
        source = %reader.source_path.display(),
        page = snapshot.current_page + 1,
        start_idx,
        sentence_count = audio_sentences.len(),
        tts_payload_source = "tts_text",
        "Built TTS playback plan from canonical tts_text payload"
    );
    Some(TtsPlaybackPlan {
        source_path: reader.source_path.clone(),
        page: snapshot.current_page,
        sentences: audio_sentences,
        start_idx,
        pause_after: Duration::from_secs_f64(reader.config.pause_after_sentence.max(0.0) as f64),
        speed: reader.config.tts_speed,
        volume: reader.config.tts_volume,
        threads: reader.config.tts_threads.max(1),
        progress_log_interval: Duration::from_secs_f64(
            reader.config.tts_progress_log_interval_secs.max(0.1) as f64,
        ),
        model_path: PathBuf::from(reader.config.tts_model_path.clone()),
        espeak_path: PathBuf::from(reader.config.tts_espeak_path.clone()),
    })
}

fn clear_tts_request_if_current(app: &tauri::AppHandle, runtime_request_id: u64) {
    let state = app.state::<Mutex<BackendState>>();
    if let Ok(mut guard) = state.lock() {
        let current_request_id = guard.tts_request.as_ref().map(|runtime| runtime.request_id);
        if current_request_id == Some(runtime_request_id) {
            guard.tts_request = None;
        }
    }
}

fn transition_tts_runtime_to_paused(
    app: &tauri::AppHandle,
    runtime_request_id: u64,
    action: &str,
    message: &str,
) {
    let state = app.state::<Mutex<BackendState>>();
    let maybe_emit = {
        let mut guard = match state.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        let current_request_id = guard.tts_request.as_ref().map(|runtime| runtime.request_id);
        if current_request_id != Some(runtime_request_id) {
            return;
        }

        let normalizer = guard.normalizer.clone();
        let panels = guard.panels;
        let reader = match guard.reader.as_mut() {
            Some(reader) => reader,
            None => return,
        };

        let event = reader.apply_command(session::SessionCommand::TtsPause, panels, &normalizer);
        let request_id = allocate_request_id(&mut guard);
        Some((request_id, event.snapshot))
    };

    if let Some((request_id, snapshot)) = maybe_emit {
        warn!(runtime_request_id, error = %message, "TTS runtime transitioned to paused");
        emit_reader_state(app, request_id, action, &snapshot);
        emit_tts_state(app, request_id, action, &snapshot.tts);
    }
}

fn collect_tts_playback_plan(
    app: &tauri::AppHandle,
    runtime_request_id: u64,
) -> Option<TtsPlaybackPlan> {
    let state = app.state::<Mutex<BackendState>>();
    let mut guard = state.lock().ok()?;
    let current_request_id = guard.tts_request.as_ref().map(|runtime| runtime.request_id);
    if current_request_id != Some(runtime_request_id) {
        return None;
    }
    build_tts_playback_plan(&mut guard)
}

fn advance_tts_runtime_cursor(app: &tauri::AppHandle, runtime_request_id: u64) -> bool {
    let state = app.state::<Mutex<BackendState>>();
    let maybe_emit = {
        let mut guard = match state.lock() {
            Ok(guard) => guard,
            Err(_) => return false,
        };
        let current_request_id = guard.tts_request.as_ref().map(|runtime| runtime.request_id);
        if current_request_id != Some(runtime_request_id) {
            return false;
        }

        let normalizer = guard.normalizer.clone();
        let panels = guard.panels;
        let reader = match guard.reader.as_mut() {
            Some(reader) => reader,
            None => return false,
        };

        let current_snapshot = reader.snapshot(panels, &normalizer);
        if current_snapshot.tts.state != session::TtsPlaybackState::Playing {
            return false;
        }

        let event = reader.apply_command(session::SessionCommand::TtsSeekNext, panels, &normalizer);
        let emit_request_id = allocate_request_id(&mut guard);
        Some((emit_request_id, event.snapshot))
    };

    if let Some((emit_request_id, snapshot)) = maybe_emit {
        emit_reader_state(app, emit_request_id, "reader_tts_runtime_step", &snapshot);
        emit_tts_state(
            app,
            emit_request_id,
            "reader_tts_runtime_step",
            &snapshot.tts,
        );
        snapshot.tts.state == session::TtsPlaybackState::Playing
    } else {
        false
    }
}

fn run_tts_runtime_loop(
    app: tauri::AppHandle,
    runtime_request_id: u64,
    cancel_token: cancellation::CancellationToken,
    pause_requested: Arc<AtomicBool>,
) {
    struct PrefetchedBatch {
        source_path: PathBuf,
        page: usize,
        start_idx: usize,
        prepared: Vec<(PathBuf, Duration)>,
    }

    struct PendingPrefetch {
        source_path: PathBuf,
        page: usize,
        start_idx: usize,
        handle: std::thread::JoinHandle<Result<Vec<(PathBuf, Duration)>, String>>,
    }

    let mut engine: Option<tts::TtsEngine> = None;
    let mut ready_prefetch: Option<PrefetchedBatch> = None;
    loop {
        if cancel_token.is_cancelled() {
            break;
        }

        let Some(plan) = collect_tts_playback_plan(&app, runtime_request_id) else {
            break;
        };
        if plan.start_idx >= plan.sentences.len() {
            break;
        }

        if engine.is_none() {
            let built_engine =
                match tts::TtsEngine::new(plan.model_path.clone(), plan.espeak_path.clone()) {
                    Ok(engine) => engine,
                    Err(err) => {
                        transition_tts_runtime_to_paused(
                            &app,
                            runtime_request_id,
                            "reader_tts_runtime_error",
                            &format!("Failed to initialize Piper TTS engine: {err}"),
                        );
                        break;
                    }
                };
            engine = Some(built_engine);
        }

        let chunk_end = (plan.start_idx + TTS_PREPARE_SENTENCE_WINDOW).min(plan.sentences.len());
        let prepared = if let Some(prefetched) = ready_prefetch.take() {
            if prefetched.source_path == plan.source_path
                && prefetched.page == plan.page
                && prefetched.start_idx == plan.start_idx
            {
                prefetched.prepared
            } else {
                let chunk_sentences = plan.sentences[plan.start_idx..chunk_end].to_vec();
                let cache_root = cache::hash_dir(&plan.source_path).join("tts");
                match engine.as_ref().unwrap().prepare_batch(
                    cache_root,
                    chunk_sentences,
                    0,
                    plan.threads,
                    plan.progress_log_interval,
                ) {
                    Ok(batch) => batch,
                    Err(err) => {
                        if cancel_token.is_cancelled() {
                            break;
                        }
                        transition_tts_runtime_to_paused(
                            &app,
                            runtime_request_id,
                            "reader_tts_runtime_error",
                            &format!("Failed to prepare TTS audio batch: {err}"),
                        );
                        break;
                    }
                }
            }
        } else {
            let chunk_sentences = plan.sentences[plan.start_idx..chunk_end].to_vec();
            let cache_root = cache::hash_dir(&plan.source_path).join("tts");
            match engine.as_ref().unwrap().prepare_batch(
                cache_root,
                chunk_sentences,
                0,
                plan.threads,
                plan.progress_log_interval,
            ) {
                Ok(batch) => batch,
                Err(err) => {
                    if cancel_token.is_cancelled() {
                        break;
                    }
                    transition_tts_runtime_to_paused(
                        &app,
                        runtime_request_id,
                        "reader_tts_runtime_error",
                        &format!("Failed to prepare TTS audio batch: {err}"),
                    );
                    break;
                }
            }
        };

        if prepared.is_empty() {
            transition_tts_runtime_to_paused(
                &app,
                runtime_request_id,
                "reader_tts_runtime_stopped",
                "Prepared TTS batch was empty",
            );
            break;
        }

        let next_chunk_start = chunk_end;
        let pending_prefetch = if next_chunk_start < plan.sentences.len() {
            let next_chunk_end =
                (next_chunk_start + TTS_PREPARE_SENTENCE_WINDOW).min(plan.sentences.len());
            let next_sentences = plan.sentences[next_chunk_start..next_chunk_end].to_vec();
            let next_source_path = plan.source_path.clone();
            let next_page = plan.page;
            let next_threads = plan.threads;
            let next_progress_interval = plan.progress_log_interval;
            let next_cache_root = cache::hash_dir(&next_source_path).join("tts");
            let next_engine = engine.as_ref().unwrap().clone();

            Some(PendingPrefetch {
                source_path: next_source_path,
                page: next_page,
                start_idx: next_chunk_start,
                handle: std::thread::spawn(move || {
                    next_engine
                        .prepare_batch(
                            next_cache_root,
                            next_sentences,
                            0,
                            next_threads,
                            next_progress_interval,
                        )
                        .map_err(|err| err.to_string())
                }),
            })
        } else {
            None
        };

        let files: Vec<PathBuf> = prepared.into_iter().map(|(path, _)| path).collect();
        let playback = match engine.as_ref().unwrap().play_files(
            &files,
            plan.pause_after,
            plan.speed,
            plan.volume,
            false,
        ) {
            Ok(playback) => playback,
            Err(err) => {
                if cancel_token.is_cancelled() {
                    break;
                }
                transition_tts_runtime_to_paused(
                    &app,
                    runtime_request_id,
                    "reader_tts_runtime_error",
                    &format!("Failed to start Piper playback: {err}"),
                );
                break;
            }
        };

        let sentence_durations = playback.sentence_durations().to_vec();
        let mut continue_playback = true;
        for duration in sentence_durations {
            let mut remaining = duration.saturating_add(plan.pause_after);
            let mut last_tick = Instant::now();
            loop {
                if cancel_token.is_cancelled() {
                    playback.stop();
                    clear_tts_request_if_current(&app, runtime_request_id);
                    return;
                }

                if pause_requested.load(Ordering::SeqCst) {
                    if !playback.is_paused() {
                        playback.pause();
                    }
                    last_tick = Instant::now();
                    std::thread::sleep(TTS_PROGRESS_POLL_INTERVAL);
                    continue;
                }

                if playback.is_paused() {
                    playback.play();
                    last_tick = Instant::now();
                }

                let now = Instant::now();
                let elapsed = now.saturating_duration_since(last_tick);
                last_tick = now;

                if elapsed >= remaining {
                    break;
                }
                remaining = remaining.saturating_sub(elapsed);
                std::thread::sleep(TTS_PROGRESS_POLL_INTERVAL);
            }

            if cancel_token.is_cancelled() {
                playback.stop();
                clear_tts_request_if_current(&app, runtime_request_id);
                return;
            }

            if !advance_tts_runtime_cursor(&app, runtime_request_id) {
                continue_playback = false;
                break;
            }
        }

        // Guard against duration under-estimation by waiting for the sink queue
        // to fully drain before stopping/chunk-switching. This prevents clipping
        // the tail end of the current sentence at batch boundaries.
        loop {
            if cancel_token.is_cancelled() {
                playback.stop();
                clear_tts_request_if_current(&app, runtime_request_id);
                return;
            }

            if pause_requested.load(Ordering::SeqCst) {
                if !playback.is_paused() {
                    playback.pause();
                }
                std::thread::sleep(TTS_PROGRESS_POLL_INTERVAL);
                continue;
            }

            if playback.is_paused() {
                playback.play();
            }

            if playback.queued_sources() == 0 {
                break;
            }

            std::thread::sleep(TTS_PROGRESS_POLL_INTERVAL);
        }

        playback.stop();

        if !continue_playback {
            break;
        }

        if let Some(pending) = pending_prefetch {
            match pending.handle.join() {
                Ok(Ok(prepared)) => {
                    ready_prefetch = Some(PrefetchedBatch {
                        source_path: pending.source_path,
                        page: pending.page,
                        start_idx: pending.start_idx,
                        prepared,
                    });
                }
                Ok(Err(err)) => {
                    warn!(
                        runtime_request_id,
                        page = pending.page + 1,
                        sentence_idx = pending.start_idx,
                        error = %err,
                        "Failed to prefetch next TTS batch; runtime will fall back to inline prepare"
                    );
                }
                Err(_) => {
                    warn!(
                        runtime_request_id,
                        page = pending.page + 1,
                        sentence_idx = pending.start_idx,
                        "TTS prefetch worker panicked; runtime will fall back to inline prepare"
                    );
                }
            }
        }
    }
    clear_tts_request_if_current(&app, runtime_request_id);
}

fn sync_tts_runtime_after_reader_change(
    app: &tauri::AppHandle,
    state: &State<'_, Mutex<BackendState>>,
) {
    let maybe_runtime = {
        let mut guard = match state.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };

        let Some(plan) = build_tts_playback_plan(&mut guard) else {
            cancel_tts_request(&mut guard);
            return;
        };

        cancel_tts_request(&mut guard);
        let request_id = allocate_request_id(&mut guard);
        let cancel_token = cancellation::CancellationToken::new();
        let pause_requested = Arc::new(AtomicBool::new(false));
        guard.tts_request = Some(TtsRequestRuntime {
            request_id,
            cancel_token: cancel_token.clone(),
            pause_requested: pause_requested.clone(),
        });
        Some((request_id, cancel_token, pause_requested, plan))
    };

    if let Some((request_id, cancel_token, pause_requested, plan)) = maybe_runtime {
        info!(
            request_id,
            page = plan.page + 1,
            sentence_idx = plan.start_idx,
            sentence_count = plan.sentences.len(),
            "Starting TTS runtime playback job"
        );
        let app_handle = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            run_tts_runtime_loop(app_handle, request_id, cancel_token, pause_requested);
        });
    }
}

fn should_sync_tts_after_reader_command(command: &session::SessionCommand) -> bool {
    match command {
        session::SessionCommand::GetSnapshot => false,
        session::SessionCommand::ApplySettings { patch } => {
            patch.font_size.is_some()
                || patch.lines_per_page.is_some()
                || patch.pause_after_sentence.is_some()
                || patch.tts_speed.is_some()
                || patch.tts_volume.is_some()
        }
        _ => true,
    }
}

fn apply_reader_command_with_sync(
    app: &tauri::AppHandle,
    state: &State<'_, Mutex<BackendState>>,
    command: session::SessionCommand,
    should_sync_tts: bool,
) -> Result<session::ReaderSnapshot, BridgeError> {
    let action = command.action();
    let (snapshot, request_id) = {
        let mut guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        let normalizer = guard.normalizer.clone();
        let panels = guard.panels;
        let request_id = allocate_request_id(&mut guard);
        let reader = guard
            .reader
            .as_mut()
            .ok_or_else(|| bridge_error("no_reader", "No active reader session"))?;
        let event = reader.apply_command(command, panels, &normalizer);
        (event.snapshot, request_id)
    };
    emit_reader_state(app, request_id, action, &snapshot);
    emit_tts_state(app, request_id, action, &snapshot.tts);
    if should_sync_tts {
        sync_tts_runtime_after_reader_change(app, state);
    }
    Ok(snapshot)
}

fn apply_reader_command(
    app: &tauri::AppHandle,
    state: &State<'_, Mutex<BackendState>>,
    command: session::SessionCommand,
) -> Result<session::ReaderSnapshot, BridgeError> {
    let should_sync_tts = should_sync_tts_after_reader_command(&command);
    apply_reader_command_with_sync(app, state, command, should_sync_tts)
}

fn apply_panel_toggle<F>(
    app: &tauri::AppHandle,
    state: &State<'_, Mutex<BackendState>>,
    action: &str,
    toggle: F,
) -> Result<SessionState, BridgeError>
where
    F: FnOnce(&mut session::PanelState),
{
    let (session, reader_snapshot, request_id) = {
        let mut guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        let request_id = allocate_request_id(&mut guard);
        toggle(&mut guard.panels);
        let panels = guard.panels;
        if let Some(reader) = guard.reader.as_mut() {
            reader.config.show_settings = panels.show_settings;
            reader.config.show_stats = panels.show_stats;
            reader.config.show_tts = panels.show_tts;
        }

        let session = to_session_state(&guard);
        let normalizer = guard.normalizer.clone();
        let reader_snapshot = guard
            .reader
            .as_mut()
            .map(|reader| reader.snapshot(panels, &normalizer));
        (session, reader_snapshot, request_id)
    };

    emit_session_state(app, request_id, action, &session);
    if let Some(snapshot) = &reader_snapshot {
        emit_reader_state(app, request_id, action, snapshot);
        emit_tts_state(app, request_id, action, &snapshot.tts);
    }
    Ok(session)
}

async fn open_resolved_source(
    app: &tauri::AppHandle,
    state: &State<'_, Mutex<BackendState>>,
    source_path: PathBuf,
) -> Result<OpenSourceResult, BridgeError> {
    let (request_id, cancel_token, started_session): (
        u64,
        cancellation::CancellationToken,
        SessionState,
    ) = {
        let mut guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        let (request_id, cancel_token) = begin_open_request(&mut guard, &source_path)?;
        let started_session = to_session_state(&guard);
        (request_id, cancel_token, started_session)
    };

    emit_session_state(app, request_id, "source_open_started", &started_session);

    let source_is_pdf = source_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false);

    info!(
        request_id,
        path = %source_path.display(),
        "Starting source open request"
    );

    let _ = app.emit(
        "source-open",
        SourceOpenEvent {
            request_id,
            phase: "started".to_string(),
            source_path: Some(source_path.to_string_lossy().to_string()),
            message: None,
        },
    );

    if source_is_pdf {
        let _ = app.emit(
            "pdf-transcription",
            PdfTranscriptionEvent {
                request_id,
                phase: "started".to_string(),
                source_path: source_path.to_string_lossy().to_string(),
                message: None,
            },
        );
    }

    cache::remember_source_path(&source_path);

    let (base_config, normalizer) = {
        let guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        (guard.base_config.clone(), guard.normalizer.clone())
    };

    let source_path = maybe_refresh_legacy_browser_tab_source(&base_config, &source_path).await;

    let source_path_for_task = source_path.clone();
    let normalizer_for_task = normalizer.clone();
    let open_cancel_for_task = cancel_token.clone();
    let reader_result = tauri::async_runtime::spawn_blocking(move || {
        session::load_session_for_source_with_cancel(
            source_path_for_task,
            &base_config,
            &normalizer_for_task,
            Some(&open_cancel_for_task),
        )
    })
    .await;

    let mut guard = state
        .lock()
        .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
    if guard.active_open_request != Some(request_id) {
        let should_emit_cancelled = guard.open_in_flight || guard.active_open_source_path.is_some();
        drop(guard);
        if should_emit_cancelled {
            let _ = app.emit(
                "source-open",
                SourceOpenEvent {
                    request_id,
                    phase: "cancelled".to_string(),
                    source_path: Some(source_path.to_string_lossy().to_string()),
                    message: Some("Source open request was superseded or cancelled".to_string()),
                },
            );
            if source_is_pdf {
                let _ = app.emit(
                    "pdf-transcription",
                    PdfTranscriptionEvent {
                        request_id,
                        phase: "cancelled".to_string(),
                        source_path: source_path.to_string_lossy().to_string(),
                        message: Some(
                            "PDF transcription cancelled by request supersession".to_string(),
                        ),
                    },
                );
            }
        }
        info!(
            request_id,
            path = %source_path.display(),
            "Discarded stale source open completion"
        );
        return Err(bridge_error(
            "open_cancelled",
            "Source open request was superseded or cancelled",
        ));
    }
    guard.open_in_flight = false;
    guard.active_open_request = None;
    guard.open_cancel_token = None;
    guard.active_open_source_path = None;
    let reader_result = match reader_result {
        Ok(result) => result,
        Err(err) => {
            let session = to_session_state(&guard);
            drop(guard);
            emit_session_state(app, request_id, "source_open_failed", &session);
            let message = format!("Failed to join load task: {err}");
            warn!(
                request_id,
                path = %source_path.display(),
                error = %message,
                "Source open request task failed"
            );
            let _ = app.emit(
                "source-open",
                SourceOpenEvent {
                    request_id,
                    phase: "failed".to_string(),
                    source_path: Some(source_path.to_string_lossy().to_string()),
                    message: Some(message.clone()),
                },
            );
            if source_is_pdf {
                let _ = app.emit(
                    "pdf-transcription",
                    PdfTranscriptionEvent {
                        request_id,
                        phase: "failed".to_string(),
                        source_path: source_path.to_string_lossy().to_string(),
                        message: Some(message.clone()),
                    },
                );
            }
            return Err(bridge_error("task_join_error", message));
        }
    };

    match reader_result {
        Ok(mut reader) => {
            let reader_panels = panels_from_config(&reader.config);
            guard.panels = reader_panels;
            let snapshot = reader.snapshot(reader_panels, &normalizer);

            guard.mode = UiMode::Reader;
            guard.active_source_path = Some(source_path.clone());
            guard.reader = Some(reader);
            let session = to_session_state(&guard);
            let result = OpenSourceResult {
                session: session.clone(),
                reader: snapshot.clone(),
            };

            drop(guard);
            emit_session_state(app, request_id, "source_open", &session);
            emit_reader_state(app, request_id, "source_open", &snapshot);
            emit_tts_state(app, request_id, "source_open", &snapshot.tts);

            let _ = app.emit(
                "source-open",
                SourceOpenEvent {
                    request_id,
                    phase: "finished".to_string(),
                    source_path: Some(source_path.to_string_lossy().to_string()),
                    message: None,
                },
            );
            if source_is_pdf {
                let _ = app.emit(
                    "pdf-transcription",
                    PdfTranscriptionEvent {
                        request_id,
                        phase: "finished".to_string(),
                        source_path: source_path.to_string_lossy().to_string(),
                        message: None,
                    },
                );
            }
            info!(
                request_id,
                path = %source_path.display(),
                page = snapshot.current_page + 1,
                total_pages = snapshot.total_pages,
                "Completed source open request"
            );
            Ok(result)
        }
        Err(err) => {
            let session = to_session_state(&guard);
            drop(guard);
            emit_session_state(app, request_id, "source_open_failed", &session);
            warn!(
                request_id,
                path = %source_path.display(),
                error = %err,
                "Source open request failed"
            );
            let _ = app.emit(
                "source-open",
                SourceOpenEvent {
                    request_id,
                    phase: "failed".to_string(),
                    source_path: Some(source_path.to_string_lossy().to_string()),
                    message: Some(err.clone()),
                },
            );
            if source_is_pdf {
                let _ = app.emit(
                    "pdf-transcription",
                    PdfTranscriptionEvent {
                        request_id,
                        phase: "failed".to_string(),
                        source_path: source_path.to_string_lossy().to_string(),
                        message: Some(err.clone()),
                    },
                );
            }
            Err(bridge_error("open_failed", err))
        }
    }
}

async fn maybe_refresh_legacy_browser_tab_source(
    cfg: &config::AppConfig,
    source_path: &Path,
) -> PathBuf {
    if !cache::is_browser_tab_manifest(source_path) {
        return source_path.to_path_buf();
    }
    let Some(manifest) = cache::load_browser_tab_manifest(source_path) else {
        return source_path.to_path_buf();
    };
    if manifest.raw_html_path.is_some() {
        return source_path.to_path_buf();
    }
    if !cfg.browser_tabs_enabled {
        return source_path.to_path_buf();
    }
    let client = match browsr_client_from_config(cfg) {
        Ok(client) => client,
        Err(err) => {
            warn!(
                path = %source_path.display(),
                error = %err.message,
                "Legacy browser-tab source is missing raw HTML snapshot and could not create browsr client"
            );
            return source_path.to_path_buf();
        }
    };
    let tab_meta = client
        .list_tabs(manifest.window_id, None, true)
        .await
        .ok()
        .and_then(|tabs| tabs.into_iter().find(|tab| tab.id == manifest.tab_id));
    let snapshot = match client.snapshot_tab(manifest.tab_id).await {
        Ok(snapshot) => snapshot,
        Err(err) => {
            warn!(
                path = %source_path.display(),
                tab_id = manifest.tab_id,
                error = %err,
                "Legacy browser-tab source is missing raw HTML snapshot and live refresh failed"
            );
            return source_path.to_path_buf();
        }
    };
    match cache::persist_browser_tab_source(&snapshot, tab_meta.as_ref()) {
        Ok(refreshed_path) => {
            info!(
                path = %refreshed_path.display(),
                tab_id = manifest.tab_id,
                title = %snapshot.title,
                "Refreshed legacy browser-tab source from live tab before open"
            );
            refreshed_path
        }
        Err(err) => {
            warn!(
                path = %source_path.display(),
                tab_id = manifest.tab_id,
                error = %err,
                "Legacy browser-tab source refresh persist failed"
            );
            source_path.to_path_buf()
        }
    }
}

#[tauri::command]
fn session_get_bootstrap(
    state: State<'_, Mutex<BackendState>>,
) -> Result<BootstrapState, BridgeError> {
    let guard = state
        .lock()
        .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
    Ok(bootstrap_state_from_backend(&guard))
}

#[tauri::command]
fn session_toggle_theme(
    state: State<'_, Mutex<BackendState>>,
) -> Result<BootstrapState, BridgeError> {
    let (request_id, bootstrap_state) = {
        let mut guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        let request_id = allocate_request_id(&mut guard);
        guard.base_config.theme = match guard.base_config.theme {
            config::ThemeMode::Day => config::ThemeMode::Night,
            config::ThemeMode::Night => config::ThemeMode::Day,
        };
        (request_id, bootstrap_state_from_backend(&guard))
    };
    info!(
        request_id,
        theme = %bootstrap_state.config.theme,
        "Toggled starter theme"
    );
    Ok(bootstrap_state)
}

#[tauri::command]
fn session_get_state(state: State<'_, Mutex<BackendState>>) -> Result<SessionState, BridgeError> {
    let guard = state
        .lock()
        .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
    Ok(to_session_state(&guard))
}

#[tauri::command]
fn session_return_to_starter(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<SessionState, BridgeError> {
    let (session, request_id, cancelled_request, cancelled_source_path) = {
        let mut guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        let request_id = allocate_request_id(&mut guard);
        let cancelled_request = if guard.open_in_flight {
            guard.active_open_request
        } else {
            None
        };
        let cancelled_source_path = guard
            .active_open_source_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string());
        let _ = cleanup_for_shutdown(&mut guard);
        (
            to_session_state(&guard),
            request_id,
            cancelled_request,
            cancelled_source_path,
        )
    };
    emit_session_state(&app, request_id, "session_return_to_starter", &session);
    if let Some(cancelled_request) = cancelled_request {
        let _ = app.emit(
            "source-open",
            SourceOpenEvent {
                request_id: cancelled_request,
                phase: "cancelled".to_string(),
                source_path: cancelled_source_path,
                message: Some("Source open request cancelled by return-to-starter".to_string()),
            },
        );
    }
    Ok(session)
}

#[tauri::command]
fn panel_toggle_settings(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<SessionState, BridgeError> {
    apply_panel_toggle(&app, &state, "panel_toggle_settings", |panels| {
        panels.show_settings = !panels.show_settings;
        if panels.show_settings {
            panels.show_stats = false;
        }
    })
}

#[tauri::command]
fn panel_toggle_stats(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<SessionState, BridgeError> {
    apply_panel_toggle(&app, &state, "panel_toggle_stats", |panels| {
        panels.show_stats = !panels.show_stats;
        if panels.show_stats {
            panels.show_settings = false;
        }
    })
}

#[tauri::command]
fn panel_toggle_tts(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<SessionState, BridgeError> {
    apply_panel_toggle(&app, &state, "panel_toggle_tts", |panels| {
        panels.show_tts = !panels.show_tts;
    })
}

#[tauri::command]
fn recent_list(limit: Option<usize>) -> Vec<RecentBook> {
    cache::list_recent_books(normalize_recent_limit(limit))
        .into_iter()
        .map(|recent| RecentBook {
            source_path: recent.source_path.to_string_lossy().to_string(),
            display_title: recent.display_title,
            snippet: recent.snippet,
            thumbnail_path: recent
                .thumbnail_path
                .as_deref()
                .and_then(thumbnail_path_to_data_url),
            last_opened_unix_secs: recent.last_opened_unix_secs,
        })
        .collect()
}

#[tauri::command]
fn recent_delete(path: String) -> Result<(), BridgeError> {
    let source = PathBuf::from(path.trim());
    if source.as_os_str().is_empty() {
        return Err(bridge_error("invalid_input", "Path cannot be empty"));
    }
    cache::delete_recent_source_and_cache(&source).map_err(|err| bridge_error("io_error", err))
}

#[tauri::command]
async fn source_open_path(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
    path: String,
) -> Result<OpenSourceResult, BridgeError> {
    let source = resolve_source_path(&path)?;
    open_resolved_source(&app, &state, source).await
}

#[tauri::command]
async fn source_open_clipboard_text(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
    text: String,
) -> Result<OpenSourceResult, BridgeError> {
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        return Err(bridge_error("invalid_input", "clipboard text is empty"));
    }
    let path = cache::persist_clipboard_text_source(&trimmed)
        .map_err(|err| bridge_error("invalid_input", err))?;
    open_resolved_source(&app, &state, path).await
}

#[tauri::command]
async fn source_open_clipboard(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<OpenSourceResult, BridgeError> {
    info!("Opening source from system clipboard");
    let app_for_read = app.clone();
    let text = tauri::async_runtime::spawn_blocking(move || {
        read_clipboard_text_with_fallback(&app_for_read)
    })
    .await
    .map_err(|err| {
        bridge_error(
            "clipboard_error",
            format!("Clipboard worker task failed: {err}"),
        )
    })?
    .map_err(|err| bridge_error("clipboard_error", err))?;
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        warn!("Clipboard read succeeded but text was empty");
        return Err(bridge_error("invalid_input", "clipboard text is empty"));
    }
    let path = cache::persist_clipboard_text_source(&trimmed)
        .map_err(|err| bridge_error("invalid_input", err))?;
    open_resolved_source(&app, &state, path).await
}

#[tauri::command]
async fn browser_tabs_health(
    state: State<'_, Mutex<BackendState>>,
) -> Result<browser_tabs::BrowsrHealth, BridgeError> {
    let cfg = {
        let guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        guard.base_config.clone()
    };
    if !cfg.browser_tabs_enabled {
        return Err(bridge_error(
            "browser_tabs_disabled",
            "Browser tabs import is disabled in config",
        ));
    }
    let client = browsr_client_from_config(&cfg)?;
    client
        .health()
        .await
        .map_err(|err| bridge_error("browsr_unavailable", err.to_string()))
}

#[tauri::command]
async fn browser_tabs_list_windows(
    state: State<'_, Mutex<BackendState>>,
) -> Result<Vec<browser_tabs::BrowserWindow>, BridgeError> {
    let cfg = {
        let guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        guard.base_config.clone()
    };
    if !cfg.browser_tabs_enabled {
        return Err(bridge_error(
            "browser_tabs_disabled",
            "Browser tabs import is disabled in config",
        ));
    }
    let client = browsr_client_from_config(&cfg)?;
    client
        .list_windows()
        .await
        .map_err(|err| bridge_error("browsr_request_failed", err.to_string()))
}

#[tauri::command]
async fn browser_tabs_list_tabs(
    state: State<'_, Mutex<BackendState>>,
    window_id: Option<u64>,
    query: Option<String>,
    refresh: Option<bool>,
) -> Result<Vec<browser_tabs::BrowserTab>, BridgeError> {
    let cfg = {
        let guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        guard.base_config.clone()
    };
    if !cfg.browser_tabs_enabled {
        return Err(bridge_error(
            "browser_tabs_disabled",
            "Browser tabs import is disabled in config",
        ));
    }
    let client = browsr_client_from_config(&cfg)?;
    client
        .list_tabs(window_id, query.as_deref(), refresh.unwrap_or(false))
        .await
        .map_err(|err| bridge_error("browsr_request_failed", err.to_string()))
}

#[tauri::command]
async fn source_open_browser_tab(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
    tab_id: u64,
    window_id: Option<u64>,
) -> Result<OpenSourceResult, BridgeError> {
    let cfg = {
        let guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        guard.base_config.clone()
    };
    if !cfg.browser_tabs_enabled {
        return Err(bridge_error(
            "browser_tabs_disabled",
            "Browser tabs import is disabled in config",
        ));
    }
    let client = browsr_client_from_config(&cfg)?;
    let tab_meta = client
        .list_tabs(window_id, None, false)
        .await
        .ok()
        .and_then(|tabs| tabs.into_iter().find(|tab| tab.id == tab_id));
    let snapshot = client
        .snapshot_tab(tab_id)
        .await
        .map_err(|err| bridge_error("browsr_snapshot_failed", err.to_string()))?;
    let source_path = cache::persist_browser_tab_source(&snapshot, tab_meta.as_ref())
        .map_err(|err| bridge_error("browser_tab_cache_error", err))?;
    info!(
        tab_id,
        window_id,
        source_path = %source_path.display(),
        title = %snapshot.title,
        url = %snapshot.url,
        html_truncated = snapshot.truncation.html.truncated,
        text_truncated = snapshot.truncation.text.truncated,
        "Persisted browser-tab snapshot source"
    );
    open_resolved_source(&app, &state, source_path).await
}

#[tauri::command]
async fn source_refresh_browser_tab(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
    path: String,
) -> Result<OpenSourceResult, BridgeError> {
    let source_path = resolve_source_path(&path)?;
    let manifest = cache::load_browser_tab_manifest(&source_path).ok_or_else(|| {
        bridge_error(
            "invalid_input",
            format!(
                "Source is not a browser-tab manifest: {}",
                source_path.display()
            ),
        )
    })?;
    let cfg = {
        let guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        guard.base_config.clone()
    };
    if !cfg.browser_tabs_enabled {
        return Err(bridge_error(
            "browser_tabs_disabled",
            "Browser tabs import is disabled in config",
        ));
    }
    let client = browsr_client_from_config(&cfg)?;
    let tab_meta = client
        .list_tabs(manifest.window_id, None, true)
        .await
        .ok()
        .and_then(|tabs| tabs.into_iter().find(|tab| tab.id == manifest.tab_id));
    let snapshot = client
        .snapshot_tab(manifest.tab_id)
        .await
        .map_err(|err| bridge_error("browsr_snapshot_failed", err.to_string()))?;
    let refreshed_source_path = cache::persist_browser_tab_source(&snapshot, tab_meta.as_ref())
        .map_err(|err| bridge_error("browser_tab_cache_error", err))?;
    info!(
        tab_id = manifest.tab_id,
        source_path = %refreshed_source_path.display(),
        title = %snapshot.title,
        url = %snapshot.url,
        html_truncated = snapshot.truncation.html.truncated,
        text_truncated = snapshot.truncation.text.truncated,
        "Refreshed browser-tab snapshot source"
    );
    open_resolved_source(&app, &state, refreshed_source_path).await
}

fn read_clipboard_text_with_fallback(app: &tauri::AppHandle) -> Result<String, String> {
    match app.clipboard().read_text() {
        Ok(text) => {
            tracing::debug!(
                chars = text.chars().count(),
                "Read clipboard text via tauri plugin"
            );
            Ok(text)
        }
        Err(primary_err) => {
            warn!("Primary clipboard read via tauri plugin failed: {primary_err}");
            #[cfg(target_os = "linux")]
            {
                let commands: &[(&str, &[&str])] = &[
                    ("wl-paste", &["--no-newline"]),
                    ("wl-paste", &[]),
                    ("xclip", &["-selection", "clipboard", "-o"]),
                    ("xsel", &["--clipboard", "--output"]),
                ];
                let mut diagnostics = Vec::new();
                for (bin, args) in commands {
                    match run_clipboard_command(bin, args) {
                        Ok(Some(text)) => {
                            info!(
                                command = %bin,
                                chars = text.chars().count(),
                                "Read clipboard text via command fallback"
                            );
                            return Ok(text);
                        }
                        Ok(None) => {
                            diagnostics.push(format!("{bin} {} => empty", args.join(" ")));
                            tracing::debug!(command = %bin, "Clipboard fallback command returned empty output");
                        }
                        Err(err) => {
                            diagnostics.push(format!("{bin} {} => {err}", args.join(" ")));
                            tracing::debug!(command = %bin, "Clipboard fallback command failed: {err}");
                        }
                    }
                }
                Err(format!(
                    "Clipboard read failed. plugin_error='{primary_err}'. fallback_attempts=[{}]",
                    diagnostics.join("; ")
                ))
            }
            #[cfg(not(target_os = "linux"))]
            {
                Err(format!("Failed to read clipboard text: {primary_err}"))
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn run_clipboard_command(bin: &str, args: &[&str]) -> Result<Option<String>, String> {
    let output = Command::new(bin)
        .args(args)
        .output()
        .map_err(|err| format!("spawn failed: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            return Err(format!("exit status {}", output.status));
        }
        return Err(format!("exit status {} stderr='{stderr}'", output.status));
    }
    let text =
        String::from_utf8(output.stdout).map_err(|err| format!("utf8 decode failed: {err}"))?;
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed))
    }
}

#[tauri::command]
fn reader_get_snapshot(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::GetSnapshot)
}

#[tauri::command]
fn reader_next_page(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::NextPage)
}

#[tauri::command]
fn reader_prev_page(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::PrevPage)
}

#[tauri::command]
fn reader_set_page(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
    page: usize,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::SetPage { page })
}

#[tauri::command]
fn reader_sentence_click(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
    sentence_idx: usize,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(
        &app,
        &state,
        session::SessionCommand::SentenceClick { sentence_idx },
    )
}

#[tauri::command]
fn reader_next_sentence(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::NextSentence)
}

#[tauri::command]
fn reader_prev_sentence(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::PrevSentence)
}

#[tauri::command]
fn reader_toggle_text_only(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::ToggleTextOnly)
}

#[tauri::command]
fn reader_apply_settings(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
    patch: session::ReaderSettingsPatch,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(
        &app,
        &state,
        session::SessionCommand::ApplySettings { patch },
    )
}

#[tauri::command]
fn reader_search_set_query(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
    query: String,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(
        &app,
        &state,
        session::SessionCommand::SearchSetQuery { query },
    )
}

#[tauri::command]
fn reader_search_next(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::SearchNext)
}

#[tauri::command]
fn reader_search_prev(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::SearchPrev)
}

#[tauri::command]
fn reader_tts_play(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    let mut should_sync_tts = true;
    {
        let mut guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        let normalizer = guard.normalizer.clone();
        let panels = guard.panels;
        let behavior = guard
            .reader
            .as_ref()
            .map(|reader| reader.config.tts_pause_resume_behavior)
            .unwrap_or_default();

        let paused = guard
            .reader
            .as_mut()
            .map(|reader| {
                reader.snapshot(panels, &normalizer).tts.state == session::TtsPlaybackState::Paused
            })
            .unwrap_or(false);

        if behavior == config::TtsPauseResumeBehavior::ResumeFromPausePoint && paused {
            if let Some(runtime) = guard.tts_request.as_ref() {
                runtime.set_paused(false);
                should_sync_tts = false;
            }
        }
    }
    apply_reader_command_with_sync(
        &app,
        &state,
        session::SessionCommand::TtsPlay,
        should_sync_tts,
    )
}

#[tauri::command]
fn reader_tts_pause(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    {
        let mut guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        let behavior = guard
            .reader
            .as_ref()
            .map(|reader| reader.config.tts_pause_resume_behavior)
            .unwrap_or_default();
        if behavior == config::TtsPauseResumeBehavior::RestartSentence {
            cancel_tts_request(&mut guard);
        } else if let Some(runtime) = guard.tts_request.as_ref() {
            runtime.set_paused(true);
        }
    }
    apply_reader_command_with_sync(&app, &state, session::SessionCommand::TtsPause, false)
}

#[tauri::command]
fn reader_tts_toggle_play_pause(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    let mut should_sync_tts = true;
    {
        let mut guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        let normalizer = guard.normalizer.clone();
        let panels = guard.panels;
        let behavior = guard
            .reader
            .as_ref()
            .map(|reader| reader.config.tts_pause_resume_behavior)
            .unwrap_or_default();

        let tts_state = guard
            .reader
            .as_mut()
            .map(|reader| reader.snapshot(panels, &normalizer).tts.state)
            .unwrap_or(session::TtsPlaybackState::Idle);

        match tts_state {
            session::TtsPlaybackState::Playing => {
                if behavior == config::TtsPauseResumeBehavior::RestartSentence {
                    cancel_tts_request(&mut guard);
                } else if let Some(runtime) = guard.tts_request.as_ref() {
                    runtime.set_paused(true);
                }
                should_sync_tts = false;
            }
            session::TtsPlaybackState::Paused => {
                if behavior == config::TtsPauseResumeBehavior::ResumeFromPausePoint {
                    if let Some(runtime) = guard.tts_request.as_ref() {
                        runtime.set_paused(false);
                        should_sync_tts = false;
                    }
                }
            }
            session::TtsPlaybackState::Idle => {}
        }
    }
    apply_reader_command_with_sync(
        &app,
        &state,
        session::SessionCommand::TtsTogglePlayPause,
        should_sync_tts,
    )
}

#[tauri::command]
fn reader_tts_play_from_page_start(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::TtsPlayFromPageStart)
}

#[tauri::command]
fn reader_tts_play_from_highlight(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::TtsPlayFromHighlight)
}

#[tauri::command]
fn reader_tts_seek_next(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::TtsSeekNext)
}

#[tauri::command]
fn reader_tts_seek_prev(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::TtsSeekPrev)
}

#[tauri::command]
fn reader_tts_repeat_sentence(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::TtsRepeatSentence)
}

#[tauri::command]
fn reader_tts_precompute_page(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    let (
        snapshot,
        request_id,
        source_path,
        sentences,
        threads,
        progress_log_interval,
        model_path,
        espeak_path,
    ) = {
        let mut guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        let normalizer = guard.normalizer.clone();
        let panels = guard.panels;
        let request_id = allocate_request_id(&mut guard);
        let reader = guard
            .reader
            .as_mut()
            .ok_or_else(|| bridge_error("no_reader", "No active reader session"))?;
        let snapshot = reader.snapshot(panels, &normalizer);
        let sentences = snapshot.sentences.clone();
        (
            snapshot,
            request_id,
            reader.source_path.clone(),
            sentences,
            reader.config.tts_threads.max(1).min(2),
            Duration::from_secs_f64(reader.config.tts_progress_log_interval_secs.max(0.1) as f64),
            PathBuf::from(reader.config.tts_model_path.clone()),
            PathBuf::from(reader.config.tts_espeak_path.clone()),
        )
    };

    emit_reader_state(&app, request_id, "reader_tts_precompute_page", &snapshot);
    emit_tts_state(
        &app,
        request_id,
        "reader_tts_precompute_page",
        &snapshot.tts,
    );

    if sentences.is_empty() {
        return Ok(snapshot);
    }

    std::thread::spawn(move || {
        let cache_root = cache::hash_dir(&source_path).join("tts");
        let engine = match tts::TtsEngine::new(model_path, espeak_path) {
            Ok(engine) => engine,
            Err(err) => {
                warn!(
                    request_id,
                    error = %err,
                    "Failed to initialize Piper TTS engine for page precompute"
                );
                return;
            }
        };

        match engine.prepare_batch(cache_root, sentences, 0, threads, progress_log_interval) {
            Ok(prepared) => {
                info!(
                    request_id,
                    file_count = prepared.len(),
                    "Precomputed page TTS audio files"
                );
            }
            Err(err) => {
                warn!(
                    request_id,
                    error = %err,
                    "Failed to precompute page TTS audio"
                );
            }
        }
    });

    Ok(snapshot)
}
#[tauri::command]
fn reader_close_session(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<SessionState, BridgeError> {
    let (session, request_id, cancelled_request, cancelled_source_path) = {
        let mut guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        let request_id = allocate_request_id(&mut guard);
        let cancelled_request = if guard.open_in_flight {
            guard.active_open_request
        } else {
            None
        };
        let cancelled_source_path = guard
            .active_open_source_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string());
        let _ = cleanup_for_shutdown(&mut guard);
        (
            to_session_state(&guard),
            request_id,
            cancelled_request,
            cancelled_source_path,
        )
    };
    emit_session_state(&app, request_id, "reader_close_session", &session);
    if let Some(cancelled_request) = cancelled_request {
        let _ = app.emit(
            "source-open",
            SourceOpenEvent {
                request_id: cancelled_request,
                phase: "cancelled".to_string(),
                source_path: cancelled_source_path,
                message: Some("Source open request cancelled by session close".to_string()),
            },
        );
    }
    Ok(session)
}

#[tauri::command]
fn app_safe_quit(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<(), BridgeError> {
    finalize_shutdown_from_mutex(state.inner());
    app.exit(0);
    Ok(())
}

#[tauri::command]
fn logging_set_level(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
    level: String,
) -> Result<String, BridgeError> {
    let parsed = parse_log_level_label(&level).ok_or_else(|| {
        bridge_error(
            "invalid_input",
            format!("Unsupported log level '{level}'. Use trace/debug/info/warn/error."),
        )
    })?;

    let request_id = {
        let mut guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        let request_id = allocate_request_id(&mut guard);
        guard.base_config.log_level = parsed;
        request_id
    };

    tauri_plugin_log::log::set_max_level(log_level_to_filter(parsed));
    let level_label = parsed.as_filter_str().to_string();
    let _ = app.emit(
        "log-level",
        LogLevelEvent {
            request_id,
            level: level_label.clone(),
        },
    );
    info!(request_id, level = %level_label, "Updated runtime log level");
    Ok(level_label)
}

#[tauri::command]
fn calibre_load_cached_books(
    state: State<'_, Mutex<BackendState>>,
) -> Result<Vec<CalibreBookDto>, BridgeError> {
    let mut guard = state
        .lock()
        .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;

    let books = calibre::load_cached_books(&guard.calibre_config)
        .map_err(|err| bridge_error("calibre_cache_load_failed", err.to_string()))?;

    guard.calibre_books = books.clone();
    Ok(books.into_iter().map(map_calibre_book).collect())
}

#[tauri::command]
async fn calibre_load_books(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
    force_refresh: Option<bool>,
) -> Result<Vec<CalibreBookDto>, BridgeError> {
    let force_refresh = force_refresh.unwrap_or(false);
    let (config, request_id, cancel_token) = {
        let mut guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        if guard.calibre_load_request.is_some() {
            return Err(bridge_error(
                "operation_conflict",
                "A calibre load operation is already in progress",
            ));
        }
        let request_id = allocate_request_id(&mut guard);
        let cancel_token = cancellation::CancellationToken::new();
        guard.calibre_load_request = Some(request_id);
        guard.calibre_cancel_token = Some(cancel_token.clone());
        (guard.calibre_config.clone(), request_id, cancel_token)
    };

    info!(request_id, force_refresh, "Starting calibre load request");

    let _ = app.emit(
        "calibre-load",
        CalibreLoadEvent {
            request_id,
            phase: "started".to_string(),
            count: None,
            message: None,
        },
    );

    let cancel_for_task = cancel_token.clone();
    let books_result = tauri::async_runtime::spawn_blocking(move || {
        calibre::load_books_with_cancel(&config, force_refresh, Some(&cancel_for_task))
    })
    .await
    .map_err(|err| {
        bridge_error(
            "task_join_error",
            format!("Failed to join calibre task: {err}"),
        )
    })
    .and_then(|result| result.map_err(|err| bridge_error("calibre_load_failed", err.to_string())));

    let mut guard = state
        .lock()
        .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
    let stale_or_cancelled = guard.calibre_load_request != Some(request_id);
    if !stale_or_cancelled {
        guard.calibre_load_request = None;
        guard.calibre_cancel_token = None;
    }

    if stale_or_cancelled || cancel_token.is_cancelled() {
        drop(guard);
        let message = "Calibre load request was cancelled".to_string();
        let _ = app.emit(
            "calibre-load",
            CalibreLoadEvent {
                request_id,
                phase: "cancelled".to_string(),
                count: None,
                message: Some(message.clone()),
            },
        );
        info!(request_id, force_refresh, "Calibre load request cancelled");
        return Err(bridge_error("operation_cancelled", message));
    }

    let books = match books_result {
        Ok(books) => books,
        Err(err) => {
            drop(guard);
            warn!(
                request_id,
                force_refresh,
                error = %err.message,
                "Calibre load request failed"
            );
            let _ = app.emit(
                "calibre-load",
                CalibreLoadEvent {
                    request_id,
                    phase: "failed".to_string(),
                    count: None,
                    message: Some(err.message.clone()),
                },
            );
            return Err(err);
        }
    };

    guard.calibre_books = books.clone();
    drop(guard);

    let _ = app.emit(
        "calibre-load",
        CalibreLoadEvent {
            request_id,
            phase: "finished".to_string(),
            count: Some(books.len()),
            message: None,
        },
    );
    info!(
        request_id,
        force_refresh,
        count = books.len(),
        "Completed calibre load request"
    );

    Ok(books.into_iter().map(map_calibre_book).collect())
}

#[tauri::command]
async fn calibre_open_book(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
    book_id: u64,
) -> Result<OpenSourceResult, BridgeError> {
    let (book, calibre_config) = {
        let guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        let book = guard
            .calibre_books
            .iter()
            .find(|book| book.id == book_id)
            .cloned()
            .ok_or_else(|| {
                bridge_error("not_found", format!("Unknown calibre book id={book_id}"))
            })?;
        (book, guard.calibre_config.clone())
    };

    let path = tauri::async_runtime::spawn_blocking(move || {
        calibre::materialize_book_path(&calibre_config, &book)
    })
    .await
    .map_err(|err| {
        bridge_error(
            "task_join_error",
            format!("Failed to join calibre-open task: {err}"),
        )
    })?
    .map_err(|err| bridge_error("calibre_open_failed", err.to_string()))?;

    open_resolved_source(&app, &state, path).await
}

#[tauri::command]
async fn calibre_ensure_thumbnail(
    state: State<'_, Mutex<BackendState>>,
    book_id: u64,
) -> Result<Option<String>, BridgeError> {
    let (calibre_config, mut book) = {
        let guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        let book = guard
            .calibre_books
            .iter()
            .find(|book| book.id == book_id)
            .cloned()
            .ok_or_else(|| {
                bridge_error("not_found", format!("Unknown calibre book id={book_id}"))
            })?;
        (guard.calibre_config.clone(), book)
    };

    let thumbnail_path = tauri::async_runtime::spawn_blocking(move || {
        let _ = calibre::ensure_thumbnail_for_book(&calibre_config, &mut book, true);
        if book.cover_thumbnail.is_none()
            && book.extension.eq_ignore_ascii_case("epub")
            && let Ok(materialized) = calibre::materialize_book_path(&calibre_config, &book)
        {
            tracing::info!(
                book_id = book.id,
                path = %materialized.display(),
                "Materialized EPUB source to retry thumbnail extraction"
            );
            book.path = Some(materialized);
            let _ = calibre::ensure_thumbnail_for_book(&calibre_config, &mut book, false);
        }
        book.cover_thumbnail
    })
    .await
    .map_err(|err| {
        bridge_error(
            "task_join_error",
            format!("Failed to join calibre-thumbnail task: {err}"),
        )
    })?;

    let mut guard = state
        .lock()
        .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
    if let Some(cached_book) = guard
        .calibre_books
        .iter_mut()
        .find(|book| book.id == book_id)
    {
        cached_book.cover_thumbnail = thumbnail_path.clone();
    }
    let data_url = thumbnail_path
        .as_deref()
        .and_then(thumbnail_path_to_data_url);
    info!(
        book_id,
        has_thumbnail = data_url.is_some(),
        "Ensured calibre thumbnail on demand"
    );
    Ok(data_url)
}

#[cfg(target_os = "linux")]
fn configure_linux_display_backend() {
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
    let xdg_session_type = std::env::var("XDG_SESSION_TYPE")
        .ok()
        .map(|value| value.to_ascii_lowercase());
    let x_display = std::env::var("DISPLAY").ok();
    let wayland_available = wayland_display.is_some()
        || matches!(
            xdg_session_type.clone(),
            Some(value) if value == "wayland"
        );
    let allow_x11 = matches!(
        std::env::var("LANTERNLEAF_ALLOW_X11")
            .ok()
            .map(|value| value.to_ascii_lowercase()),
        Some(value) if value == "1" || value == "true" || value == "yes"
    );

    if !wayland_available || allow_x11 {
        info!(
            wayland_display = ?wayland_display,
            xdg_session_type = ?xdg_session_type,
            x_display = ?x_display,
            allow_x11,
            "Skipping Wayland-first backend override"
        );
        return;
    }

    let current_gdk_backend = std::env::var("GDK_BACKEND")
        .ok()
        .map(|value| value.to_ascii_lowercase());
    let current_winit_backend = std::env::var("WINIT_UNIX_BACKEND").ok();
    let prefer_x11_first = x_display.is_some() && wayland_display.is_some();
    let desired_gdk_backend = if prefer_x11_first {
        "x11,wayland"
    } else {
        "wayland,x11"
    };

    // Prefer Wayland but include X11 fallback so startup does not hard-fail when Wayland is present
    // but runtime-incompatible on this machine/session.
    if current_gdk_backend.as_deref() != Some(desired_gdk_backend) {
        // SAFETY: startup-time process env initialization before Tauri runtime threads start.
        unsafe {
            std::env::set_var("GDK_BACKEND", desired_gdk_backend);
        }
    }
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        // SAFETY: startup-time process env initialization before Tauri runtime threads start.
        unsafe {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    info!(
        wayland_display = ?wayland_display,
        xdg_session_type = ?xdg_session_type,
        x_display = ?x_display,
        gdk_backend = desired_gdk_backend,
        winit_backend = ?current_winit_backend,
        webkit_disable_dmabuf_renderer = ?std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").ok(),
        "Configured Linux display backend defaults with safe fallback ordering"
    );
}

macro_rules! bridge_command_idents {
    ($callback:ident) => {
        $callback!(
            session_get_bootstrap,
            session_toggle_theme,
            session_get_state,
            session_return_to_starter,
            panel_toggle_settings,
            panel_toggle_stats,
            panel_toggle_tts,
            recent_list,
            recent_delete,
            source_open_path,
            source_open_clipboard,
            source_open_clipboard_text,
            browser_tabs_health,
            browser_tabs_list_windows,
            browser_tabs_list_tabs,
            source_open_browser_tab,
            source_refresh_browser_tab,
            reader_get_snapshot,
            reader_next_page,
            reader_prev_page,
            reader_set_page,
            reader_sentence_click,
            reader_next_sentence,
            reader_prev_sentence,
            reader_toggle_text_only,
            reader_apply_settings,
            reader_search_set_query,
            reader_search_next,
            reader_search_prev,
            reader_tts_play,
            reader_tts_pause,
            reader_tts_toggle_play_pause,
            reader_tts_play_from_page_start,
            reader_tts_play_from_highlight,
            reader_tts_seek_next,
            reader_tts_seek_prev,
            reader_tts_repeat_sentence,
            reader_tts_precompute_page,
            reader_close_session,
            app_safe_quit,
            logging_set_level,
            calibre_load_cached_books,
            calibre_load_books,
            calibre_open_book,
            calibre_ensure_thumbnail
        )
    };
}

macro_rules! as_generate_handler {
    ($($command:ident),* $(,)?) => {
        tauri::generate_handler![$($command),*]
    };
}

macro_rules! as_command_name_slice {
    ($($command:ident),* $(,)?) => {
        &[$(stringify!($command)),*]
    };
}

const BRIDGE_COMMAND_NAMES: &[&str] = bridge_command_idents!(as_command_name_slice);
const BRIDGE_EVENT_NAMES: &[&str] = &[
    "source-open",
    "calibre-load",
    "session-state",
    "reader-state",
    "tts-state",
    "pdf-transcription",
    "log-level",
];

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    configure_linux_display_backend();

    let config_path = app_config_path();
    let startup_config = config::load_config(&config_path);
    let log_timestamp = log_timestamp_slug();
    init_tracing(&startup_config, &log_timestamp);
    configure_cache_dir_from_config(&startup_config, &config_path);
    configure_cache_dir_from_workspace();
    configure_calibre_config_path_from_workspace();
    configure_normalizer_config_path_from_workspace();
    configure_abbreviations_config_path_from_workspace();
    let mut log_builder = tauri_plugin_log::Builder::new()
        .level(log_level_to_filter(startup_config.log_level))
        .target(Target::new(TargetKind::Stdout))
        .target(Target::new(TargetKind::Webview));

    if runtime_mode_label() == "dev" {
        let logs_dir = dev_logs_dir();
        log_builder = log_builder.target(Target::new(TargetKind::Folder {
            path: logs_dir.clone(),
            file_name: Some(format!("lanternleaf-webview-dev-{log_timestamp}")),
        }));
        info!(
            mode = %runtime_mode_label(),
            logs_dir = %logs_dir.display(),
            "Enabled dev file logging target"
        );
    }

    let log_plugin = log_builder.build();

    info!("Starting LanternLeaf tauri bridge");
    info!(
        command_count = BRIDGE_COMMAND_NAMES.len(),
        event_count = BRIDGE_EVENT_NAMES.len(),
        "Registered stable bridge surface"
    );
    let builder = tauri::Builder::default()
        .setup(|app| {
            let app_handle = app.handle().clone();
            if let Err(err) = ctrlc::set_handler(move || {
                info!("Received Ctrl+C; running safe shutdown housekeeping");
                let state = app_handle.state::<Mutex<BackendState>>();
                finalize_shutdown_from_mutex(state.inner());
                app_handle.exit(130);
            }) {
                warn!("Failed to install Ctrl+C signal handler: {err}");
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                let state = window.app_handle().state::<Mutex<BackendState>>();
                finalize_shutdown_from_mutex(state.inner());
            }
        })
        .manage(Mutex::new(BackendState::new()))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(log_plugin)
        .invoke_handler(bridge_command_idents!(as_generate_handler));

    if let Err(err) = builder.run(tauri::generate_context!()) {
        warn!("tauri runtime failed: {err}");
        panic!("tauri runtime failed: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_file(name: &str, extension: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("lanternleaf_test_{name}_{nanos}.{extension}"))
    }

    #[test]
    fn bridge_command_surface_remains_stable() {
        assert_eq!(BRIDGE_COMMAND_NAMES.len(), 45);
        assert_eq!(BRIDGE_COMMAND_NAMES[0], "session_get_bootstrap");
        assert_eq!(
            BRIDGE_COMMAND_NAMES[BRIDGE_COMMAND_NAMES.len() - 1],
            "calibre_ensure_thumbnail"
        );
        assert!(BRIDGE_COMMAND_NAMES.contains(&"source_open_path"));
        assert!(BRIDGE_COMMAND_NAMES.contains(&"session_toggle_theme"));
        assert!(BRIDGE_COMMAND_NAMES.contains(&"source_open_clipboard"));
        assert!(BRIDGE_COMMAND_NAMES.contains(&"source_open_clipboard_text"));
        assert!(BRIDGE_COMMAND_NAMES.contains(&"browser_tabs_health"));
        assert!(BRIDGE_COMMAND_NAMES.contains(&"browser_tabs_list_windows"));
        assert!(BRIDGE_COMMAND_NAMES.contains(&"browser_tabs_list_tabs"));
        assert!(BRIDGE_COMMAND_NAMES.contains(&"source_open_browser_tab"));
        assert!(BRIDGE_COMMAND_NAMES.contains(&"source_refresh_browser_tab"));
        assert!(BRIDGE_COMMAND_NAMES.contains(&"reader_tts_play"));
        assert!(BRIDGE_COMMAND_NAMES.contains(&"reader_tts_repeat_sentence"));
        assert!(BRIDGE_COMMAND_NAMES.contains(&"reader_tts_precompute_page"));
        assert!(BRIDGE_COMMAND_NAMES.contains(&"calibre_open_book"));
    }

    #[test]
    fn bridge_event_surface_remains_stable() {
        assert_eq!(
            BRIDGE_EVENT_NAMES,
            &[
                "source-open",
                "calibre-load",
                "session-state",
                "reader-state",
                "tts-state",
                "pdf-transcription",
                "log-level",
            ]
        );
    }

    #[test]
    fn bootstrap_state_roundtrips_json_contract() {
        let state = BootstrapState {
            app_name: "LanternLeaf".to_string(),
            mode: runtime_mode_label(),
            config: BootstrapConfig {
                theme: config::ThemeMode::Day,
                font_family: config::FontFamily::Lexend,
                font_weight: config::FontWeight::Bold,
                day_highlight: config::HighlightColor {
                    r: 0.2,
                    g: 0.4,
                    b: 0.7,
                    a: 0.15,
                },
                night_highlight: config::HighlightColor {
                    r: 0.8,
                    g: 0.8,
                    b: 0.5,
                    a: 0.2,
                },
                log_level: "debug".to_string(),
                default_font_size: 22,
                default_lines_per_page: 700,
                default_tts_speed: 2.5,
                default_pause_after_sentence: 0.06,
                key_toggle_play_pause: "space".to_string(),
                key_next_sentence: "f".to_string(),
                key_prev_sentence: "s".to_string(),
                key_repeat_sentence: "r".to_string(),
                key_toggle_search: "ctrl+f".to_string(),
                key_safe_quit: "q".to_string(),
                key_toggle_settings: "ctrl+t".to_string(),
                key_toggle_stats: "ctrl+g".to_string(),
                key_toggle_tts: "ctrl+y".to_string(),
                browser_tabs_enabled: true,
            },
        };

        let json = serde_json::to_string(&state).expect("serialize bootstrap");
        let decoded: BootstrapState = serde_json::from_str(&json).expect("deserialize bootstrap");
        assert_eq!(decoded.config.default_font_size, 22);
        assert_eq!(decoded.config.theme, config::ThemeMode::Day);
        assert_eq!(decoded.config.key_toggle_tts, "ctrl+y");
    }

    #[test]
    fn session_state_roundtrips_json_contract() {
        let state = SessionState {
            mode: UiMode::Reader,
            active_source_path: Some("/tmp/book.epub".to_string()),
            open_in_flight: false,
            panels: session::PanelState {
                show_settings: true,
                show_stats: false,
                show_tts: true,
            },
        };
        let json = serde_json::to_string(&state).expect("serialize session");
        let decoded: SessionState = serde_json::from_str(&json).expect("deserialize session");
        assert!(matches!(decoded.mode, UiMode::Reader));
        assert_eq!(
            decoded.active_source_path.as_deref(),
            Some("/tmp/book.epub")
        );
        assert!(decoded.panels.show_tts);
    }

    #[test]
    fn event_contracts_include_request_ids() {
        let source = SourceOpenEvent {
            request_id: 42,
            phase: "started".to_string(),
            source_path: Some("/tmp/book.epub".to_string()),
            message: None,
        };
        let source_json = serde_json::to_value(source).expect("serialize source event");
        assert_eq!(
            source_json.get("request_id").and_then(|v| v.as_u64()),
            Some(42)
        );

        let calibre = CalibreLoadEvent {
            request_id: 43,
            phase: "finished".to_string(),
            count: Some(123),
            message: None,
        };
        let calibre_json = serde_json::to_value(calibre).expect("serialize calibre event");
        assert_eq!(
            calibre_json.get("request_id").and_then(|v| v.as_u64()),
            Some(43)
        );
        assert_eq!(
            calibre_json.get("count").and_then(|v| v.as_u64()),
            Some(123)
        );

        let session_event = SessionStateEvent {
            request_id: 44,
            action: "reader_close_session".to_string(),
            session: SessionState {
                mode: UiMode::Starter,
                active_source_path: None,
                open_in_flight: false,
                panels: session::PanelState {
                    show_settings: true,
                    show_stats: false,
                    show_tts: true,
                },
            },
        };
        let session_json = serde_json::to_value(session_event).expect("serialize session event");
        assert_eq!(
            session_json.get("request_id").and_then(|v| v.as_u64()),
            Some(44)
        );

        let reader_event = ReaderStateEvent {
            request_id: 45,
            action: "reader_next_page".to_string(),
            reader: session::ReaderSnapshot {
                source_path: "/tmp/book.epub".to_string(),
                source_name: "book.epub".to_string(),
                current_page: 0,
                total_pages: 1,
                text_only_mode: false,
                has_structured_markdown: false,
                pretty_kind: session::PrettyKind::None,
                images: Vec::new(),
                tts_text_page: "hello".to_string(),
                reading_markdown_page: None,
                reading_html_page: None,
                page_text: "hello".to_string(),
                sentences: vec!["hello".to_string()],
                sentence_anchor_map: vec![Some(0)],
                highlighted_sentence_idx: Some(0),
                search_query: String::new(),
                search_matches: vec![],
                selected_search_match: None,
                settings: session::ReaderSettingsView {
                    theme: config::ThemeMode::Day,
                    font_family: config::FontFamily::Lexend,
                    font_weight: config::FontWeight::Bold,
                    day_highlight: config::HighlightColor {
                        r: 0.2,
                        g: 0.4,
                        b: 0.7,
                        a: 0.15,
                    },
                    night_highlight: config::HighlightColor {
                        r: 0.8,
                        g: 0.8,
                        b: 0.5,
                        a: 0.2,
                    },
                    font_size: 22,
                    line_spacing: 1.2,
                    word_spacing: 0,
                    letter_spacing: 0,
                    margin_horizontal: 100,
                    margin_vertical: 12,
                    lines_per_page: 700,
                    pause_after_sentence: 0.06,
                    auto_scroll_tts: false,
                    center_spoken_sentence: true,
                    time_remaining_display: config::TimeRemainingDisplay::Adaptive,
                    tts_speed: 2.5,
                    tts_volume: 1.0,
                },
                tts: session::ReaderTtsView {
                    state: session::TtsPlaybackState::Idle,
                    current_sentence_idx: Some(0),
                    sentence_count: 1,
                    can_seek_prev: false,
                    can_seek_next: false,
                    progress_pct: 0.0,
                },
                stats: session::ReaderStats {
                    page_index: 1,
                    total_pages: 1,
                    tts_progress_pct: 0.0,
                    global_progress_pct: 0.0,
                    page_time_remaining_secs: 0.0,
                    book_time_remaining_secs: 0.0,
                    page_word_count: 1,
                    page_sentence_count: 1,
                    page_start_percent: 0.0,
                    page_end_percent: 100.0,
                    words_read_up_to_page_start: 0,
                    sentences_read_up_to_page_start: 0,
                    words_read_up_to_page_end: 1,
                    sentences_read_up_to_page_end: 1,
                    words_read_up_to_current_position: 1,
                    sentences_read_up_to_current_position: 1,
                },
                panels: session::PanelState {
                    show_settings: true,
                    show_stats: false,
                    show_tts: true,
                },
            },
        };
        let reader_json = serde_json::to_value(reader_event).expect("serialize reader event");
        assert_eq!(
            reader_json.get("request_id").and_then(|v| v.as_u64()),
            Some(45)
        );
    }

    #[test]
    fn normalize_recent_limit_clamps_to_expected_bounds() {
        assert_eq!(normalize_recent_limit(None), DEFAULT_RECENT_LIMIT);
        assert_eq!(normalize_recent_limit(Some(0)), 1);
        assert_eq!(normalize_recent_limit(Some(1)), 1);
        assert_eq!(
            normalize_recent_limit(Some(MAX_RECENT_LIMIT + 123)),
            MAX_RECENT_LIMIT
        );
    }

    #[test]
    fn supported_source_extensions_match_contract() {
        assert!(is_supported_source(Path::new("/tmp/book.epub")));
        assert!(is_supported_source(Path::new("/tmp/book.PDF")));
        assert!(is_supported_source(Path::new("/tmp/book.txt")));
        assert!(is_supported_source(Path::new("/tmp/book.md")));
        assert!(is_supported_source(Path::new("/tmp/book.markdown")));
        assert!(is_supported_source(Path::new("/tmp/book.html")));
        assert!(is_supported_source(Path::new("/tmp/book.doc")));
        assert!(is_supported_source(Path::new("/tmp/book.docx")));
        assert!(!is_supported_source(Path::new("/tmp/book.odt")));
    }

    #[test]
    fn resolve_source_path_returns_expected_error_codes() {
        let empty = resolve_source_path("   ").expect_err("empty input must fail");
        assert_eq!(empty.code, "invalid_input");

        let missing = resolve_source_path("/tmp/this/path/does/not/exist.epub")
            .expect_err("missing source must fail");
        assert_eq!(missing.code, "not_found");

        let unsupported = unique_temp_file("unsupported", "odt");
        fs::write(&unsupported, "hello world").expect("write temp file");
        let err = resolve_source_path(unsupported.to_string_lossy().as_ref())
            .expect_err("unsupported extension must fail");
        assert_eq!(err.code, "unsupported_source");
        let _ = fs::remove_file(unsupported);
    }

    #[test]
    fn parse_log_level_label_accepts_supported_values() {
        assert_eq!(
            parse_log_level_label("trace"),
            Some(config::LogLevel::Trace)
        );
        assert_eq!(
            parse_log_level_label("DEBUG"),
            Some(config::LogLevel::Debug)
        );
        assert_eq!(parse_log_level_label("info"), Some(config::LogLevel::Info));
        assert_eq!(
            parse_log_level_label("warning"),
            Some(config::LogLevel::Warn)
        );
        assert_eq!(parse_log_level_label("warn"), Some(config::LogLevel::Warn));
        assert_eq!(
            parse_log_level_label("error"),
            Some(config::LogLevel::Error)
        );
        assert_eq!(parse_log_level_label("verbose"), None);
    }

    #[test]
    fn app_config_path_uses_override_env_when_present() {
        let key = "LANTERNLEAF_CONFIG_PATH";
        let previous = std::env::var_os(key);
        let override_path = unique_temp_file("config_override_path", "toml");
        // SAFETY: test-scoped env mutation; restored before test exits.
        unsafe {
            std::env::set_var(key, &override_path);
        }
        assert_eq!(app_config_path(), override_path);
        match previous {
            Some(value) => {
                // SAFETY: test-scoped env mutation restore.
                unsafe {
                    std::env::set_var(key, value);
                }
            }
            None => {
                // SAFETY: test-scoped env mutation restore.
                unsafe {
                    std::env::remove_var(key);
                }
            }
        }
    }

    #[test]
    fn cleanup_for_shutdown_clears_inflight_open_request() {
        let mut state = BackendState::new();
        state.mode = UiMode::Reader;
        state.active_source_path = Some(PathBuf::from("/tmp/active.epub"));
        state.active_open_source_path = Some(PathBuf::from("/tmp/opening.pdf"));
        state.open_in_flight = true;
        state.active_open_request = Some(77);

        let cancelled = cleanup_for_shutdown(&mut state);

        assert_eq!(cancelled, Some(77));
        assert!(matches!(state.mode, UiMode::Starter));
        assert!(state.active_source_path.is_none());
        assert!(state.active_open_source_path.is_none());
        assert!(!state.open_in_flight);
        assert!(state.active_open_request.is_none());
        assert!(state.reader.is_none());
    }

    #[test]
    fn cleanup_for_shutdown_without_inflight_open_returns_none() {
        let mut state = BackendState::new();
        state.mode = UiMode::Reader;
        state.active_source_path = Some(PathBuf::from("/tmp/active.epub"));
        state.open_in_flight = false;
        state.active_open_request = None;

        let cancelled = cleanup_for_shutdown(&mut state);

        assert_eq!(cancelled, None);
        assert!(matches!(state.mode, UiMode::Starter));
        assert!(state.active_source_path.is_none());
        assert!(!state.open_in_flight);
        assert!(state.active_open_request.is_none());
    }

    #[test]
    fn cleanup_for_shutdown_persists_active_reader_housekeeping() {
        let source = unique_temp_file("cleanup_housekeeping_source", "txt");
        fs::write(
            &source,
            "Housekeeping sentence one. Housekeeping sentence two. Housekeeping sentence three.",
        )
        .expect("write source fixture");

        let base_config = config::AppConfig::default();
        let normalizer = normalizer::TextNormalizer::default();
        let reader = session::load_session_for_source(source.clone(), &base_config, &normalizer)
            .expect("load reader session");

        let mut state = BackendState::new();
        state.mode = UiMode::Reader;
        state.active_source_path = Some(source.clone());
        state.reader = Some(reader);

        let cancelled = cleanup_for_shutdown(&mut state);

        assert_eq!(cancelled, None);
        assert!(matches!(state.mode, UiMode::Starter));
        assert!(state.active_source_path.is_none());
        assert!(state.reader.is_none());
        assert!(!state.open_in_flight);

        let bookmark = cache::load_bookmark(&source).expect("bookmark should be persisted");
        assert_eq!(bookmark.page, 0);
        let cached_config =
            cache::load_epub_config(&source).expect("reader config should be persisted");
        assert_eq!(cached_config.font_size, base_config.font_size);
        assert_eq!(cached_config.lines_per_page, base_config.lines_per_page);

        let cache_path = cache::hash_dir(&source);
        let _ = fs::remove_file(&source);
        let _ = fs::remove_dir_all(cache_path);
    }

    #[test]
    fn finalize_shutdown_persists_reader_housekeeping_without_writing_base_config() {
        let source = unique_temp_file("finalize_housekeeping_source", "txt");
        fs::write(
            &source,
            "Finalize sentence one. Finalize sentence two. Finalize sentence three.",
        )
        .expect("write source fixture");
        let config_path = unique_temp_file("finalize_housekeeping_config", "toml");

        let base_config = config::AppConfig::default();
        let normalizer = normalizer::TextNormalizer::default();
        let reader = session::load_session_for_source(source.clone(), &base_config, &normalizer)
            .expect("load reader session");

        let mut state = BackendState::new();
        state.mode = UiMode::Reader;
        state.active_source_path = Some(source.clone());
        state.base_config.log_level = config::LogLevel::Warn;
        state.reader = Some(reader);
        let state_mutex = Mutex::new(state);

        finalize_shutdown_with_config_path(&state_mutex, &config_path);

        assert!(
            !config_path.exists(),
            "base config should not be persisted during shutdown"
        );
        let bookmark = cache::load_bookmark(&source).expect("bookmark should be persisted");
        assert_eq!(bookmark.page, 0);
        let cached_config =
            cache::load_epub_config(&source).expect("reader config should be persisted");
        assert_eq!(cached_config.font_size, base_config.font_size);

        let cache_path = cache::hash_dir(&source);
        let _ = fs::remove_file(&source);
        let _ = fs::remove_file(&config_path);
        let _ = fs::remove_dir_all(cache_path);
    }

    #[test]
    fn begin_open_request_rejects_duplicates_and_tracks_path() {
        let mut state = BackendState::new();
        let first_source = PathBuf::from("/tmp/first.epub");
        let second_source = PathBuf::from("/tmp/second.pdf");

        let (request_id, cancel_token) =
            begin_open_request(&mut state, &first_source).expect("first open request");
        assert_eq!(request_id, 1);
        assert!(state.open_in_flight);
        assert_eq!(state.active_open_request, Some(1));
        assert!(state.open_cancel_token.is_some());
        assert!(!cancel_token.is_cancelled());
        assert_eq!(
            state.active_open_source_path.as_deref(),
            Some(first_source.as_path())
        );

        let duplicate =
            begin_open_request(&mut state, &second_source).expect_err("duplicate open should fail");
        assert_eq!(duplicate.code, "operation_conflict");
        assert_eq!(state.active_open_request, Some(1));
        assert_eq!(
            state.active_open_source_path.as_deref(),
            Some(first_source.as_path())
        );
    }

    #[test]
    fn cleanup_for_shutdown_cancels_registered_job_tokens() {
        let mut state = BackendState::new();
        let (_, open_token) = begin_open_request(&mut state, Path::new("/tmp/open.epub"))
            .expect("open request should register token");
        let calibre_token = cancellation::CancellationToken::new();
        let tts_token = cancellation::CancellationToken::new();
        state.calibre_load_request = Some(42);
        state.calibre_cancel_token = Some(calibre_token.clone());
        state.tts_request = Some(TtsRequestRuntime {
            request_id: 99,
            cancel_token: tts_token.clone(),
            pause_requested: Arc::new(AtomicBool::new(false)),
        });

        let _ = cleanup_for_shutdown(&mut state);

        assert!(open_token.is_cancelled());
        assert!(calibre_token.is_cancelled());
        assert!(tts_token.is_cancelled());
        assert!(state.open_cancel_token.is_none());
        assert!(state.calibre_cancel_token.is_none());
        assert!(state.calibre_load_request.is_none());
        assert!(state.tts_request.is_none());
    }
}
