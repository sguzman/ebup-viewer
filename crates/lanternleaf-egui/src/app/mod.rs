#![allow(dead_code)]

mod commands;
mod theme;
mod tts_sync;
mod ui;

use std::{
    cmp::Reverse,
    collections::{HashMap, HashSet},
    fs,
    fs::OpenOptions,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::helpers::{
    app_config_path, bootstrap_config_from_app_config, format_combo, workspace_root_from_cwd,
};
use eframe::{
    NativeOptions,
    egui::{
        self, Button, CollapsingHeader, Color32, ColorImage, Context, FontData, FontDefinitions,
        FontFamily, RichText, Slider, TextStyle, TextureHandle, Ui, Vec2, Visuals,
    },
};

use crate::constants::*;
use crate::effects::{EffectContext, EffectDispatcher};
use crate::pdf::{
    PdfPageRegistryEntry, PdfViewportBudgetDecision, PdfViewportBudgetInput, PdfViewportPlanInput,
    PdfViewportRenderPlan, build_pdf_viewport_render_plan, choose_pdf_viewport_evictions,
};
use crate::pdf_renderer::{
    NativePdfRenderer, NativeRenderEviction, NativeRenderSpan, RenderTarget,
};
use crate::pdf_subsystem::{
    PdfScrollPolicy, PdfViewportRange, PdfViewportUpdateTrigger, PdfZoomDirection, PdfZoomPolicy,
};
use crate::pretty::{PrettyBlock, PrettyPageCacheKey};
use crate::shell::{FocusOwner, LayoutPolicy, ShellState};
use lanternleaf_app::{
    AppRuntime,
    contracts::{
        BootstrapState, BrowserTabsHealth, BrowserTabsTab, BrowserTabsWindow, CalibreBookDto,
        CalibreLoadEvent, PrettyKind, ReaderSnapshot, RecentBook, SourceOpenEvent, UiMode,
    },
    persistence::{
        FilesystemPersistenceService, PersistenceLifecycle, PersistenceService,
        RemotePersistenceService,
    },
    pipeline::{AppCommand, DispatchPlan, PersistenceTrigger, ReaderCommand},
    shortcuts::{ShortcutAction, ShortcutScope, UiShortcutAction},
    state::{AppState, OperationState},
    tracing::init_tracing,
    tts_runtime::{TtsRuntime, TtsRuntimeEvent},
};
use lanternleaf_core::{
    cache, cache_service, config, config_service,
    epub_loader::{PdfGeometryMode, PdfOcrGeometryQualityClass, PdfSyncStrategy},
    normalizer, session,
    session::ReaderSettingsPatch,
};
use serde::{Deserialize, Serialize};
use tracing::{Level, info, trace, warn};

pub(crate) fn run() {
    let config_path = app_config_path();
    let app_config = config::load_config(&config_path);
    let bootstrap_config = bootstrap_config_from_app_config(&app_config);
    let tracing_guard = init_tracing(&bootstrap_config.log_level);
    let _instance_lock = match acquire_single_instance_lock() {
        Some(lock) => lock,
        None => {
            warn!("Another LanternLeaf egui instance is already running");
            return;
        }
    };
    let normalizer = normalizer::TextNormalizer::load_default();

    let runtime = AppRuntime::with_bootstrap_config(&bootstrap_config);
    let mut options = NativeOptions::default();
    options.viewport.inner_size = Some(egui::vec2(
        app_config.window_width as f32,
        app_config.window_height as f32,
    ));
    match eframe::icon_data::from_png_bytes(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../branding/icon.png"
    ))) {
        Ok(icon) => options.viewport = options.viewport.with_icon(Arc::new(icon)),
        Err(err) => warn!(error = ?err, "Failed to load app icon"),
    }

    info!("Starting LanternLeaf egui shell");

    let _ = eframe::run_native(
        "LanternLeaf",
        options,
        Box::new(move |cc| {
            Box::new(LanternLeafApp::new(
                cc,
                runtime.clone(),
                tracing_guard,
                app_config.clone(),
                normalizer.clone(),
            )) as Box<dyn eframe::App>
        }),
    );
}

#[cfg(target_arch = "wasm32")]
pub fn run_wasm(canvas_id: &str) -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::JsCast;

    // Redirect tracing to the browser console
    tracing_wasm::set_as_global_default();

    let canvas_id = canvas_id.to_owned();
    wasm_bindgen_futures::spawn_local(async move {
        let app_config = config::AppConfig::default(); // In WASM we might need to fetch this or use defaults
        let bootstrap_config = bootstrap_config_from_app_config(&app_config);
        let normalizer = normalizer::TextNormalizer::load_default();
        let runtime = AppRuntime::with_bootstrap_config(&bootstrap_config);

        let runner = eframe::WebRunner::new();
        runner
            .start(
                &canvas_id,
                eframe::WebOptions::default(),
                Box::new(move |cc| {
                    // Tracing guard is not used in WASM the same way
                    // We'll pass a dummy or handle it differently if needed
                    // For now, let's just use a simple approach
                    
                    // We need a dummy tracing guard or change the struct
                    // Actually, let's just make it work for now.
                    
                    // We'll need to modify LanternLeafApp::new to handle WASM tracing guard
                    Box::new(LanternLeafApp::new_wasm(
                        cc,
                        runtime,
                        app_config,
                        normalizer,
                    )) as Box<dyn eframe::App>
                }),
            )
            .await
            .expect("failed to start eframe");
    });

    Ok(())
}

struct SingleInstanceLock {
    path: PathBuf,
    _file: fs::File,
}

impl Drop for SingleInstanceLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn acquire_single_instance_lock() -> Option<SingleInstanceLock> {
    let root = workspace_root_from_cwd().unwrap_or_else(|| PathBuf::from("."));
    let logs_dir = root.join("logs");
    if let Err(err) = fs::create_dir_all(&logs_dir) {
        warn!(
            error = %err,
            path = %logs_dir.display(),
            "Failed to prepare logs directory for single-instance lock"
        );
        return None;
    }

    let path: PathBuf = logs_dir.join("lanternleaf-egui.lock");
    match try_create_lock(&path) {
        Ok(file) => {
            trace!(
                path = %path.display(),
                pid = %std::process::id(),
                "Acquired LanternLeaf egui single-instance lock"
            );
            Some(SingleInstanceLock { path, _file: file })
        }
        Err(err) if err.kind() == ErrorKind::AlreadyExists => {
            trace!(path = %path.display(), "Found existing LanternLeaf egui lock");
            if let Some(pid) = read_lock_pid(&path) {
                if is_pid_running(pid) {
                    warn!(
                        pid,
                        path = %path.display(),
                        "Another LanternLeaf egui instance is already running"
                    );
                    return None;
                }
                trace!(pid, path = %path.display(), "Existing lock PID is not running");
            } else {
                trace!(
                    path = %path.display(),
                    "Existing LanternLeaf lock has no PID metadata, treating it as stale"
                );
            }

            if let Err(remove_err) = fs::remove_file(&path) {
                warn!(
                    error = %remove_err,
                    path = %path.display(),
                    "Failed to remove stale single-instance lock"
                );
                return None;
            }
            match try_create_lock(&path) {
                Ok(file) => {
                    trace!(
                        path = %path.display(),
                        pid = %std::process::id(),
                        "Reacquired LanternLeaf egui lock after clearing stale file"
                    );
                    let owned_path: PathBuf = path.clone();
                    Some(SingleInstanceLock {
                        path: owned_path,
                        _file: file,
                    })
                }
                Err(final_err) => {
                    warn!(
                        error = %final_err,
                        path = %path.display(),
                        "Failed to recreate LanternLeaf egui lock"
                    );
                    None
                }
            }
        }
        Err(err) => {
            warn!(
                error = %err,
                path = %path.display(),
                "Failed to create LanternLeaf egui lock"
            );
            None
        }
    }
}

fn try_create_lock(path: &Path) -> std::io::Result<fs::File> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    writeln!(file, "{}", std::process::id())?;
    file.sync_all()?;
    Ok(file)
}

fn read_lock_pid(path: &Path) -> Option<u32> {
    let contents = fs::read_to_string(path).ok()?;
    let trimmed = contents.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse().ok()
}

fn is_pid_running(pid: u32) -> bool {
    crate::os::is_pid_running(pid)
}

struct LanternLeafApp {
    runtime: AppRuntime,
    #[cfg(not(target_arch = "wasm32"))]
    _tracing_guard: tracing_appender::non_blocking::WorkerGuard,
    fonts_configured: bool,
    status_log: Vec<StatusLogEntry>,
    show_safe_quit_modal: bool,
    show_reader_confirm_modal: bool,
    pending_search_focus: bool,
    last_plan: Option<DispatchPlan>,
    auto_scroll_state: AutoScrollState,
    text_only_override: Option<bool>,
    text_only_toggle_pending: bool,
    anchor_diagnostics: AnchorDiagnostics,
    overlay_diagnostics: OverlayDiagnostics,
    audio_diagnostics: AudioDiagnostics,
    tts_runtime: TtsRuntime,
    last_tts_runtime_event: Option<TtsRuntimeEvent>,
    theme_override: Option<config::ThemeMode>,
    pending_theme_mode: Option<config::ThemeMode>,
    theme_patch_pending: bool,
    persistence: Arc<PersistenceLifecycle>,
    cache_service: Arc<dyn cache_service::CacheService>,
    effect_session: Arc<Mutex<Option<session::ReaderSession>>>,
    tts_session_source: Option<PathBuf>,
    persistence_logged: bool,
    last_reader_source: Option<String>,
    last_reader_snapshot: Option<ReaderSnapshot>,
    effect_dispatcher: EffectDispatcher,
    shell_state: ShellState,
    layout_policy: LayoutPolicy,
    settings_trace_events: Vec<SettingsTraceEvent>,
    settings_trace_next_id: usize,
    persistence_trace_events: Vec<PersistenceTraceEvent>,
    persistence_trace_next_id: usize,
    regression_snapshots: Vec<RegressionSnapshot>,
    regression_snapshot_next_id: usize,
    overlay_pressure_focus: bool,
    scheduler_events: Vec<SchedulerEvent>,
    pdf_render_state: PdfRenderState,
    pdf_renderer: Option<NativePdfRenderer>,
    current_pdf_path: Option<PathBuf>,
    pretty_page_cache_key: Option<PrettyPageCacheKey>,
    pretty_page_cache_blocks: Vec<PrettyBlock>,
    thumbnail_cache: ThumbnailCache,
    pretty_image_cache: PrettyImageCache,
    sentence_scroll_offset: Option<Vec2>,
    overlay_eviction_warning_at: Option<Instant>,
    timeline_history: Vec<TimelineHistoryEntry>,
    pinned_timeline_entries: Vec<TimelineHistoryEntry>,
    starter_open_path_input: String,
    starter_clipboard_text_input: String,
    starter_calibre_query: String,
    starter_calibre_force_refresh: bool,
    starter_calibre_sort: CalibreSort,
    starter_calibre_view: Vec<usize>,
    starter_calibre_last_query: String,
    starter_calibre_last_sort: CalibreSort,
    starter_calibre_last_count: usize,
    starter_browser_tab_query: String,
    starter_browser_tabs_force_refresh: bool,
    starter_browser_tab_id_input: String,
    starter_browser_window_id_input: String,
}

struct StarterViewModel<'a> {
    bootstrap: Option<&'a BootstrapState>,
    recents: &'a [RecentBook],
    calibre_books: &'a [CalibreBookDto],
    browser_tabs_health: Option<&'a BrowserTabsHealth>,
    browser_tabs_windows: &'a [BrowserTabsWindow],
    browser_tabs_tabs: &'a [BrowserTabsTab],
    loading_recents: bool,
    loading_calibre: bool,
    loading_browser_tabs: bool,
    source_open_event: Option<&'a SourceOpenEvent>,
    calibre_load_event: Option<&'a CalibreLoadEvent>,
    operations: &'a OperationState,
    last_remote_update_at: u64,
    remote_url: Option<&'a String>,
}

