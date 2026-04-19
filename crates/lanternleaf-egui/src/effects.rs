use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use lanternleaf_app::contracts::{
    BootstrapState, BridgeError, CalibreBookDto, CalibreLoadEvent, LogLevelEvent, OpenSourceResult,
    ReaderStateEvent, RecentBook, SessionState, SessionStateEvent, SourceOpenEvent, UiMode,
};
use lanternleaf_app::pipeline::{
    AppEvent, EffectOwner, OperationScope, PanelToggle, PersistenceOutcome, PersistenceTrigger,
    PlannedEffect, RuntimeEffect,
};
use lanternleaf_core::{
    browser_tabs, cache, cache_service, calibre, config, config_service, normalizer, session,
};
use tracing::{debug, error, info, trace, warn};

use crate::helpers::bootstrap_config_from_app_config;

type EffectHandler =
    Arc<dyn Fn(EffectContext, PlannedEffect, mpsc::Sender<AppEvent>) + Send + Sync>;

#[derive(Clone)]
pub struct EffectContext {
    pub config: Arc<Mutex<config::AppConfig>>,
    pub normalizer: Arc<normalizer::TextNormalizer>,
    pub calibre_config: Arc<calibre::CalibreConfig>,
    pub session: Arc<Mutex<Option<session::ReaderSession>>>,
    pub panels: Arc<Mutex<session::PanelState>>,
    pub persistence: Arc<
        lanternleaf_app::persistence::PersistenceLifecycle<
            lanternleaf_app::persistence::FilesystemPersistenceService,
        >,
    >,
    pub cache_service: Arc<dyn cache_service::CacheService>,
    pub config_path: PathBuf,
    pub config_service: Arc<dyn config_service::ConfigService>,
}

impl EffectContext {
    #[cfg(test)]
    pub fn new(
        config: config::AppConfig,
        normalizer: normalizer::TextNormalizer,
        persistence: Arc<
            lanternleaf_app::persistence::PersistenceLifecycle<
                lanternleaf_app::persistence::FilesystemPersistenceService,
            >,
        >,
        config_path: PathBuf,
    ) -> Self {
        let cache_service: Arc<dyn cache_service::CacheService> =
            Arc::new(cache_service::FilesystemCacheService);
        let config_service: Arc<dyn config_service::ConfigService> =
            Arc::new(config_service::FilesystemConfigService);
        Self::with_services(
            config,
            normalizer,
            persistence,
            cache_service,
            config_path,
            config_service,
        )
    }

    pub fn with_services(
        config: config::AppConfig,
        normalizer: normalizer::TextNormalizer,
        persistence: Arc<
            lanternleaf_app::persistence::PersistenceLifecycle<
                lanternleaf_app::persistence::FilesystemPersistenceService,
            >,
        >,
        cache_service: Arc<dyn cache_service::CacheService>,
        config_path: PathBuf,
        config_service: Arc<dyn config_service::ConfigService>,
    ) -> Self {
        Self {
            config: Arc::new(Mutex::new(config)),
            normalizer: Arc::new(normalizer),
            calibre_config: Arc::new(calibre::CalibreConfig::load_default()),
            session: Arc::new(Mutex::new(None)),
            panels: Arc::new(Mutex::new(session::PanelState::default())),
            persistence,
            cache_service,
            config_path,
            config_service,
        }
    }
}

pub struct EffectDispatcher {
    effect_tx: mpsc::Sender<PlannedEffect>,
    event_rx: mpsc::Receiver<AppEvent>,
}

impl EffectDispatcher {
    pub fn new(context: EffectContext) -> Self {
        Self::with_handler(context, Arc::new(execute_effect))
    }

    fn with_handler(context: EffectContext, handler: EffectHandler) -> Self {
        let (effect_tx, effect_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        thread::spawn(move || {
            for planned in effect_rx {
                let ctx = context.clone();
                let handler = handler.clone();
                let event_tx = event_tx.clone();
                thread::spawn(move || handler(ctx, planned, event_tx));
            }
        });

        Self {
            effect_tx,
            event_rx,
        }
    }

    pub fn dispatch(&self, effect: PlannedEffect) {
        if self.effect_tx.send(effect).is_err() {
            warn!("Effect dispatcher channel closed");
        }
    }

    pub fn drain_events(&self) -> Vec<AppEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_rx.try_recv() {
            events.push(event);
        }
        events
    }
}

