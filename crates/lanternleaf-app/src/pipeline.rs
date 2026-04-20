use crate::contracts::{
    BootstrapState, BridgeError, BrowserTabsHealth, BrowserTabsTab, BrowserTabsWindow,
    CalibreBookDto, CalibreLoadEvent, LogLevelEvent, OpenSourceResult, PdfTranscriptionEvent,
    ReaderPlaybackStateEvent, ReaderStateEvent, RecentBook, SessionState, SessionStateEvent,
    SourceOpenEvent, TtsStateEvent, UiMode,
};
use crate::logging::{command_span, event_span};
use crate::state::{AppState, OperationState, RuntimeJobPatch, derive_reader_playback};
use lanternleaf_core::{cache, session};
use tracing::{debug, trace, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationScope {
    SourceOpen,
    StarterCommand,
    ReaderCommand,
    ReaderTts,
    ReaderSettings,
    BrowserTabRefresh,
    CalibreLoad,
    RuntimeConfig,
}

#[derive(Debug, Clone)]
pub enum AppCommand {
    Bootstrap,
    RefreshRecents {
        limit: Option<usize>,
    },
    OpenSourcePath {
        path: String,
    },
    OpenClipboard,
    OpenClipboardText {
        text: String,
    },
    LoadBrowserTabsHealth,
    ListBrowserTabWindows,
    ListBrowserTabs {
        window_id: Option<u64>,
        query: Option<String>,
        refresh: bool,
    },
    OpenBrowserTab {
        tab_id: u64,
        window_id: Option<u64>,
    },
    OpenBrowserTabBundle {
        tab_id: u64,
        window_id: Option<u64>,
    },
    RefreshBrowserTab {
        tab_id: u64,
        window_id: Option<u64>,
    },
    DeleteRecent {
        source_path: String,
        close_browser_tab: bool,
    },
    CloseRecentBrowserTab {
        source_path: String,
    },
    ReturnToStarter,
    CloseReaderSession,
    ToggleTheme,
    ToggleSettingsPanel,
    ToggleStatsPanel,
    ToggleTtsPanel,
    Reader(ReaderCommand),
    LoadCalibreBooks {
        force_refresh: bool,
    },
    OpenCalibreBook {
        id: u64,
    },
    EnsureCalibreThumbnail {
        id: u64,
    },
    SetRuntimeLogLevel {
        level: String,
    },
    FlushPersistence {
        trigger: PersistenceTrigger,
    },
    SafeQuit,
}

impl AppCommand {
    pub fn action(&self) -> &'static str {
        match self {
            Self::Bootstrap => "session_get_bootstrap",
            Self::RefreshRecents { .. } => "recent_list",
            Self::OpenSourcePath { .. } => "source_open_path",
            Self::OpenClipboard => "source_open_clipboard",
            Self::OpenClipboardText { .. } => "source_open_clipboard_text",
            Self::LoadBrowserTabsHealth => "browser_tabs_health",
            Self::ListBrowserTabWindows => "browser_tabs_list_windows",
            Self::ListBrowserTabs { .. } => "browser_tabs_list_tabs",
            Self::OpenBrowserTab { .. } => "source_open_browser_tab",
            Self::OpenBrowserTabBundle { .. } => "source_open_browser_tab_bundle",
            Self::RefreshBrowserTab { .. } => "source_refresh_browser_tab",
            Self::DeleteRecent { .. } => "recent_delete",
            Self::CloseRecentBrowserTab { .. } => "recent_close_browser_tab",
            Self::ReturnToStarter => "session_return_to_starter",
            Self::CloseReaderSession => "reader_close_session",
            Self::ToggleTheme => "session_toggle_theme",
            Self::ToggleSettingsPanel => "panel_toggle_settings",
            Self::ToggleStatsPanel => "panel_toggle_stats",
            Self::ToggleTtsPanel => "panel_toggle_tts",
            Self::Reader(command) => command.action(),
            Self::LoadCalibreBooks { force_refresh } => {
                if *force_refresh {
                    "calibre_load_books"
                } else {
                    "calibre_load_cached_books"
                }
            }
            Self::OpenCalibreBook { .. } => "calibre_open_book",
            Self::EnsureCalibreThumbnail { .. } => "calibre_ensure_thumbnail",
            Self::SetRuntimeLogLevel { .. } => "logging_set_level",
            Self::FlushPersistence { .. } => "persist_runtime_state",
            Self::SafeQuit => "app_safe_quit",
        }
    }
}

#[derive(Debug, Clone)]
pub enum ReaderCommand {
    Session(session::SessionCommand),
    PrecomputeTtsPage,
    LoadPdfBytes {
        path: String,
    },
    LoadPdfSyncMap {
        path: String,
    },
    PersistPdfSyncMap {
        path: String,
        locations: Vec<cache::PdfSentenceLocation>,
    },
    LoadPdfRenderPrecomputed {
        path: String,
    },
}