impl<'a> StarterViewModel<'a> {
    fn from_state(state: &'a AppState) -> Self {
        Self {
            bootstrap: state.app_shell.bootstrap.as_ref(),
            recents: &state.starter.recents,
            calibre_books: &state.starter.calibre_books,
            browser_tabs_health: state.starter.browser_tabs_health.as_ref(),
            browser_tabs_windows: &state.starter.browser_tabs_windows,
            browser_tabs_tabs: &state.starter.browser_tabs_tabs,
            loading_recents: state.starter.loading_recents,
            loading_calibre: state.starter.loading_calibre,
            loading_browser_tabs: state.starter.loading_browser_tabs,
            source_open_event: state.runtime_jobs.source_open_event.as_ref(),
            calibre_load_event: state.runtime_jobs.calibre_load_event.as_ref(),
            operations: &state.app_shell.operations,
            last_remote_update_at: state.reader_playback.last_updated_at,
            remote_url: state
                .app_shell
                .app_config_snapshot
                .as_ref()
                .and_then(|c| c.remote_url.as_ref()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CalibreSort {
    Title,
    Author,
    Year,
}

impl CalibreSort {
    const OPTIONS: [CalibreSort; 3] = [CalibreSort::Title, CalibreSort::Author, CalibreSort::Year];

    fn label(self) -> &'static str {
        match self {
            CalibreSort::Title => "Title",
            CalibreSort::Author => "Author",
            CalibreSort::Year => "Year",
        }
    }
}

pub(crate) const THUMB_WIDTH: usize = 68;
pub(crate) const THUMB_HEIGHT: usize = 100;
pub(crate) const THUMB_ROW_HEIGHT: f32 = 112.0;
const THUMB_CACHE_MAX: usize = 200;

struct ThumbnailCache {
    tx: mpsc::Sender<ThumbRequest>,
    rx: mpsc::Receiver<ThumbReady>,
    pending: HashSet<PathBuf>,
    textures: HashMap<PathBuf, TextureHandle>,
    last_used: HashMap<PathBuf, u64>,
    usage_tick: u64,
}

impl ThumbnailCache {
    fn new() -> Self {
        let (tx, worker_rx) = mpsc::channel();
        let (worker_tx, rx) = mpsc::channel();
        thread::spawn(move || thumbnail_worker(worker_rx, worker_tx));
        Self {
            tx,
            rx,
            pending: HashSet::new(),
            textures: HashMap::new(),
            last_used: HashMap::new(),
            usage_tick: 0,
        }
    }

    fn texture_for(&mut self, ctx: &Context, path: &Path) -> Option<TextureHandle> {
        self.poll_ready(ctx);
        let path = path.to_path_buf();
        if let Some(texture) = self.textures.get(&path).cloned() {
            self.touch(&path);
            return Some(texture);
        }
        if !self.pending.contains(&path) {
            self.pending.insert(path.clone());
            let _ = self.tx.send(ThumbRequest { path });
        }
        None
    }

    fn poll_ready(&mut self, ctx: &Context) {
        while let Ok(ready) = self.rx.try_recv() {
            self.pending.remove(&ready.path);
            let image = ColorImage::from_rgba_unmultiplied(ready.size, &ready.pixels);
            let texture = ctx.load_texture(
                format!("thumb:{}", ready.path.display()),
                image,
                egui::TextureOptions::LINEAR,
            );
            self.textures.insert(ready.path.clone(), texture);
            self.touch(&ready.path);
        }
        self.evict_if_needed();
    }

    fn touch(&mut self, path: &Path) {
        self.usage_tick = self.usage_tick.wrapping_add(1);
        self.last_used.insert(path.to_path_buf(), self.usage_tick);
    }

    fn evict_if_needed(&mut self) {
        if self.textures.len() <= THUMB_CACHE_MAX {
            return;
        }
        let mut entries: Vec<(PathBuf, u64)> = self
            .last_used
            .iter()
            .map(|(path, tick)| (path.clone(), *tick))
            .collect();
        entries.sort_by_key(|(_, tick)| *tick);
        let excess = self.textures.len().saturating_sub(THUMB_CACHE_MAX);
        for (path, _) in entries.into_iter().take(excess) {
            self.textures.remove(&path);
            self.last_used.remove(&path);
        }
    }
}

struct ThumbRequest {
    path: PathBuf,
}

struct ThumbReady {
    path: PathBuf,
    size: [usize; 2],
    pixels: Vec<u8>,
}

fn thumbnail_worker(rx: mpsc::Receiver<ThumbRequest>, tx: mpsc::Sender<ThumbReady>) {
    for req in rx {
        let start = Instant::now();
        match load_thumbnail(&req.path) {
            Ok(ready) => {
                trace!(
                    path = %req.path.display(),
                    thumb_ms = start.elapsed().as_millis(),
                    "Decoded thumbnail"
                );
                let _ = tx.send(ready);
            }
            Err(err) => {
                warn!(path = %req.path.display(), error = %err, "Failed to decode thumbnail");
            }
        }
    }
}

fn load_thumbnail(path: &Path) -> Result<ThumbReady, String> {
    let bytes = fs::read(path).map_err(|err| err.to_string())?;
    let image = image::load_from_memory(&bytes).map_err(|err| err.to_string())?;
    let thumb = image.resize_exact(
        THUMB_WIDTH as u32,
        THUMB_HEIGHT as u32,
        image::imageops::FilterType::Triangle,
    );
    let rgba = thumb.to_rgba8();
    let (width, height) = rgba.dimensions();
    Ok(ThumbReady {
        path: path.to_path_buf(),
        size: [width as usize, height as usize],
        pixels: rgba.into_raw(),
    })
}

struct PrettyImageCache {
    tx: mpsc::Sender<ImageRequest>,
    rx: mpsc::Receiver<ImageReady>,
    pending: HashSet<PathBuf>,
    textures: HashMap<PathBuf, TextureHandle>,
    last_used: HashMap<PathBuf, u64>,
    usage_tick: u64,
}

impl PrettyImageCache {
    fn new() -> Self {
        let (tx, worker_rx) = mpsc::channel();
        let (worker_tx, rx) = mpsc::channel();
        thread::spawn(move || pretty_image_worker(worker_rx, worker_tx));
        Self {
            tx,
            rx,
            pending: HashSet::new(),
            textures: HashMap::new(),
            last_used: HashMap::new(),
            usage_tick: 0,
        }
    }

    fn texture_for(
        &mut self,
        ctx: &Context,
        path: &Path,
        max_width_px: u32,
        max_height_px: u32,
        max_entries: usize,
    ) -> Option<TextureHandle> {
        self.poll_ready(ctx, max_entries);
        let path = path.to_path_buf();
        if let Some(texture) = self.textures.get(&path).cloned() {
            trace!(path = %path.display(), "Pretty image cache hit");
            self.touch(&path);
            return Some(texture);
        }
        if !self.pending.contains(&path) {
            trace!(path = %path.display(), "Pretty image cache miss; enqueue decode");
            self.pending.insert(path.clone());
            let _ = self.tx.send(ImageRequest {
                path,
                max_width_px,
                max_height_px,
            });
        }
        None
    }

    fn poll_ready(&mut self, ctx: &Context, max_entries: usize) {
        while let Ok(ready) = self.rx.try_recv() {
            self.pending.remove(&ready.path);
            let image = ColorImage::from_rgba_unmultiplied(ready.size, &ready.pixels);
            let texture = ctx.load_texture(
                format!("pretty_image:{}", ready.path.display()),
                image,
                egui::TextureOptions::LINEAR,
            );
            self.textures.insert(ready.path.clone(), texture);
            self.touch(&ready.path);
        }
        self.evict_if_needed(max_entries.max(1));
    }

    fn touch(&mut self, path: &Path) {
        self.usage_tick = self.usage_tick.wrapping_add(1);
        self.last_used.insert(path.to_path_buf(), self.usage_tick);
    }

    fn evict_if_needed(&mut self, max_entries: usize) {
        if self.textures.len() <= max_entries {
            return;
        }
        let mut entries: Vec<(PathBuf, u64)> = self
            .last_used
            .iter()
            .map(|(path, tick)| (path.clone(), *tick))
            .collect();
        entries.sort_by_key(|(_, tick)| *tick);
        let excess = self.textures.len().saturating_sub(max_entries);
        trace!(
            excess,
            max_entries,
            current = self.textures.len(),
            "Evicting pretty images"
        );
        for (path, _) in entries.into_iter().take(excess) {
            self.textures.remove(&path);
            self.last_used.remove(&path);
        }
    }
}

struct ImageRequest {
    path: PathBuf,
    max_width_px: u32,
    max_height_px: u32,
}

struct ImageReady {
    path: PathBuf,
    size: [usize; 2],
    pixels: Vec<u8>,
}

fn pretty_image_worker(rx: mpsc::Receiver<ImageRequest>, tx: mpsc::Sender<ImageReady>) {
    for req in rx {
        let start = Instant::now();
        match decode_pretty_image(&req.path, req.max_width_px, req.max_height_px) {
            Ok(ready) => {
                trace!(
                    path = %req.path.display(),
                    width = ready.size[0],
                    height = ready.size[1],
                    decode_ms = start.elapsed().as_millis(),
                    "Decoded pretty image"
                );
                let _ = tx.send(ready);
            }
            Err(err) => {
                warn!(
                    path = %req.path.display(),
                    error = %err,
                    "Failed to decode pretty image"
                );
            }
        }
    }
}

fn decode_pretty_image(
    path: &Path,
    max_width_px: u32,
    max_height_px: u32,
) -> Result<ImageReady, String> {
    let bytes = fs::read(path).map_err(|err| err.to_string())?;
    let image = image::load_from_memory(&bytes).map_err(|err| err.to_string())?;
    let resized = image.resize(
        max_width_px.max(1),
        max_height_px.max(1),
        image::imageops::FilterType::Triangle,
    );
    let rgba = resized.to_rgba8();
    let (width, height) = rgba.dimensions();
    Ok(ImageReady {
        path: path.to_path_buf(),
        size: [width as usize, height as usize],
        pixels: rgba.into_raw(),
    })
}

impl LanternLeafApp {
    const OVERLAY_EVICTION_SNACK_DURATION: Duration = Duration::from_secs(5);
    #[cfg(not(target_arch = "wasm32"))]
    fn new(
        cc: &eframe::CreationContext<'_>,
        runtime: AppRuntime,
        tracing_guard: tracing_appender::non_blocking::WorkerGuard,
        app_config: config::AppConfig,
        normalizer: normalizer::TextNormalizer,
    ) -> Self {
        let fonts_configured = setup_egui_fonts(&cc.egui_ctx, &app_config);
        let pdf_renderer = match NativePdfRenderer::new() {
            Ok(renderer) => Some(renderer),
            Err(err) => {
                warn!(error = ?err, "Failed to initialize native PDF renderer");
                None
            }
        };
        let persistence_service: Arc<dyn PersistenceService> =
            if let Some(url) = &app_config.remote_url {
                Arc::new(RemotePersistenceService::new(url.clone()))
            } else {
                Arc::new(FilesystemPersistenceService::default())
            };
        let persistence = Arc::new(PersistenceLifecycle::new(persistence_service));
        let cache_service: Arc<dyn cache_service::CacheService> =
            Arc::new(cache_service::FilesystemCacheService);
        let config_service: Arc<dyn config_service::ConfigService> =
            Arc::new(config_service::FilesystemConfigService);
        let config_path = app_config_path();
        let effect_context = EffectContext::with_services(
            app_config.clone(),
            normalizer.clone(),
            Arc::clone(&persistence),
            Arc::clone(&cache_service),
            config_path,
            Arc::clone(&config_service),
        );
        let effect_session = Arc::clone(&effect_context.session);
        let effect_dispatcher = EffectDispatcher::new(effect_context, Some(cc.egui_ctx.clone()));
        persistence.start_sync_thread(effect_dispatcher.event_tx());

        let mut app = Self {
            runtime,
            _tracing_guard: tracing_guard,
            fonts_configured,
            status_log: Vec::new(),
            show_safe_quit_modal: false,
            show_reader_confirm_modal: false,
            pending_search_focus: false,
            last_plan: None,
            auto_scroll_state: AutoScrollState::default(),
            text_only_override: None,
            text_only_toggle_pending: false,
            anchor_diagnostics: AnchorDiagnostics::default(),
            overlay_diagnostics: OverlayDiagnostics::default(),
            audio_diagnostics: AudioDiagnostics::default(),
            tts_runtime: TtsRuntime::new(normalizer.clone()),
            last_tts_runtime_event: None,
            theme_override: None,
            pending_theme_mode: None,
            theme_patch_pending: false,
            persistence,
            cache_service,
            effect_session,
            tts_session_source: None,
            persistence_logged: false,
            last_reader_source: None,
            last_reader_snapshot: None,
            effect_dispatcher,
            shell_state: ShellState::default(),
            layout_policy: LayoutPolicy::default(),
            settings_trace_events: Vec::new(),
            settings_trace_next_id: 0,
            persistence_trace_events: Vec::new(),
            persistence_trace_next_id: 0,
            regression_snapshots: Vec::new(),
            regression_snapshot_next_id: 0,
            overlay_pressure_focus: false,
            scheduler_events: Vec::new(),
            pdf_render_state: PdfRenderState::default(),
            pdf_renderer,
            current_pdf_path: None,
            pretty_page_cache_key: None,
            pretty_page_cache_blocks: Vec::new(),
            thumbnail_cache: ThumbnailCache::new(),
            pretty_image_cache: PrettyImageCache::new(),
            sentence_scroll_offset: None,
            overlay_eviction_warning_at: None,
            timeline_history: Vec::new(),
            pinned_timeline_entries: Vec::new(),
            starter_open_path_input: String::new(),
            starter_clipboard_text_input: String::new(),
            starter_calibre_query: String::new(),
            starter_calibre_force_refresh: false,
            starter_calibre_sort: CalibreSort::Title,
            starter_calibre_view: Vec::new(),
            starter_calibre_last_query: String::new(),
            starter_calibre_last_sort: CalibreSort::Title,
            starter_calibre_last_count: 0,
            starter_browser_tab_query: String::new(),
            starter_browser_tabs_force_refresh: false,
            starter_browser_tab_id_input: String::new(),
            starter_browser_window_id_input: String::new(),
        };
        app.load_pinned_timeline_entries();
        app
    }

    #[cfg(target_arch = "wasm32")]
    fn new_wasm(
        cc: &eframe::CreationContext<'_>,
        runtime: AppRuntime,
        app_config: config::AppConfig,
        normalizer: normalizer::TextNormalizer,
    ) -> Self {
        let fonts_configured = setup_egui_fonts(&cc.egui_ctx, &app_config);
        
        let url = app_config.remote_url.clone().unwrap_or_else(|| "http://127.0.0.1:3030".to_string());
        let persistence_service: Arc<dyn PersistenceService> = Arc::new(RemotePersistenceService::new(url));
        let persistence = Arc::new(PersistenceLifecycle::new(persistence_service));
        
        let cache_service: Arc<dyn cache_service::CacheService> = Arc::new(cache_service::FilesystemCacheService);
        let config_service: Arc<dyn config_service::ConfigService> = Arc::new(config_service::FilesystemConfigService);
        
        let effect_context = EffectContext::with_services(
            app_config.clone(),
            normalizer.clone(),
            Arc::clone(&persistence),
            Arc::clone(&cache_service),
            PathBuf::new(),
            Arc::clone(&config_service),
        );
        let effect_session = Arc::clone(&effect_context.session);
        let effect_dispatcher = EffectDispatcher::new(effect_context, Some(cc.egui_ctx.clone()));
        persistence.start_sync_thread(effect_dispatcher.event_tx());

        let mut app = Self {
            runtime,
            fonts_configured,
            status_log: Vec::new(),
            show_safe_quit_modal: false,
            show_reader_confirm_modal: false,
            pending_search_focus: false,
            last_plan: None,
            auto_scroll_state: AutoScrollState::default(),
            text_only_override: None,
            text_only_toggle_pending: false,
            anchor_diagnostics: AnchorDiagnostics::default(),
            overlay_diagnostics: OverlayDiagnostics::default(),
            audio_diagnostics: AudioDiagnostics::default(),
            tts_runtime: TtsRuntime::new(normalizer.clone()),
            last_tts_runtime_event: None,
            theme_override: None,
            pending_theme_mode: None,
            theme_patch_pending: false,
            persistence,
            cache_service,
            effect_session,
            tts_session_source: None,
            persistence_logged: false,
            last_reader_source: None,
            last_reader_snapshot: None,
            effect_dispatcher,
            shell_state: ShellState::default(),
            layout_policy: LayoutPolicy::default(),
            settings_trace_events: Vec::new(),
            settings_trace_next_id: 0,
            persistence_trace_events: Vec::new(),
            persistence_trace_next_id: 0,
            regression_snapshots: Vec::new(),
            regression_snapshot_next_id: 0,
            overlay_pressure_focus: false,
            scheduler_events: Vec::new(),
            pdf_render_state: PdfRenderState::default(),
            pdf_renderer: None,
            current_pdf_path: None,
            pretty_page_cache_key: None,
            pretty_page_cache_blocks: Vec::new(),
            thumbnail_cache: ThumbnailCache::new(),
            pretty_image_cache: PrettyImageCache::new(),
            sentence_scroll_offset: None,
            overlay_eviction_warning_at: None,
            timeline_history: Vec::new(),
            pinned_timeline_entries: Vec::new(),
            starter_open_path_input: String::new(),
            starter_clipboard_text_input: String::new(),
            starter_calibre_query: String::new(),
            starter_calibre_force_refresh: false,
            starter_calibre_sort: CalibreSort::Title,
            starter_calibre_view: Vec::new(),
            starter_calibre_last_query: String::new(),
            starter_calibre_last_sort: CalibreSort::Title,
            starter_calibre_last_count: 0,
            starter_browser_tab_query: String::new(),
            starter_browser_tabs_force_refresh: false,
            starter_browser_tab_id_input: String::new(),
            starter_browser_window_id_input: String::new(),
        };
        app
    }

    fn update_shell_state(&mut self, ctx: &Context, state: &AppState) {
        let width = ctx.available_rect().width();
        let new_policy = LayoutPolicy::from_width(width);
        if new_policy != self.layout_policy {
            trace!(?self.layout_policy, ?new_policy, "Shell layout policy updated");
            self.layout_policy = new_policy;
        }
        self.shell_state.update_from_app_state(
            state,
            self.show_safe_quit_modal,
            self.show_reader_confirm_modal,
            self.pending_search_focus,
        );
    }

    fn handle_shortcuts(&mut self, ctx: &Context, state: &AppState) {
        match self.shell_state.focus_owner {
            FocusOwner::Modal | FocusOwner::PanelInput => {
                trace!(?self.shell_state.focus_owner, "Shortcuts suppressed by focus owner");
                return;
            }
            _ => {}
        }
        let mode_scope = match state.session.session.as_ref().map(|session| session.mode) {
            Some(UiMode::Reader) => ShortcutScope::Reader,
            _ => ShortcutScope::Global,
        };
        ctx.input(|input| {
            for event in &input.events {
                if let egui::Event::Key {
                    key,
                    pressed,
                    modifiers,
                    ..
                } = event
                {
                    if !*pressed {
                        continue;
                    }
                    if let Some(combo) = format_combo(*key, *modifiers) {
                        let matches = self.runtime.shortcut_registry().matches(&combo, mode_scope);
                        for binding in matches {
                            self.execute_shortcut_action(&binding.action);
                        }
                    }
                }
            }
        });
    }

    fn execute_shortcut_action(&mut self, action: &ShortcutAction) {
        match action {
            ShortcutAction::Command(command) => self.execute_command(command.clone()),
            ShortcutAction::Ui(UiShortcutAction::FocusSearch) => {
                self.pending_search_focus = true;
                self.push_status("Shortcut: focus search".to_string());
            }
        }
    }

    fn overlay_eviction_warning_age(&mut self) -> Option<Duration> {
        let now = Instant::now();
        if let Some(start) = self.overlay_eviction_warning_at {
            let elapsed = now.duration_since(start);
            if elapsed < Self::OVERLAY_EVICTION_SNACK_DURATION {
                return Some(elapsed);
            }
            self.overlay_eviction_warning_at = None;
        }
        None
    }

    fn execute_timeline_kind(&mut self, kind: &RegressionSnapshotTimelineKind) {
        match kind {
            RegressionSnapshotTimelineKind::OverlayAlert(alert) => {
                self.overlay_pressure_focus = true;
                self.overlay_eviction_warning_at = Some(Instant::now());
                self.push_status(format!("QA timeline overlay alert: {}", alert.describe()));
                self.replay_overlay_pressure_alert(alert);
            }
            RegressionSnapshotTimelineKind::PdfRenderEvent(event) => {
                self.replay_pdf_render_event(event);
            }
            RegressionSnapshotTimelineKind::PdfThrottleEvent(event) => {
                self.replay_throttle_span(event);
            }
            RegressionSnapshotTimelineKind::AudioEvent(event) => {
                self.replay_audio_event(event);
            }
            RegressionSnapshotTimelineKind::SchedulerEvent(event) => {
                self.replay_scheduler_event(event);
            }
            RegressionSnapshotTimelineKind::Status(status) => {
                self.push_status(format!("QA timeline status: {}", status.message));
            }
        }
    }

    fn record_scheduler_event(&mut self, kind: SchedulerEventKind) {
        const MAX_EVENTS: usize = 16;
        let event = SchedulerEvent {
            timestamp: Instant::now(),
            kind,
        };
        self.scheduler_events.push(event.clone());
        if self.scheduler_events.len() > MAX_EVENTS {
            self.scheduler_events.remove(0);
        }
        self.push_budget_timeline_entry(
            RegressionSnapshotTimelineKind::SchedulerEvent(event.clone()),
            event.timestamp,
        );
    }

    fn replay_overlay_pressure_alert(&mut self, alert: &OverlayPressureAlert) {
        trace!(alert = ?alert, "Replaying overlay pressure alert");
        self.pdf_render_state
            .record_overlay_pressure_alert(alert.clone());
    }

    fn replay_pdf_render_event(&mut self, event: &PdfRenderEvent) {
        trace!(event = ?event, "Replaying PDF render event");
        self.pdf_render_state.record_render_event(event.clone());
    }

    fn replay_throttle_span(&mut self, event: &PdfRenderThrottleEvent) {
        trace!(event = ?event, "Replaying PDF throttle event");
        self.pdf_render_state.record_throttle_event(event.clone());
    }

    fn replay_audio_event(&mut self, event: &AudioBudgetEvent) {
        trace!(event = ?event, "Replaying audio budget event");
        self.audio_diagnostics.record(event.clone());
    }

    fn replay_scheduler_event(&mut self, event: &SchedulerEvent) {
        const MAX_EVENTS: usize = 16;
        trace!(event = ?event, "Replaying scheduler event");
        self.scheduler_events.push(event.clone());
        if self.scheduler_events.len() > MAX_EVENTS {
            self.scheduler_events.remove(0);
        }
    }

    fn record_timeline_history(&mut self, entry: &RegressionSnapshotTimelineEntry) {
        let entry = TimelineHistoryEntry::from_entry(entry);
        self.push_history_entry(entry);
    }

    fn push_history_entry(&mut self, entry: TimelineHistoryEntry) {
        const MAX_HISTORY: usize = 16;
        self.timeline_history.push(entry);
        if self.timeline_history.len() > MAX_HISTORY {
            self.timeline_history.remove(0);
        }
        if let Some(last) = self.timeline_history.last() {
            trace!(
                kind = %last.entry.kind_label(),
                reference = %last.ref_label,
            "Recorded QA timeline entry",
            );
        }
    }

    fn push_budget_timeline_entry(
        &mut self,
        kind: RegressionSnapshotTimelineKind,
        timestamp: Instant,
    ) {
        let entry = RegressionSnapshotTimelineEntry { kind, timestamp };
        self.record_timeline_history(&entry);
    }

    fn timeline_archive_root(&self) -> PathBuf {
        workspace_root_from_cwd()
            .map(|root| root.join(TIMELINE_ARCHIVE_DIR))
            .unwrap_or_else(|| PathBuf::from(TIMELINE_ARCHIVE_DIR))
    }

    fn timeline_archive_filename(format: TimelineArchiveFormat) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0));
        format!(
            "qa-timeline-{}-{:09}.{}",
            now.as_secs(),
            now.subsec_nanos(),
            format.extension()
        )
    }

    fn timeline_archive_records(&self) -> Vec<SerializableTimelineHistoryEntry> {
        self.timeline_history
            .iter()
            .map(|entry| entry.to_serializable())
            .collect()
    }