fn execute_effect(
    context: EffectContext,
    planned: PlannedEffect,
    event_tx: mpsc::Sender<AppEvent>,
) {
    let request_id = planned.request_id;
    let effect = planned.effect;
    let failure_effect = effect.clone();
    trace!(
        request_id,
        effect = ?effect,
        "Dispatching runtime effect off the UI thread"
    );
    let result = match effect {
        RuntimeEffect::LoadBootstrap => handle_load_bootstrap(&context, request_id),
        RuntimeEffect::ListRecents { limit } => handle_list_recents(&context, request_id, limit),
        RuntimeEffect::DeleteRecent { source_path } => {
            handle_delete_recent(&context, request_id, &source_path)
        }
        RuntimeEffect::CloseRecentBrowserTab { source_path } => {
            handle_close_recent_browser_tab(&context, request_id, &source_path)
        }
        RuntimeEffect::OpenSourcePath { path } => handle_open_source(&context, request_id, &path),
        RuntimeEffect::OpenClipboardText { text } => {
            handle_open_clipboard_text(&context, request_id, &text)
        }
        RuntimeEffect::OpenClipboard => Err(bridge_error(
            "clipboard_unavailable",
            "Clipboard access not wired in egui",
        )),
        RuntimeEffect::LoadBrowserTabsHealth => handle_browser_tabs_health(&context, request_id),
        RuntimeEffect::ListBrowserTabWindows => handle_browser_tabs_windows(&context, request_id),
        RuntimeEffect::ListBrowserTabs {
            window_id,
            query,
            refresh,
        } => handle_browser_tabs_list(&context, request_id, window_id, query, refresh),
        RuntimeEffect::OpenBrowserTab { tab_id, window_id } => {
            handle_open_browser_tab(&context, request_id, tab_id, window_id, false)
        }
        RuntimeEffect::OpenBrowserTabBundle { tab_id, window_id } => {
            handle_open_browser_tab(&context, request_id, tab_id, window_id, true)
        }
        RuntimeEffect::RefreshBrowserTab { tab_id, window_id } => {
            handle_refresh_browser_tab(&context, request_id, tab_id, window_id)
        }
        RuntimeEffect::LoadCalibreCachedBooks => handle_calibre_cached_books(&context, request_id),
        RuntimeEffect::LoadCalibreBooks { force_refresh } => {
            handle_calibre_books(&context, request_id, force_refresh)
        }
        RuntimeEffect::OpenCalibreBook { id } => handle_calibre_open_book(&context, request_id, id),
        RuntimeEffect::EnsureCalibreThumbnail { id } => {
            handle_calibre_thumbnail(&context, request_id, id)
        }
        RuntimeEffect::ApplyReaderCommand { command, .. } => {
            handle_reader_command(&context, request_id, command)
        }
        RuntimeEffect::TogglePanel { panel } => handle_toggle_panel(&context, request_id, panel),
        RuntimeEffect::SetRuntimeLogLevel { level } => {
            handle_set_log_level(&context, request_id, &level)
        }
        RuntimeEffect::FlushPersistence { trigger } => {
            handle_persistence_flush(&context, request_id, trigger)
        }
        RuntimeEffect::ReturnToStarter => handle_return_to_starter(&context, request_id),
        RuntimeEffect::CloseReaderSession => handle_close_reader_session(&context, request_id),
        RuntimeEffect::SafeQuit => handle_safe_quit(request_id),
        RuntimeEffect::ToggleTheme => handle_toggle_theme(&context, request_id),
        RuntimeEffect::PrecomputeTtsPage
        | RuntimeEffect::LoadPdfBytes { .. }
        | RuntimeEffect::LoadPdfSyncMap { .. }
        | RuntimeEffect::PersistPdfSyncMap { .. }
        | RuntimeEffect::LoadPdfRenderPrecomputed { .. } => Err(bridge_error(
            "not_implemented",
            "Runtime effect not wired in egui dispatcher",
        )),
    };

    match result {
        Ok(events) => {
            for event in events {
                let _ = event_tx.send(event);
            }
        }
        Err(error) => {
            emit_failure_progress(&failure_effect, request_id, &event_tx);
            error!(
                request_id,
                code = %error.code,
                message = %error.message,
                "Runtime effect failed"
            );
            let scope = effect_scope(&failure_effect);
            if let RuntimeEffect::FlushPersistence { trigger } = failure_effect {
                let _ = event_tx.send(AppEvent::PersistenceFlushed {
                    request_id,
                    trigger,
                    outcome: PersistenceOutcome::Failed,
                });
            }
            let _ = event_tx.send(AppEvent::CommandFailed {
                request_id,
                scope,
                error,
            });
        }
    }
}

fn handle_load_bootstrap(
    context: &EffectContext,
    request_id: u64,
) -> Result<Vec<AppEvent>, BridgeError> {
    let config = context
        .config
        .lock()
        .map_err(|_| bridge_error("lock_poisoned", "Config lock poisoned"))?
        .clone();
    let bootstrap = BootstrapState {
        app_name: "LanternLeaf".to_string(),
        mode: "egui".to_string(),
        config: bootstrap_config_from_app_config(&config),
    };
    info!(request_id, "Loaded bootstrap config");
    Ok(vec![AppEvent::BootstrapLoaded {
        request_id,
        bootstrap,
    }])
}

fn handle_list_recents(
    context: &EffectContext,
    request_id: u64,
    limit: Option<usize>,
) -> Result<Vec<AppEvent>, BridgeError> {
    let limit = normalize_recent_limit(limit);
    let recents: Vec<RecentBook> = context
        .cache_service
        .list_recent_books(limit)
        .into_iter()
        .map(map_recent_book)
        .collect();
    info!(request_id, count = recents.len(), "Loaded recents");
    Ok(vec![AppEvent::RecentsLoaded {
        request_id,
        recents,
    }])
}

fn handle_delete_recent(
    context: &EffectContext,
    request_id: u64,
    source_path: &str,
) -> Result<Vec<AppEvent>, BridgeError> {
    let path = normalize_source_path(source_path)?;
    context
        .cache_service
        .delete_recent_source_and_cache(&path)
        .map_err(|err| bridge_error("io_error", err))?;
    let recents: Vec<RecentBook> = context
        .cache_service
        .list_recent_books(normalize_recent_limit(None))
        .into_iter()
        .map(map_recent_book)
        .collect();
    info!(
        request_id,
        source_path = %path.display(),
        count = recents.len(),
        "Deleted recent source"
    );
    Ok(vec![AppEvent::RecentsLoaded {
        request_id,
        recents,
    }])
}

fn handle_close_recent_browser_tab(
    context: &EffectContext,
    request_id: u64,
    source_path: &str,
) -> Result<Vec<AppEvent>, BridgeError> {
    let path = normalize_source_path(source_path)?;
    let manifest = context
        .cache_service
        .load_browser_tab_manifest(&path)
        .map_err(|err| {
            bridge_error(
                "invalid_input",
                format!(
                    "Source is not a browser-tab manifest: {} ({})",
                    path.display(),
                    err
                ),
            )
        })?;
    let cfg = load_config(context)?;
    ensure_browser_tabs_enabled(&cfg)?;
    let client = browsr_blocking_client_from_config(&cfg)?;
    client
        .close_tab(manifest.tab_id)
        .map_err(|err| bridge_error("browsr_close_failed", err.to_string()))?;
    info!(
        request_id,
        source_path = %path.display(),
        tab_id = manifest.tab_id,
        "Closed browser tab for recent"
    );
    Ok(Vec::new())
}