impl ReaderCommand {
    pub fn action(&self) -> &'static str {
        match self {
            Self::Session(command) => command.action(),
            Self::PrecomputeTtsPage => "reader_tts_precompute_page",
            Self::LoadPdfBytes { .. } => "reader_load_pdf_bytes",
            Self::LoadPdfSyncMap { .. } => "reader_load_pdf_sync_map",
            Self::PersistPdfSyncMap { .. } => "reader_persist_pdf_sync_map",
            Self::LoadPdfRenderPrecomputed { .. } => "reader_load_pdf_render_precomputed",
        }
    }

    pub fn operation_scope(&self) -> OperationScope {
        match self {
            Self::Session(session::SessionCommand::ApplySettings { .. }) => {
                OperationScope::ReaderSettings
            }
            Self::Session(session::SessionCommand::TtsPlay)
            | Self::Session(session::SessionCommand::TtsPause)
            | Self::Session(session::SessionCommand::TtsTogglePlayPause)
            | Self::Session(session::SessionCommand::TtsPlayFromPageStart)
            | Self::Session(session::SessionCommand::TtsPlayFromHighlight)
            | Self::Session(session::SessionCommand::TtsSeekNext)
            | Self::Session(session::SessionCommand::TtsSeekPrev)
            | Self::Session(session::SessionCommand::TtsRepeatSentence)
            | Self::Session(session::SessionCommand::TtsStop)
            | Self::PrecomputeTtsPage => OperationScope::ReaderTts,
            Self::LoadPdfBytes { .. }
            | Self::LoadPdfSyncMap { .. }
            | Self::PersistPdfSyncMap { .. }
            | Self::LoadPdfRenderPrecomputed { .. } => OperationScope::ReaderCommand,
            Self::Session(_) => OperationScope::ReaderCommand,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtsSyncPolicy {
    KeepRuntimeState,
    SyncAfterCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistenceTrigger {
    SourceOpen,
    SessionClose,
    ReaderCommand,
    RuntimeConfigChange,
    SafeQuit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistenceOutcome {
    Completed,
    SkippedNoSession,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectOwner {
    AppShell,
    RecentBooks,
    SourceOpen,
    BrowserTabs,
    ReaderSession,
    PdfArtifacts,
    Logging,
    Calibre,
    Persistence,
}

#[derive(Debug, Clone)]
pub enum RuntimeEffect {
    LoadBootstrap,
    ListRecents {
        limit: Option<usize>,
    },
    DeleteRecent {
        source_path: String,
    },
    CloseRecentBrowserTab {
        source_path: String,
    },
    OpenSourcePath {
        path: String,
    },
    OpenClipboard,
    OpenClipboardText {
        text: String,
    },
    LoadBrowserTabsHealth,
    ListBrowserTabWindows,
    ListBrowserTabs {
        window_id: Option<u64>,
        query: Option<String>,
        refresh: bool,
    },
    OpenBrowserTab {
        tab_id: u64,
        window_id: Option<u64>,
    },
    OpenBrowserTabBundle {
        tab_id: u64,
        window_id: Option<u64>,
    },
    RefreshBrowserTab {
        tab_id: u64,
        window_id: Option<u64>,
    },
    ReturnToStarter,
    CloseReaderSession,
    ToggleTheme,
    TogglePanel {
        panel: PanelToggle,
    },
    ApplyReaderCommand {
        command: session::SessionCommand,
        sync_tts: TtsSyncPolicy,
    },
    PrecomputeTtsPage,
    LoadPdfBytes {
        path: String,
    },
    LoadPdfSyncMap {
        path: String,
    },
    PersistPdfSyncMap {
        path: String,
        locations: Vec<cache::PdfSentenceLocation>,
    },
    LoadPdfRenderPrecomputed {
        path: String,
    },
    LoadCalibreCachedBooks,
    LoadCalibreBooks {
        force_refresh: bool,
    },
    OpenCalibreBook {
        id: u64,
    },
    EnsureCalibreThumbnail {
        id: u64,
    },
    SetRuntimeLogLevel {
        level: String,
    },
    FlushPersistence {
        trigger: PersistenceTrigger,
    },
    SafeQuit,
}

impl RuntimeEffect {
    pub fn owner(&self) -> EffectOwner {
        match self {
            Self::LoadBootstrap
            | Self::ReturnToStarter
            | Self::ToggleTheme
            | Self::TogglePanel { .. }
            | Self::SafeQuit => EffectOwner::AppShell,
            Self::ListRecents { .. } | Self::DeleteRecent { .. } => EffectOwner::RecentBooks,
            Self::OpenSourcePath { .. } | Self::OpenClipboard | Self::OpenClipboardText { .. } => {
                EffectOwner::SourceOpen
            }
            Self::OpenBrowserTab { .. }
            | Self::OpenBrowserTabBundle { .. }
            | Self::RefreshBrowserTab { .. }
            | Self::CloseRecentBrowserTab { .. }
            | Self::LoadBrowserTabsHealth
            | Self::ListBrowserTabWindows
            | Self::ListBrowserTabs { .. } => EffectOwner::BrowserTabs,
            Self::ApplyReaderCommand { .. }
            | Self::PrecomputeTtsPage
            | Self::CloseReaderSession => EffectOwner::ReaderSession,
            Self::LoadPdfBytes { .. }
            | Self::LoadPdfSyncMap { .. }
            | Self::PersistPdfSyncMap { .. }
            | Self::LoadPdfRenderPrecomputed { .. } => EffectOwner::PdfArtifacts,
            Self::SetRuntimeLogLevel { .. } => EffectOwner::Logging,
            Self::LoadCalibreCachedBooks
            | Self::LoadCalibreBooks { .. }
            | Self::OpenCalibreBook { .. }
            | Self::EnsureCalibreThumbnail { .. } => EffectOwner::Calibre,
            Self::FlushPersistence { .. } => EffectOwner::Persistence,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelToggle {
    Settings,
    Stats,
    Tts,
}

#[derive(Debug, Clone)]
pub struct PlannedEffect {
    pub request_id: u64,
    pub effect: RuntimeEffect,
}

impl PlannedEffect {
    pub fn owner(&self) -> EffectOwner {
        self.effect.owner()
    }
}

#[derive(Debug, Clone)]
pub struct DispatchPlan {
    pub request_id: u64,
    pub action: &'static str,
    pub local_events: Vec<AppEvent>,
    pub effects: Vec<PlannedEffect>,
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    OperationChanged {
        scope: OperationScope,
        active: bool,
    },
    LoadingBootstrapChanged(bool),
    LoadingRecentsChanged(bool),
    LoadingCalibreChanged(bool),
    LoadingBrowserTabsChanged(bool),
    BootstrapLoaded {
        request_id: u64,
        bootstrap: BootstrapState,
    },
    SessionUpdated(SessionStateEvent),
    ReaderUpdated(ReaderStateEvent),
    ReaderPlaybackUpdated(ReaderPlaybackStateEvent),
    SourceOpenProgress(SourceOpenEvent),
    SourceOpened {
        request_id: u64,
        result: OpenSourceResult,
    },
    RecentsLoaded {
        request_id: u64,
        recents: Vec<RecentBook>,
    },
    CalibreBooksLoaded {
        request_id: u64,
        books: Vec<CalibreBookDto>,
        from_cache: bool,
    },
    BrowserTabsHealthLoaded {
        request_id: u64,
        health: BrowserTabsHealth,
    },
    BrowserTabWindowsLoaded {
        request_id: u64,
        windows: Vec<BrowserTabsWindow>,
    },
    BrowserTabsLoaded {
        request_id: u64,
        tabs: Vec<BrowserTabsTab>,
    },
    CalibreLoadProgress(CalibreLoadEvent),
    TtsStateUpdated(TtsStateEvent),
    PdfTranscriptionProgress(PdfTranscriptionEvent),
    LogLevelUpdated(LogLevelEvent),
    NotificationRaised {
        request_id: u64,
        notification: crate::state::Notification,
    },
    NotificationDismissed {
        request_id: u64,
        notification_id: u64,
    },
    PersistenceFlushed {
        request_id: u64,
        trigger: PersistenceTrigger,
        outcome: PersistenceOutcome,
    },
    CommandFailed {
        request_id: u64,
        scope: Option<OperationScope>,
        error: BridgeError,
    },
    RemotePlaybackStateUpdated(crate::contracts::ReaderPlaybackState),
}

pub fn plan_command(state: &AppState, request_id: u64, command: AppCommand) -> DispatchPlan {
    let action = command.action();
    let span = command_span(request_id, &command);
    let _guard = span.enter();
    trace!(action = action, "Planning app command");
    let (local_events, effects) = match command {
        AppCommand::Bootstrap => (
            vec![
                AppEvent::LoadingBootstrapChanged(true),
                AppEvent::OperationChanged {
                    scope: OperationScope::RuntimeConfig,
                    active: true,
                },
            ],
            vec![RuntimeEffect::LoadBootstrap],
        ),
        AppCommand::RefreshRecents { limit } => (
            vec![
                AppEvent::LoadingRecentsChanged(true),
                AppEvent::OperationChanged {
                    scope: OperationScope::StarterCommand,
                    active: true,
                },
            ],
            vec![RuntimeEffect::ListRecents { limit }],
        ),
        AppCommand::OpenSourcePath { path } => (
            vec![AppEvent::OperationChanged {
                scope: OperationScope::SourceOpen,
                active: true,
            }],
            vec![RuntimeEffect::OpenSourcePath { path }],
        ),
        AppCommand::OpenClipboard => (
            vec![AppEvent::OperationChanged {
                scope: OperationScope::SourceOpen,
                active: true,
            }],
            vec![RuntimeEffect::OpenClipboard],
        ),
        AppCommand::OpenClipboardText { text } => (
            vec![AppEvent::OperationChanged {
                scope: OperationScope::SourceOpen,
                active: true,
            }],
            vec![RuntimeEffect::OpenClipboardText { text }],
        ),
        AppCommand::LoadBrowserTabsHealth => (
            vec![
                AppEvent::LoadingBrowserTabsChanged(true),
                AppEvent::OperationChanged {
                    scope: OperationScope::BrowserTabRefresh,
                    active: true,
                },
            ],
            vec![RuntimeEffect::LoadBrowserTabsHealth],
        ),
        AppCommand::ListBrowserTabWindows => (
            vec![
                AppEvent::LoadingBrowserTabsChanged(true),
                AppEvent::OperationChanged {
                    scope: OperationScope::BrowserTabRefresh,
                    active: true,
                },
            ],
            vec![RuntimeEffect::ListBrowserTabWindows],
        ),
        AppCommand::ListBrowserTabs {
            window_id,
            query,
            refresh,
        } => (
            vec![
                AppEvent::LoadingBrowserTabsChanged(true),
                AppEvent::OperationChanged {
                    scope: OperationScope::BrowserTabRefresh,
                    active: true,
                },
            ],
            vec![RuntimeEffect::ListBrowserTabs {
                window_id,
                query,
                refresh,
            }],
        ),
        AppCommand::OpenBrowserTab { tab_id, window_id } => (
            vec![AppEvent::OperationChanged {
                scope: OperationScope::SourceOpen,
                active: true,
            }],
            vec![RuntimeEffect::OpenBrowserTab { tab_id, window_id }],
        ),
        AppCommand::OpenBrowserTabBundle { tab_id, window_id } => (
            vec![AppEvent::OperationChanged {
                scope: OperationScope::SourceOpen,
                active: true,
            }],
            vec![RuntimeEffect::OpenBrowserTabBundle { tab_id, window_id }],
        ),
        AppCommand::RefreshBrowserTab { tab_id, window_id } => (
            vec![AppEvent::OperationChanged {
                scope: OperationScope::BrowserTabRefresh,
                active: true,
            }],
            vec![RuntimeEffect::RefreshBrowserTab { tab_id, window_id }],
        ),
        AppCommand::DeleteRecent {
            source_path,
            close_browser_tab,
        } => {
            let mut effects = vec![RuntimeEffect::DeleteRecent {
                source_path: source_path.clone(),
            }];
            if close_browser_tab {
                effects.push(RuntimeEffect::CloseRecentBrowserTab { source_path });
            }
            (
                vec![AppEvent::OperationChanged {
                    scope: OperationScope::StarterCommand,
                    active: true,
                }],
                effects,
            )
        }
        AppCommand::CloseRecentBrowserTab { source_path } => (
            vec![AppEvent::OperationChanged {
                scope: OperationScope::BrowserTabRefresh,
                active: true,
            }],
            vec![RuntimeEffect::CloseRecentBrowserTab { source_path }],
        ),
        AppCommand::ReturnToStarter => (
            vec![AppEvent::OperationChanged {
                scope: OperationScope::ReaderCommand,
                active: true,
            }],
            vec![RuntimeEffect::ReturnToStarter],
        ),
        AppCommand::CloseReaderSession => (
            vec![AppEvent::OperationChanged {
                scope: OperationScope::ReaderCommand,
                active: true,
            }],
            vec![RuntimeEffect::CloseReaderSession],
        ),
        AppCommand::ToggleTheme => (
            vec![AppEvent::OperationChanged {
                scope: OperationScope::RuntimeConfig,
                active: true,
            }],
            vec![RuntimeEffect::ToggleTheme],
        ),
        AppCommand::ToggleSettingsPanel => (
            vec![AppEvent::OperationChanged {
                scope: OperationScope::ReaderCommand,
                active: true,
            }],
            vec![RuntimeEffect::TogglePanel {
                panel: PanelToggle::Settings,
            }],
        ),
        AppCommand::ToggleStatsPanel => (
            vec![AppEvent::OperationChanged {
                scope: OperationScope::ReaderCommand,
                active: true,
            }],
            vec![RuntimeEffect::TogglePanel {
                panel: PanelToggle::Stats,
            }],
        ),
        AppCommand::ToggleTtsPanel => (
            vec![AppEvent::OperationChanged {
                scope: OperationScope::ReaderCommand,
                active: true,
            }],
            vec![RuntimeEffect::TogglePanel {
                panel: PanelToggle::Tts,
            }],
        ),
        AppCommand::Reader(command) => {
            let scope = command.operation_scope();
            let effect = match command {
                ReaderCommand::Session(session_command) => RuntimeEffect::ApplyReaderCommand {
                    sync_tts: tts_sync_policy(&session_command),
                    command: session_command,
                },
                ReaderCommand::PrecomputeTtsPage => RuntimeEffect::PrecomputeTtsPage,
                ReaderCommand::LoadPdfBytes { path } => RuntimeEffect::LoadPdfBytes { path },
                ReaderCommand::LoadPdfSyncMap { path } => RuntimeEffect::LoadPdfSyncMap { path },
                ReaderCommand::PersistPdfSyncMap { path, locations } => {
                    RuntimeEffect::PersistPdfSyncMap { path, locations }
                }
                ReaderCommand::LoadPdfRenderPrecomputed { path } => {
                    RuntimeEffect::LoadPdfRenderPrecomputed { path }
                }
            };
            (
                vec![AppEvent::OperationChanged {
                    scope,
                    active: true,
                }],
                vec![effect],
            )
        }
        AppCommand::LoadCalibreBooks { force_refresh } => {
            let mut effects = Vec::new();
            if !force_refresh && state.starter.calibre_books.is_empty() {
                effects.push(RuntimeEffect::LoadCalibreCachedBooks);
            }
            effects.push(RuntimeEffect::LoadCalibreBooks { force_refresh });
            (
                vec![
                    AppEvent::LoadingCalibreChanged(true),
                    AppEvent::OperationChanged {
                        scope: OperationScope::CalibreLoad,
                        active: true,
                    },
                ],
                effects,
            )
        }
        AppCommand::OpenCalibreBook { id } => (
            vec![AppEvent::OperationChanged {
                scope: OperationScope::SourceOpen,
                active: true,
            }],
            vec![RuntimeEffect::OpenCalibreBook { id }],
        ),
        AppCommand::EnsureCalibreThumbnail { id } => (
            vec![AppEvent::OperationChanged {
                scope: OperationScope::CalibreLoad,
                active: true,
            }],
            vec![RuntimeEffect::EnsureCalibreThumbnail { id }],
        ),
        AppCommand::SetRuntimeLogLevel { level } => (
            vec![AppEvent::OperationChanged {
                scope: OperationScope::RuntimeConfig,
                active: true,
            }],
            vec![RuntimeEffect::SetRuntimeLogLevel { level }],
        ),
        AppCommand::FlushPersistence { trigger } => (
            vec![AppEvent::OperationChanged {
                scope: OperationScope::RuntimeConfig,
                active: true,
            }],
            vec![RuntimeEffect::FlushPersistence { trigger }],
        ),
        AppCommand::SafeQuit => (
            vec![AppEvent::OperationChanged {
                scope: OperationScope::RuntimeConfig,
                active: true,
            }],
            vec![
                RuntimeEffect::FlushPersistence {
                    trigger: PersistenceTrigger::SafeQuit,
                },
                RuntimeEffect::SafeQuit,
            ],
        ),
    };

    let planned_effects: Vec<PlannedEffect> = effects
        .into_iter()
        .map(|effect| PlannedEffect { request_id, effect })
        .collect();
    trace!(effect_count = planned_effects.len(), "Dispatch plan ready");
    DispatchPlan {
        request_id,
        action,
        local_events,
        effects: planned_effects,
    }
}

pub fn apply_event(state: &mut AppState, event: AppEvent) {
    let span = event_span(&event);
    let _guard = span.enter();
    trace!("Applying app event");
    match event {
        AppEvent::OperationChanged { scope, active } => {
            let mut operations = state.app_shell.operations.clone();
            set_operation_flag(&mut operations, scope, active);
            state.set_operations(operations);
        }
        AppEvent::LoadingBootstrapChanged(loading) => state.set_loading_bootstrap(loading),
        AppEvent::LoadingRecentsChanged(loading) => state.set_loading_recents(loading),
        AppEvent::LoadingCalibreChanged(loading) => state.set_loading_calibre(loading),
        AppEvent::LoadingBrowserTabsChanged(loading) => state.set_loading_browser_tabs(loading),
        AppEvent::BootstrapLoaded {
            request_id,
            bootstrap,
        } => {
            debug!(request_id, "Bootstrap loaded");
            state.set_bootstrap_config(bootstrap.config.clone());
            state.set_bootstrap(Some(bootstrap));
            state.set_loading_bootstrap(false);
            clear_scope(state, OperationScope::RuntimeConfig);
        }
        AppEvent::SessionUpdated(event) => {
            if event.request_id < state.runtime_jobs.last_session_event_request_id {
                warn!(
                    request_id = event.request_id,
                    last_request_id = state.runtime_jobs.last_session_event_request_id,
                    "Ignoring stale session event"
                );
                return;
            }
            let request_id = event.request_id;
            let mode = event.session.mode;
            state.set_session(Some(event.session));
            state.set_startup_mode(mode);
            state.apply_runtime_job_patch(RuntimeJobPatch {
                last_session_event_request_id: Some(request_id),
                ..RuntimeJobPatch::default()
            });
            if matches!(mode, UiMode::Starter) {
                state.set_reader_document(None);
                state.set_reader_playback(None);
                clear_scope(state, OperationScope::ReaderCommand);
                clear_scope(state, OperationScope::ReaderSettings);
                clear_scope(state, OperationScope::ReaderTts);
                clear_scope(state, OperationScope::SourceOpen);
            }
        }
        AppEvent::ReaderUpdated(event) => {
            if event.request_id < state.runtime_jobs.last_reader_event_request_id {
                warn!(
                    request_id = event.request_id,
                    last_request_id = state.runtime_jobs.last_reader_event_request_id,
                    "Ignoring stale reader event"
                );
                return;
            }
            let request_id = event.request_id;
            let reader = event.reader;
            state.set_reader_document(Some(reader.clone()));
            state.set_reader_playback(derive_reader_playback(Some(&reader)));
            let session = SessionState {
                mode: UiMode::Reader,
                active_source_path: Some(reader.source_path.clone()),
                open_in_flight: false,
                panels: reader.panels,
            };
            state.set_session(Some(session));
            state.apply_runtime_job_patch(RuntimeJobPatch {
                last_reader_event_request_id: Some(request_id),
                last_session_event_request_id: Some(
                    state
                        .runtime_jobs
                        .last_session_event_request_id
                        .max(request_id),
                ),
                ..RuntimeJobPatch::default()
            });
            clear_scope(state, OperationScope::ReaderCommand);
            clear_scope(state, OperationScope::ReaderSettings);
            clear_scope(state, OperationScope::ReaderTts);
            clear_scope(state, OperationScope::SourceOpen);
        }
        AppEvent::ReaderPlaybackUpdated(event) => {
            if event.request_id < state.runtime_jobs.last_reader_playback_event_request_id {
                warn!(
                    request_id = event.request_id,
                    last_request_id = state.runtime_jobs.last_reader_playback_event_request_id,
                    "Ignoring stale reader playback event"
                );
                return;
            }
            let request_id = event.request_id;
            state.set_reader_playback(Some(event.playback.clone()));
            state.set_reader_playback_event(Some(event));
            state.apply_runtime_job_patch(RuntimeJobPatch {
                last_reader_playback_event_request_id: Some(request_id),
                ..RuntimeJobPatch::default()
            });
        }
        AppEvent::SourceOpenProgress(event) => {
            if event.request_id < state.runtime_jobs.last_source_open_event_request_id {
                warn!(
                    request_id = event.request_id,
                    last_request_id = state.runtime_jobs.last_source_open_event_request_id,
                    "Ignoring stale source-open event"
                );
                return;
            }
            let request_id = event.request_id;
            let terminal = matches!(event.phase.as_str(), "failed" | "cancelled");
            state.apply_runtime_job_patch(RuntimeJobPatch {
                source_open_event: Some(event),
                last_source_open_event_request_id: Some(request_id),
                ..RuntimeJobPatch::default()
            });
            if terminal {
                clear_scope(state, OperationScope::SourceOpen);
            }
        }
        AppEvent::SourceOpened { request_id, result } => {
            debug!(request_id, source_path = %result.reader.source_path, "Source opened");
            let reader = result.reader;
            state.set_session(Some(result.session));
            state.set_reader_document(Some(reader.clone()));
            state.set_reader_playback(derive_reader_playback(Some(&reader)));
            clear_scope(state, OperationScope::SourceOpen);
        }
        AppEvent::RecentsLoaded {
            request_id,
            recents,
        } => {
            debug!(request_id, count = recents.len(), "Recent books loaded");
            state.set_starter_recents(recents);
            state.set_loading_recents(false);
            clear_scope(state, OperationScope::StarterCommand);
        }
        AppEvent::CalibreBooksLoaded {
            request_id,
            books,
            from_cache,
        } => {
            debug!(
                request_id,
                count = books.len(),
                from_cache,
                "Calibre books loaded"
            );
            state.set_starter_calibre_books(books);
            if !from_cache {
                state.set_loading_calibre(false);
                clear_scope(state, OperationScope::CalibreLoad);
            }
        }
        AppEvent::BrowserTabsHealthLoaded { request_id, health } => {
            debug!(
                request_id,
                ok = health.ok,
                extension_connected = health.extension_connected,
                "Browser tabs health loaded"
            );
            state.set_service_health(&health);
            state.set_starter_browser_tabs_health(Some(health));
            state.set_loading_browser_tabs(false);
            clear_scope(state, OperationScope::BrowserTabRefresh);
        }
        AppEvent::BrowserTabWindowsLoaded {
            request_id,
            windows,
        } => {
            debug!(
                request_id,
                count = windows.len(),
                "Browser tabs windows loaded"
            );
            state.set_starter_browser_tabs_windows(windows);
            state.set_loading_browser_tabs(false);
            clear_scope(state, OperationScope::BrowserTabRefresh);
        }
        AppEvent::BrowserTabsLoaded { request_id, tabs } => {
            debug!(request_id, count = tabs.len(), "Browser tabs loaded");
            state.set_starter_browser_tabs_tabs(tabs);
            state.set_loading_browser_tabs(false);
            clear_scope(state, OperationScope::BrowserTabRefresh);
        }
        AppEvent::CalibreLoadProgress(event) => {
            if event.request_id < state.runtime_jobs.last_calibre_event_request_id {
                warn!(
                    request_id = event.request_id,
                    last_request_id = state.runtime_jobs.last_calibre_event_request_id,
                    "Ignoring stale calibre event"
                );
                return;
            }
            let request_id = event.request_id;
            let terminal = matches!(event.phase.as_str(), "failed" | "cancelled" | "loaded");
            let calibre_available = match event.phase.as_str() {
                "failed" | "cancelled" => Some(false),
                "loaded" => Some(true),
                _ => None,
            };
            if let Some(available) = calibre_available {
                state.set_calibre_available(Some(available));
            }
            state.apply_runtime_job_patch(RuntimeJobPatch {
                calibre_load_event: Some(event),
                last_calibre_event_request_id: Some(request_id),
                ..RuntimeJobPatch::default()
            });
            if terminal {
                state.set_loading_calibre(false);
                clear_scope(state, OperationScope::CalibreLoad);
            }
        }
        AppEvent::TtsStateUpdated(event) => {
            if event.request_id < state.runtime_jobs.last_tts_event_request_id {
                warn!(
                    request_id = event.request_id,
                    last_request_id = state.runtime_jobs.last_tts_event_request_id,
                    "Ignoring stale tts event"
                );
                return;
            }
            let request_id = event.request_id;
            state.set_tts_state_event(Some(event));
            state.apply_runtime_job_patch(RuntimeJobPatch {
                last_tts_event_request_id: Some(request_id),
                ..RuntimeJobPatch::default()
            });
        }
        AppEvent::PdfTranscriptionProgress(event) => {
            if event.request_id < state.runtime_jobs.last_pdf_event_request_id {
                warn!(
                    request_id = event.request_id,
                    last_request_id = state.runtime_jobs.last_pdf_event_request_id,
                    "Ignoring stale pdf transcription event"
                );
                return;
            }
            let request_id = event.request_id;
            let terminal = matches!(event.phase.as_str(), "failed" | "cancelled" | "complete");
            state.apply_runtime_job_patch(RuntimeJobPatch {
                pdf_transcription_event: Some(event),
                last_pdf_event_request_id: Some(request_id),
                ..RuntimeJobPatch::default()
            });
            if terminal {
                clear_scope(state, OperationScope::ReaderCommand);
            }
        }
        AppEvent::LogLevelUpdated(event) => {
            if event.request_id < state.runtime_jobs.last_log_level_event_request_id {
                warn!(
                    request_id = event.request_id,
                    last_request_id = state.runtime_jobs.last_log_level_event_request_id,
                    "Ignoring stale log-level event"
                );
                return;
            }
            let request_id = event.request_id;
            state.update_runtime_log_level(event.level.clone());
            state.apply_runtime_job_patch(RuntimeJobPatch {
                log_level_event: Some(event),
                last_log_level_event_request_id: Some(request_id),
                ..RuntimeJobPatch::default()
            });
            clear_scope(state, OperationScope::RuntimeConfig);
        }
        AppEvent::NotificationRaised {
            request_id,
            notification,
        } => {
            debug!(request_id, "Notification raised");
            state.push_notification(notification);
        }
        AppEvent::NotificationDismissed {
            request_id,
            notification_id,
        } => {
            debug!(request_id, notification_id, "Notification dismissed");
            state.dismiss_notification(notification_id);
        }
        AppEvent::PersistenceFlushed {
            request_id,
            trigger,
            outcome,
        } => {
            state.set_persistence_status(trigger, outcome, request_id);
            clear_scope(state, OperationScope::RuntimeConfig);
        }
        AppEvent::CommandFailed {
            request_id,
            scope,
            error,
        } => {
            warn!(request_id, code = %error.code, message = %error.message, "Command failed");
            if let Some(scope) = scope {
                clear_scope(state, scope);
            }
            state.set_loading_bootstrap(false);
            state.set_loading_recents(false);
            state.set_loading_calibre(false);
            state.set_loading_browser_tabs(false);
        }
        AppEvent::RemotePlaybackStateUpdated(playback) => {
            if let Some(snapshot) = &state.reader_document.snapshot {
                if snapshot.source_path == playback.source_path {
                    if playback.updated_at > state.reader_playback.last_updated_at {
                        state.set_reader_playback(Some(playback));
                    } else {
                        warn!(
                            remote = playback.updated_at,
                            local = state.reader_playback.last_updated_at,
                            "Rejected stale remote playback update"
                        );
                    }
                }
            }
        }
    }
}

fn clear_scope(state: &mut AppState, scope: OperationScope) {
    let mut operations = state.app_shell.operations.clone();
    set_operation_flag(&mut operations, scope, false);
    state.set_operations(operations);
}

fn set_operation_flag(operations: &mut OperationState, scope: OperationScope, active: bool) {
    match scope {
        OperationScope::SourceOpen => operations.source_open = active,
        OperationScope::StarterCommand => operations.starter_command = active,
        OperationScope::ReaderCommand => operations.reader_command = active,
        OperationScope::ReaderTts => operations.reader_tts = active,
        OperationScope::ReaderSettings => operations.reader_settings = active,
        OperationScope::BrowserTabRefresh => operations.browser_tab_refresh = active,
        OperationScope::CalibreLoad => operations.calibre_load = active,
        OperationScope::RuntimeConfig => operations.runtime_config = active,
    }
}

fn tts_sync_policy(command: &session::SessionCommand) -> TtsSyncPolicy {
    match command {
        session::SessionCommand::GetSnapshot => TtsSyncPolicy::KeepRuntimeState,
        session::SessionCommand::ApplySettings { patch } => {
            if patch.font_size.is_some()
                || patch.lines_per_page.is_some()
                || patch.pause_after_sentence.is_some()
                || patch.tts_speed.is_some()
                || patch.tts_volume.is_some()
            {
                TtsSyncPolicy::SyncAfterCommand
            } else {
                TtsSyncPolicy::KeepRuntimeState
            }
        }
        _ => TtsSyncPolicy::SyncAfterCommand,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::BootstrapConfig;
    use crate::contracts::ReaderPlaybackState;
    use lanternleaf_core::{config, session};

    fn make_reader_snapshot() -> session::ReaderSnapshot {
        session::ReaderSnapshot {
            source_path: "/tmp/book.epub".to_string(),
            source_name: "book.epub".to_string(),
            current_page: 3,
            total_pages: 12,
            text_only_mode: false,
            has_structured_markdown: true,
            pretty_kind: session::PrettyKind::Html,
            pdf_geometry_mode: None,
            pdf_sync_strategy: None,
            pdf_classification: None,
            pdf_runtime_policy: None,
            pdf_ocr_alignment: None,
            pdf_ocr_pipeline: None,
            images: Vec::new(),
            tts_text_page: "tts".to_string(),
            reading_markdown_page: None,
            reading_html_page: Some("<p>hi</p>".to_string()),
            tts_current_sentence_text: Some("one".to_string()),
            page_text: "page".to_string(),
            sentences: vec!["one".to_string()],
            canonical_sentences: vec!["one".to_string()],
            page_sentence_counts: vec![1],
            sentence_anchor_map: vec![Some(0)],
            highlighted_sentence_idx: Some(0),
            search_query: "query".to_string(),
            search_matches: vec![0],
            selected_search_match: Some(0),
            settings: session::ReaderSettingsView {
                theme: config::ThemeMode::Day,
                font_family: config::FontFamily::Lexend,
                font_weight: config::FontWeight::Bold,
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
            tts: session::ReaderTtsView {
                state: session::TtsPlaybackState::Playing,
                current_sentence_idx: Some(0),
                sentence_count: 1,
                can_seek_prev: false,
                can_seek_next: false,
                progress_pct: 0.5,
            },
            stats: session::ReaderStats {
                page_index: 3,
                total_pages: 12,
                tts_progress_pct: 0.5,
                global_progress_pct: 0.25,
                page_time_remaining_secs: 10.0,
                book_time_remaining_secs: 100.0,
                page_word_count: 100,
                page_sentence_count: 1,
                page_start_percent: 0.2,
                page_end_percent: 0.3,
                words_read_up_to_page_start: 50,
                sentences_read_up_to_page_start: 2,
                words_read_up_to_page_end: 150,
                sentences_read_up_to_page_end: 3,
                words_read_up_to_current_position: 55,
                sentences_read_up_to_current_position: 2,
            },
            panels: session::PanelState {
                show_settings: true,
                show_stats: false,
                show_tts: true,
            },
        }
    }

    #[test]
    fn reader_tts_commands_plan_tts_effects_with_sync() {
        let plan = plan_command(
            &AppState::default(),
            41,
            AppCommand::Reader(ReaderCommand::Session(session::SessionCommand::TtsPlay)),
        );

        assert_eq!(plan.request_id, 41);
        assert_eq!(plan.action, "reader_tts_play");
        assert_eq!(plan.effects.len(), 1);
        match &plan.effects[0].effect {
            RuntimeEffect::ApplyReaderCommand { command, sync_tts } => {
                assert_eq!(command.action(), "reader_tts_play");
                assert_eq!(*sync_tts, TtsSyncPolicy::SyncAfterCommand);
            }
            effect => panic!("unexpected effect: {effect:?}"),
        }
    }

    #[test]
    fn apply_settings_without_tts_fields_keeps_tts_runtime_unsynced() {
        let patch = session::ReaderSettingsPatch {
            theme: Some(config::ThemeMode::Night),
            ..session::ReaderSettingsPatch::default()
        };
        let plan = plan_command(
            &AppState::default(),
            42,
            AppCommand::Reader(ReaderCommand::Session(
                session::SessionCommand::ApplySettings { patch },
            )),
        );

        match &plan.effects[0].effect {
            RuntimeEffect::ApplyReaderCommand { sync_tts, .. } => {
                assert_eq!(*sync_tts, TtsSyncPolicy::KeepRuntimeState);
            }
            effect => panic!("unexpected effect: {effect:?}"),
        }
    }

    #[test]
    fn source_open_completion_populates_session_and_reader_domains() {
        let mut state = AppState::default();
        apply_event(
            &mut state,
            AppEvent::SourceOpened {
                request_id: 7,
                result: OpenSourceResult {
                    session: SessionState {
                        mode: UiMode::Reader,
                        active_source_path: Some("/tmp/book.epub".to_string()),
                        open_in_flight: false,
                        panels: session::PanelState {
                            show_settings: true,
                            show_stats: false,
                            show_tts: true,
                        },
                    },
                    reader: make_reader_snapshot(),
                },
            },
        );

        assert_eq!(
            state
                .session
                .session
                .as_ref()
                .and_then(|value| value.active_source_path.as_deref()),
            Some("/tmp/book.epub")
        );
        assert_eq!(state.reader_ui.current_page, Some(3));
        assert_eq!(
            state
                .reader_playback
                .playback
                .as_ref()
                .and_then(|value| value.highlighted_sentence_idx),
            Some(0)
        );
    }

    #[test]
    fn stale_reader_events_do_not_overwrite_newer_state() {
        let mut state = AppState::default();
        apply_event(
            &mut state,
            AppEvent::ReaderUpdated(ReaderStateEvent {
                request_id: 9,
                action: "reader_next_page".to_string(),
                reader: make_reader_snapshot(),
            }),
        );

        let mut stale = make_reader_snapshot();
        stale.current_page = 1;
        apply_event(
            &mut state,
            AppEvent::ReaderUpdated(ReaderStateEvent {
                request_id: 8,
                action: "reader_prev_page".to_string(),
                reader: stale,
            }),
        );

        assert_eq!(state.reader_ui.current_page, Some(3));
        assert_eq!(state.runtime_jobs.last_reader_event_request_id, 9);
    }

    #[test]
    fn bootstrap_completion_clears_loading_and_busy_flags() {
        let mut state = AppState::default();
        apply_event(&mut state, AppEvent::LoadingBootstrapChanged(true));
        apply_event(
            &mut state,
            AppEvent::OperationChanged {
                scope: OperationScope::RuntimeConfig,
                active: true,
            },
        );

        apply_event(
            &mut state,
            AppEvent::BootstrapLoaded {
                request_id: 3,
                bootstrap: BootstrapState {
                    app_name: "LanternLeaf".to_string(),
                    mode: "dev".to_string(),
                    config: BootstrapConfig {
                        theme: config::ThemeMode::Day,
                        font_family: config::FontFamily::Lexend,
                        font_weight: config::FontWeight::Bold,
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
                        log_level: "info".to_string(),
                        default_font_size: 18,
                        default_lines_per_page: 30,
                        default_tts_speed: 1.0,
                        default_pause_after_sentence: 0.0,
                        key_toggle_play_pause: "Space".to_string(),
                        key_next_sentence: "J".to_string(),
                        key_prev_sentence: "K".to_string(),
                        key_repeat_sentence: "L".to_string(),
                        key_toggle_search: "/".to_string(),
                        key_safe_quit: "Q".to_string(),
                        key_toggle_settings: "S".to_string(),
                        key_toggle_stats: "D".to_string(),
                        key_toggle_tts: "T".to_string(),
                        browser_tabs_enabled: true,
                        close_browser_tab_on_recent_delete: false,
                        remote_url: None,
                    },
                },
            },
        );

        assert!(!state.app_shell.loading_bootstrap);
        assert!(!state.app_shell.busy);
        assert_eq!(
            state
                .app_shell
                .bootstrap
                .as_ref()
                .map(|value| value.app_name.as_str()),
            Some("LanternLeaf")
        );
    }

    #[test]
    fn close_session_plans_reader_command_scope() {
        let plan = plan_command(&AppState::default(), 55, AppCommand::CloseReaderSession);

        assert_eq!(plan.action, "reader_close_session");
        assert!(matches!(
            plan.local_events.as_slice(),
            [AppEvent::OperationChanged {
                scope: OperationScope::ReaderCommand,
                active: true
            }]
        ));
        assert!(matches!(
            plan.effects.as_slice(),
            [PlannedEffect {
                effect: RuntimeEffect::CloseReaderSession,
                ..
            }]
        ));
    }

    #[test]
    fn return_to_starter_plans_reader_command_scope() {
        let plan = plan_command(&AppState::default(), 56, AppCommand::ReturnToStarter);

        assert_eq!(plan.action, "session_return_to_starter");
        assert!(matches!(
            plan.effects.as_slice(),
            [PlannedEffect {
                effect: RuntimeEffect::ReturnToStarter,
                ..
            }]
        ));
    }

    #[test]
    fn search_navigation_plans_reader_command_effect() {
        let plan = plan_command(
            &AppState::default(),
            57,
            AppCommand::Reader(ReaderCommand::Session(session::SessionCommand::SearchNext)),
        );

        match &plan.effects[0].effect {
            RuntimeEffect::ApplyReaderCommand { command, sync_tts } => {
                assert_eq!(command.action(), "reader_search_next");
                assert_eq!(*sync_tts, TtsSyncPolicy::SyncAfterCommand);
            }
            effect => panic!("unexpected effect: {effect:?}"),
        }
    }

    #[test]
    fn flush_persistence_plans_runtime_config_scope() {
        let plan = plan_command(
            &AppState::default(),
            58,
            AppCommand::FlushPersistence {
                trigger: PersistenceTrigger::RuntimeConfigChange,
            },
        );

        assert!(matches!(
            plan.local_events.as_slice(),
            [AppEvent::OperationChanged {
                scope: OperationScope::RuntimeConfig,
                active: true
            }]
        ));
        assert!(matches!(
            plan.effects.as_slice(),
            [PlannedEffect {
                effect: RuntimeEffect::FlushPersistence { .. },
                ..
            }]
        ));
    }

    #[test]
    fn pdf_transcription_terminal_events_clear_reader_command_scope() {
        let mut state = AppState::default();
        state.app_shell.operations.reader_command = true;

        apply_event(
            &mut state,
            AppEvent::PdfTranscriptionProgress(PdfTranscriptionEvent {
                request_id: 90,
                phase: "complete".to_string(),
                source_path: "/tmp/book.pdf".to_string(),
                message: None,
            }),
        );

        assert!(!state.app_shell.operations.reader_command);
    }

    #[test]
    fn reader_playback_updates_keep_latest_event() {
        let mut state = AppState::default();
        let playback = ReaderPlaybackState {
            source_path: "/tmp/book.epub".to_string(),
            current_page: 2,
            highlighted_sentence_idx: Some(1),
            tts: session::ReaderTtsView {
                state: session::TtsPlaybackState::Playing,
                current_sentence_idx: Some(1),
                sentence_count: 4,
                can_seek_prev: true,
                can_seek_next: true,
                progress_pct: 0.5,
            },
            stats: make_reader_snapshot().stats,
            updated_at: 0,
        };
        apply_event(
            &mut state,
            AppEvent::ReaderPlaybackUpdated(ReaderPlaybackStateEvent {
                request_id: 81,
                action: "reader_tts_play".to_string(),
                playback: playback.clone(),
            }),
        );

        assert_eq!(
            state
                .reader_playback
                .playback_event
                .as_ref()
                .map(|event| event.request_id),
            Some(81)
        );
        assert_eq!(
            state
                .reader_playback
                .playback
                .as_ref()
                .map(|state| state.current_page),
            Some(2)
        );
    }

    #[test]
    fn runtime_effects_define_ownership() {
        let effect = RuntimeEffect::OpenSourcePath {
            path: "/tmp/book.epub".to_string(),
        };
        assert_eq!(effect.owner(), EffectOwner::SourceOpen);
        let effect = RuntimeEffect::LoadPdfBytes {
            path: "/tmp/book.pdf".to_string(),
        };
        assert_eq!(effect.owner(), EffectOwner::PdfArtifacts);
        let effect = RuntimeEffect::FlushPersistence {
            trigger: PersistenceTrigger::SafeQuit,
        };
        assert_eq!(effect.owner(), EffectOwner::Persistence);
    }

    #[test]
    fn persistence_flush_updates_status_and_clears_scope() {
        let mut state = AppState::default();
        apply_event(
            &mut state,
            AppEvent::OperationChanged {
                scope: OperationScope::RuntimeConfig,
                active: true,
            },
        );

        apply_event(
            &mut state,
            AppEvent::PersistenceFlushed {
                request_id: 42,
                trigger: PersistenceTrigger::SourceOpen,
                outcome: PersistenceOutcome::Completed,
            },
        );

        assert_eq!(
            state.app_shell.persistence_status.last_trigger,
            Some(PersistenceTrigger::SourceOpen)
        );
        assert_eq!(
            state.app_shell.persistence_status.last_outcome,
            Some(PersistenceOutcome::Completed)
        );
        assert_eq!(state.app_shell.persistence_status.last_request_id, 42);
        assert!(!state.app_shell.operations.runtime_config);
    }

    #[test]
    fn bootstrap_sets_config_snapshot_and_service_flags() {
        let mut state = AppState::default();
        let bootstrap = BootstrapState {
            app_name: "LanternLeaf".to_string(),
            mode: "egui".to_string(),
            config: BootstrapConfig {
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
                    r: 0.2,
                    g: 0.3,
                    b: 0.4,
                    a: 0.5,
                },
                log_level: "info".to_string(),
                default_font_size: 16,
                default_lines_per_page: 300,
                default_tts_speed: 1.0,
                default_pause_after_sentence: 0.0,
                key_toggle_play_pause: "space".to_string(),
                key_next_sentence: "n".to_string(),
                key_prev_sentence: "p".to_string(),
                key_repeat_sentence: "r".to_string(),
                key_toggle_search: "/".to_string(),
                key_safe_quit: "q".to_string(),
                key_toggle_settings: "s".to_string(),
                key_toggle_stats: "t".to_string(),
                key_toggle_tts: "y".to_string(),
                browser_tabs_enabled: true,
                close_browser_tab_on_recent_delete: false,
                remote_url: None,
            },
        };

        apply_event(
            &mut state,
            AppEvent::BootstrapLoaded {
                request_id: 1,
                bootstrap: bootstrap.clone(),
            },
        );

        assert_eq!(
            state
                .app_shell
                .app_config_snapshot
                .as_ref()
                .map(|cfg| cfg.log_level.as_str()),
            Some("info")
        );
        assert!(state.app_shell.service_status.browser_tabs_enabled);
    }

    #[test]
    fn session_updates_startup_mode_and_availability() {
        let mut state = AppState::default();
        apply_event(
            &mut state,
            AppEvent::SessionUpdated(SessionStateEvent {
                request_id: 2,
                action: "session_open".to_string(),
                session: SessionState {
                    mode: UiMode::Reader,
                    active_source_path: Some("/tmp/book.epub".to_string()),
                    open_in_flight: false,
                    panels: session::PanelState::default(),
                },
            }),
        );

        assert_eq!(state.app_shell.startup_mode, Some(UiMode::Reader));
        assert!(state.session.reader_mode_available);
        assert_eq!(
            state.session.current_source_path.as_deref(),
            Some("/tmp/book.epub")
        );
    }

    #[test]
    fn service_status_updates_from_health_and_calibre_events() {
        let mut state = AppState::default();
        apply_event(
            &mut state,
            AppEvent::BrowserTabsHealthLoaded {
                request_id: 3,
                health: BrowserTabsHealth {
                    ok: true,
                    extension_connected: true,
                    now: None,
                },
            },
        );
        assert_eq!(
            state.app_shell.service_status.browser_tabs_available,
            Some(true)
        );

        apply_event(
            &mut state,
            AppEvent::CalibreLoadProgress(CalibreLoadEvent {
                request_id: 4,
                phase: "failed".to_string(),
                count: None,
                message: None,
            }),
        );
        assert_eq!(
            state.app_shell.service_status.calibre_available,
            Some(false)
        );
    }

    #[test]
    fn playback_updates_do_not_replace_reader_document() {
        let mut state = AppState::default();
        let snapshot = make_reader_snapshot();
        apply_event(
            &mut state,
            AppEvent::ReaderUpdated(ReaderStateEvent {
                request_id: 5,
                action: "reader_snapshot".to_string(),
                reader: snapshot.clone(),
            }),
        );
        let original_source = state
            .reader_document
            .snapshot
            .as_ref()
            .map(|value| value.source_path.clone());

        apply_event(
            &mut state,
            AppEvent::ReaderPlaybackUpdated(ReaderPlaybackStateEvent {
                request_id: 6,
                action: "reader_tts_play".to_string(),
                playback: ReaderPlaybackState {
                    source_path: "/tmp/book.epub".to_string(),
                    current_page: 2,
                    highlighted_sentence_idx: Some(1),
                    tts: snapshot.tts.clone(),
                    stats: snapshot.stats.clone(),
                    updated_at: 0,
                },
            }),
        );

        assert_eq!(
            state
                .reader_document
                .snapshot
                .as_ref()
                .map(|value| value.source_path.clone()),
            original_source
        );
    }

    #[test]
    fn panel_toggle_session_update_does_not_clear_reader_document() {
        let mut state = AppState::default();
        let snapshot = make_reader_snapshot();
        apply_event(
            &mut state,
            AppEvent::ReaderUpdated(ReaderStateEvent {
                request_id: 7,
                action: "reader_snapshot".to_string(),
                reader: snapshot.clone(),
            }),
        );
        apply_event(
            &mut state,
            AppEvent::SessionUpdated(SessionStateEvent {
                request_id: 8,
                action: "panel_toggle".to_string(),
                session: SessionState {
                    mode: UiMode::Reader,
                    active_source_path: Some(snapshot.source_path.clone()),
                    open_in_flight: false,
                    panels: session::PanelState {
                        show_settings: false,
                        show_stats: true,
                        show_tts: true,
                    },
                },
            }),
        );
        assert!(state.reader_document.snapshot.is_some());
    }
}