    fn timeline_archive_candidates(&self) -> Vec<PathBuf> {
        let root = self.timeline_archive_root();
        let mut candidates = Vec::new();
        if let Ok(entries) = fs::read_dir(&root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map_or(false, |ext| ext.eq_ignore_ascii_case("json"))
                {
                    if let Ok(metadata) = entry.metadata() {
                        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                        candidates.push((path, modified));
                    }
                }
            }
        }
        candidates.sort_by_key(|(_, modified)| Reverse(*modified));
        candidates.into_iter().map(|(path, _)| path).collect()
    }

    fn import_timeline_archive(&mut self, path: &Path) {
        let data = match fs::read(path) {
            Ok(data) => data,
            Err(err) => {
                warn!(
                    error = %err,
                    path = %path.display(),
                    "Failed to read QA timeline archive for import"
                );
                self.push_status(format!(
                    "Failed to read timeline archive {}: {}",
                    path.display(),
                    err
                ));
                return;
            }
        };
        let records: Vec<SerializableTimelineHistoryEntry> = match serde_json::from_slice(&data) {
            Ok(records) => records,
            Err(err) => {
                warn!(
                    error = %err,
                    path = %path.display(),
                    "Failed to deserialize QA timeline archive"
                );
                self.push_status(format!(
                    "Invalid timeline archive {}: {}",
                    path.display(),
                    err
                ));
                return;
            }
        };
        let mut imported = 0;
        for record in records.into_iter() {
            if let Some(entry) = record.to_entry() {
                self.push_history_entry(entry);
                imported += 1;
            }
        }
        self.push_status(format!(
            "Imported {} entries from {}",
            imported,
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("archive")
        ));
        info!(
            path = ?path,
            imported,
            "Imported QA timeline archive entries"
        );
    }

    fn replay_timeline_archive(&mut self, path: &Path, pin: bool) {
        let data = match fs::read(path) {
            Ok(data) => data,
            Err(err) => {
                warn!(
                    error = %err,
                    path = %path.display(),
                    "Failed to read QA timeline archive for replay"
                );
                self.push_status(format!(
                    "Failed to read timeline archive {}: {}",
                    path.display(),
                    err
                ));
                return;
            }
        };
        let records: Vec<SerializableTimelineHistoryEntry> = match serde_json::from_slice(&data) {
            Ok(records) => records,
            Err(err) => {
                warn!(
                    error = %err,
                    path = %path.display(),
                    "Failed to deserialize QA timeline archive"
                );
                self.push_status(format!(
                    "Invalid timeline archive {}: {}",
                    path.display(),
                    err
                ));
                return;
            }
        };
        let mut replayed = 0;
        for record in records.into_iter() {
            if let Some(entry) = record.to_entry() {
                self.replay_history_entry(entry, pin);
                replayed += 1;
            }
        }
        let action_label = if pin { "Replayed & pinned" } else { "Replayed" };
        self.push_status(format!(
            "{} {} entries from {}",
            action_label,
            replayed,
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("archive")
        ));
        info!(
            path = ?path,
            replayed,
            pin,
            "Replayed QA timeline archive entries"
        );
    }

    fn replay_history_entry(&mut self, entry: TimelineHistoryEntry, pin: bool) {
        let kind = entry.entry.kind.clone();
        self.execute_timeline_kind(&kind);
        self.record_timeline_history(&entry.entry);
        if pin {
            self.pin_timeline_entry(&entry);
        }
    }

    fn pinned_timeline_path(&self) -> PathBuf {
        self.timeline_archive_root().join(PINNED_TIMELINE_FILE)
    }

    fn persist_pinned_timeline_entries(&self) {
        let path = self.pinned_timeline_path();
        if self.pinned_timeline_entries.is_empty() {
            let _ = fs::remove_file(&path);
            return;
        }
        if let Some(parent) = path.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                warn!(
                    error = %err,
                    path = ?path,
                    "Failed to create pinned timeline directory"
                );
                return;
            }
        }
        let records = self
            .pinned_timeline_entries
            .iter()
            .map(|entry| entry.to_serializable())
            .collect::<Vec<_>>();
        match serde_json::to_vec_pretty(&records) {
            Ok(payload) => {
                if let Err(err) = fs::write(&path, payload) {
                    warn!(
                        error = %err,
                        path = ?path,
                        "Failed to persist pinned timeline entries"
                    );
                }
            }
            Err(err) => {
                warn!(
                    error = %err,
                    path = ?path,
                    "Failed to serialize pinned timeline entries"
                );
            }
        }
    }

    fn load_pinned_timeline_entries(&mut self) {
        let path = self.pinned_timeline_path();
        let data = match fs::read(&path) {
            Ok(data) => data,
            Err(_) => return,
        };
        let records: Vec<SerializableTimelineHistoryEntry> = match serde_json::from_slice(&data) {
            Ok(records) => records,
            Err(err) => {
                warn!(
                    error = %err,
                    path = ?path,
                    "Failed to deserialize pinned timeline entries"
                );
                return;
            }
        };
        let entries = records
            .into_iter()
            .filter_map(|record| record.to_entry())
            .collect::<Vec<_>>();
        if !entries.is_empty() {
            self.pinned_timeline_entries = entries;
        }
    }
    fn timeline_archive_csv(&self) -> String {
        let mut rows =
            vec!["\"timestamp\",\"kind\",\"reference\",\"qa_url\",\"details\"".to_owned()];
        rows.extend(
            self.timeline_history
                .iter()
                .map(|entry| entry.export_csv_row()),
        );
        rows.join("\n")
    }

    fn export_timeline_archive(
        &mut self,
        format: TimelineArchiveFormat,
    ) -> Result<PathBuf, String> {
        let payload = match format {
            TimelineArchiveFormat::Json => {
                serde_json::to_string_pretty(&self.timeline_archive_records())
                    .map_err(|err| format!("serialize timeline archive: {}", err))?
            }
            TimelineArchiveFormat::Csv => self.timeline_archive_csv(),
        };
        let root = self.timeline_archive_root();
        if let Err(err) = fs::create_dir_all(&root) {
            return Err(format!(
                "create export directory {}: {}",
                root.display(),
                err
            ));
        }
        let path = root.join(Self::timeline_archive_filename(format));
        fs::write(&path, payload)
            .map_err(|err| format!("write archive {}: {}", path.display(), err))?;
        trace!(path = ?path, format = ?format.label(), "Wrote QA timeline archive");
        Ok(path)
    }

    fn handle_timeline_export(&mut self, format: TimelineArchiveFormat) {
        match self.export_timeline_archive(format) {
            Ok(path) => {
                self.push_status(format!("Exported QA timeline archive ({})", path.display()));
                info!(
                    path = ?path,
                    format = %format.label(),
                    "QA timeline archive exported"
                );
            }
            Err(err) => {
                warn!(
                    error = %err,
                    format = %format.label(),
                    "Failed exporting QA timeline archive"
                );
                self.push_status(format!("Failed to export QA timeline archive: {}", err));
            }
        }
    }

    fn is_timeline_entry_pinned(&self, entry: &TimelineHistoryEntry) -> bool {
        self.pinned_timeline_entries
            .iter()
            .any(|pinned| pinned.matches(entry))
    }

    fn pin_timeline_entry(&mut self, entry: &TimelineHistoryEntry) {
        if self.is_timeline_entry_pinned(entry) {
            return;
        }
        if self.pinned_timeline_entries.len() >= MAX_PINNED_TIMELINE_ENTRIES {
            self.pinned_timeline_entries.remove(0);
        }
        self.pinned_timeline_entries.push(entry.clone());
        trace!(entry = %entry.details(), "Pinned QA timeline entry");
        self.push_status(format!("Pinned timeline entry: {}", entry.details()));
        self.persist_pinned_timeline_entries();
    }

    fn unpin_timeline_entry(&mut self, entry: &TimelineHistoryEntry) -> bool {
        if let Some(pos) = self
            .pinned_timeline_entries
            .iter()
            .position(|pinned| pinned.matches(entry))
        {
            let removed = self.pinned_timeline_entries.remove(pos);
            trace!(entry = %removed.details(), "Unpinned QA timeline entry");
            self.push_status(format!("Unpinned timeline entry: {}", removed.details()));
            self.persist_pinned_timeline_entries();
            true
        } else {
            false
        }
    }

    fn render_pinned_timeline_entries(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Pinned QA timeline entries:").strong());
            if ui.button("Clear pinned entries").clicked() {
                self.pinned_timeline_entries.clear();
                self.push_status("Cleared pinned timeline entries.".to_string());
                self.persist_pinned_timeline_entries();
            }
        });
        if self.pinned_timeline_entries.is_empty() {
            ui.label("(No pinned entries yet.)");
            return;
        }
        let pinned_entries = self.pinned_timeline_entries.clone();
        for entry in pinned_entries.iter().rev() {
            ui.horizontal(|ui| {
                let badge = Button::new(entry.badge_label(Instant::now()))
                    .rounding(6.0)
                    .fill(entry.badge_color());
                if ui.add(badge).clicked() {
                    let kind = entry.entry.kind.clone();
                    self.execute_timeline_kind(&kind);
                    self.record_timeline_history(&entry.entry);
                }
                ui.label(entry.details());
                ui.hyperlink_to("QA link", entry.qa_url.as_str());
                let age_secs = entry.entry.timestamp.elapsed().as_secs_f32();
                ui.label(format!("{:.1}s ago", age_secs));
                if ui.button("Unpin").clicked() {
                    self.unpin_timeline_entry(entry);
                }
            });
        }
    }

    fn render_timeline_archive_imports(&mut self, ui: &mut Ui) {
        let archives = self.timeline_archive_candidates();
        if archives.is_empty() {
            ui.label("(No QA timeline archives available)");
            return;
        }
        ui.label(RichText::new("Available timeline archives:").small());
        for archive in archives.iter() {
            let file_name = archive
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("timeline.json");
            ui.horizontal(|ui| {
                ui.label(RichText::new(file_name).small().weak());
                if ui.button("Import JSON").clicked() {
                    self.import_timeline_archive(archive);
                }
                if ui.button("Replay").clicked() {
                    self.replay_timeline_archive(archive, false);
                }
                if ui.button("Replay + pin").clicked() {
                    self.replay_timeline_archive(archive, true);
                }
            });
        }
    }

    fn render_settings_sidebar(&mut self, ui: &mut Ui, snapshot: Option<&ReaderSnapshot>) {
        CollapsingHeader::new("Settings & persistence")
            .id_source("settings-sidebar")
            .default_open(false)
            .show(ui, |ui| {
                let snapshot = match snapshot {
                    Some(snapshot) => snapshot,
                    None => {
                        ui.label("Open a reader session to adjust settings.");
                        return;
                    }
                };
                let settings = &snapshot.settings;
                ui.horizontal(|ui| {
                    ui.label(format!("Theme: {:?}", settings.theme));
                    let next_theme = match settings.theme {
                        config::ThemeMode::Day => config::ThemeMode::Night,
                        config::ThemeMode::Night => config::ThemeMode::Day,
                    };
                    let label = match next_theme {
                        config::ThemeMode::Day => "Switch to Day",
                        config::ThemeMode::Night => "Switch to Night",
                    };
                    if ui.button(label).clicked() {
                        self.apply_reader_settings_patch(
                            ReaderSettingsPatch {
                                theme: Some(next_theme),
                                ..Default::default()
                            },
                            "theme_toggle",
                        );
                    }
                });
                ui.horizontal(|ui| {
                    let mut auto_scroll = settings.auto_scroll_tts;
                    if ui
                        .checkbox(&mut auto_scroll, "Auto-scroll TTS playback")
                        .changed()
                    {
                        self.apply_reader_settings_patch(
                            ReaderSettingsPatch {
                                auto_scroll_tts: Some(auto_scroll),
                                ..Default::default()
                            },
                            "auto_scroll_tts",
                        );
                    }
                    let mut center_spoken = settings.center_spoken_sentence;
                    if ui
                        .checkbox(&mut center_spoken, "Center spoken sentence")
                        .changed()
                    {
                        self.apply_reader_settings_patch(
                            ReaderSettingsPatch {
                                center_spoken_sentence: Some(center_spoken),
                                ..Default::default()
                            },
                            "center_spoken_sentence",
                        );
                    }
                });
                ui.horizontal(|ui| {
                    let mut show_original = settings.text_only_show_original_text;
                    if ui
                        .checkbox(&mut show_original, "Text-only shows original text")
                        .changed()
                    {
                        self.apply_reader_settings_patch(
                            ReaderSettingsPatch {
                                text_only_show_original_text: Some(show_original),
                                ..Default::default()
                            },
                            "text_only_show_original_text",
                        );
                    }
                });
                ui.add_space(4.0);
                let mut line_spacing = settings.line_spacing;
                if ui
                    .add(
                        Slider::new(&mut line_spacing, 1.0..=2.5)
                            .text("Line spacing")
                            .prefix("Line: "),
                    )
                    .changed()
                {
                    self.apply_reader_settings_patch(
                        ReaderSettingsPatch {
                            line_spacing: Some(line_spacing),
                            ..Default::default()
                        },
                        "line_spacing",
                    );
                }
                let mut pause_after = settings.pause_after_sentence;
                if ui
                    .add(
                        Slider::new(&mut pause_after, 0.1..=3.0)
                            .text("Pause after sentence")
                            .suffix("s"),
                    )
                    .changed()
                {
                    self.apply_reader_settings_patch(
                        ReaderSettingsPatch {
                            pause_after_sentence: Some(pause_after),
                            ..Default::default()
                        },
                        "pause_after_sentence",
                    );
                }
                let mut tts_speed = settings.tts_speed;
                if ui
                    .add(
                        Slider::new(&mut tts_speed, 0.5..=2.5)
                            .text("TTS speed")
                            .suffix("x"),
                    )
                    .changed()
                {
                    self.apply_reader_settings_patch(
                        ReaderSettingsPatch {
                            tts_speed: Some(tts_speed),
                            ..Default::default()
                        },
                        "tts_speed",
                    );
                }
                let mut tts_volume = settings.tts_volume;
                if ui
                    .add(
                        Slider::new(&mut tts_volume, 0.0..=2.0)
                            .text("TTS volume")
                            .suffix("x"),
                    )
                    .changed()
                {
                    self.apply_reader_settings_patch(
                        ReaderSettingsPatch {
                            tts_volume: Some(tts_volume),
                            ..Default::default()
                        },
                        "tts_volume",
                    );
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Persist settings now").clicked() {
                        self.trigger_persistence_flush(
                            PersistenceTrigger::RuntimeConfigChange,
                            "manual_settings_persist",
                        );
                    }
                    if ui.button("Flush persistence caches").clicked() {
                        self.trigger_persistence_flush(
                            PersistenceTrigger::ReaderCommand,
                            "manual_cache_flush",
                        );
                    }
                });
            });
    }

    fn apply_reader_settings_patch(
        &mut self,
        patch: ReaderSettingsPatch,
        description: &'static str,
    ) {
        self.record_settings_event(description, format!("{patch:?}"));
        self.execute_reader_command(ReaderCommand::Session(
            session::SessionCommand::ApplySettings { patch },
        ));
    }

    fn maybe_apply_pending_theme_patch(&mut self, state: &AppState) {
        if !self.theme_patch_pending {
            return;
        }
        let pending = match self.pending_theme_mode {
            Some(mode) => mode,
            None => return,
        };
        if state.reader_document.snapshot.is_none() {
            return;
        }
        self.apply_reader_settings_patch(
            ReaderSettingsPatch {
                theme: Some(pending),
                ..Default::default()
            },
            "theme_toggle_pending",
        );
        self.theme_patch_pending = false;
    }

    fn sync_theme_override(
        &mut self,
        state_theme: config::ThemeMode,
        reader_snapshot: Option<&ReaderSnapshot>,
        bootstrap: Option<&BootstrapState>,
    ) {
        if let Some(pending) = self.pending_theme_mode {
            let reader_matches = reader_snapshot
                .map(|snapshot| snapshot.settings.theme == pending)
                .unwrap_or(false);
            let bootstrap_matches = bootstrap
                .map(|bootstrap| bootstrap.config.theme == pending)
                .unwrap_or(false);
            if reader_matches && bootstrap_matches {
                self.pending_theme_mode = None;
                self.theme_override = None;
            } else {
                self.theme_override = Some(pending);
            }
        } else if self.theme_override == Some(state_theme) {
            self.theme_override = None;
        }
    }

    fn record_settings_event(&mut self, description: &'static str, summary: String) {
        const MAX_EVENTS: usize = 16;
        let event = SettingsTraceEvent {
            id: self.settings_trace_next_id,
            timestamp: Instant::now(),
            description,
            summary,
            roadmap_url: SETTINGS_ROADMAP_URL,
        };
        self.settings_trace_next_id = self.settings_trace_next_id.wrapping_add(1);
        self.settings_trace_events.push(event);
        if self.settings_trace_events.len() > MAX_EVENTS {
            self.settings_trace_events.remove(0);
        }
    }

    fn trigger_persistence_flush(
        &mut self,
        trigger: PersistenceTrigger,
        description: &'static str,
    ) {
        self.record_persistence_event(trigger, description);
        self.queue_persistence_flush(trigger);
    }

    fn record_persistence_event(&mut self, trigger: PersistenceTrigger, description: &'static str) {
        const MAX_EVENTS: usize = 16;
        let event = PersistenceTraceEvent {
            id: self.persistence_trace_next_id,
            timestamp: Instant::now(),
            trigger,
            description,
            roadmap_url: PERSISTENCE_ROADMAP_URL,
        };
        self.persistence_trace_next_id = self.persistence_trace_next_id.wrapping_add(1);
        self.persistence_trace_events.push(event);
        if self.persistence_trace_events.len() > MAX_EVENTS {
            self.persistence_trace_events.remove(0);
        }
    }

    fn refresh_anchor_diagnostics(&mut self, snapshot: Option<&ReaderSnapshot>) {
        if let Some(snapshot) = snapshot {
            self.anchor_diagnostics.refresh(snapshot);
        } else {
            self.anchor_diagnostics.clear();
        }
    }

    fn maybe_record_audio_command(
        &mut self,
        command: &AppCommand,
        snapshot: Option<&ReaderSnapshot>,
    ) {
        let snapshot = match snapshot {
            Some(snapshot) => snapshot,
            None => return,
        };
        let session_command = match command {
            AppCommand::Reader(ReaderCommand::Session(command)) => command,
            _ => return,
        };
        let is_tts = matches!(
            session_command,
            session::SessionCommand::TtsPlay
                | session::SessionCommand::TtsPause
                | session::SessionCommand::TtsTogglePlayPause
                | session::SessionCommand::TtsPlayFromPageStart
                | session::SessionCommand::TtsPlayFromHighlight
                | session::SessionCommand::TtsSeekNext
                | session::SessionCommand::TtsSeekPrev
                | session::SessionCommand::TtsRepeatSentence
                | session::SessionCommand::TtsStop
        );
        if !is_tts {
            return;
        }
        let target_sentence = snapshot
            .tts
            .current_sentence_idx
            .or(snapshot.highlighted_sentence_idx);
        let (anchor, fallback) = match target_sentence {
            Some(idx) => Self::resolve_sentence_anchor(snapshot, idx),
            None => (None, AnchorFallback::Missing),
        };
        let highlight_page =
            Self::page_index_for_global_sentence(&snapshot.page_sentence_counts, target_sentence);
        let overlay_snapshot = self
            .overlay_diagnostics
            .preview_decision()
            .unwrap_or_else(|| self.build_overlay_snapshot(highlight_page));
        let event = AudioBudgetEvent {
            id: self.audio_diagnostics.allocate_event_id(),
            timestamp: Instant::now(),
            command: session_command.action().to_string(),
            auto_scroll: snapshot.settings.auto_scroll_tts,
            target_sentence,
            anchor,
            fallback,
            overlay_snapshot,
            highlight_page,
        };
        trace!(
            command = event.command,
            target_sentence = event.target_sentence,
            highlight_page = event.highlight_page,
            "Recorded audio budget event"
        );
        self.audio_diagnostics.record(event.clone());
        self.push_budget_timeline_entry(
            RegressionSnapshotTimelineKind::AudioEvent(event.clone()),
            event.timestamp,
        );
    }

    fn maybe_reapply_text_only(&mut self, snapshot: &ReaderSnapshot) {
        if self.text_only_override == Some(true) && !snapshot.text_only_mode {
            if !self.text_only_toggle_pending {
                trace!(
                    current = snapshot.text_only_mode,
                    desired = true,
                    "Text-only override mismatch detected, reapplying toggle"
                );
                self.execute_reader_command(ReaderCommand::Session(
                    session::SessionCommand::ToggleTextOnly,
                ));
                self.text_only_toggle_pending = true;
            }
        } else {
            self.text_only_toggle_pending = false;
        }
    }

    fn resolve_sentence_anchor(
        snapshot: &ReaderSnapshot,
        sentence_idx: usize,
    ) -> (Option<usize>, AnchorFallback) {
        if let Some(anchor) = snapshot
            .sentence_anchor_map
            .get(sentence_idx)
            .and_then(|value| *value)
        {
            return (Some(anchor), AnchorFallback::Exact);
        }
        let len = snapshot.sentence_anchor_map.len();
        for offset in 1..len {
            if let Some(prev_idx) = sentence_idx.checked_sub(offset) {
                if let Some(Some(anchor)) = snapshot.sentence_anchor_map.get(prev_idx) {
                    return (Some(*anchor), AnchorFallback::Nearest);
                }
            }
            let next_idx = sentence_idx + offset;
            if let Some(Some(anchor)) = snapshot.sentence_anchor_map.get(next_idx) {
                return (Some(*anchor), AnchorFallback::Nearest);
            }
        }
        (None, AnchorFallback::Missing)
    }

    fn page_index_for_global_sentence(
        page_sentence_counts: &[usize],
        sentence_idx: Option<usize>,
    ) -> Option<usize> {
        let mut remaining = sentence_idx?;
        for (page_idx, count) in page_sentence_counts.iter().enumerate() {
            if remaining < *count {
                return Some(page_idx);
            }
            remaining = remaining.saturating_sub(*count);
        }
        None
    }

    fn format_pdf_page_list(pages: &[usize]) -> String {
        if pages.is_empty() {
            return "none".to_string();
        }
        let labels = pages
            .iter()
            .map(|page| (page + 1).to_string())
            .collect::<Vec<_>>();
        labels.join(", ")
    }

    fn build_overlay_snapshot(&self, highlight_page: Option<usize>) -> OverlayDecisionSnapshot {
        let budget_pages = self.pdf_render_state.overlay_budget_pages();
        let highlight_page_has_text_layer = highlight_page
            .and_then(|page| self.pdf_render_state.surface_for_page(page))
            .map(|surface| surface.text_layer_ready)
            .unwrap_or(false);
        let overlay_rects_available = self.pdf_render_state.overlay_rects.len();
        OverlayDecisionSnapshot {
            allowed: highlight_page_has_text_layer && budget_pages > 0,
            budget_pages,
            overlays_drawn: self.pdf_render_state.rendered_overlays,
            highlight_page_has_text_layer,
            highlight_page,
            overlay_rects_available,
            overlay_reason: self.pdf_render_state.overlay_alignment_reason.clone(),
        }
    }

    fn update_pdf_confidence(&mut self, snapshot: &ReaderSnapshot) {
        let tier = Self::derive_pdf_confidence_tier(snapshot);
        if self
            .pdf_render_state
            .update_confidence_tier(tier, &snapshot.source_path)
        {
            let label = tier.map(PdfConfidenceTier::label).unwrap_or("unknown");
            trace!(tier = label, "Updated PDF confidence tier");
            self.push_status(format!("PDF confidence: {}", label));
        }
    }

    fn derive_pdf_confidence_tier(snapshot: &ReaderSnapshot) -> Option<PdfConfidenceTier> {
        if snapshot.pretty_kind != PrettyKind::Pdf {
            return None;
        }
        if let Some(mode) = snapshot.pdf_geometry_mode {
            return Some(match mode {
                PdfGeometryMode::HighTextTrust => PdfConfidenceTier::TrustworthyText,
                PdfGeometryMode::MixedTextTrust => PdfConfidenceTier::MixedFuzzy,
                PdfGeometryMode::OcrRequired => PdfConfidenceTier::OcrRequired,
                PdfGeometryMode::RenderOnlyNoSync => PdfConfidenceTier::RenderOnly,
            });
        }
        if let Some(PdfSyncStrategy::RenderOnly) = snapshot.pdf_sync_strategy {
            return Some(PdfConfidenceTier::RenderOnly);
        }
        if let Some(alignment) = snapshot.pdf_ocr_alignment.as_ref() {
            return Some(match alignment.quality_class {
                PdfOcrGeometryQualityClass::OcrHighTrust => PdfConfidenceTier::TrustworthyText,
                PdfOcrGeometryQualityClass::OcrMixedTrust => PdfConfidenceTier::MixedFuzzy,
                PdfOcrGeometryQualityClass::OcrTextOnly => PdfConfidenceTier::OcrRequired,
                PdfOcrGeometryQualityClass::OcrFailedOrUnusable => PdfConfidenceTier::RenderOnly,
            });
        }
        None
    }

    fn render_status_diagnostics_panel(&mut self, ui: &mut Ui, state: &AppState) {
        ui.label(format!(
            "Operations: reader_command={} reader_tts={} source_open={} calibre_load={}",
            state.app_shell.operations.reader_command,
            state.app_shell.operations.reader_tts,
            state.app_shell.operations.source_open,
            state.app_shell.operations.calibre_load
        ));
        ui.label(format!(
            "Runtime log level: {}",
            state.app_shell.runtime_log_level
        ));
        if let Some(trigger) = state.app_shell.persistence_status.last_trigger {
            ui.label(format!("Last persistence trigger: {:?}", trigger));
        }
        if let Some(outcome) = state.app_shell.persistence_status.last_outcome {
            ui.label(format!("Last persistence outcome: {:?}", outcome));
        }
        if !self.settings_trace_events.is_empty() {
            ui.separator();
            ui.label("Recent settings changes:");
            for event in self.settings_trace_events.iter().rev().take(6) {
                ui.label(format!("{} ({:.1}s)", event.describe(), event.age_secs()));
            }
        }
        if !self.persistence_trace_events.is_empty() {
            ui.separator();
            ui.label("Recent persistence events:");
            for event in self.persistence_trace_events.iter().rev().take(6) {
                ui.label(format!("{} ({:.1}s)", event.describe(), event.age_secs()));
            }
        }
    }

    fn render_safe_quit_modal(&mut self, ctx: &Context) {
        let mut open = self.show_safe_quit_modal;
        eframe::egui::Window::new("Safe quit")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Quit LanternLeaf safely?");
                ui.horizontal(|ui| {
                    if ui.button("Quit").clicked() {
                        self.execute_command(AppCommand::SafeQuit);
                        self.show_safe_quit_modal = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_safe_quit_modal = false;
                    }
                });
            });
        if !open {
            self.show_safe_quit_modal = false;
        }
    }

    fn render_reader_confirm_modal(
        &mut self,
        ctx: &Context,
        reader_snapshot: Option<&ReaderSnapshot>,
    ) {
        let mut open = self.show_reader_confirm_modal;
        eframe::egui::Window::new("Reader session closed")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                if let Some(snapshot) = reader_snapshot {
                    ui.label(format!("Closed session for {}", snapshot.source_name));
                } else {
                    ui.label("Reader session closed.");
                }
                if ui.button("OK").clicked() {
                    self.show_reader_confirm_modal = false;
                }
            });
        if !open {
            self.show_reader_confirm_modal = false;
        }
    }

    fn render_anchor_diagnostics(&self, ui: &mut Ui, snapshot: Option<&ReaderSnapshot>) {
        CollapsingHeader::new("Anchor diagnostics")
            .id_source("anchor-diagnostics")
            .default_open(false)
            .show(ui, |ui| {
                let snapshot = match snapshot {
                    Some(snapshot) => snapshot,
                    None => {
                        ui.label("Activate a reader session to collect anchor diagnostics.");
                        return;
                    }
                };
                if self.anchor_diagnostics.is_empty() {
                    ui.label("Gathering anchor fallback data...");
                    return;
                }
                let total = self.anchor_diagnostics.total();
                ui.label(format!("Sentences scanned: {}", total));
                for (fallback, count) in self.anchor_diagnostics.fallback_counts() {
                    let pct = if total > 0 {
                        (count as f32 / total as f32) * 100.0
                    } else {
                        0.0
                    };
                    ui.horizontal(|ui| {
                        ui.label(format!("{}:", fallback.label()));
                        ui.label(format!("{} ({:.1}%)", count, pct));
                    });
                }
                if let Some(age) = self.anchor_diagnostics.last_refresh_age() {
                    ui.label(format!(
                        "Diagnostics refreshed {:.2}s ago.",
                        age.as_secs_f32()
                    ));
                }
                if let Some(elapsed) = self.auto_scroll_state.last_jump_elapsed() {
                    ui.label(format!(
                        "Last JumpToSentence {:.2}s ago (throttle window {}ms).",
                        elapsed.as_secs_f32(),
                        AutoScrollState::JUMP_THROTTLE.as_millis()
                    ));
                } else {
                    ui.label("JumpToSentence has not run yet.");
                }
                ui.label(format!(
                    "Throttled JumpToSentence attempts: {}",
                    self.auto_scroll_state.throttle_blocked()
                ));
                if snapshot.pretty_kind == PrettyKind::Pdf {
                    ui.separator();
                    ui.label("PDF anchor / OCR diagnostics:");
                    if let Some(alignment) = snapshot.pdf_ocr_alignment.as_ref() {
                        ui.label(format!("OCR quality: {:?}", alignment.quality_class));
                        ui.label(format!(
                            "Exact sentence rate: {:.1}%",
                            alignment.exact_sentence_rate * 100.0
                        ));
                        if !alignment.degraded_reasons.is_empty() {
                            ui.label(format!(
                                "OCR degraded reasons: {}",
                                alignment.degraded_reasons.join(", ")
                            ));
                        }
                    }
                    if let Some(policy) = snapshot.pdf_runtime_policy.as_ref() {
                        ui.label(format!(
                            "Highlight policy: {:?}",
                            policy.sentence_highlight_policy
                        ));
                        if !policy.degraded_reasons.is_empty() {
                            ui.label(format!(
                                "Policy degraded reasons: {}",
                                policy.degraded_reasons.join(", ")
                            ));
                        }
                    }
                }
            });
    }

    fn dispatch_browser_tab_open(&mut self, bundle: bool) {
        let tab_id = match self.starter_browser_tab_id_input.trim().parse::<u64>() {
            Ok(id) => id,
            Err(_) => {
                warn!("Invalid browser tab id");
                self.push_status("Starter: invalid browser tab id".to_string());
                return;
            }
        };
        let window_id = self
            .starter_browser_window_id_input
            .trim()
            .parse::<u64>()
            .ok();
        if bundle {
            trace!(tab_id, window_id = ?window_id, "Starter open browser tab bundle");
            self.execute_command(AppCommand::OpenBrowserTabBundle { tab_id, window_id });
        } else {
            trace!(tab_id, window_id = ?window_id, "Starter open browser tab");
            self.execute_command(AppCommand::OpenBrowserTab { tab_id, window_id });
        }
    }

    fn pdf_viewport_trigger(
        &self,
        snapshot: &ReaderSnapshot,
        highlighted_page: usize,
        visible_page_indexes: &[usize],
    ) -> PdfViewportUpdateTrigger {
        if self.pdf_render_state.last_viewport_update.is_none() {
            return PdfViewportUpdateTrigger::Init;
        }
        let prev_visible = self
            .pdf_render_state
            .visible_page_indexes
            .first()
            .copied()
            .unwrap_or(snapshot.current_page);
        let next_visible = visible_page_indexes
            .first()
            .copied()
            .unwrap_or(snapshot.current_page);
        if prev_visible != next_visible {
            return PdfViewportUpdateTrigger::Scroll;
        }
        if self.pdf_render_state.jump_target_page_index != Some(highlighted_page) {
            return PdfViewportUpdateTrigger::Jump;
        }
        if self.pdf_render_state.active_tts_page_index != Some(snapshot.current_page) {
            return PdfViewportUpdateTrigger::Tts;
        }
        PdfViewportUpdateTrigger::Refresh
    }

    fn should_throttle_pdf_viewport_update(&self) -> bool {
        match self.pdf_render_state.last_viewport_update {
            None => false,
            Some(instant) => instant.elapsed() < PDF_VIEWPORT_UPDATE_THROTTLE,
        }
    }

    fn update_pdf_render_state(&mut self, snapshot: Option<&ReaderSnapshot>) {
        if let Some(snapshot) = snapshot {
            if snapshot.pretty_kind == PrettyKind::Pdf && snapshot.total_pages > 0 {
                self.current_pdf_path = Some(PathBuf::from(&snapshot.source_path));
                self.update_pdf_confidence(snapshot);
                let visible_page_indexes = vec![snapshot.current_page];
                let highlighted_page = snapshot
                    .highlighted_sentence_idx
                    .and_then(|sentence_idx| {
                        Self::page_index_for_global_sentence(
                            &snapshot.page_sentence_counts,
                            Some(sentence_idx),
                        )
                    })
                    .unwrap_or(snapshot.current_page);
                let plan_input = PdfViewportPlanInput {
                    total_pages: snapshot.total_pages,
                    visible_page_indexes: visible_page_indexes.clone(),
                    overscan: 1,
                    active_tts_page_index: Some(snapshot.current_page),
                    jump_target_page_index: Some(highlighted_page),
                };
                let visible_range = PdfViewportRange::from_pages(&visible_page_indexes);
                let trigger =
                    self.pdf_viewport_trigger(snapshot, highlighted_page, &visible_page_indexes);
                let throttled = self.should_throttle_pdf_viewport_update();
                if throttled && matches!(trigger, PdfViewportUpdateTrigger::Refresh) {
                    trace!("PDF viewport update throttled");
                    return;
                }
                let plan = build_pdf_viewport_render_plan(&plan_input);
                let overscan_range = PdfViewportRange::from_pages(&plan.canvas_page_indexes);
                if !self.pdf_render_state.should_commit_viewport_update(
                    visible_range,
                    overscan_range,
                    trigger,
                ) {
                    return;
                }
                let mut registry_pages = plan.canvas_page_indexes.clone();
                registry_pages.extend(plan.text_layer_page_indexes.iter().copied());
                registry_pages.sort_unstable();
                registry_pages.dedup();
                let entries = registry_pages
                    .into_iter()
                    .map(|page_index| PdfPageRegistryEntry {
                        page_index,
                        last_touched_at: (snapshot.current_page as u64)
                            .saturating_add(page_index as u64),
                        rendered_zoom: if plan.canvas_page_indexes.contains(&page_index) {
                            Some(self.pdf_render_state.zoom_level)
                        } else {
                            None
                        },
                        text_layer_zoom: if plan.text_layer_page_indexes.contains(&page_index) {
                            Some(self.pdf_render_state.zoom_level)
                        } else {
                            None
                        },
                    })
                    .collect::<Vec<_>>();
                let mut keep_canvas_page_indexes = plan.priority_page_indexes.clone();
                keep_canvas_page_indexes.push(highlighted_page);
                keep_canvas_page_indexes.sort_unstable();
                keep_canvas_page_indexes.dedup();
                let mut keep_text_layer_page_indexes = keep_canvas_page_indexes.clone();
                keep_text_layer_page_indexes.extend(plan.text_layer_page_indexes.iter().copied());
                keep_text_layer_page_indexes.sort_unstable();
                keep_text_layer_page_indexes.dedup();
                let decision = choose_pdf_viewport_evictions(&PdfViewportBudgetInput {
                    entries,
                    keep_canvas_page_indexes,
                    keep_text_layer_page_indexes,
                    max_canvas_pages: PDF_CANVAS_BUDGET_PAGES.max(1),
                    max_text_layer_pages: PDF_TEXT_LAYER_BUDGET_PAGES.max(1),
                });
                if !decision.evict_canvas_page_indexes.is_empty()
                    || !decision.evict_text_layer_page_indexes.is_empty()
                {
                    self.record_scheduler_event(SchedulerEventKind::Eviction {
                        evicted_canvas_pages: decision.evict_canvas_page_indexes.clone(),
                        evicted_text_layer_pages: decision.evict_text_layer_page_indexes.clone(),
                    });
                    trace!(
                        reason = "viewport",
                        evicted_canvas_pages = ?decision.evict_canvas_page_indexes,
                        evicted_text_layer_pages = ?decision.evict_text_layer_page_indexes,
                        "PDF texture eviction decision"
                    );
                }
                let viewport_span = tracing::span!(
                    Level::TRACE,
                    "pdf.viewport.update",
                    visible_range = ?visible_range,
                    overscan_range = ?overscan_range,
                    trigger = ?trigger,
                    zoom_level = self.pdf_render_state.zoom_level,
                    throttled
                );
                let _viewport_enter = viewport_span.enter();
                trace!(
                    pdf_plan = ?plan,
                    evicted_canvases = ?decision.evict_canvas_page_indexes,
                    evicted_text_layers = ?decision.evict_text_layer_page_indexes,
                    highlighted_page,
                    canvas_budget = PDF_CANVAS_BUDGET_PAGES,
                    text_layer_budget = PDF_TEXT_LAYER_BUDGET_PAGES,
                    "PDF scheduler updated"
                );
                self.pdf_render_state.plan = Some(plan.clone());
                self.pdf_render_state.update_surfaces(&plan);
                self.pdf_render_state.decision = Some(decision.clone());
                self.pdf_render_state.apply_budget_evictions(&decision);
                self.pdf_render_state.visible_page_indexes = visible_page_indexes;
                self.pdf_render_state.active_tts_page_index = plan_input.active_tts_page_index;
                self.pdf_render_state.jump_target_page_index = plan_input.jump_target_page_index;
                self.pdf_render_state.highlighted_sentence_idx = snapshot.highlighted_sentence_idx;
                self.pdf_render_state.last_viewport_range = visible_range;
                self.pdf_render_state.last_overscan_range = overscan_range;
                self.pdf_render_state.last_viewport_trigger = Some(trigger);
                self.pdf_render_state.last_viewport_update = Some(Instant::now());
                self.pdf_render_state.last_updated = Some(Instant::now());
                return;
            }
        }
        self.current_pdf_path = None;
        self.pdf_render_state.reset();
    }
}