fn handle_open_source(
    context: &EffectContext,
    request_id: u64,
    path: &str,
) -> Result<Vec<AppEvent>, BridgeError> {
    let source_path = normalize_source_path(path)?;
    open_source_from_path(context, request_id, source_path)
}

fn handle_open_clipboard_text(
    context: &EffectContext,
    request_id: u64,
    text: &str,
) -> Result<Vec<AppEvent>, BridgeError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(bridge_error("invalid_input", "clipboard text is empty"));
    }
    let path = context
        .cache_service
        .persist_clipboard_text_source(trimmed)
        .map_err(|err| bridge_error("clipboard_error", err))?;
    open_source_from_path(context, request_id, path)
}

fn handle_browser_tabs_health(
    context: &EffectContext,
    request_id: u64,
) -> Result<Vec<AppEvent>, BridgeError> {
    let cfg = load_config(context)?;
    ensure_browser_tabs_enabled(&cfg)?;
    let client = browsr_blocking_client_from_config(&cfg)?;
    let health = client
        .health()
        .map_err(|err| bridge_error("browsr_unavailable", err.to_string()))?;
    info!(request_id, "Browsr health loaded");
    Ok(vec![AppEvent::BrowserTabsHealthLoaded {
        request_id,
        health,
    }])
}

fn handle_browser_tabs_windows(
    context: &EffectContext,
    request_id: u64,
) -> Result<Vec<AppEvent>, BridgeError> {
    let cfg = load_config(context)?;
    ensure_browser_tabs_enabled(&cfg)?;
    let client = browsr_blocking_client_from_config(&cfg)?;
    let windows = client
        .list_windows()
        .map_err(|err| bridge_error("browsr_request_failed", err.to_string()))?;
    info!(request_id, count = windows.len(), "Browsr windows loaded");
    Ok(vec![AppEvent::BrowserTabWindowsLoaded {
        request_id,
        windows,
    }])
}

fn handle_browser_tabs_list(
    context: &EffectContext,
    request_id: u64,
    window_id: Option<u64>,
    query: Option<String>,
    refresh: bool,
) -> Result<Vec<AppEvent>, BridgeError> {
    let cfg = load_config(context)?;
    ensure_browser_tabs_enabled(&cfg)?;
    let client = browsr_blocking_client_from_config(&cfg)?;
    let tabs = client
        .list_tabs(window_id, query.as_deref(), refresh)
        .map_err(|err| bridge_error("browsr_request_failed", err.to_string()))?;
    info!(request_id, count = tabs.len(), "Browsr tabs loaded");
    Ok(vec![AppEvent::BrowserTabsLoaded { request_id, tabs }])
}

fn handle_open_browser_tab(
    context: &EffectContext,
    request_id: u64,
    tab_id: u64,
    window_id: Option<u64>,
    bundle: bool,
) -> Result<Vec<AppEvent>, BridgeError> {
    let cfg = load_config(context)?;
    ensure_browser_tabs_enabled(&cfg)?;
    let client = browsr_blocking_client_from_config(&cfg)?;
    let tab_meta = lookup_browser_tab_metadata(&client, tab_id, window_id, false);
    let source_path = if bundle {
        let waited = client
            .start_import_bundle_and_wait(tab_id)
            .map_err(|err| bridge_error("browsr_import_bundle_failed", err.to_string()))?;
        let completed_job = waited.result.job;
        let manifest = match completed_job.status.as_str() {
            "completed" => waited.result.manifest.ok_or_else(|| {
                bridge_error(
                    "browsr_import_bundle_failed",
                    format!(
                        "Import bundle {} completed without an attached manifest",
                        completed_job.job_id
                    ),
                )
            })?,
            "failed" | "cancelled" => {
                let message = completed_job
                    .error
                    .as_ref()
                    .and_then(|value| value.message.clone())
                    .unwrap_or_else(|| {
                        format!(
                            "bundle import {} for tab {}",
                            completed_job.status, completed_job.tab_id
                        )
                    });
                return Err(bridge_error("browsr_import_bundle_failed", message));
            }
            other => {
                return Err(bridge_error(
                    "browsr_import_bundle_failed",
                    format!("Unexpected terminal import bundle status: {other}"),
                ));
            }
        };
        let document = manifest.bundle.document.as_ref().ok_or_else(|| {
            bridge_error(
                "browsr_import_bundle_failed",
                format!(
                    "Import bundle {} did not include a document payload",
                    completed_job.job_id
                ),
            )
        })?;
        let mut assets = Vec::new();
        for asset_ref in manifest
            .bundle
            .assets
            .iter()
            .filter(|asset| asset.body_available && !asset.url.trim().is_empty())
        {
            match client.get_import_bundle_asset(&completed_job.job_id, &asset_ref.asset_id) {
                Ok(asset) => assets.push(asset),
                Err(err) => warn!(
                    job_id = %completed_job.job_id,
                    tab_id,
                    asset_id = %asset_ref.asset_id,
                    url = %asset_ref.url,
                    "Skipping bundle asset fetch failure: {err}"
                ),
            }
        }
        let bundle_capture = browser_tabs::BrowserTabBundleCapture {
            tab_id,
            title: manifest
                .bundle
                .tab
                .title
                .clone()
                .or_else(|| tab_meta.as_ref().map(|value| value.title.clone()))
                .unwrap_or_else(|| format!("Browser tab {tab_id}")),
            url: manifest
                .bundle
                .tab
                .url
                .clone()
                .or_else(|| tab_meta.as_ref().map(|value| value.url.clone()))
                .unwrap_or_default(),
            captured_at: completed_job.updated_at.clone(),
            html: document.html.clone().unwrap_or_default(),
            text: document.text.clone(),
            selection: document.selection.clone(),
            assets,
        };
        context
            .cache_service
            .persist_browser_tab_bundle_source(&bundle_capture, tab_meta.as_ref())
            .map_err(|err| bridge_error("browser_tab_cache_error", err))?
    } else {
        let snapshot = client
            .snapshot_tab(tab_id)
            .map_err(|err| bridge_error("browsr_snapshot_failed", err.to_string()))?;
        context
            .cache_service
            .persist_browser_tab_source(&snapshot, tab_meta.as_ref())
            .map_err(|err| bridge_error("browser_tab_cache_error", err))?
    };
    info!(
        request_id,
        tab_id,
        window_id,
        source_path = %source_path.display(),
        "Persisted browser-tab source"
    );
    open_source_from_path(context, request_id, source_path)
}

fn handle_refresh_browser_tab(
    context: &EffectContext,
    request_id: u64,
    tab_id: u64,
    window_id: Option<u64>,
) -> Result<Vec<AppEvent>, BridgeError> {
    handle_open_browser_tab(context, request_id, tab_id, window_id, false)
}

fn handle_calibre_cached_books(
    context: &EffectContext,
    request_id: u64,
) -> Result<Vec<AppEvent>, BridgeError> {
    let books = calibre::load_cached_books(&context.calibre_config)
        .map_err(|err| bridge_error("calibre_cache_load_failed", err.to_string()))?;
    let mapped = books.into_iter().map(map_calibre_book).collect();
    info!(request_id, "Loaded cached Calibre books");
    Ok(vec![AppEvent::CalibreBooksLoaded {
        request_id,
        books: mapped,
        from_cache: true,
    }])
}

fn handle_calibre_books(
    context: &EffectContext,
    request_id: u64,
    force_refresh: bool,
) -> Result<Vec<AppEvent>, BridgeError> {
    let mut events = vec![AppEvent::CalibreLoadProgress(CalibreLoadEvent {
        request_id,
        phase: "started".to_string(),
        count: None,
        message: None,
    })];
    let books = calibre::load_books_with_cancel(&context.calibre_config, force_refresh, None)
        .map_err(|err| bridge_error("calibre_load_failed", err.to_string()))?;
    let mapped: Vec<CalibreBookDto> = books.into_iter().map(map_calibre_book).collect();
    let count = mapped.len();
    events.push(AppEvent::CalibreBooksLoaded {
        request_id,
        books: mapped,
        from_cache: false,
    });
    events.push(AppEvent::CalibreLoadProgress(CalibreLoadEvent {
        request_id,
        phase: "finished".to_string(),
        count: Some(count),
        message: None,
    }));
    Ok(events)
}

fn handle_calibre_open_book(
    context: &EffectContext,
    request_id: u64,
    book_id: u64,
) -> Result<Vec<AppEvent>, BridgeError> {
    let mut book = calibre::load_cached_books(&context.calibre_config)
        .map_err(|err| bridge_error("calibre_cache_load_failed", err.to_string()))?
        .into_iter()
        .find(|book| book.id == book_id)
        .ok_or_else(|| bridge_error("calibre_not_found", "Book not found"))?;
    let path = calibre::materialize_book_path(&context.calibre_config, &book)
        .map_err(|err| bridge_error("calibre_open_failed", err.to_string()))?;
    if book.cover_thumbnail.is_none() {
        let _ = calibre::ensure_thumbnail_for_book(&context.calibre_config, &mut book, true);
    }
    open_source_from_path(context, request_id, path)
}

fn handle_calibre_thumbnail(
    context: &EffectContext,
    request_id: u64,
    book_id: u64,
) -> Result<Vec<AppEvent>, BridgeError> {
    let mut book = calibre::load_cached_books(&context.calibre_config)
        .map_err(|err| bridge_error("calibre_cache_load_failed", err.to_string()))?
        .into_iter()
        .find(|book| book.id == book_id)
        .ok_or_else(|| bridge_error("calibre_not_found", "Book not found"))?;
    let _ = calibre::ensure_thumbnail_for_book(&context.calibre_config, &mut book, true);
    handle_calibre_cached_books(context, request_id)
}

fn handle_reader_command(
    context: &EffectContext,
    request_id: u64,
    command: session::SessionCommand,
) -> Result<Vec<AppEvent>, BridgeError> {
    let mut guard = context
        .session
        .lock()
        .map_err(|_| bridge_error("lock_poisoned", "Reader session lock poisoned"))?;
    let reader = guard
        .as_mut()
        .ok_or_else(|| bridge_error("no_session", "No reader session available"))?;
    let panels = context
        .panels
        .lock()
        .map_err(|_| bridge_error("lock_poisoned", "Panel lock poisoned"))?
        .clone();
    let event = reader.apply_command(command, panels, &context.normalizer);
    Ok(vec![AppEvent::ReaderUpdated(ReaderStateEvent {
        request_id,
        action: event.action.to_string(),
        reader: event.snapshot,
    })])
}

fn handle_toggle_panel(
    context: &EffectContext,
    request_id: u64,
    panel: PanelToggle,
) -> Result<Vec<AppEvent>, BridgeError> {
    let mut panels = context
        .panels
        .lock()
        .map_err(|_| bridge_error("lock_poisoned", "Panel lock poisoned"))?;
    match panel {
        PanelToggle::Settings => {
            panels.show_settings = !panels.show_settings;
            if panels.show_settings {
                panels.show_stats = false;
            }
        }
        PanelToggle::Stats => {
            panels.show_stats = !panels.show_stats;
            if panels.show_stats {
                panels.show_settings = false;
            }
        }
        PanelToggle::Tts => {
            panels.show_tts = !panels.show_tts;
        }
    }
    let session = SessionState {
        mode: UiMode::Reader,
        active_source_path: None,
        open_in_flight: false,
        panels: *panels,
    };
    Ok(vec![AppEvent::SessionUpdated(SessionStateEvent {
        request_id,
        action: "panel_toggle".to_string(),
        session,
    })])
}