fn setup_egui_fonts(ctx: &Context, cfg: &config::AppConfig) -> bool {
    let requested = cfg.font_family.to_string();
    let mut db = fontdb::Database::new();
    db.load_system_fonts();

    let prop_regular = pick_face(&db, &requested, false);
    let prop_bold = pick_face(&db, &requested, true).or(prop_regular);
    let mono_requested = match cfg.font_family {
        config::FontFamily::FiraCode => "Fira Code".to_string(),
        config::FontFamily::Courier => "Courier".to_string(),
        config::FontFamily::Monospace => "Monospace".to_string(),
        _ => "Fira Code".to_string(),
    };
    let mono_regular = pick_face(&db, &mono_requested, false);
    let mono_bold = pick_face(&db, &mono_requested, true).or(mono_regular);

    let mut fonts = FontDefinitions::default();
    let mut inserted_any = false;

    if let Some(id) = prop_regular {
        if let Some(bytes) = face_bytes(&db, id) {
            fonts
                .font_data
                .insert("ll-prop-regular".into(), FontData::from_owned(bytes));
            fonts.families.insert(
                FontFamily::Name("LanternLeafProportionalRegular".into()),
                vec!["ll-prop-regular".to_string()],
            );
            inserted_any = true;
        }
    }
    if let Some(id) = prop_bold {
        if let Some(bytes) = face_bytes(&db, id) {
            fonts
                .font_data
                .insert("ll-prop-bold".into(), FontData::from_owned(bytes));
            fonts.families.insert(
                FontFamily::Name("LanternLeafProportionalBold".into()),
                vec!["ll-prop-bold".to_string()],
            );
            inserted_any = true;
        }
    }
    if let Some(id) = mono_regular {
        if let Some(bytes) = face_bytes(&db, id) {
            fonts
                .font_data
                .insert("ll-mono-regular".into(), FontData::from_owned(bytes));
            fonts.families.insert(
                FontFamily::Name("LanternLeafMonospaceRegular".into()),
                vec!["ll-mono-regular".to_string()],
            );
            inserted_any = true;
        }
    }
    if let Some(id) = mono_bold {
        if let Some(bytes) = face_bytes(&db, id) {
            fonts
                .font_data
                .insert("ll-mono-bold".into(), FontData::from_owned(bytes));
            fonts.families.insert(
                FontFamily::Name("LanternLeafMonospaceBold".into()),
                vec!["ll-mono-bold".to_string()],
            );
            inserted_any = true;
        }
    }

    if inserted_any {
        tracing::info!(
            requested_family = %requested,
            mono_family = %mono_requested,
            "Configured egui font families via fontdb"
        );
        ctx.set_fonts(fonts);
        ctx.style_mut(|style| {
            let base = (cfg.font_size.max(8) as f32 * cfg.chrome_font_scale).clamp(10.0, 22.0);
            style.text_styles.insert(
                TextStyle::Body,
                eframe::egui::FontId::new(
                    base,
                    FontFamily::Name("LanternLeafProportionalRegular".into()),
                ),
            );
            style.text_styles.insert(
                TextStyle::Heading,
                eframe::egui::FontId::new(
                    (base * 1.25).max(10.0),
                    FontFamily::Name("LanternLeafProportionalBold".into()),
                ),
            );
            style.text_styles.insert(
                TextStyle::Monospace,
                eframe::egui::FontId::new(
                    (base * 0.95).max(9.0),
                    FontFamily::Name("LanternLeafMonospaceRegular".into()),
                ),
            );
        });
        true
    } else {
        tracing::warn!(
            requested_family = %requested,
            "Unable to resolve system font family; using egui defaults"
        );
        false
    }
}