fn handle_set_log_level(
    context: &EffectContext,
    request_id: u64,
    level: &str,
) -> Result<Vec<AppEvent>, BridgeError> {
    let mut cfg = context
        .config
        .lock()
        .map_err(|_| bridge_error("lock_poisoned", "Config lock poisoned"))?;
    cfg.log_level = parse_log_level(level)?;
    let updated_config = cfg.clone();
    drop(cfg);
    let mut events = vec![AppEvent::LogLevelUpdated(LogLevelEvent {
        request_id,
        level: level.to_string(),
    })];
    if let Err(err) = context
        .config_service
        .save_base_config(&context.config_path, &updated_config)
    {
        warn!(
            request_id,
            error = %err,
            "Failed to persist base config after log level update"
        );
        events.push(AppEvent::CommandFailed {
            request_id,
            scope: Some(OperationScope::RuntimeConfig),
            error: BridgeError {
                code: "config_persist_failed".to_string(),
                message: err,
            },
        });
    }
    Ok(events)
}

fn handle_toggle_theme(
    context: &EffectContext,
    request_id: u64,
) -> Result<Vec<AppEvent>, BridgeError> {
    let mut cfg = context
        .config
        .lock()
        .map_err(|_| bridge_error("lock_poisoned", "Config lock poisoned"))?;
    cfg.theme = match cfg.theme {
        config::ThemeMode::Day => config::ThemeMode::Night,
        config::ThemeMode::Night => config::ThemeMode::Day,
    };
    let updated_config = cfg.clone();
    drop(cfg);
    if let Err(err) = context
        .config_service
        .save_base_config(&context.config_path, &updated_config)
    {
        warn!(
            request_id,
            error = %err,
            "Failed to persist base config after theme toggle"
        );
    }
    let bootstrap = BootstrapState {
        app_name: "LanternLeaf".to_string(),
        mode: "egui".to_string(),
        config: bootstrap_config_from_app_config(&updated_config),
    };
    Ok(vec![AppEvent::BootstrapLoaded {
        request_id,
        bootstrap,
    }])
}

fn handle_persistence_flush(
    context: &EffectContext,
    request_id: u64,
    trigger: PersistenceTrigger,
) -> Result<Vec<AppEvent>, BridgeError> {
    if trigger == PersistenceTrigger::RuntimeConfigChange {
        let cfg = context
            .config
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Config lock poisoned"))?
            .clone();
        context
            .config_service
            .save_base_config(&context.config_path, &cfg)
            .map_err(|err| bridge_error("config_persist_failed", err))?;
    }
    let panels = {
        let guard = context
            .panels
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Panel state lock poisoned"))?;
        *guard
    };
    let (snapshot, config) = {
        let mut guard = context
            .session
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Reader session lock poisoned"))?;
        let Some(session) = guard.as_mut() else {
            if trigger == PersistenceTrigger::RuntimeConfigChange {
                debug!(
                    request_id,
                    trigger = ?trigger,
                    "Persisted runtime config without active reader session"
                );
                return Ok(vec![AppEvent::PersistenceFlushed {
                    request_id,
                    trigger,
                    outcome: PersistenceOutcome::Completed,
                }]);
            }
            debug!(
                request_id,
                trigger = ?trigger,
                "Skipping persistence flush (no active reader session)"
            );
            return Ok(vec![AppEvent::PersistenceFlushed {
                request_id,
                trigger,
                outcome: PersistenceOutcome::SkippedNoSession,
            }]);
        };
        let snapshot = session.snapshot(panels, &context.normalizer);
        let config = session.config.clone();
        (snapshot, config)
    };
    context.persistence.flush_trigger(
        Some(lanternleaf_app::persistence::ReaderHousekeeping {
            snapshot: &snapshot,
            config: &config,
        }),
        trigger,
    );
    debug!(
        request_id,
        trigger = ?trigger,
        "Persistence flush completed"
    );
    Ok(vec![AppEvent::PersistenceFlushed {
        request_id,
        trigger,
        outcome: PersistenceOutcome::Completed,
    }])
}

fn handle_return_to_starter(
    context: &EffectContext,
    request_id: u64,
) -> Result<Vec<AppEvent>, BridgeError> {
    let mut guard = context
        .session
        .lock()
        .map_err(|_| bridge_error("lock_poisoned", "Reader session lock poisoned"))?;
    *guard = None;
    Ok(vec![AppEvent::SessionUpdated(SessionStateEvent {
        request_id,
        action: "session_return_to_starter".to_string(),
        session: SessionState {
            mode: UiMode::Starter,
            active_source_path: None,
            open_in_flight: false,
            panels: session::PanelState::default(),
        },
    })])
}

fn handle_close_reader_session(
    context: &EffectContext,
    request_id: u64,
) -> Result<Vec<AppEvent>, BridgeError> {
    handle_return_to_starter(context, request_id)
}

fn handle_safe_quit(request_id: u64) -> Result<Vec<AppEvent>, BridgeError> {
    info!(request_id, "Safe quit requested (egui dispatcher)");
    Ok(Vec::new())
}