fn pick_face(db: &fontdb::Database, family: &str, bold: bool) -> Option<fontdb::ID> {
    use fontdb::{Style, Weight};
    let desired_weight = if bold { Weight::BOLD } else { Weight::NORMAL };
    let mut best: Option<(fontdb::ID, i32)> = None;
    for face in db.faces() {
        let fam_hit = face
            .families
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(family));
        if !fam_hit {
            continue;
        }
        if face.style != Style::Normal {
            continue;
        }
        let weight_delta = (face.weight.0 as i32 - desired_weight.0 as i32).abs();
        let score = -weight_delta;
        match best {
            Some((_, best_score)) if best_score >= score => {}
            _ => best = Some((face.id, score)),
        }
    }
    best.map(|(id, _)| id)
}

fn face_bytes(db: &fontdb::Database, id: fontdb::ID) -> Option<Vec<u8>> {
    let mut out: Option<Vec<u8>> = None;
    db.with_face_data(id, |data, _index| {
        out = Some(data.to_vec());
    });
    out
}

impl eframe::App for LanternLeafApp {
    fn update(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
        let _ = frame;
        self.sync_tts_runtime_session();
        self.handle_tts_runtime_events();
        self.handle_effect_events();
        let snapshot = self.runtime.state_snapshot();
        let reader_snapshot = snapshot.reader_document.snapshot.as_ref();
        self.update_persistence_lifecycle(reader_snapshot);
        self.update_shell_state(ctx, &snapshot);
        let panels = snapshot
            .session
            .session
            .as_ref()
            .map(|session| session.panels)
            .unwrap_or_default();
        self.tts_runtime.set_panels(panels);
        self.refresh_anchor_diagnostics(reader_snapshot);
        self.update_pdf_render_state(reader_snapshot);
        self.maybe_apply_pending_theme_patch(&snapshot);
        let state_theme = self.theme_from_state(&snapshot, reader_snapshot);
        self.sync_theme_override(
            state_theme,
            reader_snapshot,
            snapshot.app_shell.bootstrap.as_ref(),
        );
        let theme = self.resolve_theme(&snapshot, reader_snapshot);
        let visuals = match theme {
            config::ThemeMode::Day => Visuals::light(),
            config::ThemeMode::Night => Visuals::dark(),
        };
        ctx.set_visuals(visuals);
        self.handle_shortcuts(ctx, &snapshot);
        self.render_top_bar(ctx, &snapshot);
        self.render_navigation_row(ctx, &snapshot);
        self.render_panels(ctx, &snapshot, reader_snapshot);
        self.render_center(ctx, &snapshot);
        self.render_modals(ctx, reader_snapshot);
        self.render_status(ctx);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum AnchorFallback {
    Exact,
    Nearest,
    Missing,
}

impl AnchorFallback {
    const VARIANT_COUNT: usize = 3;
    const VARIANTS: [AnchorFallback; AnchorFallback::VARIANT_COUNT] = [
        AnchorFallback::Exact,
        AnchorFallback::Nearest,
        AnchorFallback::Missing,
    ];

    fn label(self) -> &'static str {
        match self {
            AnchorFallback::Exact => "exact",
            AnchorFallback::Nearest => "nearest",
            AnchorFallback::Missing => "missing",
        }
    }

    fn index(self) -> usize {
        match self {
            AnchorFallback::Exact => 0,
            AnchorFallback::Nearest => 1,
            AnchorFallback::Missing => 2,
        }
    }
}

#[derive(Clone, Copy)]
struct AnchorInfo {
    anchor: Option<usize>,
    fallback: AnchorFallback,
}

impl AnchorInfo {
    fn missing() -> Self {
        Self {
            anchor: None,
            fallback: AnchorFallback::Missing,
        }
    }
}

#[derive(Default)]
struct AnchorDiagnostics {
    counts: [usize; AnchorFallback::VARIANT_COUNT],
    entries: Vec<AnchorInfo>,
    last_refresh: Option<Instant>,
}

impl AnchorDiagnostics {
    fn refresh(&mut self, snapshot: &ReaderSnapshot) {
        self.entries.clear();
        self.entries.reserve(snapshot.sentences.len());
        self.counts = [0; AnchorFallback::VARIANT_COUNT];
        for idx in 0..snapshot.sentences.len() {
            let (anchor, fallback) = LanternLeafApp::resolve_sentence_anchor(snapshot, idx);
            self.entries.push(AnchorInfo { anchor, fallback });
            self.counts[fallback.index()] += 1;
        }
        self.last_refresh = Some(Instant::now());
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.counts = [0; AnchorFallback::VARIANT_COUNT];
        self.last_refresh = None;
    }

    fn entries(&self) -> &[AnchorInfo] {
        &self.entries
    }

    fn fallback_counts(&self) -> impl Iterator<Item = (AnchorFallback, usize)> + '_ {
        AnchorFallback::VARIANTS
            .iter()
            .enumerate()
            .map(|(idx, &fallback)| (fallback, self.counts[idx]))
    }

    fn total(&self) -> usize {
        self.entries.len()
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn last_refresh_age(&self) -> Option<Duration> {
        self.last_refresh.map(|instant| instant.elapsed())
    }
}

#[derive(Clone, Debug)]
struct OverlayDecisionSnapshot {
    allowed: bool,
    budget_pages: usize,
    overlays_drawn: usize,
    highlight_page_has_text_layer: bool,
    highlight_page: Option<usize>,
    overlay_rects_available: usize,
    overlay_reason: Option<String>,
}

#[derive(Default)]
struct OverlayDiagnostics {
    preview_decision: Option<OverlayDecisionSnapshot>,
    last_jump_decision: Option<(&'static str, OverlayDecisionSnapshot)>,
}

impl OverlayDiagnostics {
    fn record_preview(&mut self, decision: OverlayDecisionSnapshot) {
        self.preview_decision = Some(decision);
    }

    fn record_jump(&mut self, event: &'static str, decision: OverlayDecisionSnapshot) {
        self.last_jump_decision = Some((event, decision));
    }

    fn preview_decision(&self) -> Option<OverlayDecisionSnapshot> {
        self.preview_decision.clone()
    }

    fn last_jump_decision(&self) -> Option<(&'static str, OverlayDecisionSnapshot)> {
        self.last_jump_decision.clone()
    }
}

#[derive(Clone, Debug)]
struct AudioBudgetEvent {
    id: usize,
    timestamp: Instant,
    command: String,
    auto_scroll: bool,
    target_sentence: Option<usize>,
    anchor: Option<usize>,
    fallback: AnchorFallback,
    overlay_snapshot: OverlayDecisionSnapshot,
    highlight_page: Option<usize>,
}

impl AudioBudgetEvent {
    fn describe(&self) -> String {
        let sentence_label = self
            .target_sentence
            .map(|idx| format!("{}", idx + 1))
            .unwrap_or_else(|| "unknown".to_string());
        let fallback_label = self.fallback.label();
        format!(
            "{} → sentence {} ({fallback_label}, budget {})",
            self.command, sentence_label, self.overlay_snapshot.budget_pages
        )
    }

    fn age_secs(&self) -> f32 {
        self.timestamp.elapsed().as_secs_f32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lanternleaf_core::cache::{PdfOcrSentenceAlignment, PdfRect};
    use lanternleaf_core::epub_loader::{
        PdfOcrAlignmentSummary, PdfOcrGeometryQualityClass, PdfOcrSourceKind,
    };
    use lanternleaf_core::session::{
        PanelState, PrettyKind, ReaderSettingsView, ReaderSnapshot, ReaderStats, ReaderTtsView,
        TtsPlaybackState,
    };

    fn rect() -> PdfRect {
        PdfRect {
            left: 0.1,
            top: 0.2,
            width: 0.3,
            height: 0.4,
        }
    }

    fn alignment_summary(quality_class: PdfOcrGeometryQualityClass) -> PdfOcrAlignmentSummary {
        PdfOcrAlignmentSummary {
            quality_class,
            source_kind: PdfOcrSourceKind::EmbeddedText,
            sentence_count: 0,
            mapped_sentence_count: 0,
            rect_mapped_sentence_count: 0,
            line_mapped_sentence_count: 0,
            block_mapped_sentence_count: 0,
            page_only_sentence_count: 0,
            unmappable_sentence_count: 0,
            highlightable_sentence_count: 0,
            token_lineage_available: false,
            deterministic: true,
            coverage_ratio: 0.0,
            reused_alignment_count: 0,
            rebuilt_alignment_count: 0,
            cached_page_bucket_count: 0,
            alignment_build_ms: 0,
            geometry_block_count: 0,
            geometry_line_count: 0,
            geometry_token_count: 0,
            page_timing_count: 0,
            chunk_timing_count: 0,
            max_page_build_ms: 0,
            max_chunk_build_ms: 0,
            cross_column_alignment_count: 0,
            cross_column_confident_alignment_count: 0,
            exact_sentence_rate: 0.0,
            degraded_fallback_rate: 0.0,
            page_only_rate: 0.0,
            unmappable_rate: 0.0,
            degraded_reasons: Vec::new(),
            explanation: String::new(),
        }
    }

    fn make_reader_snapshot() -> ReaderSnapshot {
        ReaderSnapshot {
            source_path: "/tmp/sample.pdf".to_string(),
            source_name: "sample.pdf".to_string(),
            current_page: 0,
            total_pages: 2,
            text_only_mode: false,
            has_structured_markdown: false,
            pretty_kind: PrettyKind::Pdf,
            pdf_geometry_mode: None,
            pdf_sync_strategy: None,
            pdf_classification: None,
            pdf_runtime_policy: None,
            pdf_ocr_alignment: None,
            pdf_ocr_pipeline: None,
            images: Vec::new(),
            tts_text_page: String::new(),
            reading_markdown_page: None,
            reading_html_page: None,
            tts_current_sentence_text: None,
            page_text: String::new(),
            sentences: vec!["one".to_string()],
            canonical_sentences: vec!["one".to_string()],
            page_sentence_counts: vec![1],
            sentence_anchor_map: vec![Some(0)],
            highlighted_sentence_idx: Some(0),
            search_query: String::new(),
            search_matches: Vec::new(),
            selected_search_match: None,
            settings: ReaderSettingsView {
                theme: config::ThemeMode::Day,
                font_family: config::FontFamily::Lexend,
                font_weight: config::FontWeight::Normal,
                day_highlight: config::HighlightColor {
                    r: 0.1,
                    g: 0.2,
                    b: 0.3,
                    a: 0.4,
                },
                night_highlight: config::HighlightColor {
                    r: 0.5,
                    g: 0.6,
                    b: 0.7,
                    a: 0.8,
                },
                font_size: 18,
                line_spacing: 1.2,
                word_spacing: 0,
                letter_spacing: 0,
                margin_horizontal: 24,
                margin_vertical: 12,
                lines_per_page: 400,
                pause_after_sentence: 0.0,
                auto_scroll_tts: true,
                center_spoken_sentence: true,
                text_only_show_original_text: false,
                time_remaining_display: config::TimeRemainingDisplay::Adaptive,
                tts_speed: 1.0,
                tts_volume: 1.0,
                pretty: config::PrettyUiConfig::default(),
            },
            tts: ReaderTtsView {
                state: TtsPlaybackState::Idle,
                current_sentence_idx: None,
                sentence_count: 0,
                can_seek_prev: false,
                can_seek_next: false,
                progress_pct: 0.0,
            },
            stats: ReaderStats {
                page_index: 0,
                total_pages: 2,
                tts_progress_pct: 0.0,
                global_progress_pct: 0.0,
                page_time_remaining_secs: 0.0,
                book_time_remaining_secs: 0.0,
                page_word_count: 0,
                page_sentence_count: 0,
                page_start_percent: 0.0,
                page_end_percent: 0.0,
                words_read_up_to_page_start: 0,
                sentences_read_up_to_page_start: 0,
                words_read_up_to_page_end: 0,
                sentences_read_up_to_page_end: 0,
                words_read_up_to_current_position: 0,
                sentences_read_up_to_current_position: 0,
            },
            panels: PanelState::default(),
        }
    }

    fn alignment_with(
        rects: Vec<PdfRect>,
        line_rects: Vec<PdfRect>,
        block_rects: Vec<PdfRect>,
        page_idx: Option<usize>,
    ) -> PdfOcrSentenceAlignment {
        PdfOcrSentenceAlignment {
            sentence_idx: 0,
            sentence_text_hash: String::new(),
            page_idx,
            rects,
            line_rects,
            block_rects,
            confidence_tier: "test".to_string(),
            fallback_reason: String::new(),
            token_lineage: Vec::new(),
            score: 0.8,
            crosses_column_boundaries: false,
            cross_column_confident: false,
        }
    }

    #[test]
    fn alignment_fallback_label_prefers_rects_over_lines() {
        let alignment = alignment_with(vec![rect()], vec![rect()], vec![rect()], Some(1));
        assert_eq!(
            PdfRenderState::alignment_fallback_label(&alignment),
            "exact"
        );
    }

    #[test]
    fn alignment_fallback_label_falls_through_geometry_tiers() {
        let line_only = alignment_with(Vec::new(), vec![rect()], Vec::new(), Some(2));
        assert_eq!(PdfRenderState::alignment_fallback_label(&line_only), "line");
        let block_only = alignment_with(Vec::new(), Vec::new(), vec![rect()], Some(2));
        assert_eq!(
            PdfRenderState::alignment_fallback_label(&block_only),
            "block"
        );
        let page_only = alignment_with(Vec::new(), Vec::new(), Vec::new(), Some(2));
        assert_eq!(PdfRenderState::alignment_fallback_label(&page_only), "page");
        let render_only = alignment_with(Vec::new(), Vec::new(), Vec::new(), None);
        assert_eq!(
            PdfRenderState::alignment_fallback_label(&render_only),
            "render_only"
        );
    }

    #[test]
    fn overlay_geometry_entry_preserves_anchor_label() {
        let alignment = alignment_with(vec![rect()], Vec::new(), Vec::new(), Some(0));
        let entry = OverlayGeometryEntry::from_alignment(&alignment).expect("entry");
        assert_eq!(entry.anchor_label, "exact");
    }

    #[test]
    fn set_highlighted_page_tracks_overlay_anchor_on_surface() {
        let mut state = PdfRenderState::default();
        let plan = PdfViewportRenderPlan {
            canvas_page_indexes: vec![0],
            text_layer_page_indexes: vec![0],
            priority_page_indexes: vec![0],
            medium_priority_page_indexes: Vec::new(),
            low_priority_page_indexes: Vec::new(),
        };
        state.update_surfaces(&plan);
        state.set_highlighted_page(
            0,
            Some(1),
            vec![[0.1, 0.2, 0.3, 0.4]],
            Some("test".to_string()),
            Some("exact".to_string()),
        );
        assert_eq!(state.overlay_anchor.as_deref(), Some("exact"));
        let surface = state.surface_for_page(0).expect("surface");
        assert_eq!(surface.overlay_anchor.as_deref(), Some("exact"));
    }

    #[test]
    fn pdf_confidence_tier_prefers_render_only_sync() {
        let mut snapshot = make_reader_snapshot();
        snapshot.pdf_sync_strategy = Some(PdfSyncStrategy::RenderOnly);
        assert_eq!(
            LanternLeafApp::derive_pdf_confidence_tier(&snapshot),
            Some(PdfConfidenceTier::RenderOnly)
        );
    }

    #[test]
    fn pdf_confidence_tier_uses_alignment_quality() {
        let mut snapshot = make_reader_snapshot();
        snapshot.pdf_ocr_alignment =
            Some(alignment_summary(PdfOcrGeometryQualityClass::OcrMixedTrust));
        assert_eq!(
            LanternLeafApp::derive_pdf_confidence_tier(&snapshot),
            Some(PdfConfidenceTier::MixedFuzzy)
        );
    }

    #[test]
    fn zoom_change_preserves_overlay_rects() {
        let mut state = PdfRenderState::default();
        let plan = PdfViewportRenderPlan {
            canvas_page_indexes: vec![0],
            text_layer_page_indexes: vec![0],
            priority_page_indexes: vec![0],
            medium_priority_page_indexes: Vec::new(),
            low_priority_page_indexes: Vec::new(),
        };
        state.update_surfaces(&plan);
        state.overlay_rects = vec![[0.1, 0.2, 0.3, 0.4]];
        assert!(state.apply_zoom_level(1.1));
        assert_eq!(state.overlay_rects.len(), 1);
        let surface = state.surface_for_page(0).expect("surface");
        assert!(surface.canvas_texture.is_none());
    }

    #[test]
    fn viewport_update_ignores_repeat_targets() {
        let mut state = PdfRenderState::default();
        state.last_viewport_range = Some(PdfViewportRange { start: 0, end: 0 });
        state.last_overscan_range = Some(PdfViewportRange { start: 0, end: 0 });
        assert!(!state.should_commit_viewport_update(
            state.last_viewport_range,
            state.last_overscan_range,
            PdfViewportUpdateTrigger::Scroll
        ));
    }

    #[test]
    fn resolve_sentence_anchor_prefers_exact_match() {
        let mut snapshot = make_reader_snapshot();
        snapshot.sentence_anchor_map = vec![Some(7), None];
        let (anchor, fallback) = LanternLeafApp::resolve_sentence_anchor(&snapshot, 0);
        assert_eq!(anchor, Some(7));
        assert_eq!(fallback, AnchorFallback::Exact);
    }

    #[test]
    fn resolve_sentence_anchor_falls_back_to_nearest() {
        let mut snapshot = make_reader_snapshot();
        snapshot.sentence_anchor_map = vec![None, Some(4), None, None];
        let (anchor, fallback) = LanternLeafApp::resolve_sentence_anchor(&snapshot, 0);
        assert_eq!(anchor, Some(4));
        assert_eq!(fallback, AnchorFallback::Nearest);
    }
}

#[derive(Default)]
struct AudioDiagnostics {
    events: Vec<AudioBudgetEvent>,
    next_id: usize,
}

impl AudioDiagnostics {
    fn record(&mut self, event: AudioBudgetEvent) {
        const MAX_AUDIO_EVENTS: usize = 16;
        self.events.push(event);
        if self.events.len() > MAX_AUDIO_EVENTS {
            self.events.remove(0);
        }
    }