fn open_source_from_path(
    context: &EffectContext,
    request_id: u64,
    source_path: PathBuf,
) -> Result<Vec<AppEvent>, BridgeError> {
    let source_is_pdf = source_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false);
    let mut events = Vec::new();
    events.push(AppEvent::SourceOpenProgress(SourceOpenEvent {
        request_id,
        phase: "started".to_string(),
        source_path: Some(source_path.to_string_lossy().to_string()),
        message: None,
    }));
    if source_is_pdf {
        events.push(AppEvent::PdfTranscriptionProgress(
            lanternleaf_app::contracts::PdfTranscriptionEvent {
                request_id,
                phase: "started".to_string(),
                source_path: source_path.to_string_lossy().to_string(),
                message: None,
            },
        ));
    }

    let config = load_config(context)?;
    let reader = session::load_session_for_source_with_cancel(
        source_path.clone(),
        &config,
        &context.normalizer,
        None,
    )
    .map_err(|err| bridge_error("source_open_failed", err))?;
    let panels = panels_from_config(&reader.config);
    if let Ok(mut guard) = context.session.lock() {
        *guard = Some(reader);
    }
    if let Ok(mut panels_guard) = context.panels.lock() {
        *panels_guard = panels;
    }
    let snapshot = {
        let mut guard = context
            .session
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Reader session lock poisoned"))?;
        guard
            .as_mut()
            .expect("session just set")
            .snapshot(panels, &context.normalizer)
    };
    let session_state = SessionState {
        mode: UiMode::Reader,
        active_source_path: Some(snapshot.source_path.clone()),
        open_in_flight: false,
        panels,
    };
    events.push(AppEvent::SourceOpened {
        request_id,
        result: OpenSourceResult {
            session: session_state,
            reader: snapshot.clone(),
        },
    });
    events.push(AppEvent::SourceOpenProgress(SourceOpenEvent {
        request_id,
        phase: "finished".to_string(),
        source_path: Some(source_path.to_string_lossy().to_string()),
        message: None,
    }));
    if source_is_pdf {
        events.push(AppEvent::PdfTranscriptionProgress(
            lanternleaf_app::contracts::PdfTranscriptionEvent {
                request_id,
                phase: "finished".to_string(),
                source_path: source_path.to_string_lossy().to_string(),
                message: None,
            },
        ));
    }
    info!(
        request_id,
        path = %source_path.display(),
        "Completed source open in egui dispatcher"
    );
    Ok(events)
}

fn lookup_browser_tab_metadata(
    client: &browser_tabs::BrowsrBlockingClient,
    tab_id: u64,
    window_id: Option<u64>,
    refresh: bool,
) -> Option<browser_tabs::BrowserTab> {
    client
        .list_tabs(window_id, None, refresh)
        .ok()
        .and_then(|tabs| tabs.into_iter().find(|tab| tab.id == tab_id))
}

fn browsr_blocking_client_from_config(
    cfg: &config::AppConfig,
) -> Result<browser_tabs::BrowsrBlockingClient, BridgeError> {
    browser_tabs::BrowsrBlockingClient::new(&cfg.browsr_base_url, cfg.browsr_timeout_ms)
        .map_err(|err| bridge_error("browsr_config_error", err.to_string()))
}

fn ensure_browser_tabs_enabled(cfg: &config::AppConfig) -> Result<(), BridgeError> {
    if cfg.browser_tabs_enabled {
        Ok(())
    } else {
        Err(bridge_error(
            "browser_tabs_disabled",
            "Browser tabs import is disabled in config",
        ))
    }
}

fn load_config(context: &EffectContext) -> Result<config::AppConfig, BridgeError> {
    context
        .config
        .lock()
        .map_err(|_| bridge_error("lock_poisoned", "Config lock poisoned"))
        .map(|guard| guard.clone())
}

fn parse_log_level(level: &str) -> Result<config::LogLevel, BridgeError> {
    match level.trim().to_ascii_lowercase().as_str() {
        "trace" => Ok(config::LogLevel::Trace),
        "debug" => Ok(config::LogLevel::Debug),
        "info" => Ok(config::LogLevel::Info),
        "warn" | "warning" => Ok(config::LogLevel::Warn),
        "error" => Ok(config::LogLevel::Error),
        _ => Err(bridge_error(
            "invalid_log_level",
            format!("Unsupported log level '{level}'"),
        )),
    }
}

fn normalize_recent_limit(limit: Option<usize>) -> usize {
    limit.unwrap_or(30).clamp(1, 200)
}

fn normalize_source_path(path: &str) -> Result<PathBuf, BridgeError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(bridge_error("invalid_input", "Path cannot be empty"));
    }
    let candidate = PathBuf::from(trimmed);
    let canonical = candidate.canonicalize().unwrap_or(candidate);
    Ok(canonical)
}

fn panels_from_config(config: &config::AppConfig) -> session::PanelState {
    session::PanelState {
        show_settings: config.show_settings,
        show_stats: config.show_stats,
        show_tts: config.show_tts,
    }
}

fn map_recent_book(recent: cache::RecentBook) -> RecentBook {
    RecentBook {
        source_path: recent.source_path.to_string_lossy().to_string(),
        display_title: recent.display_title,
        snippet: recent.snippet,
        thumbnail_path: recent
            .thumbnail_path
            .map(|path| path.to_string_lossy().to_string()),
        last_opened_unix_secs: recent.last_opened_unix_secs,
        browser_tab_id: recent.browser_tab_id,
        browser_window_id: recent.browser_window_id,
    }
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
            .map(|path| path.to_string_lossy().to_string()),
    }
}

fn bridge_error(code: impl Into<String>, message: impl Into<String>) -> BridgeError {
    BridgeError {
        code: code.into(),
        message: message.into(),
    }
}

fn effect_scope(effect: &RuntimeEffect) -> Option<OperationScope> {
    match effect.owner() {
        EffectOwner::SourceOpen => Some(OperationScope::SourceOpen),
        EffectOwner::BrowserTabs => Some(OperationScope::BrowserTabRefresh),
        EffectOwner::Calibre => Some(OperationScope::CalibreLoad),
        EffectOwner::Logging | EffectOwner::Persistence => Some(OperationScope::RuntimeConfig),
        EffectOwner::ReaderSession => Some(OperationScope::ReaderCommand),
        EffectOwner::RecentBooks => Some(OperationScope::StarterCommand),
        EffectOwner::AppShell => None,
        EffectOwner::PdfArtifacts => Some(OperationScope::ReaderCommand),
    }
}