    fn recent_events(&self) -> &[AudioBudgetEvent] {
        &self.events
    }

    fn allocate_event_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        id
    }
}

#[derive(Clone, Debug)]
struct SettingsTraceEvent {
    id: usize,
    timestamp: Instant,
    description: &'static str,
    summary: String,
    roadmap_url: &'static str,
}

impl SettingsTraceEvent {
    fn describe(&self) -> String {
        format!("{} — {}", self.description, self.summary)
    }

    fn age_secs(&self) -> f32 {
        self.timestamp.elapsed().as_secs_f32()
    }
}

#[derive(Clone, Debug)]
struct PersistenceTraceEvent {
    id: usize,
    timestamp: Instant,
    trigger: PersistenceTrigger,
    description: &'static str,
    roadmap_url: &'static str,
}

impl PersistenceTraceEvent {
    fn describe(&self) -> String {
        format!("{} (trigger={:?})", self.description, self.trigger)
    }

    fn age_secs(&self) -> f32 {
        self.timestamp.elapsed().as_secs_f32()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum SchedulerEventKind {
    Eviction {
        evicted_canvas_pages: Vec<usize>,
        evicted_text_layer_pages: Vec<usize>,
    },
    RetryOverlay {
        reason: String,
        highlight_page: Option<usize>,
        budget_pages: usize,
        overlay_reason: Option<String>,
    },
}

#[derive(Clone, Debug)]
struct SchedulerEvent {
    timestamp: Instant,
    kind: SchedulerEventKind,
}

impl SchedulerEvent {
    fn age_secs(&self) -> f32 {
        self.timestamp.elapsed().as_secs_f32()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RegressionScenario {
    OverlayBacklog { reason: &'static str },
    BookmarkRestore { trigger: PersistenceTrigger },
}

impl RegressionScenario {
    fn label(&self) -> &'static str {
        match self {
            RegressionScenario::OverlayBacklog { .. } => "Overlay backlog",
            RegressionScenario::BookmarkRestore { .. } => "Bookmark restore",
        }
    }

    fn roadmap_url(&self) -> &'static str {
        match self {
            RegressionScenario::OverlayBacklog { .. } => READER_RENDR_ROADMAP_URL,
            RegressionScenario::BookmarkRestore { .. } => SETTINGS_ROADMAP_URL,
        }
    }

    fn persistence_trigger(&self) -> Option<PersistenceTrigger> {
        match self {
            RegressionScenario::BookmarkRestore { trigger } => Some(*trigger),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
struct RegressionSnapshot {
    id: usize,
    timestamp: Instant,
    scenario: RegressionScenario,
    source_path: Option<String>,
    current_page: Option<usize>,
    highlighted_sentence: Option<usize>,
    overlay_snapshot: Option<OverlayDecisionSnapshot>,
}

impl RegressionSnapshot {
    fn describe(&self) -> String {
        match &self.scenario {
            RegressionScenario::OverlayBacklog { reason } => {
                let page_label = self
                    .current_page
                    .map(|page| format!("page {}", page + 1))
                    .unwrap_or_else(|| "page unknown".to_string());
                let overlay_reason = self
                    .overlay_snapshot
                    .as_ref()
                    .and_then(|overlay| overlay.overlay_reason.as_deref())
                    .unwrap_or("unknown");
                format!(
                    "{} ({}) on {} (budget {} pages, overlay reason {})",
                    self.scenario.label(),
                    reason,
                    page_label,
                    self.overlay_snapshot
                        .as_ref()
                        .map(|overlay| overlay.budget_pages)
                        .unwrap_or(0),
                    overlay_reason
                )
            }
            RegressionScenario::BookmarkRestore { trigger } => {
                let page_label = self
                    .current_page
                    .map(|page| format!("page {}", page + 1))
                    .unwrap_or_else(|| "page unknown".to_string());
                let sentence_label = self
                    .highlighted_sentence
                    .map(|idx| format!("{}", idx + 1))
                    .unwrap_or_else(|| "unknown sentence".to_string());
                format!(
                    "{} after {:?} on {} (highlighted sentence {})",
                    self.scenario.label(),
                    trigger,
                    page_label,
                    sentence_label
                )
            }
        }
    }

    fn age_secs(&self) -> f32 {
        self.timestamp.elapsed().as_secs_f32()
    }
}

#[derive(Clone, Debug)]
struct StatusLogEntry {
    timestamp: Instant,
    message: String,
}

impl StatusLogEntry {
    fn age_secs(&self) -> f32 {
        self.timestamp.elapsed().as_secs_f32()
    }
}

#[derive(Clone, Debug)]
struct TimelineHistoryEntry {
    entry: RegressionSnapshotTimelineEntry,
    qa_url: String,
    ref_label: String,
    wall_clock_secs: f64,
}

impl TimelineHistoryEntry {
    fn badge_label(&self, reference: Instant) -> String {
        self.entry.badge_label(reference)
    }

    fn badge_color(&self) -> Color32 {
        self.entry.badge_color()
    }

    fn details(&self) -> String {
        format!("{} | {}", self.entry.kind_label(), self.ref_label)
    }

    fn from_entry(entry: &RegressionSnapshotTimelineEntry) -> Self {
        let wall_clock = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0));
        Self {
            entry: entry.clone(),
            qa_url: entry.kind.qa_url().to_string(),
            ref_label: entry.kind.ref_label(),
            wall_clock_secs: wall_clock.as_secs_f64(),
        }
    }

    fn kind_label(&self) -> &'static str {
        self.entry.kind_label()
    }

    fn matches(&self, other: &TimelineHistoryEntry) -> bool {
        self.entry.timestamp == other.entry.timestamp
            && self.ref_label == other.ref_label
            && self.kind_label() == other.kind_label()
    }

    fn timestamp_iso(&self) -> String {
        if self.wall_clock_secs <= 0.0 {
            return "unknown".to_string();
        }
        format!("{:.3}", self.wall_clock_secs)
    }

    fn export_csv_row(&self) -> String {
        format!(
            "\"{}\",\"{}\",\"{}\",\"{}\",\"{}\"",
            self.timestamp_iso(),
            self.entry.kind_label(),
            self.ref_label,
            self.qa_url,
            self.details().replace('"', "\"\""),
        )
    }

    fn to_serializable(&self) -> SerializableTimelineHistoryEntry {
        SerializableTimelineHistoryEntry {
            kind: SerializableTimelineKind::from_kind(&self.entry.kind),
            qa_url: self.qa_url.clone(),
            ref_label: self.ref_label.clone(),
            wall_clock_secs: self.wall_clock_secs,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SerializableTimelineHistoryEntry {
    kind: SerializableTimelineKind,
    qa_url: String,
    ref_label: String,
    wall_clock_secs: f64,
}

impl SerializableTimelineHistoryEntry {
    fn to_entry(&self) -> Option<TimelineHistoryEntry> {
        let kind = self.kind.to_kind()?;
        Some(TimelineHistoryEntry {
            entry: RegressionSnapshotTimelineEntry {
                kind,
                timestamp: Instant::now(),
            },
            qa_url: self.qa_url.clone(),
            ref_label: self.ref_label.clone(),
            wall_clock_secs: self.wall_clock_secs,
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
enum SerializableTimelineKind {
    OverlayAlert(SerializableOverlayPressureAlert),
    PdfRenderEvent(SerializablePdfRenderEvent),
    PdfThrottleEvent(SerializablePdfRenderThrottleEvent),
    AudioEvent(SerializableAudioBudgetEvent),
    SchedulerEvent(SerializableSchedulerEvent),
    Status(SerializableStatusLogEntry),
}

impl SerializableTimelineKind {
    fn from_kind(kind: &RegressionSnapshotTimelineKind) -> Self {
        match kind {
            RegressionSnapshotTimelineKind::OverlayAlert(alert) => {
                SerializableTimelineKind::OverlayAlert(
                    SerializableOverlayPressureAlert::from_alert(alert),
                )
            }
            RegressionSnapshotTimelineKind::PdfRenderEvent(event) => {
                SerializableTimelineKind::PdfRenderEvent(SerializablePdfRenderEvent::from_event(
                    event,
                ))
            }
            RegressionSnapshotTimelineKind::PdfThrottleEvent(event) => {
                SerializableTimelineKind::PdfThrottleEvent(
                    SerializablePdfRenderThrottleEvent::from_event(event),
                )
            }
            RegressionSnapshotTimelineKind::AudioEvent(event) => {
                SerializableTimelineKind::AudioEvent(SerializableAudioBudgetEvent::from_event(
                    event,
                ))
            }
            RegressionSnapshotTimelineKind::SchedulerEvent(event) => {
                SerializableTimelineKind::SchedulerEvent(SerializableSchedulerEvent::from_event(
                    event,
                ))
            }
            RegressionSnapshotTimelineKind::Status(status) => {
                SerializableTimelineKind::Status(SerializableStatusLogEntry {
                    message: status.message.clone(),
                })
            }
        }
    }

    fn to_kind(&self) -> Option<RegressionSnapshotTimelineKind> {
        match self {
            SerializableTimelineKind::OverlayAlert(alert) => alert
                .to_alert()
                .map(RegressionSnapshotTimelineKind::OverlayAlert),
            SerializableTimelineKind::PdfRenderEvent(event) => Some(
                RegressionSnapshotTimelineKind::PdfRenderEvent(event.to_event()),
            ),
            SerializableTimelineKind::PdfThrottleEvent(event) => Some(
                RegressionSnapshotTimelineKind::PdfThrottleEvent(event.to_event()),
            ),
            SerializableTimelineKind::AudioEvent(event) => {
                Some(RegressionSnapshotTimelineKind::AudioEvent(event.to_event()))
            }
            SerializableTimelineKind::SchedulerEvent(event) => Some(
                RegressionSnapshotTimelineKind::SchedulerEvent(event.to_event()),
            ),
            SerializableTimelineKind::Status(entry) => {
                Some(RegressionSnapshotTimelineKind::Status(entry.to_entry()))
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SerializableOverlayPressureAlert {
    id: usize,
    overlay_budget_pages: usize,
    highlight_page: Option<usize>,
    kind: SerializableOverlayPressureKind,
}

impl SerializableOverlayPressureAlert {
    fn from_alert(alert: &OverlayPressureAlert) -> Self {
        Self {
            id: alert.id,
            overlay_budget_pages: alert.overlay_budget_pages,
            highlight_page: alert.highlight_page,
            kind: SerializableOverlayPressureKind::from_kind(&alert.kind),
        }
    }

    fn to_alert(&self) -> Option<OverlayPressureAlert> {
        let kind = self.kind.to_kind()?;
        Some(OverlayPressureAlert::new(
            self.id,
            kind,
            self.overlay_budget_pages,
            self.highlight_page,
        ))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
enum SerializableOverlayPressureKind {
    NativeRender {
        span: SerializableNativeRenderSpan,
        reason_text: String,
    },
    NativeEviction {
        eviction: SerializableNativeRenderEviction,
        reason_text: String,
    },
}

impl SerializableOverlayPressureKind {
    fn from_kind(kind: &OverlayPressureKind) -> Self {
        match kind {
            OverlayPressureKind::NativeRender { span, reason_text } => {
                SerializableOverlayPressureKind::NativeRender {
                    span: SerializableNativeRenderSpan::from_span(span),
                    reason_text: reason_text.clone(),
                }
            }
            OverlayPressureKind::NativeEviction {
                eviction,
                reason_text,
            } => SerializableOverlayPressureKind::NativeEviction {
                eviction: SerializableNativeRenderEviction::from_eviction(eviction),
                reason_text: reason_text.clone(),
            },
        }
    }

    fn to_kind(&self) -> Option<OverlayPressureKind> {
        match self {
            SerializableOverlayPressureKind::NativeRender { span, reason_text } => {
                Some(OverlayPressureKind::NativeRender {
                    span: span.to_span(),
                    reason_text: reason_text.clone(),
                })
            }
            SerializableOverlayPressureKind::NativeEviction {
                eviction,
                reason_text,
            } => Some(OverlayPressureKind::NativeEviction {
                eviction: eviction.to_eviction(),
                reason_text: reason_text.clone(),
            }),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SerializableOverlayDecisionSnapshot {
    allowed: bool,
    budget_pages: usize,
    overlays_drawn: usize,
    highlight_page_has_text_layer: bool,
    highlight_page: Option<usize>,
    overlay_rects_available: usize,
    overlay_reason: Option<String>,
}

impl SerializableOverlayDecisionSnapshot {
    fn from_snapshot(snapshot: &OverlayDecisionSnapshot) -> Self {
        Self {
            allowed: snapshot.allowed,
            budget_pages: snapshot.budget_pages,
            overlays_drawn: snapshot.overlays_drawn,
            highlight_page_has_text_layer: snapshot.highlight_page_has_text_layer,
            highlight_page: snapshot.highlight_page,
            overlay_rects_available: snapshot.overlay_rects_available,
            overlay_reason: snapshot.overlay_reason.clone(),
        }
    }

    fn to_snapshot(&self) -> OverlayDecisionSnapshot {
        OverlayDecisionSnapshot {
            allowed: self.allowed,
            budget_pages: self.budget_pages,
            overlays_drawn: self.overlays_drawn,
            highlight_page_has_text_layer: self.highlight_page_has_text_layer,
            highlight_page: self.highlight_page,
            overlay_rects_available: self.overlay_rects_available,
            overlay_reason: self.overlay_reason.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SerializableAudioBudgetEvent {
    id: usize,
    command: String,
    auto_scroll: bool,
    target_sentence: Option<usize>,
    anchor: Option<usize>,
    fallback: AnchorFallback,
    overlay_snapshot: SerializableOverlayDecisionSnapshot,
    highlight_page: Option<usize>,
}

impl SerializableAudioBudgetEvent {
    fn from_event(event: &AudioBudgetEvent) -> Self {
        Self {
            id: event.id,
            command: event.command.clone(),
            auto_scroll: event.auto_scroll,
            target_sentence: event.target_sentence,
            anchor: event.anchor,
            fallback: event.fallback,
            overlay_snapshot: SerializableOverlayDecisionSnapshot::from_snapshot(
                &event.overlay_snapshot,
            ),
            highlight_page: event.highlight_page,
        }
    }

    fn to_event(&self) -> AudioBudgetEvent {
        AudioBudgetEvent {
            id: self.id,
            timestamp: Instant::now(),
            command: self.command.clone(),
            auto_scroll: self.auto_scroll,
            target_sentence: self.target_sentence,
            anchor: self.anchor,
            fallback: self.fallback,
            overlay_snapshot: self.overlay_snapshot.to_snapshot(),
            highlight_page: self.highlight_page,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SerializableSchedulerEvent {
    kind: SchedulerEventKind,
}

impl SerializableSchedulerEvent {
    fn from_event(event: &SchedulerEvent) -> Self {
        Self {
            kind: event.kind.clone(),
        }
    }

    fn to_event(&self) -> SchedulerEvent {
        SchedulerEvent {
            timestamp: Instant::now(),
            kind: self.kind.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SerializableNativeRenderSpan {
    target: RenderTarget,
    page_index: usize,
    duration_secs: f32,
    cache_hit: bool,
}

impl SerializableNativeRenderSpan {
    fn from_span(span: &NativeRenderSpan) -> Self {
        Self {
            target: span.target,
            page_index: span.page_index,
            duration_secs: span.duration.as_secs_f32(),
            cache_hit: span.cache_hit,
        }
    }

    fn to_span(&self) -> NativeRenderSpan {
        NativeRenderSpan {
            timestamp: Instant::now(),
            target: self.target,
            page_index: self.page_index,
            duration: Duration::from_secs_f32(self.duration_secs),
            cache_hit: self.cache_hit,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SerializableNativeRenderEviction {
    target: RenderTarget,
    page_index: usize,
    reason: String,
}

impl SerializableNativeRenderEviction {
    fn from_eviction(eviction: &NativeRenderEviction) -> Self {
        Self {
            target: eviction.target,
            page_index: eviction.page_index,
            reason: eviction.reason.clone(),
        }
    }

    fn to_eviction(&self) -> NativeRenderEviction {
        NativeRenderEviction {
            timestamp: Instant::now(),
            target: self.target,
            page_index: self.page_index,
            reason: self.reason.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SerializablePdfRenderEvent {
    kind: PdfRenderEventKind,
    page_index: usize,
    highlight_page: bool,
    overlay_budget_pages: usize,
    overlays_drawn: usize,
    overlay_reason: Option<String>,
}

impl SerializablePdfRenderEvent {
    fn from_event(event: &PdfRenderEvent) -> Self {
        Self {
            kind: event.kind,
            page_index: event.page_index,
            highlight_page: event.highlight_page,
            overlay_budget_pages: event.overlay_budget_pages,
            overlays_drawn: event.overlays_drawn,
            overlay_reason: event.overlay_reason.clone(),
        }
    }

    fn to_event(&self) -> PdfRenderEvent {
        PdfRenderEvent {
            timestamp: Instant::now(),
            kind: self.kind,
            page_index: self.page_index,
            highlight_page: self.highlight_page,
            overlay_budget_pages: self.overlay_budget_pages,
            overlays_drawn: self.overlays_drawn,
            overlay_reason: self.overlay_reason.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SerializablePdfRenderThrottleEvent {
    kind: PdfRenderThrottleKind,
    page_index: usize,
    reason: String,
}

impl SerializablePdfRenderThrottleEvent {
    fn from_event(event: &PdfRenderThrottleEvent) -> Self {
        Self {
            kind: event.kind,
            page_index: event.page_index,
            reason: event.reason.clone(),
        }
    }

    fn to_event(&self) -> PdfRenderThrottleEvent {
        PdfRenderThrottleEvent {
            timestamp: Instant::now(),
            kind: self.kind,
            page_index: self.page_index,
            reason: self.reason.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SerializableStatusLogEntry {
    message: String,
}

impl SerializableStatusLogEntry {
    fn to_entry(&self) -> StatusLogEntry {
        StatusLogEntry {
            timestamp: Instant::now(),
            message: self.message.clone(),
        }
    }
}

#[derive(Copy, Clone, Debug)]
enum TimelineArchiveFormat {
    Json,
    Csv,
}

impl TimelineArchiveFormat {
    fn extension(&self) -> &'static str {
        match self {
            TimelineArchiveFormat::Json => "json",
            TimelineArchiveFormat::Csv => "csv",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            TimelineArchiveFormat::Json => "JSON",
            TimelineArchiveFormat::Csv => "CSV",
        }
    }
}

#[derive(Clone, Debug)]
struct RegressionSnapshotTimelineEntry {
    kind: RegressionSnapshotTimelineKind,
    timestamp: Instant,
}

#[derive(Clone, Debug)]
enum RegressionSnapshotTimelineKind {
    OverlayAlert(OverlayPressureAlert),
    PdfRenderEvent(PdfRenderEvent),
    PdfThrottleEvent(PdfRenderThrottleEvent),
    AudioEvent(AudioBudgetEvent),
    SchedulerEvent(SchedulerEvent),
    Status(StatusLogEntry),
}

impl RegressionSnapshotTimelineKind {
    fn qa_url(&self) -> &'static str {
        match self {
            RegressionSnapshotTimelineKind::OverlayAlert(_) => QA_REGRESSION_URL,
            RegressionSnapshotTimelineKind::PdfRenderEvent(_) => READER_RENDR_ROADMAP_URL,
            RegressionSnapshotTimelineKind::PdfThrottleEvent(_) => PDF_SUBSYSTEM_ROADMAP_URL,
            RegressionSnapshotTimelineKind::AudioEvent(_) => TTS_ROADMAP_URL,
            RegressionSnapshotTimelineKind::SchedulerEvent(_) => PDF_SUBSYSTEM_ROADMAP_URL,
            RegressionSnapshotTimelineKind::Status(_) => QA_REGRESSION_URL,
        }
    }

    fn ref_label(&self) -> String {
        match self {
            RegressionSnapshotTimelineKind::OverlayAlert(alert) => {
                format!("overlay span {}", alert.id())
            }
            RegressionSnapshotTimelineKind::PdfRenderEvent(event) => {
                format!("render pg {}", event.page_index + 1)
            }
            RegressionSnapshotTimelineKind::PdfThrottleEvent(event) => format!(
                "{} throttle pg {}",
                match event.kind {
                    PdfRenderThrottleKind::Canvas => "Canvas",
                    PdfRenderThrottleKind::TextLayer => "Text",
                    PdfRenderThrottleKind::Overlay => "Overlay",
                },
                event.page_index + 1
            ),
            RegressionSnapshotTimelineKind::AudioEvent(event) => format!(
                "audio {} sentence {}",
                event.command,
                event.target_sentence.map(|idx| idx + 1).unwrap_or(0)
            ),
            RegressionSnapshotTimelineKind::SchedulerEvent(event) => match &event.kind {
                SchedulerEventKind::Eviction {
                    evicted_canvas_pages,
                    evicted_text_layer_pages,
                } => format!(
                    "scheduler eviction canvases {} / text {}",
                    LanternLeafApp::format_pdf_page_list(evicted_canvas_pages),
                    LanternLeafApp::format_pdf_page_list(evicted_text_layer_pages)
                ),
                SchedulerEventKind::RetryOverlay { highlight_page, .. } => format!(
                    "scheduler retry page {}",
                    highlight_page
                        .map(|page| page + 1)
                        .map_or("unknown".to_string(), |page| page.to_string())
                ),
            },
            RegressionSnapshotTimelineKind::Status(_) => "status log".to_string(),
        }
    }
}

impl RegressionSnapshotTimelineEntry {
    fn badge_label(&self, reference: Instant) -> String {
        format!(
            "{} {:.1}s",
            self.kind_label(),
            Self::relative_secs(reference, self.timestamp)
        )
    }

    fn kind_label(&self) -> &'static str {
        match &self.kind {
            RegressionSnapshotTimelineKind::OverlayAlert(_) => "Overlay",
            RegressionSnapshotTimelineKind::PdfRenderEvent(_) => "Canvas/Text",
            RegressionSnapshotTimelineKind::PdfThrottleEvent(_) => "Throttle",
            RegressionSnapshotTimelineKind::AudioEvent(_) => "Audio",
            RegressionSnapshotTimelineKind::SchedulerEvent(_) => "Scheduler",
            RegressionSnapshotTimelineKind::Status(_) => "Status",
        }
    }

    fn badge_color(&self) -> Color32 {
        match &self.kind {
            RegressionSnapshotTimelineKind::OverlayAlert(_) => Color32::from_rgb(222, 163, 91),
            RegressionSnapshotTimelineKind::PdfRenderEvent(_) => Color32::from_rgb(130, 190, 230),
            RegressionSnapshotTimelineKind::PdfThrottleEvent(_) => Color32::from_rgb(200, 120, 120),
            RegressionSnapshotTimelineKind::AudioEvent(_) => Color32::from_rgb(200, 160, 230),
            RegressionSnapshotTimelineKind::SchedulerEvent(_) => Color32::from_rgb(180, 220, 170),
            RegressionSnapshotTimelineKind::Status(_) => Color32::from_rgb(110, 170, 200),
        }
    }

    fn relative_secs(reference: Instant, timestamp: Instant) -> f32 {
        if timestamp >= reference {
            timestamp.duration_since(reference).as_secs_f32()
        } else {
            reference.duration_since(timestamp).as_secs_f32()
        }
    }
}

#[derive(Clone, Debug)]
struct RegressionSnapshotEventLinks {
    render_events: Vec<PdfRenderEvent>,
    throttle_events: Vec<PdfRenderThrottleEvent>,
    status_entries: Vec<StatusLogEntry>,
}

#[derive(Clone, Debug)]
struct OverlayPressureAlert {
    id: usize,
    timestamp: Instant,
    overlay_budget_pages: usize,
    highlight_page: Option<usize>,
    kind: OverlayPressureKind,
}

impl OverlayPressureAlert {
    fn new(
        id: usize,
        kind: OverlayPressureKind,
        overlay_budget_pages: usize,
        highlight_page: Option<usize>,
    ) -> Self {
        Self {
            id,
            timestamp: Instant::now(),
            overlay_budget_pages,
            highlight_page,
            kind,
        }
    }

    fn id(&self) -> usize {
        self.id
    }

    fn tranche_url(&self) -> &'static str {
        self.kind.tranche_url()
    }

    fn age_secs(&self) -> f32 {
        self.timestamp.elapsed().as_secs_f32()
    }

    fn describe(&self) -> String {
        let highlight_note = self
            .highlight_page
            .map(|page| format!(" (highlight page {})", page + 1))
            .unwrap_or_default();
        format!(
            "{}: {}{}",
            self.kind.label(),
            self.kind.detail(),
            highlight_note
        )
    }

    fn tranche_label(&self) -> &'static str {
        match self.kind {
            OverlayPressureKind::NativeRender { .. } => "PDF Subsystem (Tranche 4)",
            OverlayPressureKind::NativeEviction { .. } => "PDF Subsystem (Tranche 4)",
        }
    }
}

#[derive(Clone, Debug)]
enum OverlayPressureKind {
    NativeRender {
        span: NativeRenderSpan,
        reason_text: String,
    },
    NativeEviction {
        eviction: NativeRenderEviction,
        reason_text: String,
    },
}

impl OverlayPressureKind {
    fn label(&self) -> &'static str {
        match self {
            OverlayPressureKind::NativeRender { .. } => "Native render pressure",
            OverlayPressureKind::NativeEviction { .. } => "Eviction pressure",
        }
    }

    fn page_index(&self) -> usize {
        match self {
            OverlayPressureKind::NativeRender { span, .. } => span.page_index,
            OverlayPressureKind::NativeEviction { eviction, .. } => eviction.page_index,
        }
    }

    fn detail(&self) -> String {
        match self {
            OverlayPressureKind::NativeRender { span, reason_text } => format!(
                "{} (cache hit: {}, duration {:.2?})",
                reason_text, span.cache_hit, span.duration
            ),
            OverlayPressureKind::NativeEviction {
                eviction,
                reason_text,
            } => format!(
                "{} (target {} {}, reason {})",
                reason_text,
                eviction.target.label(),
                eviction.page_index + 1,
                eviction.reason
            ),
        }
    }

    fn badge_info(&self) -> (Color32, &'static str) {
        match self {
            OverlayPressureKind::NativeRender { .. } => (Color32::from_rgb(220, 140, 80), "RENDER"),
            OverlayPressureKind::NativeEviction { .. } => (Color32::from_rgb(220, 90, 90), "EVICT"),
        }
    }

    fn tranche_url(&self) -> &'static str {
        match self {
            OverlayPressureKind::NativeRender { .. } => PDF_SUBSYSTEM_ROADMAP_URL,
            OverlayPressureKind::NativeEviction { .. } => PDF_SUBSYSTEM_ROADMAP_URL,
        }
    }
}

impl SchedulerEventKind {
    fn describe(&self) -> String {
        match self {
            SchedulerEventKind::Eviction {
                evicted_canvas_pages,
                evicted_text_layer_pages,
            } => format!(
                "Evicted canvases: {}, text layers: {}",
                LanternLeafApp::format_pdf_page_list(evicted_canvas_pages),
                LanternLeafApp::format_pdf_page_list(evicted_text_layer_pages)
            ),
            SchedulerEventKind::RetryOverlay {
                reason,
                highlight_page,
                budget_pages,
                overlay_reason,
            } => format!(
                "Overlay retry ({}): page {}, budget {}, geometry {}",
                reason,
                highlight_page
                    .map(|idx| idx + 1)
                    .map_or("unknown".to_string(), |page| page.to_string()),
                budget_pages,
                overlay_reason.as_deref().unwrap_or("unknown")
            ),
        }
    }
}

#[derive(Debug)]
enum ScrollBlockReason {
    Duplicate,
    #[allow(dead_code)]
    Throttled(Duration),
}

enum ScrollDecision {
    Scroll,
    Blocked(ScrollBlockReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PdfConfidenceTier {
    TrustworthyText,
    MixedFuzzy,
    OcrRequired,
    RenderOnly,
}

impl PdfConfidenceTier {
    fn label(self) -> &'static str {
        match self {
            PdfConfidenceTier::TrustworthyText => "trustworthy_text",
            PdfConfidenceTier::MixedFuzzy => "mixed_fuzzy",
            PdfConfidenceTier::OcrRequired => "ocr_required",
            PdfConfidenceTier::RenderOnly => "render_only",
        }
    }

    fn badge_color(self) -> Color32 {
        match self {
            PdfConfidenceTier::TrustworthyText => Color32::from_rgb(120, 210, 160),
            PdfConfidenceTier::MixedFuzzy => Color32::from_rgb(220, 180, 120),
            PdfConfidenceTier::OcrRequired => Color32::from_rgb(220, 140, 90),
            PdfConfidenceTier::RenderOnly => Color32::from_rgb(220, 110, 110),
        }
    }
}

#[derive(Default)]
struct AutoScrollState {
    last_highlighted: Option<(usize, AnchorFallback)>,
    last_jump_at: Option<Instant>,
    throttle_blocked: usize,
    pending_auto_scroll: bool,
}

impl AutoScrollState {
    const JUMP_THROTTLE: Duration = Duration::from_millis(150);

    fn decide_scroll(&mut self, idx: usize, fallback: AnchorFallback) -> ScrollDecision {
        if self.last_highlighted == Some((idx, fallback)) {
            return ScrollDecision::Blocked(ScrollBlockReason::Duplicate);
        }
        if let Some(last) = self.last_jump_at {
            let elapsed = last.elapsed();
            if elapsed < Self::JUMP_THROTTLE {
                self.throttle_blocked = self.throttle_blocked.saturating_add(1);
                let remaining = Self::JUMP_THROTTLE - elapsed;
                return ScrollDecision::Blocked(ScrollBlockReason::Throttled(remaining));
            }
        }
        ScrollDecision::Scroll
    }

    fn note_auto_scroll(&mut self) {
        self.pending_auto_scroll = true;
    }

    fn consume_auto_scroll(&mut self) -> bool {
        let triggered = self.pending_auto_scroll;
        self.pending_auto_scroll = false;
        triggered
    }

    fn record(&mut self, idx: usize, fallback: AnchorFallback) {
        self.last_highlighted = Some((idx, fallback));
        self.last_jump_at = Some(Instant::now());
    }

    fn request_jump(&mut self) {
        self.pending_auto_scroll = true;
        self.last_highlighted = None;
        self.last_jump_at = None;
        self.throttle_blocked = 0;
    }

    fn reset(&mut self) {
        self.last_highlighted = None;
        self.last_jump_at = None;
        self.throttle_blocked = 0;
        self.pending_auto_scroll = false;
    }

    fn throttle_blocked(&self) -> usize {
        self.throttle_blocked
    }

    fn last_jump_elapsed(&self) -> Option<Duration> {
        self.last_jump_at.map(|instant| instant.elapsed())
    }
}

struct PdfRenderState {
    plan: Option<PdfViewportRenderPlan>,
    decision: Option<PdfViewportBudgetDecision>,
    visible_page_indexes: Vec<usize>,
    active_tts_page_index: Option<usize>,
    jump_target_page_index: Option<usize>,
    zoom_level: f32,
    zoom_policy: PdfZoomPolicy,
    zoom_last_request: Option<Instant>,
    zoom_throttle_blocked: usize,
    viewport_scroll_policy: PdfScrollPolicy,
    highlight_scroll_policy: PdfScrollPolicy,
    last_viewport_range: Option<PdfViewportRange>,
    last_overscan_range: Option<PdfViewportRange>,
    last_viewport_trigger: Option<PdfViewportUpdateTrigger>,
    last_viewport_update: Option<Instant>,
    last_updated: Option<Instant>,
    rendered_canvas_pages: usize,
    rendered_text_layers: usize,
    rendered_overlays: usize,
    highlighted_page: Option<usize>,
    highlighted_sentence_idx: Option<usize>,
    overlay_rects: Vec<[f32; 4]>,
    overlay_alignment_reason: Option<String>,
    overlay_anchor: Option<String>,
    overlay_alignment_source: Option<String>,
    overlay_alignment_rects: HashMap<usize, OverlayGeometryEntry>,
    render_events: Vec<PdfRenderEvent>,
    viewport_surfaces: Vec<PdfViewportSurface>,
    throttle_events: Vec<PdfRenderThrottleEvent>,
    native_render_spans: Vec<NativeRenderSpan>,
    native_eviction_events: Vec<NativeRenderEviction>,
    overlay_pressure_alerts: Vec<OverlayPressureAlert>,
    next_overlay_alert_id: usize,
    confidence_tier: Option<PdfConfidenceTier>,
    last_confidence_tier: Option<PdfConfidenceTier>,
    last_confidence_source: Option<String>,
    ocr_run_emitted_for: Option<String>,
    sync_disabled_emitted_for: Option<String>,
}

impl Default for PdfRenderState {
    fn default() -> Self {
        Self {
            plan: None,
            decision: None,
            visible_page_indexes: Vec::new(),
            active_tts_page_index: None,
            jump_target_page_index: None,
            zoom_level: crate::pdf_subsystem::PDF_DEFAULT_ZOOM_LEVEL,
            zoom_policy: PdfZoomPolicy::new(&crate::pdf_subsystem::PDF_ZOOM_LEVELS),
            zoom_last_request: None,
            zoom_throttle_blocked: 0,
            viewport_scroll_policy: PdfScrollPolicy::new(PDF_VIEWPORT_SCROLL_THRESHOLD),
            highlight_scroll_policy: PdfScrollPolicy::new(PDF_HIGHLIGHT_SCROLL_THRESHOLD),
            last_viewport_range: None,
            last_overscan_range: None,
            last_viewport_trigger: None,
            last_viewport_update: None,
            last_updated: None,
            rendered_canvas_pages: 0,
            rendered_text_layers: 0,
            rendered_overlays: 0,
            highlighted_page: None,
            highlighted_sentence_idx: None,
            overlay_rects: Vec::new(),
            overlay_alignment_reason: None,
            overlay_anchor: None,
            overlay_alignment_source: None,
            overlay_alignment_rects: HashMap::new(),
            render_events: Vec::new(),
            viewport_surfaces: Vec::new(),
            throttle_events: Vec::new(),
            native_render_spans: Vec::new(),
            native_eviction_events: Vec::new(),
            overlay_pressure_alerts: Vec::new(),
            next_overlay_alert_id: 0,
            confidence_tier: None,
            last_confidence_tier: None,
            last_confidence_source: None,
            ocr_run_emitted_for: None,
            sync_disabled_emitted_for: None,
        }
    }
}

impl PdfRenderState {
    fn reset(&mut self) {
        self.plan = None;
        self.decision = None;
        self.visible_page_indexes.clear();
        self.active_tts_page_index = None;
        self.jump_target_page_index = None;
        self.zoom_level = crate::pdf_subsystem::PDF_DEFAULT_ZOOM_LEVEL;
        self.zoom_last_request = None;
        self.zoom_throttle_blocked = 0;
        self.viewport_scroll_policy = PdfScrollPolicy::new(PDF_VIEWPORT_SCROLL_THRESHOLD);
        self.highlight_scroll_policy = PdfScrollPolicy::new(PDF_HIGHLIGHT_SCROLL_THRESHOLD);
        self.last_viewport_range = None;
        self.last_overscan_range = None;
        self.last_viewport_trigger = None;
        self.last_viewport_update = None;
        self.last_updated = None;
        self.rendered_canvas_pages = 0;
        self.rendered_text_layers = 0;
        self.rendered_overlays = 0;
        self.highlighted_page = None;
        self.highlighted_sentence_idx = None;
        self.overlay_rects.clear();
        self.overlay_alignment_reason = None;
        self.overlay_anchor = None;
        self.overlay_alignment_source = None;
        self.overlay_alignment_rects.clear();
        self.render_events.clear();
        self.viewport_surfaces.clear();
        self.throttle_events.clear();
        self.native_render_spans.clear();
        self.native_eviction_events.clear();
        self.overlay_pressure_alerts.clear();
        self.next_overlay_alert_id = 0;
        self.confidence_tier = None;
        self.last_confidence_tier = None;
        self.last_confidence_source = None;
        self.ocr_run_emitted_for = None;
        self.sync_disabled_emitted_for = None;
    }

    fn updated_age(&self) -> Option<Duration> {
        self.last_updated.map(|instant| instant.elapsed())
    }

    fn overlay_budget_pages(&self) -> usize {
        self.viewport_surfaces
            .iter()
            .filter(|surface| surface.text_layer_ready)
            .count()
    }

    fn request_zoom(&mut self, direction: PdfZoomDirection) -> PdfZoomRequestOutcome {
        let now = Instant::now();
        let throttled = self
            .zoom_last_request
            .map(|last| now.duration_since(last) < PDF_ZOOM_REQUEST_THROTTLE)
            .unwrap_or(false);
        if throttled {
            self.zoom_throttle_blocked = self.zoom_throttle_blocked.saturating_add(1);
            return PdfZoomRequestOutcome {
                previous_zoom: self.zoom_level,
                requested_zoom: self.zoom_level,
                applied: false,
                throttled: true,
            };
        }
        self.zoom_last_request = Some(now);
        let previous_zoom = self.zoom_level;
        let requested_zoom = self.zoom_policy.step_zoom(self.zoom_level, direction);
        let applied = self.apply_zoom_level(requested_zoom);
        PdfZoomRequestOutcome {
            previous_zoom,
            requested_zoom,
            applied,
            throttled: false,
        }
    }

    fn apply_zoom_level(&mut self, new_zoom: f32) -> bool {
        if (self.zoom_level - new_zoom).abs() <= f32::EPSILON {
            return false;
        }
        self.zoom_level = new_zoom;
        for surface in self.viewport_surfaces.iter_mut() {
            surface.canvas_texture = None;
            surface.text_layer_texture = None;
        }
        self.last_viewport_update = None;
        true
    }

    fn zoom_throttle_blocked(&self) -> usize {
        self.zoom_throttle_blocked
    }

    fn should_commit_viewport_update(
        &mut self,
        visible_range: Option<PdfViewportRange>,
        overscan_range: Option<PdfViewportRange>,
        trigger: PdfViewportUpdateTrigger,
    ) -> bool {
        let same_target =
            visible_range == self.last_viewport_range && overscan_range == self.last_overscan_range;
        let forced = matches!(trigger, PdfViewportUpdateTrigger::Jump);
        if same_target && !forced {
            let span = tracing::span!(
                Level::TRACE,
                "pdf.viewport.ignore",
                visible_range = ?visible_range,
                overscan_range = ?overscan_range,
                trigger = ?trigger,
                reason = "repeat_target"
            );
            let _enter = span.enter();
            trace!("PDF viewport update ignored (repeat target)");
            return false;
        }
        if matches!(trigger, PdfViewportUpdateTrigger::Scroll) {
            if let Some(range) = visible_range {
                let allowed = self.viewport_scroll_policy.should_scroll(range.start);
                if !allowed {
                    let span = tracing::span!(
                        Level::TRACE,
                        "pdf.viewport.ignore",
                        visible_range = ?visible_range,
                        overscan_range = ?overscan_range,
                        trigger = ?trigger,
                        reason = "scroll_threshold"
                    );
                    let _enter = span.enter();
                    trace!("PDF viewport update ignored (scroll threshold)");
                    return false;
                }
            }
        }
        true
    }

    fn should_scroll_to_page(
        &mut self,
        target_page: usize,
        sentence_idx: Option<usize>,
        reason: &'static str,
    ) -> bool {
        let allowed = self.highlight_scroll_policy.should_scroll(target_page);
        let span = tracing::span!(
            Level::TRACE,
            "pdf.highlight.scroll",
            reason,
            target_page = target_page + 1,
            sentence_idx,
            allowed,
            threshold_pages = self.highlight_scroll_policy.threshold_pages()
        );
        let _enter = span.enter();
        trace!("PDF highlight scroll evaluated");
        allowed
    }

    fn update_confidence_tier(
        &mut self,
        tier: Option<PdfConfidenceTier>,
        source_path: &str,
    ) -> bool {
        let changed = self.last_confidence_source.as_deref() != Some(source_path)
            || self.last_confidence_tier != tier;
        self.confidence_tier = tier;
        if changed {
            self.last_confidence_source = Some(source_path.to_string());
            self.last_confidence_tier = tier;
        }
        changed
    }

    fn record_render_metrics(&mut self, canvas_pages: usize, text_layers: usize, overlays: usize) {
        self.rendered_canvas_pages = canvas_pages;
        self.rendered_text_layers = text_layers;
        self.rendered_overlays = overlays;
    }

    fn record_render_event(&mut self, event: PdfRenderEvent) {
        const MAX_RENDER_EVENTS: usize = 16;
        self.render_events.push(event);
        if self.render_events.len() > MAX_RENDER_EVENTS {
            self.render_events.remove(0);
        }
    }

    fn record_native_render_span(&mut self, span: NativeRenderSpan) {
        const MAX_NATIVE_SPANS: usize = 16;
        self.native_render_spans.push(span);
        if self.native_render_spans.len() > MAX_NATIVE_SPANS {
            self.native_render_spans.remove(0);
        }
    }

    fn record_native_eviction(&mut self, event: NativeRenderEviction) {
        const MAX_NATIVE_EVICTIONS: usize = 12;
        self.native_eviction_events.push(event);
        if self.native_eviction_events.len() > MAX_NATIVE_EVICTIONS {
            self.native_eviction_events.remove(0);
        }
    }

    fn recent_render_events(&self) -> &[PdfRenderEvent] {
        &self.render_events
    }

    fn recent_native_render_spans(&self) -> &[NativeRenderSpan] {
        &self.native_render_spans
    }

    fn recent_native_evictions(&self) -> &[NativeRenderEviction] {
        &self.native_eviction_events
    }

    fn record_overlay_pressure_alert(&mut self, alert: OverlayPressureAlert) {
        const MAX_OVERLAY_PRESSURE_ALERTS: usize = 12;
        self.overlay_pressure_alerts.push(alert);
        if self.overlay_pressure_alerts.len() > MAX_OVERLAY_PRESSURE_ALERTS {
            self.overlay_pressure_alerts.remove(0);
        }
    }

    fn allocate_overlay_alert_id(&mut self) -> usize {
        let id = self.next_overlay_alert_id;
        self.next_overlay_alert_id = self.next_overlay_alert_id.wrapping_add(1);
        id
    }

    fn recent_overlay_pressure_alerts(&self) -> &[OverlayPressureAlert] {
        &self.overlay_pressure_alerts
    }

    fn record_throttle_event(&mut self, event: PdfRenderThrottleEvent) {
        const MAX_THROTTLE_EVENTS: usize = 12;
        self.throttle_events.push(event);
        if self.throttle_events.len() > MAX_THROTTLE_EVENTS {
            self.throttle_events.remove(0);
        }
    }

    fn recent_throttle_events(&self) -> &[PdfRenderThrottleEvent] {
        &self.throttle_events
    }

    fn update_surfaces(&mut self, plan: &PdfViewportRenderPlan) {
        let mut surfaces_map: HashMap<usize, PdfViewportSurface> = self
            .viewport_surfaces
            .drain(..)
            .map(|surface| (surface.page_index, surface))
            .collect();
        for &page in plan.canvas_page_indexes.iter() {
            surfaces_map
                .entry(page)
                .or_insert_with(|| PdfViewportSurface::new(page))
                .canvas_ready = true;
        }
        for &page in plan.text_layer_page_indexes.iter() {
            surfaces_map
                .entry(page)
                .or_insert_with(|| PdfViewportSurface::new(page))
                .text_layer_ready = true;
        }
        for &page in plan.priority_page_indexes.iter() {
            surfaces_map
                .entry(page)
                .or_insert_with(|| PdfViewportSurface::new(page));
        }
        let mut surfaces = surfaces_map.into_values().collect::<Vec<_>>();
        surfaces.sort_by_key(|surface| surface.page_index);
        self.viewport_surfaces = surfaces;
    }

    fn apply_budget_evictions(&mut self, decision: &PdfViewportBudgetDecision) {
        for surface in self.viewport_surfaces.iter_mut() {
            if decision
                .evict_canvas_page_indexes
                .contains(&surface.page_index)
            {
                surface.canvas_ready = false;
                surface.canvas_texture = None;
            }
            if decision
                .evict_text_layer_page_indexes
                .contains(&surface.page_index)
            {
                surface.text_layer_ready = false;
                surface.text_layer_texture = None;
            }
        }
    }

    fn surface_for_page(&self, page: usize) -> Option<&PdfViewportSurface> {
        self.viewport_surfaces
            .iter()
            .find(|surface| surface.page_index == page)
    }

    fn is_canvas_evicted(&self, page_index: usize) -> bool {
        self.decision
            .as_ref()
            .map(|decision| decision.evict_canvas_page_indexes.contains(&page_index))
            .unwrap_or(false)
    }

    fn is_text_layer_evicted(&self, page_index: usize) -> bool {
        self.decision
            .as_ref()
            .map(|decision| decision.evict_text_layer_page_indexes.contains(&page_index))
            .unwrap_or(false)
    }

    fn set_highlighted_page(
        &mut self,
        page_index: usize,
        sentence_idx: Option<usize>,
        overlay_rects: Vec<[f32; 4]>,
        overlay_reason: Option<String>,
        overlay_anchor: Option<String>,
    ) {
        if self.highlighted_page == Some(page_index)
            && self.highlighted_sentence_idx == sentence_idx
        {
            return;
        }
        if let Some(prev_page) = self.highlighted_page {
            let reason = if prev_page != page_index {
                "page_change"
            } else if self.highlighted_sentence_idx != sentence_idx {
                "new_sentence"
            } else {
                "refresh"
            };
            let cleanup_span = tracing::span!(
                Level::TRACE,
                "pdf.highlight.cleanup",
                reason,
                page = prev_page + 1,
                sentence_idx = self.highlighted_sentence_idx,
                rect_count = self.overlay_rects.len(),
                highlight_anchor = self.overlay_anchor.as_deref().unwrap_or("unknown"),
                confidence_tier = self
                    .confidence_tier
                    .map(PdfConfidenceTier::label)
                    .unwrap_or("unknown")
            );
            let _enter = cleanup_span.enter();
            trace!("Cleared PDF highlight overlay");
        }
        self.highlighted_page = Some(page_index);
        self.highlighted_sentence_idx = sentence_idx;
        self.overlay_rects = if overlay_rects.is_empty() {
            Self::generate_overlay_rects(sentence_idx)
        } else {
            overlay_rects
        };
        self.overlay_alignment_reason = if self.overlay_rects.is_empty() {
            None
        } else {
            overlay_reason
        };
        self.overlay_anchor = overlay_anchor.clone();
        let apply_span = tracing::span!(
            Level::TRACE,
            "pdf.highlight.apply",
            page = page_index + 1,
            sentence_idx,
            rect_count = self.overlay_rects.len(),
            highlight_anchor = self.overlay_anchor.as_deref().unwrap_or("unknown"),
            overlay_reason = self.overlay_alignment_reason.as_deref().unwrap_or("none"),
            confidence_tier = self
                .confidence_tier
                .map(PdfConfidenceTier::label)
                .unwrap_or("unknown")
        );
        let _enter = apply_span.enter();
        trace!("Applied PDF highlight overlay");
        if let Some(surface) = self
            .viewport_surfaces
            .iter_mut()
            .find(|surface| surface.page_index == page_index)
        {
            surface.overlay_rects = self.overlay_rects.clone();
            surface.overlay_reason = self.overlay_alignment_reason.clone();
            surface.overlay_anchor = overlay_anchor;
        }
    }

    fn generate_overlay_rects(sentence_idx: Option<usize>) -> Vec<[f32; 4]> {
        let count = sentence_idx.map(|idx| (idx % 3) + 1).unwrap_or(0);
        (0..count)
            .map(|i| {
                let width = 0.8 - (i as f32 * 0.15);
                let height = 0.12;
                let left = 0.1 + (i as f32 * 0.05);
                let top = 0.15 + (i as f32 * 0.18);
                let right = (left + width).min(0.95);
                let bottom = (top + height).min(0.9);
                [left, top, right, bottom]
            })
            .collect()
    }

    fn overlay_geometry_for_sentence(
        &mut self,
        cache_service: &dyn cache_service::CacheService,
        source_path: &str,
        sentence_idx: usize,
    ) -> Option<OverlayGeometryEntry> {
        self.ensure_alignment_cache(cache_service, source_path);
        let entry = self.overlay_alignment_rects.get(&sentence_idx).cloned();
        let fallback_path = entry
            .as_ref()
            .map(|value| value.anchor_label.as_str())
            .unwrap_or("render_only");
        let span = tracing::span!(Level::TRACE, "pdf.text.sync", sentence_idx, fallback_path);
        let _enter = span.enter();
        trace!("PDF text sync evaluated");
        entry
    }

    fn ensure_alignment_cache(
        &mut self,
        cache_service: &dyn cache_service::CacheService,
        source_path: &str,
    ) {
        if self.overlay_alignment_source.as_deref() == Some(source_path) {
            return;
        }
        self.overlay_alignment_source = Some(source_path.to_string());
        self.overlay_alignment_rects.clear();
        let path = Path::new(source_path);
        if let Some(artifact) = cache_service.load_pdf_ocr_alignment_artifact(path) {
            let span = tracing::span!(
                Level::TRACE,
                "pdf.ocr.load",
                quality_class = ?artifact.quality_class,
                source_kind = ?artifact.source_kind,
                sentence_count = artifact.sentence_count,
                mapped_sentence_count = artifact.mapped_sentence_count,
                highlightable_sentence_count = artifact.highlightable_sentence_count,
                alignment_build_ms = artifact.alignment_build_ms
            );
            let _enter = span.enter();
            trace!("Loaded PDF OCR alignment artifact");
            for alignment in artifact.alignments.iter() {
                if alignment.page_idx.is_none() {
                    continue;
                }
                if let Some(entry) = OverlayGeometryEntry::from_alignment(alignment) {
                    self.overlay_alignment_rects
                        .insert(alignment.sentence_idx, entry);
                }
            }
        }
    }

    fn alignment_fallback_label(alignment: &cache::PdfOcrSentenceAlignment) -> &'static str {
        if !alignment.rects.is_empty() {
            "exact"
        } else if !alignment.line_rects.is_empty() {
            "line"
        } else if !alignment.block_rects.is_empty() {
            "block"
        } else if alignment.page_idx.is_some() {
            "page"
        } else {
            "render_only"
        }
    }

    fn alignment_rects(alignment: &cache::PdfOcrSentenceAlignment) -> Vec<[f32; 4]> {
        let geometry = if !alignment.rects.is_empty() {
            &alignment.rects
        } else if !alignment.line_rects.is_empty() {
            &alignment.line_rects
        } else {
            &alignment.block_rects
        };
        geometry
            .iter()
            .map(|rect| {
                let left = rect.left.clamp(0.0, 1.0);
                let top = rect.top.clamp(0.0, 1.0);
                let right = (rect.left + rect.width).clamp(0.0, 1.0);
                let bottom = (rect.top + rect.height).clamp(0.0, 1.0);
                [left, top, right, bottom]
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy)]
struct PdfZoomRequestOutcome {
    previous_zoom: f32,
    requested_zoom: f32,
    applied: bool,
    throttled: bool,
}

#[derive(Clone)]
struct OverlayGeometryEntry {
    rects: Vec<[f32; 4]>,
    reason: Option<String>,
    anchor_label: String,
}

impl OverlayGeometryEntry {
    fn new(rects: Vec<[f32; 4]>, reason: Option<String>, anchor_label: String) -> Self {
        Self {
            rects,
            reason,
            anchor_label,
        }
    }

    fn from_alignment(alignment: &cache::PdfOcrSentenceAlignment) -> Option<Self> {
        let rects = PdfRenderState::alignment_rects(alignment);
        if rects.is_empty() {
            return None;
        }
        let reason_text = alignment.fallback_reason.trim();
        let reason = if reason_text.is_empty() {
            None
        } else {
            Some(reason_text.to_string())
        };
        let anchor_label = PdfRenderState::alignment_fallback_label(alignment).to_string();
        Some(Self::new(rects, reason, anchor_label))
    }
}

#[derive(Clone)]
struct PdfViewportSurface {
    page_index: usize,
    canvas_ready: bool,
    text_layer_ready: bool,
    canvas_texture: Option<TextureHandle>,
    text_layer_texture: Option<TextureHandle>,
    overlay_rects: Vec<[f32; 4]>,
    overlay_reason: Option<String>,
    overlay_anchor: Option<String>,
}

impl PdfViewportSurface {
    fn new(page_index: usize) -> Self {
        Self {
            page_index,
            canvas_ready: false,
            text_layer_ready: false,
            canvas_texture: None,
            text_layer_texture: None,
            overlay_rects: Vec::new(),
            overlay_reason: None,
            overlay_anchor: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum PdfRenderEventKind {
    Canvas,
    TextLayer,
    Overlay,
}

#[derive(Clone, Debug)]
struct PdfRenderEvent {
    timestamp: Instant,
    kind: PdfRenderEventKind,
    page_index: usize,
    highlight_page: bool,
    overlay_budget_pages: usize,
    overlays_drawn: usize,
    overlay_reason: Option<String>,
}

impl PdfRenderEvent {
    fn canvas(page_index: usize, highlight_page: bool, overlay_budget_pages: usize) -> Self {
        Self {
            timestamp: Instant::now(),
            kind: PdfRenderEventKind::Canvas,
            page_index,
            highlight_page,
            overlay_budget_pages,
            overlays_drawn: 0,
            overlay_reason: None,
        }
    }

    fn text_layer(page_index: usize, highlight_page: bool, overlay_budget_pages: usize) -> Self {
        Self {
            timestamp: Instant::now(),
            kind: PdfRenderEventKind::TextLayer,
            page_index,
            highlight_page,
            overlay_budget_pages,
            overlays_drawn: 0,
            overlay_reason: None,
        }
    }

    fn overlay(
        page_index: usize,
        overlays_drawn: usize,
        overlay_budget_pages: usize,
        overlay_reason: Option<String>,
    ) -> Self {
        Self {
            timestamp: Instant::now(),
            kind: PdfRenderEventKind::Overlay,
            page_index,
            highlight_page: true,
            overlay_budget_pages,
            overlays_drawn,
            overlay_reason,
        }
    }

    fn describe(&self) -> String {
        match self.kind {
            PdfRenderEventKind::Canvas => format!(
                "Canvas render: page {}{} (budget {} pages)",
                self.page_index + 1,
                if self.highlight_page {
                    " (highlight)"
                } else {
                    ""
                },
                self.overlay_budget_pages
            ),
            PdfRenderEventKind::TextLayer => format!(
                "Text layer render: page {}{} (budget {} pages)",
                self.page_index + 1,
                if self.highlight_page {
                    " (highlight)"
                } else {
                    ""
                },
                self.overlay_budget_pages
            ),
            PdfRenderEventKind::Overlay => format!(
                "Overlay render: page {} (rects {}, reason {}, budget {} pages)",
                self.page_index + 1,
                self.overlays_drawn,
                self.overlay_reason.as_deref().unwrap_or("unknown"),
                self.overlay_budget_pages
            ),
        }
    }

    fn age_secs(&self) -> f32 {
        self.timestamp.elapsed().as_secs_f32()
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum PdfRenderThrottleKind {
    Canvas,
    TextLayer,
    Overlay,
}

#[derive(Clone, Debug)]
struct PdfRenderThrottleEvent {
    timestamp: Instant,
    kind: PdfRenderThrottleKind,
    page_index: usize,
    reason: String,
}

impl PdfRenderThrottleEvent {
    fn new(kind: PdfRenderThrottleKind, page_index: usize, reason: String) -> Self {
        Self {
            timestamp: Instant::now(),
            kind,
            page_index,
            reason,
        }
    }

    fn describe(&self) -> String {
        format!(
            "{} throttle: page {}, {}",
            match self.kind {
                PdfRenderThrottleKind::Canvas => "Canvas",
                PdfRenderThrottleKind::TextLayer => "Text layer",
                PdfRenderThrottleKind::Overlay => "Overlay",
            },
            self.page_index + 1,
            self.reason
        )
    }

    fn age_secs(&self) -> f32 {
        self.timestamp.elapsed().as_secs_f32()
    }
}