fn emit_failure_progress(effect: &RuntimeEffect, request_id: u64, tx: &mpsc::Sender<AppEvent>) {
    match effect {
        RuntimeEffect::OpenSourcePath { .. }
        | RuntimeEffect::OpenClipboardText { .. }
        | RuntimeEffect::OpenClipboard
        | RuntimeEffect::OpenBrowserTab { .. }
        | RuntimeEffect::OpenBrowserTabBundle { .. }
        | RuntimeEffect::RefreshBrowserTab { .. }
        | RuntimeEffect::OpenCalibreBook { .. } => {
            let path = match effect {
                RuntimeEffect::OpenSourcePath { path } => Some(path.clone()),
                RuntimeEffect::OpenClipboardText { .. } | RuntimeEffect::OpenClipboard => None,
                RuntimeEffect::OpenBrowserTab { .. }
                | RuntimeEffect::OpenBrowserTabBundle { .. }
                | RuntimeEffect::RefreshBrowserTab { .. }
                | RuntimeEffect::OpenCalibreBook { .. } => None,
                _ => None,
            };
            let _ = tx.send(AppEvent::SourceOpenProgress(SourceOpenEvent {
                request_id,
                phase: "failed".to_string(),
                source_path: path,
                message: Some("Source open failed".to_string()),
            }));
        }
        RuntimeEffect::LoadCalibreBooks { .. } => {
            let _ = tx.send(AppEvent::CalibreLoadProgress(CalibreLoadEvent {
                request_id,
                phase: "failed".to_string(),
                count: None,
                message: Some("Calibre load failed".to_string()),
            }));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn dispatcher_runs_effects_off_thread() {
        let persistence = Arc::new(lanternleaf_app::persistence::PersistenceLifecycle::new(
            lanternleaf_app::persistence::FilesystemPersistenceService::default(),
        ));
        let config_path = std::env::temp_dir().join("lanternleaf-egui-config.toml");
        let context = EffectContext::new(
            config::AppConfig::default(),
            normalizer::TextNormalizer::default(),
            persistence,
            config_path,
        );
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let release_rx = Mutex::new(release_rx);
        let handler: EffectHandler = Arc::new(move |_ctx, _planned, _event_tx| {
            let _ = started_tx.send(thread::current().id());
            let _ = release_rx
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_millis(200));
        });
        let dispatcher = EffectDispatcher::with_handler(context, handler);

        dispatcher.dispatch(PlannedEffect {
            request_id: 1,
            effect: RuntimeEffect::LoadBootstrap,
        });

        let worker_id = started_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("worker thread started");
        assert_ne!(worker_id, thread::current().id());
        let _ = release_tx.send(());
    }

    struct TestConfigService {
        called: Arc<AtomicBool>,
    }

    impl config_service::ConfigService for TestConfigService {
        fn save_base_config(
            &self,
            _path: &Path,
            _config: &config::AppConfig,
        ) -> Result<(), String> {
            self.called.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn runtime_config_flush_persists_base_config_without_session() {
        let persistence = Arc::new(lanternleaf_app::persistence::PersistenceLifecycle::new(
            lanternleaf_app::persistence::FilesystemPersistenceService::default(),
        ));
        let cache_service: Arc<dyn cache_service::CacheService> =
            Arc::new(cache_service::FilesystemCacheService);
        let called = Arc::new(AtomicBool::new(false));
        let config_service: Arc<dyn config_service::ConfigService> = Arc::new(TestConfigService {
            called: Arc::clone(&called),
        });
        let config_path = std::env::temp_dir().join("lanternleaf-egui-config-test.toml");
        let context = EffectContext::with_services(
            config::AppConfig::default(),
            normalizer::TextNormalizer::default(),
            persistence,
            cache_service,
            config_path,
            config_service,
        );

        let events = handle_persistence_flush(&context, 1, PersistenceTrigger::RuntimeConfigChange)
            .expect("flush should succeed");

        assert!(called.load(Ordering::SeqCst));
        assert!(events.iter().any(|event| matches!(
            event,
            AppEvent::PersistenceFlushed {
                outcome: PersistenceOutcome::Completed,
                ..
            }
        )));
    }

    struct CountingConfigService {
        calls: Arc<AtomicUsize>,
    }

    impl config_service::ConfigService for CountingConfigService {
        fn save_base_config(
            &self,
            _path: &Path,
            _config: &config::AppConfig,
        ) -> Result<(), String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct FailingConfigService;

    impl config_service::ConfigService for FailingConfigService {
        fn save_base_config(
            &self,
            _path: &Path,
            _config: &config::AppConfig,
        ) -> Result<(), String> {
            Err("persist_failed".to_string())
        }
    }

    #[test]
    fn set_log_level_persists_config() {
        let persistence = Arc::new(lanternleaf_app::persistence::PersistenceLifecycle::new(
            lanternleaf_app::persistence::FilesystemPersistenceService::default(),
        ));
        let cache_service: Arc<dyn cache_service::CacheService> =
            Arc::new(cache_service::FilesystemCacheService);
        let calls = Arc::new(AtomicUsize::new(0));
        let config_service: Arc<dyn config_service::ConfigService> =
            Arc::new(CountingConfigService {
                calls: Arc::clone(&calls),
            });
        let config_path = std::env::temp_dir().join("lanternleaf-egui-config-test.toml");
        let context = EffectContext::with_services(
            config::AppConfig::default(),
            normalizer::TextNormalizer::default(),
            persistence,
            cache_service,
            config_path,
            config_service,
        );

        let events = handle_set_log_level(&context, 17, "info").expect("log level update");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(matches!(events[0], AppEvent::LogLevelUpdated(_)));
        let cfg = context.config.lock().expect("config lock");
        assert!(matches!(cfg.log_level, config::LogLevel::Info));
    }

    #[test]
    fn set_log_level_reports_persist_failure() {
        let persistence = Arc::new(lanternleaf_app::persistence::PersistenceLifecycle::new(
            lanternleaf_app::persistence::FilesystemPersistenceService::default(),
        ));
        let cache_service: Arc<dyn cache_service::CacheService> =
            Arc::new(cache_service::FilesystemCacheService);
        let config_service: Arc<dyn config_service::ConfigService> = Arc::new(FailingConfigService);
        let config_path = std::env::temp_dir().join("lanternleaf-egui-config-test.toml");
        let context = EffectContext::with_services(
            config::AppConfig::default(),
            normalizer::TextNormalizer::default(),
            persistence,
            cache_service,
            config_path,
            config_service,
        );

        let events = handle_set_log_level(&context, 33, "warn").expect("log level update");
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AppEvent::CommandFailed { .. }))
        );
    }

    struct TestCacheService {
        recents: Vec<cache::RecentBook>,
        deleted: Arc<Mutex<Option<PathBuf>>>,
    }

    impl TestCacheService {
        fn new(recents: Vec<cache::RecentBook>) -> (Self, Arc<Mutex<Option<PathBuf>>>) {
            let deleted = Arc::new(Mutex::new(None));
            (
                Self {
                    recents,
                    deleted: Arc::clone(&deleted),
                },
                deleted,
            )
        }
    }

    impl cache_service::CacheService for TestCacheService {
        fn save_bookmark(&self, _source_path: &Path, _bookmark: &cache::Bookmark) {}

        fn save_epub_config(&self, _source_path: &Path, _config: &config::AppConfig) {}

        fn delete_recent_source_and_cache(&self, source_path: &Path) -> Result<(), String> {
            *self.deleted.lock().expect("deleted lock") = Some(source_path.to_path_buf());
            Ok(())
        }

        fn remember_source_path(&self, _source_path: &Path) {}

        fn persist_clipboard_text_source(&self, _text: &str) -> Result<PathBuf, String> {
            Err("not_used".to_string())
        }

        fn persist_browser_tab_source(
            &self,
            _snapshot: &browser_tabs::BrowserTabSnapshot,
            _tab_meta: Option<&browser_tabs::BrowserTab>,
        ) -> Result<PathBuf, String> {
            Err("not_used".to_string())
        }

        fn persist_browser_tab_bundle_source(
            &self,
            _capture: &browser_tabs::BrowserTabBundleCapture,
            _tab_meta: Option<&browser_tabs::BrowserTab>,
        ) -> Result<PathBuf, String> {
            Err("not_used".to_string())
        }

        fn list_recent_books(&self, _limit: usize) -> Vec<cache::RecentBook> {
            self.recents.clone()
        }

        fn load_browser_tab_manifest(
            &self,
            _source_path: &Path,
        ) -> Result<cache::BrowserTabSourceManifest, String> {
            Err("not_used".to_string())
        }

        fn load_pdf_ocr_alignment_artifact(
            &self,
            _source_path: &Path,
        ) -> Option<cache::PdfOcrAlignmentArtifact> {
            None
        }

        fn load_pdf_sentence_map(
            &self,
            _source_path: &Path,
        ) -> Option<Vec<cache::PdfSentenceLocation>> {
            None
        }

        fn load_pdf_render_precomputed_state(
            &self,
            _source_path: &Path,
        ) -> Option<cache::PdfRenderPrecomputedState> {
            None
        }

        fn persist_pdf_sentence_map(
            &self,
            _source_path: &Path,
            _locations: &[cache::PdfSentenceLocation],
        ) {
        }

        fn persist_pdf_render_precomputed_state(
            &self,
            _source_path: &Path,
            _artifact: &cache::PdfRenderPrecomputedState,
        ) {
        }
    }

    #[test]
    fn list_recents_uses_cache_service() {
        let persistence = Arc::new(lanternleaf_app::persistence::PersistenceLifecycle::new(
            lanternleaf_app::persistence::FilesystemPersistenceService::default(),
        ));
        let recent = cache::RecentBook {
            source_path: PathBuf::from("/tmp/recents.epub"),
            display_title: "Recents".to_string(),
            snippet: "snippet".to_string(),
            thumbnail_path: None,
            last_opened_unix_secs: 12,
            browser_tab_id: None,
            browser_window_id: None,
        };
        let (cache_service, _deleted) = TestCacheService::new(vec![recent.clone()]);
        let cache_service: Arc<dyn cache_service::CacheService> = Arc::new(cache_service);
        let config_service: Arc<dyn config_service::ConfigService> =
            Arc::new(config_service::FilesystemConfigService);
        let config_path = std::env::temp_dir().join("lanternleaf-egui-config-test.toml");
        let context = EffectContext::with_services(
            config::AppConfig::default(),
            normalizer::TextNormalizer::default(),
            persistence,
            cache_service,
            config_path,
            config_service,
        );

        let events = handle_list_recents(&context, 9, Some(5)).expect("recents");
        let AppEvent::RecentsLoaded { recents, .. } = &events[0] else {
            panic!("expected recents event");
        };
        assert_eq!(recents.len(), 1);
        assert_eq!(recents[0].display_title, recent.display_title);
    }

    #[test]
    fn delete_recent_records_path_and_refreshes() {
        let persistence = Arc::new(lanternleaf_app::persistence::PersistenceLifecycle::new(
            lanternleaf_app::persistence::FilesystemPersistenceService::default(),
        ));
        let recent = cache::RecentBook {
            source_path: PathBuf::from("/tmp/recents.epub"),
            display_title: "Recents".to_string(),
            snippet: "snippet".to_string(),
            thumbnail_path: None,
            last_opened_unix_secs: 12,
            browser_tab_id: None,
            browser_window_id: None,
        };
        let (cache_service, deleted) = TestCacheService::new(vec![recent]);
        let cache_service: Arc<dyn cache_service::CacheService> = Arc::new(cache_service);
        let config_service: Arc<dyn config_service::ConfigService> =
            Arc::new(config_service::FilesystemConfigService);
        let config_path = std::env::temp_dir().join("lanternleaf-egui-config-test.toml");
        let context = EffectContext::with_services(
            config::AppConfig::default(),
            normalizer::TextNormalizer::default(),
            persistence,
            cache_service,
            config_path,
            config_service,
        );

        let events =
            handle_delete_recent(&context, 10, "/tmp/recents.epub").expect("delete recent");
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AppEvent::RecentsLoaded { .. }))
        );
        let deleted_path = deleted.lock().expect("deleted lock").clone();
        assert_eq!(deleted_path, Some(PathBuf::from("/tmp/recents.epub")));
    }
}
