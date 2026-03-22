mod helpers;
mod pdf;
mod pdf_renderer;
mod shell;

use std::{
    cmp::Reverse,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use eframe::{
    NativeOptions,
    egui::{
        self, Align, Align2, Button, CentralPanel, CollapsingHeader, Color32, ColorImage, Context,
        ComboBox, FontFamily, FontId, Id, Order, Pos2, Rect, RichText, ScrollArea, Sense, SidePanel,
        Slider, Stroke, TextureHandle, TextureOptions, TopBottomPanel, Ui, Vec2, Visuals,
    },
};
use helpers::{
    app_config_path, bootstrap_config_from_app_config, format_combo, workspace_root_from_cwd,
};

use crate::pdf::{
    PdfPageRegistryEntry, PdfViewportBudgetDecision, PdfViewportBudgetInput, PdfViewportPlanInput,
    PdfViewportRenderPlan, build_pdf_viewport_render_plan, choose_pdf_viewport_evictions,
};
use crate::pdf_renderer::{
    NativePdfRenderer, NativeRenderEviction, NativeRenderSpan, RenderTarget,
};
use crate::shell::{FocusOwner, LayoutPolicy, NotificationLevel, ShellState};
use lanternleaf_app::{
    AppRuntime,
    contracts::{
        BootstrapState, BridgeError, BrowserTabsHealth, BrowserTabsTab, BrowserTabsWindow,
        CalibreBookDto, CalibreLoadEvent, PrettyKind, ReaderPlaybackStateEvent, ReaderSnapshot,
        ReaderStateEvent, RecentBook, SourceOpenEvent, TtsStateEvent, UiMode,
    },
    persistence::{FilesystemPersistenceService, PersistenceLifecycle},
    pipeline::{AppCommand, AppEvent, DispatchPlan, OperationScope, PersistenceTrigger, ReaderCommand},
    shortcuts::{ShortcutAction, ShortcutScope, UiShortcutAction},
    state::{AppState, OperationState},
    tracing::init_tracing,
    tts_runtime::{TtsCommand, TtsRuntime, TtsRuntimeEvent, TtsRuntimeEventKind},
};
use lanternleaf_core::{
    cache, config, normalizer,
    session::{ReaderSettingsPatch, SessionCommand, TtsPlaybackState},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{Level, info, trace, warn};

pub const PDF_CANVAS_BUDGET_PAGES: usize = 2;
pub const PDF_TEXT_LAYER_BUDGET_PAGES: usize = 1;
pub const PDF_CANVAS_TEXTURE_SIZE: [usize; 2] = [320, 450];
pub const PDF_TEXT_TEXTURE_SIZE: [usize; 2] = [300, 420];
const REGRESSION_EVENT_WINDOW: Duration = Duration::from_secs(3);
const READER_RENDR_ROADMAP_URL: &str = "https://github.com/sguzman/lantern-leaf/blob/main/docs/roadmaps/egui-reader-rendering-roadmap.md";
const PDF_SUBSYSTEM_ROADMAP_URL: &str =
    "https://github.com/sguzman/lantern-leaf/blob/main/docs/roadmaps/egui-native-pdf-roadmap.md";
const PRIORITIZATION_ROADMAP_URL: &str = "https://github.com/sguzman/lantern-leaf/blob/main/docs/roadmaps/implementation-prioritization-roadmap.md";
const TTS_ROADMAP_URL: &str = "https://github.com/sguzman/lantern-leaf/blob/main/docs/roadmaps/egui-tts-audio-and-playback-roadmap.md";
const SETTINGS_ROADMAP_URL: &str = "https://github.com/sguzman/lantern-leaf/blob/main/docs/roadmaps/egui-config-cache-and-persistence-roadmap.md";
const PERSISTENCE_ROADMAP_URL: &str = SETTINGS_ROADMAP_URL;
const QA_REGRESSION_URL: &str = "https://github.com/sguzman/lantern-leaf/blob/main/docs/roadmaps/egui-testing-and-parity-roadmap.md";
const TIMELINE_ARCHIVE_DIR: &str = "logs/qa-timeline";
const MAX_PINNED_TIMELINE_ENTRIES: usize = 8;
const PINNED_TIMELINE_FILE: &str = "pinned-timeline.json";

fn format_duration_secs(seconds: f64) -> String {
    if !seconds.is_finite() {
        return "n/a".to_string();
    }
    let total = seconds.max(0.0).round() as u64;
    let mins = total / 60;
    let secs = total % 60;
    if mins > 0 {
        format!("{mins}m {secs}s")
    } else {
        format!("{secs}s")
    }
}

fn format_bytes(bytes: Option<u64>) -> String {
    let Some(bytes) = bytes else {
        return "n/a".to_string();
    };
    let kb = 1024.0;
    let mb = kb * 1024.0;
    let gb = mb * 1024.0;
    let value = bytes as f64;
    if value >= gb {
        format!("{:.2} GB", value / gb)
    } else if value >= mb {
        format!("{:.2} MB", value / mb)
    } else if value >= kb {
        format!("{:.2} KB", value / kb)
    } else {
        format!("{} B", bytes)
    }
}

fn format_relative_unix_secs(unix_secs: u64) -> String {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(unix_secs);
    if unix_secs >= now_secs {
        return "just now".to_string();
    }
    let delta = now_secs - unix_secs;
    let mins = delta / 60;
    let hours = mins / 60;
    let days = hours / 24;
    if days > 0 {
        format!("{}d ago", days)
    } else if hours > 0 {
        format!("{}h ago", hours)
    } else if mins > 0 {
        format!("{}m ago", mins)
    } else {
        format!("{}s ago", delta)
    }
}

fn main() {
    let config_path = app_config_path();
    let app_config = config::load_config(&config_path);
    let bootstrap_config = bootstrap_config_from_app_config(&app_config);
    let tracing_guard = init_tracing(&bootstrap_config.log_level);

    let runtime = AppRuntime::with_bootstrap_config(&bootstrap_config);
    let mut options = NativeOptions::default();
    options.viewport.inner_size = Some(egui::vec2(
        app_config.window_width as f32,
        app_config.window_height as f32,
    ));

    info!("Starting LanternLeaf egui shell");

    let _ = eframe::run_native(
        "LanternLeaf",
        options,
        Box::new(move |cc| {
            Box::new(LanternLeafApp::new(cc, runtime.clone(), tracing_guard))
                as Box<dyn eframe::App>
        }),
    );
}

struct LanternLeafApp {
    runtime: AppRuntime,
    _tracing_guard: tracing_appender::non_blocking::WorkerGuard,
    status_log: Vec<StatusLogEntry>,
    show_safe_quit_modal: bool,
    show_reader_confirm_modal: bool,
    pending_search_focus: bool,
    last_plan: Option<DispatchPlan>,
    auto_scroll_state: AutoScrollState,
    anchor_diagnostics: AnchorDiagnostics,
    overlay_diagnostics: OverlayDiagnostics,
    audio_diagnostics: AudioDiagnostics,
    tts_runtime: TtsRuntime,
    last_tts_runtime_event: Option<TtsRuntimeEvent>,
    persistence: PersistenceLifecycle<FilesystemPersistenceService>,
    persistence_logged: bool,
    last_reader_source: Option<String>,
    last_reader_snapshot: Option<ReaderSnapshot>,
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
    sentence_scroll_offset: Option<Vec2>,
    overlay_eviction_warning_at: Option<Instant>,
    timeline_history: Vec<TimelineHistoryEntry>,
    pinned_timeline_entries: Vec<TimelineHistoryEntry>,
    starter_open_path_input: String,
    starter_clipboard_text_input: String,
    starter_calibre_query: String,
    starter_calibre_force_refresh: bool,
    starter_calibre_sort: CalibreSort,
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

impl LanternLeafApp {
    const OVERLAY_EVICTION_SNACK_DURATION: Duration = Duration::from_secs(5);
    fn new(
        _cc: &eframe::CreationContext<'_>,
        runtime: AppRuntime,
        tracing_guard: tracing_appender::non_blocking::WorkerGuard,
    ) -> Self {
        let pdf_renderer = match NativePdfRenderer::new() {
            Ok(renderer) => Some(renderer),
            Err(err) => {
                warn!(error = ?err, "Failed to initialize native PDF renderer");
                None
            }
        };
        let mut app = Self {
            runtime,
            _tracing_guard: tracing_guard,
            status_log: Vec::new(),
            show_safe_quit_modal: false,
            show_reader_confirm_modal: false,
            pending_search_focus: false,
            last_plan: None,
            auto_scroll_state: AutoScrollState::default(),
            anchor_diagnostics: AnchorDiagnostics::default(),
            overlay_diagnostics: OverlayDiagnostics::default(),
            audio_diagnostics: AudioDiagnostics::default(),
            tts_runtime: TtsRuntime::new(normalizer::TextNormalizer::load_default()),
            last_tts_runtime_event: None,
            persistence: PersistenceLifecycle::new(FilesystemPersistenceService),
            persistence_logged: false,
            last_reader_source: None,
            last_reader_snapshot: None,
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
            sentence_scroll_offset: None,
            overlay_eviction_warning_at: None,
            timeline_history: Vec::new(),
            pinned_timeline_entries: Vec::new(),
            starter_open_path_input: String::new(),
            starter_clipboard_text_input: String::new(),
            starter_calibre_query: String::new(),
            starter_calibre_force_refresh: false,
            starter_calibre_sort: CalibreSort::Title,
            starter_browser_tab_query: String::new(),
            starter_browser_tabs_force_refresh: false,
            starter_browser_tab_id_input: String::new(),
            starter_browser_window_id_input: String::new(),
        };
        app.load_pinned_timeline_entries();
        app
    }

    fn execute_command(&mut self, command: AppCommand) {
        let state_snapshot = self.runtime.state_snapshot();
        let reader_snapshot = state_snapshot.reader_document.snapshot.as_ref();
        self.maybe_record_audio_command(&command, reader_snapshot);
        self.apply_persistence_trigger(&command, reader_snapshot);
        let plan = self.runtime.plan_command(command.clone());
        self.log_plan(&plan);
        self.last_plan = Some(plan);
        self.apply_tts_command_if_needed(&command);
    }

    fn execute_reader_command(&mut self, command: ReaderCommand) {
        self.execute_command(AppCommand::Reader(command));
    }

    fn apply_tts_command_if_needed(&mut self, command: &AppCommand) {
        let AppCommand::Reader(ReaderCommand::Session(session_command)) = command else {
            return;
        };
        let Some(tts_command) = TtsCommand::from_session_command(session_command) else {
            return;
        };
        trace!(
            tts_command = tts_command.label(),
            action = session_command.action(),
            "Dispatching TTS command to egui runtime"
        );
        let _ = self.tts_runtime.apply_command(tts_command);
    }

    fn apply_persistence_trigger(
        &mut self,
        command: &AppCommand,
        snapshot: Option<&ReaderSnapshot>,
    ) {
        match command {
            AppCommand::Reader(_) => {
                self.persistence
                    .flush_trigger(snapshot, PersistenceTrigger::ReaderCommand);
            }
            AppCommand::SetRuntimeLogLevel { .. } => {
                self.persistence
                    .flush_trigger(snapshot, PersistenceTrigger::RuntimeConfigChange);
            }
            AppCommand::SafeQuit => {
                if let Some(snapshot) = snapshot {
                    self.persistence.on_safe_quit(snapshot);
                    self.record_persistence_status("safe_quit", &snapshot.source_path);
                }
            }
            _ => {}
        }
    }

    fn update_persistence_lifecycle(&mut self, snapshot: Option<&ReaderSnapshot>) {
        if !self.persistence_logged {
            self.persistence.on_startup();
            self.push_status("Persistence: startup".to_string());
            self.persistence_logged = true;
        }

        match snapshot {
            Some(snapshot) => {
                if self
                    .last_reader_source
                    .as_deref()
                    .map(|path| path != snapshot.source_path)
                    .unwrap_or(true)
                {
                    self.persistence.on_source_open(snapshot);
                    self.record_persistence_status("source_open", &snapshot.source_path);
                    self.last_reader_source = Some(snapshot.source_path.clone());
                }
                self.last_reader_snapshot = Some(snapshot.clone());
            }
            None => {
                if let Some(last_snapshot) = self.last_reader_snapshot.take() {
                    self.persistence.on_session_close(&last_snapshot);
                    self.record_persistence_status("session_close", &last_snapshot.source_path);
                }
                self.last_reader_source = None;
            }
        }
    }

    fn record_persistence_status(&mut self, label: &str, source_path: &str) {
        self.push_status(format!("Persistence: {label} ({source_path})"));
    }

    fn handle_tts_runtime_events(&mut self) {
        for event in self.tts_runtime.collect_events() {
            let request_id = event.request_id;
            match event.kind {
                TtsRuntimeEventKind::Progress | TtsRuntimeEventKind::StateChanged => {
                    trace!(
                        request_id,
                        action = %event.action,
                        kind = ?event.kind,
                        "Applying TTS runtime state event"
                    );
                }
                TtsRuntimeEventKind::Queued => {
                    info!(
                        request_id,
                        action = %event.action,
                        message = event.message.as_deref().unwrap_or("queued"),
                        "Queued TTS runtime batch"
                    );
                }
                TtsRuntimeEventKind::Completed => {
                    info!(
                        request_id,
                        action = %event.action,
                        "TTS runtime completed"
                    );
                }
                TtsRuntimeEventKind::Cancelled => {
                    warn!(
                        request_id,
                        action = %event.action,
                        "TTS runtime cancelled"
                    );
                }
                TtsRuntimeEventKind::Failed => {
                    warn!(
                        request_id,
                        action = %event.action,
                        message = event.message.as_deref().unwrap_or("unknown"),
                        "TTS runtime failed"
                    );
                }
            }

            if let Some(snapshot) = event.snapshot.clone() {
                if let Some(cursor) = event.cursor {
                    trace!(
                        request_id,
                        page = cursor.page + 1,
                        audio_idx = ?cursor.audio_idx,
                        display_idx = ?cursor.display_idx,
                        "TTS cursor updated"
                    );
                }
                self.runtime.apply_event(AppEvent::ReaderUpdated(ReaderStateEvent {
                    request_id,
                    action: event.action.clone(),
                    reader: snapshot.clone(),
                }));
                self.runtime.apply_event(AppEvent::TtsStateUpdated(TtsStateEvent {
                    request_id,
                    action: event.action.clone(),
                    tts: snapshot.tts.clone(),
                }));
            } else if let Some(playback) = event.playback.clone() {
                self.runtime
                    .apply_event(AppEvent::ReaderPlaybackUpdated(ReaderPlaybackStateEvent {
                        request_id,
                        action: event.action.clone(),
                        playback,
                    }));
            }

            if event.kind == TtsRuntimeEventKind::Failed {
                let error_message = event
                    .message
                    .clone()
                    .unwrap_or_else(|| "TTS runtime failed".to_string());
                let error = BridgeError {
                    code: "tts_runtime_failed".to_string(),
                    message: error_message,
                };
                self.runtime.apply_event(AppEvent::CommandFailed {
                    request_id,
                    scope: Some(OperationScope::ReaderTts),
                    error,
                });
            }

            self.last_tts_runtime_event = Some(event);
        }
    }

    fn log_plan(&mut self, plan: &DispatchPlan) {
        let entry = format!("Planned {} ({})", plan.action, plan.effects.len());
        self.push_status(entry);
    }

    fn push_status(&mut self, message: String) {
        let message_lower = message.to_lowercase();
        let level = if message_lower.contains("error") || message_lower.contains("failed") {
            NotificationLevel::Error
        } else if message_lower.contains("warn") {
            NotificationLevel::Warn
        } else {
            NotificationLevel::Info
        };
        self.status_log.push(StatusLogEntry {
            timestamp: Instant::now(),
            message,
        });
        if let Some(entry) = self.status_log.last() {
            self.shell_state
                .record_notification(level, entry.message.clone());
        }
        if self.status_log.len() > 8 {
            self.status_log.remove(0);
        }
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

    fn render_navigation_row(&mut self, ctx: &Context, state: &AppState) {
        if !self.layout_policy.show_status_row || !self.layout_policy.is_narrow() {
            return;
        }
        TopBottomPanel::top("nav_status_row").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Mode: {:?}", self.shell_state.active_mode));
                if state.app_shell.busy {
                    ui.label("Busy");
                }
                if state.app_shell.operations.source_open {
                    ui.label("Opening source");
                }
                if state.app_shell.operations.calibre_load {
                    ui.label("Loading Calibre");
                }
                if state.app_shell.operations.browser_tab_refresh {
                    ui.label("Refreshing browser tabs");
                }
            });
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

    fn render_top_bar(&mut self, ctx: &Context, state: &AppState) {
        TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.heading("LanternLeaf (egui)");
                    ui.separator();
                    let allow_recents = !state.app_shell.operations.source_open;
                    if ui
                        .add_enabled(
                            allow_recents,
                            egui::Button::new("Refresh recents (AppCommand::RefreshRecents)"),
                        )
                        .clicked()
                    {
                        self.execute_command(AppCommand::RefreshRecents { limit: Some(10) });
                    }
                    if ui.button("Safe quit (AppCommand::SafeQuit)").clicked() {
                        self.show_safe_quit_modal = true;
                    }
                    let session_mode = state.session.session.as_ref().map(|session| session.mode);
                    ui.label(format!(
                        "Mode: {:?}",
                        session_mode.unwrap_or(UiMode::Starter)
                    ));
                    ui.label(format!("Busy: {}", state.app_shell.busy));
                    if let Some(decision) = self.overlay_diagnostics.preview_decision() {
                        if !decision.allowed {
                            let reason = if !decision.highlight_page_has_text_layer {
                                "no text layer"
                            } else {
                                "overlay budget exhausted"
                            };
                            ui.label(
                                RichText::new(format!(
                                    "Overlay warning: {} (budget {} pages)",
                                    reason, decision.budget_pages
                                ))
                                .color(Color32::from_rgb(255, 190, 110))
                                .strong(),
                            );
                        }
                    }
                });
                if let Some(elapsed) = self.overlay_eviction_warning_age() {
                    if let Some(alert) = self
                        .pdf_render_state
                        .recent_overlay_pressure_alerts()
                        .last()
                    {
                        ui.label(
                            RichText::new(format!(
                                "Overlay eviction warning: {} ({:.1}s ago)",
                                alert.describe(),
                                elapsed.as_secs_f32()
                            ))
                            .color(Color32::from_rgb(255, 130, 90))
                            .small()
                            .strong(),
                        );
                    }
                }
                self.render_overlay_pressure_toast(ui);
            });
        });
    }

    fn render_overlay_pressure_toast(&mut self, ui: &mut Ui) {
        if let Some(alert) = self
            .pdf_render_state
            .recent_overlay_pressure_alerts()
            .last()
            .cloned()
        {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(self.overlay_pressure_badge(&alert));
                ui.label(
                    RichText::new(format!(
                        "{} (span #{}, {:.1}s ago, budget {} pages)",
                        alert.describe(),
                        alert.id(),
                        alert.age_secs(),
                        alert.overlay_budget_pages
                    ))
                    .small()
                    .weak(),
                );
                if ui.small_button("Copy QA JSON").clicked() {
                    let summary = self.overlay_pressure_span_summary(&alert);
                    self.log_qa_span_copy(&alert, &summary);
                }
                if ui.small_button("Replay budget span").clicked() {
                    self.execute_timeline_kind(&RegressionSnapshotTimelineKind::OverlayAlert(
                        alert.clone(),
                    ));
                }
                if ui.small_button("Pin span").clicked() {
                    let entry = RegressionSnapshotTimelineEntry {
                        kind: RegressionSnapshotTimelineKind::OverlayAlert(alert.clone()),
                        timestamp: alert.timestamp,
                    };
                    let history_entry = TimelineHistoryEntry::from_entry(&entry);
                    self.pin_timeline_entry(&history_entry);
                }
                if ui.small_button("Open overlay diagnostics").clicked() {
                    self.overlay_pressure_focus = true;
                    ui.ctx().request_repaint();
                }
            });
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

    fn render_panels(
        &mut self,
        ctx: &Context,
        state: &AppState,
        reader_snapshot: Option<&ReaderSnapshot>,
    ) {
        let panels = state
            .session
            .session
            .as_ref()
            .map(|session| session.panels)
            .unwrap_or_default();
        let show_search_panel = self.pending_search_focus
            || !state.reader_ui.search_query.trim().is_empty()
            || !state.reader_ui.search_matches.is_empty();
        SidePanel::left("panel_toggle").show(ctx, |ui| {
            ui.heading("Panels");
            if ui
                .button("Toggle settings (AppCommand::ToggleSettingsPanel)")
                .clicked()
            {
                self.execute_command(AppCommand::ToggleSettingsPanel);
            }
            if ui
                .button("Toggle stats (AppCommand::ToggleStatsPanel)")
                .clicked()
            {
                self.execute_command(AppCommand::ToggleStatsPanel);
            }
            if ui
                .button("Toggle TTS (AppCommand::ToggleTtsPanel)")
                .clicked()
            {
                self.execute_command(AppCommand::ToggleTtsPanel);
            }
            ui.label(format!("Settings: {}", panels.show_settings));
            ui.label(format!("Stats: {}", panels.show_stats));
            ui.label(format!("TTS: {}", panels.show_tts));
            ui.label(format!("Search: {}", show_search_panel));
            if panels.show_settings {
                ui.separator();
                ui.heading("Settings");
                self.render_settings_sidebar(ui, reader_snapshot);
            }
            if panels.show_stats {
                ui.separator();
                ui.heading("Stats");
                self.render_stats_panel(ui, reader_snapshot);
            }
            if show_search_panel {
                ui.separator();
                ui.heading("Search");
                self.render_search_panel(ui, state);
            }
            if panels.show_tts {
                ui.separator();
                ui.heading("TTS");
                if let Some(snapshot) = reader_snapshot {
                    self.render_tts_widget(ui, snapshot);
                } else {
                    ui.label("No reader session.");
                }
            }
            ui.separator();
            ui.heading("Status diagnostics");
            self.render_status_diagnostics_panel(ui, state);
            self.render_anchor_diagnostics(ui, reader_snapshot);
        });
        SidePanel::right("shortcuts").show(ctx, |ui| {
            ui.heading("Shortcut registry");
            for binding in self.runtime.shortcut_registry().bindings() {
                ui.label(format!("{} → {:?}", binding.combo, binding.action));
            }
        });
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
                ui.label(format!("Theme: {:?}", settings.theme));
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

    fn refresh_anchor_diagnostics(&mut self, snapshot: Option<&ReaderSnapshot>) {
        if let Some(snapshot) = snapshot {
            self.anchor_diagnostics.refresh(snapshot);
        } else {
            self.anchor_diagnostics.clear();
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

    fn render_center(&mut self, ctx: &Context, state: &AppState) {
        CentralPanel::default().show(ctx, |ui| {
            match state.session.session.as_ref().map(|session| session.mode) {
                Some(UiMode::Reader) => self.render_reader_content(ui, state),
                _ => self.render_starter_content(ui, state),
            }
            if self.pending_search_focus {
                ui.label("Search field would be focused (shortcut handled).");
                self.pending_search_focus = false;
            }
            if let Some(plan) = self.last_plan.as_ref() {
                ui.separator();
                ui.label(format!(
                    "Last command: {} ({} effects)",
                    plan.action,
                    plan.effects.len()
                ));
            }
        });
    }

    fn render_starter_content(&mut self, ui: &mut Ui, state: &AppState) {
        let model = StarterViewModel::from_state(state);
        ui.heading("Starter shell");
        ui.add_space(8.0);
        if self.layout_policy.is_narrow() {
            self.render_starter_open_controls(ui, &model);
            self.render_starter_recents(ui, &model);
            self.render_starter_calibre(ui, &model);
            self.render_starter_browser_tabs(ui, &model);
        } else {
            ui.columns(2, |columns| {
                self.render_starter_open_controls(&mut columns[0], &model);
                self.render_starter_recents(&mut columns[0], &model);
                self.render_starter_calibre(&mut columns[1], &model);
                self.render_starter_browser_tabs(&mut columns[1], &model);
            });
        }
        ui.add_space(8.0);
        self.render_starter_diagnostics(ui, &model);
    }

    fn render_starter_open_controls(&mut self, ui: &mut Ui, model: &StarterViewModel<'_>) {
        ui.group(|ui| {
            ui.label("Open source");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.starter_open_path_input)
                        .hint_text("Path to file"),
                );
                if ui.button("Open file").clicked() {
                    let path = self.starter_open_path_input.trim().to_string();
                    if path.is_empty() {
                        warn!("Starter open path empty");
                        self.push_status("Starter: open path is empty".to_string());
                    } else {
                        trace!(path = %path, "Starter open path");
                        self.execute_command(AppCommand::OpenSourcePath { path });
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Open clipboard").clicked() {
                    trace!("Starter open clipboard");
                    self.execute_command(AppCommand::OpenClipboard);
                }
                if model.operations.source_open {
                    ui.label("Opening…");
                }
            });
            ui.add_space(6.0);
            ui.label("Open clipboard text");
            ui.add(
                egui::TextEdit::multiline(&mut self.starter_clipboard_text_input)
                    .hint_text("Paste text to open")
                    .desired_rows(3),
            );
            if ui.button("Open clipboard text").clicked() {
                let text = self.starter_clipboard_text_input.trim().to_string();
                if text.is_empty() {
                    warn!("Starter clipboard text empty");
                    self.push_status("Starter: clipboard text is empty".to_string());
                } else {
                    trace!(bytes = text.len(), "Starter open clipboard text");
                    self.execute_command(AppCommand::OpenClipboardText { text });
                }
            }
        });
    }

    fn render_starter_recents(&mut self, ui: &mut Ui, model: &StarterViewModel<'_>) {
        ui.add_space(8.0);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label("Recents");
                if ui.button("Refresh").clicked() {
                    trace!("Starter refresh recents");
                    self.execute_command(AppCommand::RefreshRecents { limit: Some(30) });
                }
                if model.loading_recents || model.operations.starter_command {
                    ui.label("Loading…");
                }
            });
            if model.recents.is_empty() && !model.loading_recents {
                ui.label("No recent books yet.");
                return;
            }
            ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                for recent in model.recents {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(&recent.display_title);
                        ui.add_space(6.0);
                        ui.label(format_relative_unix_secs(recent.last_opened_unix_secs));
                    });
                    ui.label(&recent.snippet);
                    ui.label(&recent.source_path);
                    if let Some(tab_id) = recent.browser_tab_id {
                        ui.label(format!("Browser tab: {}", tab_id));
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Open").clicked() {
                            trace!(path = %recent.source_path, "Starter open recent");
                            self.execute_command(AppCommand::OpenSourcePath {
                                path: recent.source_path.clone(),
                            });
                        }
                        if ui.button("Delete").clicked() {
                            let close_browser_tab = model
                                .bootstrap
                                .map(|bootstrap| bootstrap.config.close_browser_tab_on_recent_delete)
                                .unwrap_or(false)
                                && recent.browser_tab_id.is_some();
                            trace!(
                                path = %recent.source_path,
                                close_browser_tab,
                                "Starter delete recent"
                            );
                            self.execute_command(AppCommand::DeleteRecent {
                                source_path: recent.source_path.clone(),
                                close_browser_tab,
                            });
                        }
                    });
                }
            });
        });
    }

    fn render_starter_calibre(&mut self, ui: &mut Ui, model: &StarterViewModel<'_>) {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label("Calibre");
                if ui.button("Refresh").clicked() {
                    trace!(force = self.starter_calibre_force_refresh, "Starter refresh Calibre");
                    self.execute_command(AppCommand::LoadCalibreBooks {
                        force_refresh: self.starter_calibre_force_refresh,
                    });
                }
                ui.checkbox(&mut self.starter_calibre_force_refresh, "Force refresh");
                if model.loading_calibre || model.operations.calibre_load {
                    ui.label("Loading…");
                }
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.starter_calibre_query)
                        .hint_text("Search title or author"),
                );
                ComboBox::from_id_source("calibre_sort")
                    .selected_text(self.starter_calibre_sort.label())
                    .show_ui(ui, |ui| {
                        for option in CalibreSort::OPTIONS {
                            ui.selectable_value(&mut self.starter_calibre_sort, option, option.label());
                        }
                    });
            });
            if model.calibre_books.is_empty() && !model.loading_calibre {
                ui.label("No Calibre books loaded.");
                return;
            }
            let query = self.starter_calibre_query.trim().to_lowercase();
            let mut books: Vec<CalibreBookDto> = model.calibre_books.to_vec();
            if !query.is_empty() {
                books.retain(|book| {
                    book.title.to_lowercase().contains(&query)
                        || book.authors.to_lowercase().contains(&query)
                });
            }
            match self.starter_calibre_sort {
                CalibreSort::Title => {
                    books.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
                }
                CalibreSort::Author => {
                    books.sort_by(|a, b| a.authors.to_lowercase().cmp(&b.authors.to_lowercase()))
                }
                CalibreSort::Year => {
                    books.sort_by(|a, b| b.year.cmp(&a.year).then_with(|| a.title.cmp(&b.title)))
                }
            }
            ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                for book in books {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(&book.title);
                        if let Some(year) = book.year {
                            ui.label(format!("({year})"));
                        }
                    });
                    ui.label(&book.authors);
                    ui.horizontal(|ui| {
                        ui.label(format!("{} • {}", book.extension, format_bytes(book.file_size_bytes)));
                        if book.cover_thumbnail.is_some() {
                            ui.label("Thumbnail cached");
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Open").clicked() {
                            trace!(id = book.id, "Starter open Calibre book");
                            self.execute_command(AppCommand::OpenCalibreBook { id: book.id });
                        }
                        if ui.button("Ensure thumbnail").clicked() {
                            trace!(id = book.id, "Starter ensure Calibre thumbnail");
                            self.execute_command(AppCommand::EnsureCalibreThumbnail { id: book.id });
                        }
                    });
                }
            });
        });
    }

    fn render_starter_browser_tabs(&mut self, ui: &mut Ui, model: &StarterViewModel<'_>) {
        ui.add_space(8.0);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label("Browser tabs");
                if ui.button("Health").clicked() {
                    trace!("Starter load browser tabs health");
                    self.execute_command(AppCommand::LoadBrowserTabsHealth);
                }
                if ui.button("List windows").clicked() {
                    trace!("Starter load browser tab windows");
                    self.execute_command(AppCommand::ListBrowserTabWindows);
                }
                if ui.button("List tabs").clicked() {
                    let window_id = self
                        .starter_browser_window_id_input
                        .trim()
                        .parse::<u64>()
                        .ok();
                    let query = self.starter_browser_tab_query.trim();
                    trace!(window_id = ?window_id, query = %query, refresh = self.starter_browser_tabs_force_refresh, "Starter load browser tabs");
                    self.execute_command(AppCommand::ListBrowserTabs {
                        window_id,
                        query: if query.is_empty() { None } else { Some(query.to_string()) },
                        refresh: self.starter_browser_tabs_force_refresh,
                    });
                }
                ui.checkbox(&mut self.starter_browser_tabs_force_refresh, "Force refresh");
                if model.loading_browser_tabs || model.operations.browser_tab_refresh {
                    ui.label("Loading…");
                }
            });
            let browser_tabs_enabled = model
                .bootstrap
                .map(|bootstrap| bootstrap.config.browser_tabs_enabled)
                .unwrap_or(true);
            if !browser_tabs_enabled {
                ui.colored_label(Color32::YELLOW, "Browser tabs are disabled in config.");
            }
            match model.browser_tabs_health {
                Some(health) => {
                    if !health.ok {
                        ui.colored_label(Color32::RED, "Browser tabs service offline.");
                    } else if !health.extension_connected {
                        ui.colored_label(Color32::YELLOW, "Browser extension disconnected.");
                    } else {
                        ui.label("Browser tabs service healthy.");
                    }
                }
                None => {
                    ui.label("Browser tabs health unknown.");
                }
            }
            if !model.browser_tabs_windows.is_empty() {
                let mut selected_window = self
                    .starter_browser_window_id_input
                    .trim()
                    .parse::<u64>()
                    .ok();
                ComboBox::from_id_source("browser_window_select")
                    .selected_text(
                        selected_window
                            .map(|id| format!("Window {}", id))
                            .unwrap_or_else(|| "All windows".to_string()),
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut selected_window, None, "All windows");
                        for window in model.browser_tabs_windows {
                            let label = if window.focused {
                                format!("Window {} (focused)", window.id)
                            } else {
                                format!("Window {}", window.id)
                            };
                            ui.selectable_value(&mut selected_window, Some(window.id), label);
                        }
                    });
                match selected_window {
                    Some(id) => self.starter_browser_window_id_input = id.to_string(),
                    None => self.starter_browser_window_id_input.clear(),
                }
            } else {
                ui.add(
                    egui::TextEdit::singleline(&mut self.starter_browser_window_id_input)
                        .hint_text("Window id (optional)"),
                );
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.starter_browser_tab_id_input)
                    .hint_text("Tab id"),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.starter_browser_tab_query)
                    .hint_text("Search/filter tabs"),
            );
            ui.horizontal(|ui| {
                if ui.button("Open tab").clicked() {
                    self.dispatch_browser_tab_open(false);
                }
                if ui.button("Import bundle").clicked() {
                    self.dispatch_browser_tab_open(true);
                }
                if ui.button("Refresh tab").clicked() {
                    self.dispatch_browser_tab_refresh();
                }
            });
            if model.browser_tabs_tabs.is_empty() && !model.loading_browser_tabs {
                ui.label("No browser tabs loaded.");
                return;
            }
            let query = self.starter_browser_tab_query.trim().to_lowercase();
            ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                for tab in model.browser_tabs_tabs {
                    if !query.is_empty()
                        && !tab.title.to_lowercase().contains(&query)
                        && !tab.url.to_lowercase().contains(&query)
                    {
                        continue;
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(&tab.title);
                        if tab.active.unwrap_or(false) {
                            ui.label("(active)");
                        }
                    });
                    ui.label(&tab.url);
                    ui.label(format!("Tab {} • Window {}", tab.id, tab.window_id));
                    ui.horizontal(|ui| {
                        if ui.button("Open").clicked() {
                            trace!(tab_id = tab.id, window_id = tab.window_id, "Starter open browser tab from list");
                            self.execute_command(AppCommand::OpenBrowserTab {
                                tab_id: tab.id,
                                window_id: Some(tab.window_id),
                            });
                        }
                        if ui.button("Import bundle").clicked() {
                            trace!(tab_id = tab.id, window_id = tab.window_id, "Starter import browser tab bundle from list");
                            self.execute_command(AppCommand::OpenBrowserTabBundle {
                                tab_id: tab.id,
                                window_id: Some(tab.window_id),
                            });
                        }
                        if ui.button("Refresh").clicked() {
                            trace!(tab_id = tab.id, window_id = tab.window_id, "Starter refresh browser tab from list");
                            self.execute_command(AppCommand::RefreshBrowserTab {
                                tab_id: tab.id,
                                window_id: Some(tab.window_id),
                            });
                        }
                    });
                }
            });
        });
    }

    fn render_starter_diagnostics(&mut self, ui: &mut Ui, model: &StarterViewModel<'_>) {
        ui.group(|ui| {
            ui.label("Starter diagnostics");
            if let Some(event) = model.source_open_event {
                ui.label(format!(
                    "Source open: {} ({})",
                    event.phase,
                    event.message.clone().unwrap_or_else(|| "no message".to_string())
                ));
            }
            if let Some(event) = model.calibre_load_event {
                ui.label(format!(
                    "Calibre load: {} (count {:?})",
                    event.phase,
                    event.count
                ));
            }
            if let Some(health) = model.browser_tabs_health {
                ui.label(format!(
                    "Browser tabs health: ok={} extension_connected={}",
                    health.ok, health.extension_connected
                ));
            }
            ui.horizontal(|ui| {
                ui.label(format!("source_open: {}", model.operations.source_open));
                ui.label(format!("starter: {}", model.operations.starter_command));
                ui.label(format!("calibre: {}", model.operations.calibre_load));
                ui.label(format!("browser_tabs: {}", model.operations.browser_tab_refresh));
            });
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

    fn dispatch_browser_tab_refresh(&mut self) {
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
        trace!(tab_id, window_id = ?window_id, "Starter refresh browser tab");
        self.execute_command(AppCommand::RefreshBrowserTab { tab_id, window_id });
    }

    fn render_reader_content(&mut self, ui: &mut Ui, state: &AppState) {
        if let Some(snapshot) = state.reader_document.snapshot.as_ref() {
            trace!(
                page = snapshot.current_page,
                highlight = ?snapshot.highlighted_sentence_idx,
                sentences = snapshot.sentences.len(),
                "rendering reader shell content"
            );
            ui.heading("Reader shell");
            self.render_quick_actions_dock(ui);
            self.render_tts_widget(ui, snapshot);
            ui.separator();
            self.render_reader_summary(ui, snapshot);
            ui.add_space(6.0);
            self.render_sentence_list(ui, snapshot);
            ui.add_space(6.0);
            self.render_canonical_preview(ui, snapshot);
            ui.add_space(6.0);
            self.render_pdf_diagnostics(ui, snapshot);
            ui.add_space(6.0);
            if ui
                .button("Close reader session (AppCommand::CloseReaderSession)")
                .clicked()
            {
                self.execute_command(AppCommand::CloseReaderSession);
                self.show_reader_confirm_modal = true;
            }
        } else {
            ui.heading("Reader shell");
            ui.label("No reader session currently active.");
        }
    }

    fn render_quick_actions_dock(&mut self, ui: &mut Ui) {
        ui.group(|ui| {
            ui.label("Quick actions");
            ui.horizontal(|ui| {
                if ui.button("Play/Pause").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::TtsTogglePlayPause,
                    ));
                }
                if ui.button("Prev").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::TtsSeekPrev,
                    ));
                }
                if ui.button("Next").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::TtsSeekNext,
                    ));
                }
                if ui.button("Repeat").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::TtsRepeatSentence,
                    ));
                }
            });
        });
    }

    fn render_tts_widget(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        ui.group(|ui| {
            ui.label("TTS controls");
            ui.horizontal(|ui| {
                if ui.button("Play").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(SessionCommand::TtsPlay));
                }
                if ui.button("Pause").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(SessionCommand::TtsPause));
                }
                if ui.button("Stop").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(SessionCommand::TtsStop));
                }
                if ui.button("Repeat").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::TtsRepeatSentence,
                    ));
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Play from page").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::TtsPlayFromPageStart,
                    ));
                }
                if ui.button("Play from highlight").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::TtsPlayFromHighlight,
                    ));
                }
                if ui.button("Prev sentence").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::TtsSeekPrev,
                    ));
                }
                if ui.button("Next sentence").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::TtsSeekNext,
                    ));
                }
            });
            ui.horizontal(|ui| {
                let mut tts_speed = snapshot.settings.tts_speed;
                if ui
                    .add(Slider::new(&mut tts_speed, 0.5..=2.5).text("Speed"))
                    .changed()
                {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::ApplySettings {
                            patch: ReaderSettingsPatch {
                                tts_speed: Some(tts_speed),
                                ..Default::default()
                            },
                        },
                    ));
                }
                let mut tts_volume = snapshot.settings.tts_volume;
                if ui
                    .add(Slider::new(&mut tts_volume, 0.0..=2.0).text("Volume"))
                    .changed()
                {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::ApplySettings {
                            patch: ReaderSettingsPatch {
                                tts_volume: Some(tts_volume),
                                ..Default::default()
                            },
                        },
                    ));
                }
            });
            ui.horizontal(|ui| {
                ui.label(format!(
                    "TTS progress: {:.1}%",
                    snapshot.tts.progress_pct
                ));
                ui.separator();
                ui.label(format!(
                    "Page ETA: {}",
                    format_duration_secs(snapshot.stats.page_time_remaining_secs)
                ));
                ui.separator();
                ui.label(format!(
                    "Book ETA: {}",
                    format_duration_secs(snapshot.stats.book_time_remaining_secs)
                ));
            });
            if let Some(event) = self.last_tts_runtime_event.as_ref() {
                ui.horizontal(|ui| {
                    ui.label(format!("Last TTS event: {:?}", event.kind));
                    ui.separator();
                    ui.label(event.action.as_str());
                    if let Some(message) = event.message.as_ref() {
                        ui.separator();
                        ui.label(message);
                    }
                });
            } else {
                ui.label("Last TTS event: none");
            }
        });
    }

    fn render_stats_panel(&mut self, ui: &mut Ui, snapshot: Option<&ReaderSnapshot>) {
        let Some(snapshot) = snapshot else {
            ui.label("No reader session.");
            return;
        };
        ui.label(format!(
            "Page {} / {}",
            snapshot.stats.page_index + 1,
            snapshot.stats.total_pages
        ));
        ui.label(format!(
            "Page progress: {:.1}%",
            snapshot.stats.page_end_percent * 100.0
        ));
        ui.label(format!(
            "Book progress: {:.1}%",
            snapshot.stats.global_progress_pct * 100.0
        ));
        ui.label(format!(
            "Page ETA: {}",
            format_duration_secs(snapshot.stats.page_time_remaining_secs)
        ));
        ui.label(format!(
            "Book ETA: {}",
            format_duration_secs(snapshot.stats.book_time_remaining_secs)
        ));
    }

    fn render_search_panel(&mut self, ui: &mut Ui, state: &AppState) {
        ui.label(format!(
            "Query: {}",
            if state.reader_ui.search_query.is_empty() {
                "none"
            } else {
                &state.reader_ui.search_query
            }
        ));
        ui.label(format!(
            "Matches: {}",
            state.reader_ui.search_matches.len()
        ));
        if ui.button("Focus search").clicked() {
            self.pending_search_focus = true;
            self.push_status("Search focus requested".to_string());
        }
    }

    fn render_status_diagnostics_panel(&mut self, ui: &mut Ui, state: &AppState) {
        ui.label(format!("Active mode: {:?}", self.shell_state.active_mode));
        ui.label(format!(
            "Layout: {:?}",
            self.layout_policy.size_class
        ));
        ui.label(format!("Focus owner: {:?}", self.shell_state.focus_owner));
        ui.label(format!("Busy: {}", state.app_shell.busy));
        ui.label(format!(
            "Operations: source_open={}, calibre_load={}, browser_tabs={}",
            state.app_shell.operations.source_open,
            state.app_shell.operations.calibre_load,
            state.app_shell.operations.browser_tab_refresh
        ));
    }

    fn render_reader_summary(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        let anchor_hits = snapshot
            .sentence_anchor_map
            .iter()
            .filter(|value| value.is_some())
            .count();
        let progress_pct = (snapshot.tts.progress_pct * 100.0).max(0.0);
        ui.horizontal(|ui| {
            ui.label(format!(
                "Page {}/{}",
                snapshot.current_page + 1,
                snapshot.total_pages
            ));
            ui.separator();
            ui.label(format!(
                "Mode: {}",
                if snapshot.text_only_mode {
                    "text-only".to_string()
                } else {
                    format!("pretty ({:?})", snapshot.pretty_kind)
                }
            ));
            ui.separator();
            ui.label(format!(
                "TTS: {:?} ({:.0}% progress)",
                snapshot.tts.state, progress_pct
            ));
        });
        ui.horizontal(|ui| {
            let highlighted = snapshot
                .highlighted_sentence_idx
                .map(|idx| format!("{}", idx + 1))
                .unwrap_or_else(|| "none".to_string());
            ui.label(format!("Highlighted sentence: {}", highlighted));
            ui.separator();
            ui.label(format!("Search matches: {}", snapshot.search_matches.len()));
            ui.separator();
            ui.label(format!(
                "Anchors mapped: {}/{}",
                anchor_hits,
                snapshot.sentence_anchor_map.len()
            ));
        });
        if let Some(decision) = self.overlay_diagnostics.preview_decision() {
            let reason = if decision.allowed {
                "overlays rendering"
            } else if !decision.highlight_page_has_text_layer {
                "no text layer to honor overlay budget"
            } else {
                "budget exhausted"
            };
            let badge = if decision.allowed { "✅" } else { "⚠️" };
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{} Overlay budget: {} pages, {} overlays drawn ({})",
                    badge, decision.budget_pages, decision.overlays_drawn, reason
                ));
            });
        }
        if snapshot.pretty_kind == PrettyKind::Pdf {
            let overlay_budget = self.pdf_render_state.overlay_budget_pages();
            let highlight_ready = self.highlight_page_has_text_layer();
            let overlay_status = if overlay_budget > 0 {
                "Overlay budget available"
            } else {
                "Overlay budget exhausted"
            };
            let overlay_color = if highlight_ready && overlay_budget > 0 {
                Color32::from_rgb(130, 210, 170)
            } else {
                Color32::from_rgb(220, 130, 110)
            };
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} — cached rects: {}, highlight layer ready: {}",
                        overlay_status,
                        self.pdf_render_state.overlay_rects.len(),
                        highlight_ready
                    ))
                    .color(overlay_color)
                    .strong(),
                );
                if overlay_budget == 0 {
                    ui.label(
                        RichText::new("Budget blocked")
                            .color(Color32::from_rgb(220, 180, 120))
                            .small(),
                    );
                }
            });
        }
        if let Some(alert) = self
            .pdf_render_state
            .recent_overlay_pressure_alerts()
            .last()
            .cloned()
        {
            ui.horizontal(|ui| {
                ui.label(self.overlay_pressure_badge(&alert));
                ui.label(
                    RichText::new(format!(
                        "Overlay pressure on page {} (budget {} pages, {:.1}s ago)",
                        alert.kind.page_index() + 1,
                        alert.overlay_budget_pages,
                        alert.age_secs()
                    ))
                    .small(),
                );
                if ui
                    .small_button("Inspect PDF diagnostics")
                    .on_hover_text("Highlight the diagnostics panel to replay the pressure span")
                    .clicked()
                {
                    self.overlay_pressure_focus = true;
                }
            });
        }
    }

    fn render_sentence_list(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        if snapshot.sentences.is_empty() {
            ui.label("No sentences available for this page.");
            return;
        }
        let highlight_color = Self::sentence_highlight_color(snapshot);
        let anchor_hits = snapshot
            .sentence_anchor_map
            .iter()
            .filter(|value| value.is_some())
            .count();
        trace!(anchor_hits = anchor_hits, "rendering sentence list");
        let anchor_info = self.anchor_diagnostics.entries().to_vec();
        let auto_scroll_enabled = self.should_auto_scroll(snapshot);
        if !auto_scroll_enabled {
            self.auto_scroll_state.reset();
        }
        let auto_scroll_align = if snapshot.settings.center_spoken_sentence {
            Align::Center
        } else {
            Align::Min
        };
        let scroll_response = ScrollArea::vertical()
            .auto_shrink([false, true])
            .id_source("reader-sentence-scroll")
            .show(ui, |ui| {
                for (idx, sentence) in snapshot.sentences.iter().enumerate() {
                    let is_highlighted = snapshot.highlighted_sentence_idx == Some(idx);
                    let is_search_match = snapshot.search_matches.contains(&idx);
                    let anchor_idx = snapshot
                        .sentence_anchor_map
                        .get(idx)
                        .and_then(|value| *value);
                    let canonical_preview = anchor_idx.and_then(|anchor| {
                        snapshot
                            .canonical_sentences
                            .get(anchor)
                            .map(|text| (anchor, text))
                    });
                    let anchor_meta = anchor_info
                        .get(idx)
                        .copied()
                        .unwrap_or_else(AnchorInfo::missing);
                    let overlay_available = snapshot.pdf_ocr_alignment.is_some();
                    let overlay_highlightable_sentences = snapshot
                        .pdf_ocr_alignment
                        .as_ref()
                        .map(|alignment| alignment.highlightable_sentence_count)
                        .unwrap_or(0);
                    let overlay_budget_pages = self.pdf_render_state.overlay_budget_pages();
                    let overlay_eviction_count = self
                        .pdf_render_state
                        .decision
                        .as_ref()
                        .map(|decision| decision.evict_text_layer_page_indexes.len())
                        .unwrap_or(0);
                    if is_highlighted {
                        let highlight_page = Self::page_index_for_global_sentence(
                            &snapshot.page_sentence_counts,
                            Some(idx),
                        )
                        .unwrap_or(snapshot.current_page);
                        let overlay_geometry = Self::global_sentence_index(snapshot, idx)
                            .and_then(|global_idx| {
                                self.pdf_render_state
                                    .overlay_geometry_for_sentence(&snapshot.source_path, global_idx)
                            });
                        let overlay_rects = overlay_geometry
                            .as_ref()
                            .map(|entry| entry.rects.clone())
                            .unwrap_or_default();
                        let overlay_reason = overlay_geometry
                            .as_ref()
                            .and_then(|entry| entry.reason.clone());
                        self.pdf_render_state.set_highlighted_page(
                            highlight_page,
                            Some(idx),
                            overlay_rects,
                            overlay_reason.clone(),
                        );
                    }
                    let mut label_text = format!("{}: {}", idx + 1, sentence);
                    if is_search_match {
                        label_text.push_str(" (search match)");
                    }
                    let mut text = RichText::new(label_text).size(14.0);
                    if is_highlighted {
                        text = text.text_style(egui::TextStyle::Body);
                    }
                    let button = Button::new(text)
                        .fill(if is_highlighted {
                            highlight_color
                        } else {
                            ui.visuals().widgets.inactive.bg_fill
                        })
                        .wrap(true);
                    let response = ui.add(button);
                    if is_highlighted && auto_scroll_enabled {
                        match self
                            .auto_scroll_state
                            .decide_scroll(idx, anchor_meta.fallback)
                        {
                            ScrollDecision::Scroll => {
                                let scroll_alignment_label =
                                    if snapshot.settings.center_spoken_sentence {
                                        "center"
                                    } else {
                                        "top"
                                    };
                                let overlay_snapshot = self.capture_overlay_decision();
                                let overlay_span =
                                    self.overlay_budget_span("auto-scroll", &overlay_snapshot);
                                let _overlay_enter = overlay_span.enter();
                                let jump_span = tracing::span!(
                                    Level::TRACE,
                                    "JumpToSentence",
                                    budget_plan = "shell.performance_budget",
                                    anchor_path = anchor_meta.fallback.label(),
                                    target_sentence = idx,
                                    command = "reader.highlight",
                                    auto_scroll = true,
                                    scroll_alignment = scroll_alignment_label,
                                    canonical_anchor = ?anchor_meta.anchor,
                                    overlay_available = overlay_available,
                                    overlay_highlightable_sentences = overlay_highlightable_sentences,
                                    overlay_budget_pages = overlay_budget_pages,
                                    overlay_eviction_count = overlay_eviction_count,
                                );
                                let _enter = jump_span.enter();
                                trace!(
                                    jump_to_sentence = idx,
                                    highlight_anchor = anchor_meta.fallback.label(),
                                    canonical_anchor = ?anchor_meta.anchor,
                                    "JumpToSentence: auto-scrolling highlighted sentence"
                                );
                                self.auto_scroll_state.note_auto_scroll();
                                response.scroll_to_me(Some(auto_scroll_align));
                                self.auto_scroll_state.record(idx, anchor_meta.fallback);
                                self.overlay_diagnostics.record_jump("auto-scroll", overlay_snapshot);
                            }
                            ScrollDecision::Blocked(reason) => {
                                trace!(
                                    jump_to_sentence = idx,
                                    reason = ?reason,
                                    "JumpToSentence suppressed"
                                );
                            }
                        }
                    }
                    if response.clicked() {
                        trace!(sentence_idx = idx, anchor = ?anchor_idx, "reader sentence clicked");
                        let overlay_snapshot = self.capture_overlay_decision();
                        let overlay_span =
                            self.overlay_budget_span("sentence-click", &overlay_snapshot);
                        let _overlay_enter = overlay_span.enter();
                        let manual_span = tracing::span!(
                            Level::TRACE,
                            "JumpToSentence",
                            budget_plan = "shell.performance_budget",
                            anchor_path = anchor_meta.fallback.label(),
                            target_sentence = idx,
                            command = "reader.sentence_click",
                            auto_scroll = false,
                            scroll_alignment = "manual",
                            canonical_anchor = ?anchor_meta.anchor,
                            overlay_available = overlay_available,
                            overlay_highlightable_sentences = overlay_highlightable_sentences,
                            overlay_budget_pages = overlay_budget_pages,
                            overlay_eviction_count = overlay_eviction_count,
                        );
                        let _enter = manual_span.enter();
                        trace!(
                            jump_to_sentence = idx,
                            highlight_anchor = anchor_meta.fallback.label(),
                            canonical_anchor = ?anchor_meta.anchor,
                            "JumpToSentence: manual sentence click"
                        );
                        self.execute_reader_command(ReaderCommand::Session(
                            SessionCommand::SentenceClick { sentence_idx: idx },
                        ));
                        self.overlay_diagnostics
                            .record_jump("sentence-click", overlay_snapshot);
                    }
                    let fallback_label = anchor_meta.fallback.label();
                    if let Some((anchor, canonical)) = canonical_preview {
                        ui.label(
                            RichText::new(format!(
                                "anchor {} → {} ({})",
                                anchor, canonical, fallback_label
                            ))
                            .small()
                            .italics()
                            .weak(),
                        );
                    } else if let Some(anchor) = anchor_idx {
                        ui.label(
                            RichText::new(format!("anchor {} ({})", anchor, fallback_label))
                                .small()
                                .italics()
                                .weak(),
                        );
                    } else {
                        ui.label(
                            RichText::new(format!("anchor missing ({})", fallback_label))
                                .small()
                                .italics()
                                .weak(),
                        );
                    }
                    ui.separator();
                }
            });
        let offset = scroll_response.state.offset;
        let manual_scroll_delta = self
            .sentence_scroll_offset
            .map(|last| offset - last)
            .unwrap_or(Vec2::ZERO);
        let offset_changed = self
            .sentence_scroll_offset
            .map(|last| offset != last)
            .unwrap_or(false);
        self.sentence_scroll_offset = Some(offset);
        let auto_scroll_this_frame = self.auto_scroll_state.consume_auto_scroll();
        let overlay_available = snapshot.pdf_ocr_alignment.is_some();
        let overlay_highlightable_sentences = snapshot
            .pdf_ocr_alignment
            .as_ref()
            .map(|alignment| alignment.highlightable_sentence_count)
            .unwrap_or(0);
        let overlay_budget_pages = self.pdf_render_state.overlay_budget_pages();
        let overlay_eviction_count = self
            .pdf_render_state
            .decision
            .as_ref()
            .map(|decision| decision.evict_text_layer_page_indexes.len())
            .unwrap_or(0);
        if offset_changed
            && !auto_scroll_this_frame
            && manual_scroll_delta != Vec2::ZERO
            && snapshot.highlighted_sentence_idx.is_some()
        {
            let highlighted_idx = snapshot.highlighted_sentence_idx;
            let anchor_meta = highlighted_idx
                .and_then(|idx| anchor_info.get(idx).copied())
                .unwrap_or_else(AnchorInfo::missing);
            let overlay_snapshot = self.capture_overlay_decision();
            let overlay_span = self.overlay_budget_span("manual-scroll", &overlay_snapshot);
            let _overlay_enter = overlay_span.enter();
            let manual_span = tracing::span!(
                Level::TRACE,
                "JumpToSentence",
                budget_plan = "shell.performance_budget",
                anchor_path = anchor_meta.fallback.label(),
                target_sentence = ?highlighted_idx,
                command = "reader.scroll",
                auto_scroll = false,
                scroll_alignment = "manual",
                scroll_delta_y = manual_scroll_delta.y,
                canonical_anchor = ?anchor_meta.anchor,
                overlay_available = overlay_available,
                overlay_highlightable_sentences = overlay_highlightable_sentences,
                overlay_budget_pages = overlay_budget_pages,
                overlay_eviction_count = overlay_eviction_count,
            );
            let _enter = manual_span.enter();
            trace!(
                scroll_delta = ?manual_scroll_delta,
                highlight_anchor = anchor_meta.fallback.label(),
                highlight_idx = ?highlighted_idx,
                overlay_available = overlay_available,
                overlay_highlightable_sentences = overlay_highlightable_sentences,
                overlay_budget_pages = overlay_budget_pages,
                overlay_eviction_count = overlay_eviction_count,
                "JumpToSentence: manual scroll request"
            );
            self.overlay_diagnostics
                .record_jump("manual-scroll", overlay_snapshot);
        }
    }

    fn should_auto_scroll(&self, snapshot: &ReaderSnapshot) -> bool {
        snapshot.settings.auto_scroll_tts && snapshot.tts.state == TtsPlaybackState::Playing
    }

    fn highlight_page_has_text_layer(&self) -> bool {
        self.pdf_render_state
            .highlighted_page
            .and_then(|page| {
                self.pdf_render_state
                    .surface_for_page(page)
                    .map(|surface| surface.text_layer_ready)
            })
            .unwrap_or(false)
    }

    fn capture_overlay_decision(&self) -> OverlayDecisionSnapshot {
        let highlight_has_text_layer = self.highlight_page_has_text_layer();
        let budget_pages = self.pdf_render_state.overlay_budget_pages();
        let overlay_rects_available = self.pdf_render_state.overlay_rects.len();
        let overlay_reason = self.pdf_render_state.overlay_alignment_reason.clone();
        OverlayDecisionSnapshot {
            allowed: highlight_has_text_layer && budget_pages > 0,
            budget_pages,
            overlays_drawn: self.pdf_render_state.rendered_overlays,
            highlight_page_has_text_layer: highlight_has_text_layer,
            highlight_page: self.pdf_render_state.highlighted_page,
            overlay_rects_available,
            overlay_reason,
        }
    }

    fn maybe_record_overlay_retry(
        &mut self,
        decision: &OverlayDecisionSnapshot,
        snapshot: &ReaderSnapshot,
    ) {
        if decision.allowed {
            return;
        }
        let reason_label = if !decision.highlight_page_has_text_layer {
            "text_layer_missing"
        } else if decision.budget_pages == 0 {
            "budget_exhausted"
        } else {
            "overlay_blocked"
        };
        self.record_regression_snapshot(
            RegressionScenario::OverlayBacklog {
                reason: reason_label,
            },
            Some(snapshot),
            Some(decision.clone()),
        );
        self.record_scheduler_event(SchedulerEventKind::RetryOverlay {
            reason: reason_label.to_string(),
            highlight_page: decision.highlight_page,
            budget_pages: decision.budget_pages,
            overlay_reason: decision.overlay_reason.clone(),
        });
    }

    fn record_scheduler_event(&mut self, kind: SchedulerEventKind) {
        let event = SchedulerEvent {
            timestamp: Instant::now(),
            kind,
        };
        if self
            .scheduler_events
            .last()
            .map(|last| last.kind == event.kind)
            .unwrap_or(false)
        {
            return;
        }
        self.scheduler_events.push(event.clone());
        if self.scheduler_events.len() > 8 {
            self.scheduler_events.remove(0);
        }
        let span = tracing::span!(
            Level::TRACE,
            "PdfSchedulerEvent",
            budget_plan = "shell.performance_budget",
            kind = ?event.kind,
            highlight_page = match &event.kind {
                SchedulerEventKind::RetryOverlay { highlight_page, .. } => highlight_page.map(|idx| idx + 1),
                _ => None,
            },
        );
        let _enter = span.enter();
        trace!(event = %event.kind.describe(), "PDF scheduler event recorded");
        self.push_budget_timeline_entry(
            RegressionSnapshotTimelineKind::SchedulerEvent(event.clone()),
            event.timestamp,
        );
    }

    fn overlay_budget_span(
        &self,
        event: &'static str,
        decision: &OverlayDecisionSnapshot,
    ) -> tracing::span::Span {
        tracing::span!(
            Level::TRACE,
            "OverlayBudgetDecision",
            budget_plan = "shell.performance_budget",
            overlay_budget_pages = decision.budget_pages,
            overlay_budget_allowed = decision.allowed,
            overlay_budget_drawn = decision.overlays_drawn,
            highlight_page = ?decision.highlight_page,
            highlight_page_text_layer = decision.highlight_page_has_text_layer,
            overlay_rect_count = decision.overlay_rects_available,
            overlay_alignment_reason = ?decision.overlay_reason.as_deref(),
            event = event,
        )
    }

    fn replay_overlay_span(&self, event: &'static str, decision: OverlayDecisionSnapshot) {
        let span = self.overlay_budget_span(event, &decision);
        let _enter = span.enter();
        trace!(decision = ?decision, "Replayed overlay budget decision for QA");
    }

    fn audio_budget_span(&self, event: &AudioBudgetEvent) -> tracing::span::Span {
        tracing::span!(
            Level::TRACE,
            "JumpToSentence",
            budget_plan = "shell.performance_budget",
            audio_command = event.command.as_str(),
            target_sentence = ?event.target_sentence,
            anchor_path = event.fallback.label(),
            anchor_index = ?event.anchor,
            auto_scroll = event.auto_scroll,
            overlay_budget_pages = event.overlay_snapshot.budget_pages,
            overlay_budget_allowed = event.overlay_snapshot.allowed,
            overlay_rect_count = event.overlay_snapshot.overlay_rects_available,
            overlay_alignment_reason = ?event.overlay_snapshot.overlay_reason.as_deref(),
            highlight_page = ?event.highlight_page,
        )
    }

    fn replay_audio_event(&self, event: &AudioBudgetEvent) {
        let span = self.audio_budget_span(event);
        let _enter = span.enter();
        trace!(event = %event.describe(), "Replayed audio budget span for QA");
    }

    fn audio_event_payload(&self, event: &AudioBudgetEvent, summary: &str) -> String {
        let payload = json!({
            "id": event.id,
            "command": event.command,
            "auto_scroll": event.auto_scroll,
            "target_sentence": event.target_sentence,
            "anchor_fallback": event.fallback.label(),
            "anchor_index": event.anchor,
            "overlay_budget_pages": event.overlay_snapshot.budget_pages,
            "overlay_allowed": event.overlay_snapshot.allowed,
            "highlight_page": event.highlight_page,
            "overlay_rects_cached": event.overlay_snapshot.overlay_rects_available,
            "overlay_reason": event.overlay_snapshot.overlay_reason,
            "summary": summary,
        });
        serde_json::to_string(&payload).unwrap_or_else(|_| summary.to_string())
    }

    fn log_qa_audio_copy(&mut self, event: &AudioBudgetEvent, summary: &str) {
        let payload = self.audio_event_payload(event, summary);
        self.push_status(format!("QA audio span copy: {}", payload));
    }

    fn apply_reader_settings_patch(
        &mut self,
        patch: ReaderSettingsPatch,
        description: &'static str,
    ) {
        let summary = format!("{:?}", patch);
        let span = tracing::span!(
            Level::TRACE,
            "ReaderSettingsChange",
            budget_plan = "shell.performance_budget",
            settings_action = description,
            patch = %summary,
        );
        let _enter = span.enter();
        self.record_settings_event(description, summary.clone());
        self.execute_reader_command(ReaderCommand::Session(SessionCommand::ApplySettings {
            patch,
        }));
    }

    fn record_settings_event(&mut self, description: &'static str, summary: String) {
        const MAX_EVENTS: usize = 12;
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

    fn replay_settings_event(&self, event: &SettingsTraceEvent) {
        let span = tracing::span!(
            Level::TRACE,
            "ReaderSettingsReplay",
            budget_plan = "shell.performance_budget",
            description = event.description,
            summary = event.summary.as_str(),
        );
        let _enter = span.enter();
        trace!(event = event.describe(), "Replayed settings change for QA");
    }

    fn settings_event_payload(&self, event: &SettingsTraceEvent) -> String {
        let payload = json!({
            "id": event.id,
            "description": event.description,
            "summary": event.summary,
            "roadmap_url": event.roadmap_url,
            "age_secs": event.age_secs(),
        });
        serde_json::to_string(&payload).unwrap_or_else(|_| event.summary.clone())
    }

    fn log_settings_trace_copy(&mut self, event: &SettingsTraceEvent) {
        let payload = self.settings_event_payload(event);
        self.push_status(format!("QA settings span copy: {}", payload));
    }

    fn trigger_persistence_flush(
        &mut self,
        trigger: PersistenceTrigger,
        description: &'static str,
    ) {
        let span = tracing::span!(
            Level::TRACE,
            "PersistenceFlush",
            budget_plan = "shell.performance_budget",
            trigger = ?trigger,
            description = description,
        );
        let _enter = span.enter();
        self.record_persistence_event(trigger, description);
        self.execute_command(AppCommand::FlushPersistence { trigger });
    }

    fn record_persistence_event(&mut self, trigger: PersistenceTrigger, description: &'static str) {
        const MAX_EVENTS: usize = 12;
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

    fn replay_persistence_event(&self, event: &PersistenceTraceEvent) {
        let span = tracing::span!(
            Level::TRACE,
            "PersistenceReplay",
            budget_plan = "shell.performance_budget",
            trigger = ?event.trigger,
            description = event.description,
        );
        let _enter = span.enter();
        trace!(event = event.describe(), "Replayed persistence span for QA");
    }

    fn persistence_event_payload(&self, event: &PersistenceTraceEvent) -> String {
        let payload = json!({
            "id": event.id,
            "description": event.description,
            "trigger": format!("{:?}", event.trigger),
            "roadmap_url": event.roadmap_url,
            "age_secs": event.age_secs(),
        });
        serde_json::to_string(&payload).unwrap_or_else(|_| event.describe())
    }

    fn log_persistence_trace_copy(&mut self, event: &PersistenceTraceEvent) {
        let payload = self.persistence_event_payload(event);
        self.push_status(format!("QA persistence span copy: {}", payload));
    }

    fn replay_regression_snapshot(&self, snapshot: &RegressionSnapshot) {
        let span = tracing::span!(
            Level::TRACE,
            "RegressionSnapshotReplay",
            budget_plan = "shell.performance_budget",
            scenario = snapshot.scenario.label(),
            snapshot_id = snapshot.id,
        );
        let _enter = span.enter();
        trace!(snapshot = %snapshot.describe(), "Replayed regression snapshot for QA");
    }

    fn regression_snapshot_payload(&self, snapshot: &RegressionSnapshot) -> String {
        let payload = json!({
            "id": snapshot.id,
            "scenario": snapshot.scenario.label(),
            "description": snapshot.describe(),
            "source_path": snapshot.source_path,
            "page": snapshot.current_page.map(|page| page + 1),
            "highlighted_sentence": snapshot.highlighted_sentence.map(|idx| idx + 1),
            "overlay_budget_pages": snapshot.overlay_snapshot.as_ref().map(|overlay| overlay.budget_pages),
            "overlay_reason": snapshot.overlay_snapshot.as_ref().and_then(|overlay| overlay.overlay_reason.clone()),
            "persistence_trigger": snapshot.scenario.persistence_trigger().map(|trigger| format!("{:?}", trigger)),
            "roadmap_url": snapshot.scenario.roadmap_url(),
            "qa_checklist": QA_REGRESSION_URL,
            "age_secs": snapshot.age_secs(),
        });
        serde_json::to_string(&payload).unwrap_or_else(|_| snapshot.describe())
    }

    fn log_regression_snapshot_copy(&mut self, snapshot: &RegressionSnapshot, payload: &str) {
        self.push_status(format!(
            "QA regression snapshot copy ({}): {}",
            snapshot.scenario.label(),
            payload
        ));
    }

    fn record_regression_snapshot(
        &mut self,
        scenario: RegressionScenario,
        snapshot: Option<&ReaderSnapshot>,
        overlay_snapshot: Option<OverlayDecisionSnapshot>,
    ) {
        const MAX_SNAPSHOTS: usize = 10;
        let now = Instant::now();
        let scenario_clone = scenario.clone();
        if let Some(last) = self.regression_snapshots.last() {
            if last.scenario == scenario_clone
                && now.duration_since(last.timestamp) < Duration::from_secs(8)
            {
                return;
            }
        }
        let (source_path, current_page, highlighted_sentence) = snapshot
            .map(|snapshot| {
                (
                    Some(snapshot.source_path.clone()),
                    Some(snapshot.current_page),
                    snapshot.highlighted_sentence_idx,
                )
            })
            .unwrap_or((None, None, None));
        let entry = RegressionSnapshot {
            id: self.regression_snapshot_next_id,
            timestamp: now,
            scenario,
            source_path,
            current_page,
            highlighted_sentence,
            overlay_snapshot,
        };
        self.regression_snapshot_next_id = self.regression_snapshot_next_id.wrapping_add(1);
        self.regression_snapshots.push(entry.clone());
        if self.regression_snapshots.len() > MAX_SNAPSHOTS {
            self.regression_snapshots.remove(0);
        }
        trace!(
            regression_snapshot = entry.describe(),
            id = entry.id,
            "Captured regression snapshot for QA"
        );
    }

    fn maybe_record_audio_command(
        &mut self,
        command: &AppCommand,
        snapshot: Option<&ReaderSnapshot>,
    ) {
        let session_cmd = match command {
            AppCommand::Reader(ReaderCommand::Session(session_cmd)) => session_cmd,
            _ => return,
        };
        let label = match Self::audio_command_label(session_cmd) {
            Some(label) => label,
            None => return,
        };
        let snapshot = match snapshot {
            Some(snapshot) => snapshot,
            None => return,
        };
        let target_sentence = snapshot.highlighted_sentence_idx;
        let (anchor, fallback) = target_sentence
            .map(|idx| LanternLeafApp::resolve_sentence_anchor(snapshot, idx))
            .unwrap_or((None, AnchorFallback::Missing));
        let overlay_snapshot = self.capture_overlay_decision();
        let highlight_page = self.pdf_render_state.highlighted_page;
        let event = AudioBudgetEvent {
            id: self.audio_diagnostics.allocate_event_id(),
            timestamp: Instant::now(),
            command: label.to_string(),
            auto_scroll: Self::audio_command_auto_scroll(session_cmd),
            target_sentence,
            anchor,
            fallback,
            overlay_snapshot: overlay_snapshot.clone(),
            highlight_page,
        };
        let span = self.audio_budget_span(&event);
        let _enter = span.enter();
        trace!(
            audio_command = event.command.as_str(),
            target_sentence = ?event.target_sentence,
            auto_scroll = event.auto_scroll,
            budget_pages = event.overlay_snapshot.budget_pages,
            "Recorded audio JumpToSentence decision"
        );
        self.audio_diagnostics.record(event.clone());
        self.push_budget_timeline_entry(
            RegressionSnapshotTimelineKind::AudioEvent(event.clone()),
            event.timestamp,
        );
    }

    fn audio_command_label(command: &SessionCommand) -> Option<&'static str> {
        match command {
            SessionCommand::TtsPlay => Some("tts.play"),
            SessionCommand::TtsPause => Some("tts.pause"),
            SessionCommand::TtsTogglePlayPause => Some("tts.toggle_play_pause"),
            SessionCommand::TtsPlayFromPageStart => Some("tts.play_page_start"),
            SessionCommand::TtsPlayFromHighlight => Some("tts.play_from_highlight"),
            SessionCommand::TtsSeekNext => Some("tts.seek_next"),
            SessionCommand::TtsSeekPrev => Some("tts.seek_prev"),
            SessionCommand::TtsRepeatSentence => Some("tts.repeat_sentence"),
            SessionCommand::TtsStop => Some("tts.stop"),
            _ => None,
        }
    }

    fn audio_command_auto_scroll(command: &SessionCommand) -> bool {
        matches!(
            command,
            SessionCommand::TtsPlay
                | SessionCommand::TtsPlayFromPageStart
                | SessionCommand::TtsPlayFromHighlight
                | SessionCommand::TtsSeekNext
                | SessionCommand::TtsSeekPrev
                | SessionCommand::TtsRepeatSentence
        )
    }

    fn capture_overlay_pressure_from_native_render_span(&mut self, span: &NativeRenderSpan) {
        if span.target != RenderTarget::TextLayer || span.cache_hit {
            return;
        }
        let overlay_budget_pages = self.pdf_render_state.overlay_budget_pages();
        if overlay_budget_pages == 0 {
            return;
        }
        let highlight_page = self.pdf_render_state.highlighted_page;
        let reason_text = if highlight_page == Some(span.page_index) {
            "Highlight text layer rendered while overlay budget contested"
        } else {
            "Neighbor text layer render consumed the overlay budget"
        };
        let alert_id = self.pdf_render_state.allocate_overlay_alert_id();
        let alert = OverlayPressureAlert::new(
            alert_id,
            OverlayPressureKind::NativeRender {
                span: span.clone(),
                reason_text: reason_text.to_string(),
            },
            overlay_budget_pages,
            highlight_page,
        );
        self.pdf_render_state
            .record_overlay_pressure_alert(alert.clone());
        self.push_budget_timeline_entry(
            RegressionSnapshotTimelineKind::OverlayAlert(alert.clone()),
            alert.timestamp,
        );
    }

    fn capture_overlay_pressure_from_native_eviction(&mut self, eviction: &NativeRenderEviction) {
        if eviction.target != RenderTarget::TextLayer {
            return;
        }
        let highlight_page = self.pdf_render_state.highlighted_page;
        if highlight_page != Some(eviction.page_index) {
            return;
        }
        let alert_id = self.pdf_render_state.allocate_overlay_alert_id();
        let alert = OverlayPressureAlert::new(
            alert_id,
            OverlayPressureKind::NativeEviction {
                eviction: eviction.clone(),
                reason_text: "Highlight text layer evicted by budget pressure".to_string(),
            },
            self.pdf_render_state.overlay_budget_pages(),
            highlight_page,
        );
        self.pdf_render_state
            .record_overlay_pressure_alert(alert.clone());
        self.push_budget_timeline_entry(
            RegressionSnapshotTimelineKind::OverlayAlert(alert.clone()),
            alert.timestamp,
        );
        self.overlay_eviction_warning_at = Some(Instant::now());
        let eviction_span = tracing::span!(
            Level::WARN,
            "OverlayEvictionWarning",
            budget_plan = "shell.performance_budget",
            page = eviction.page_index + 1,
            highlight_page = highlight_page.is_some(),
            overlay_budget_pages = self.pdf_render_state.overlay_budget_pages(),
            reason = eviction.reason,
            target = ?eviction.target,
        );
        let _enter = eviction_span.enter();
        trace!(event = %eviction.describe(), "Overlay eviction logged for QA");
    }

    fn overlay_pressure_badge(&self, alert: &OverlayPressureAlert) -> RichText {
        let (color, label) = alert.kind.badge_info();
        RichText::new(label).color(color).small().strong()
    }

    fn overlay_pressure_span_summary(&self, alert: &OverlayPressureAlert) -> String {
        match &alert.kind {
            OverlayPressureKind::NativeRender { span, reason_text } => format!(
                "[OverlayBudget][Render] page={} target={} cache_hit={} duration_ms={:.2} budget={} reason={}",
                span.page_index + 1,
                span.target.label(),
                span.cache_hit,
                span.duration.as_secs_f32() * 1000.0,
                alert.overlay_budget_pages,
                reason_text,
            ),
            OverlayPressureKind::NativeEviction {
                eviction,
                reason_text,
            } => format!(
                "[OverlayBudget][Eviction] page={} target={} reason={} budget={}",
                eviction.page_index + 1,
                eviction.target.label(),
                reason_text,
                alert.overlay_budget_pages,
            ),
        }
    }

    fn overlay_pressure_span_payload(&self, alert: &OverlayPressureAlert, summary: &str) -> String {
        let kind_info = match &alert.kind {
            OverlayPressureKind::NativeRender { span, reason_text } => json!({
                "type": "native_render",
                "page": span.page_index + 1,
                "target": span.target.label(),
                "cache_hit": span.cache_hit,
                "duration_ms": span.duration.as_secs_f32() * 1000.0,
                "reason": reason_text,
            }),
            OverlayPressureKind::NativeEviction {
                eviction,
                reason_text,
            } => json!({
                "type": "native_eviction",
                "page": eviction.page_index + 1,
                "target": eviction.target.label(),
                "eviction_reason": eviction.reason,
                "reason": reason_text,
            }),
        };
        let payload = json!({
            "id": alert.id(),
            "tranche": alert.tranche_label(),
            "tranche_url": alert.tranche_url(),
            "overlay_budget_pages": alert.overlay_budget_pages,
            "highlight_page": alert.highlight_page,
            "age_secs": alert.age_secs(),
            "span": kind_info,
            "summary": summary,
        });
        serde_json::to_string(&payload).unwrap_or_else(|_| summary.to_string())
    }

    fn log_qa_span_copy(&mut self, alert: &OverlayPressureAlert, summary: &str) {
        let payload = self.overlay_pressure_span_payload(alert, summary);
        self.push_status(format!("QA span copy: {}", payload));
    }

    fn scheduler_event_payload(&self, event: &SchedulerEvent, summary: &str) -> String {
        let details = match &event.kind {
            SchedulerEventKind::Eviction {
                evicted_canvas_pages,
                evicted_text_layer_pages,
            } => json!({
                "evicted_canvas_pages": evicted_canvas_pages,
                "evicted_text_layer_pages": evicted_text_layer_pages,
            }),
            SchedulerEventKind::RetryOverlay {
                reason,
                highlight_page,
                budget_pages,
                overlay_reason,
            } => json!({
                "reason": reason,
                "highlight_page": highlight_page.map(|page| page + 1),
                "budget_pages": budget_pages,
                "overlay_reason": overlay_reason,
            }),
        };
        let payload = json!({
            "kind": event.kind.describe(),
            "age_secs": event.age_secs(),
            "details": details,
            "summary": summary,
        });
        serde_json::to_string(&payload).unwrap_or_else(|_| summary.to_string())
    }

    fn log_scheduler_event_copy(&mut self, event: &SchedulerEvent, summary: &str) {
        let payload = self.scheduler_event_payload(event, summary);
        self.push_status(format!("QA scheduler span copy: {}", payload));
    }

    fn replay_pdf_render_event(&self, event: &PdfRenderEvent) {
        let highlight_page = self.pdf_render_state.highlighted_page == Some(event.page_index);
        let replay_span = tracing::span!(
            Level::TRACE,
            "PdfRenderEventReplay",
            budget_plan = "shell.performance_budget",
            page = event.page_index + 1,
            kind = ?event.kind,
            highlight_page = highlight_page,
            overlay_budget_pages = event.overlay_budget_pages,
            overlays_drawn = event.overlays_drawn,
            overlay_reason = ?event.overlay_reason.as_deref(),
        );
        let _enter = replay_span.enter();
        trace!(event = ?event.describe(), "Replayed PDF render event for QA");
    }

    fn replay_native_render_span(&self, span: &NativeRenderSpan) {
        let highlight_page = self.pdf_render_state.highlighted_page == Some(span.page_index);
        let replay_span = tracing::span!(
            Level::TRACE,
            "PdfNativeRenderReplay",
            budget_plan = "shell.performance_budget",
            target = ?span.target,
            page = span.page_index + 1,
            highlight_page = highlight_page,
            cache_hit = span.cache_hit,
            duration_ms = span.duration.as_secs_f32(),
            overlay_budget_pages = self.pdf_render_state.overlay_budget_pages(),
        );
        let _enter = replay_span.enter();
        trace!(span = ?span.describe(), "Replayed native render span for QA");
    }

    fn replay_native_eviction(&self, event: &NativeRenderEviction) {
        let highlight_page = self.pdf_render_state.highlighted_page == Some(event.page_index);
        let replay_span = tracing::span!(
            Level::TRACE,
            "PdfNativeEvictionReplay",
            budget_plan = "shell.performance_budget",
            target = ?event.target,
            page = event.page_index + 1,
            highlight_page = highlight_page,
            reason = event.reason,
            overlay_budget_pages = self.pdf_render_state.overlay_budget_pages(),
        );
        let _enter = replay_span.enter();
        trace!(event = ?event.describe(), "Replayed native eviction for QA");
    }

    fn replay_overlay_pressure_alert(&self, alert: &OverlayPressureAlert) {
        match &alert.kind {
            OverlayPressureKind::NativeRender { span, .. } => self.replay_native_render_span(span),
            OverlayPressureKind::NativeEviction { eviction, .. } => {
                self.replay_native_eviction(eviction)
            }
        }
    }

    fn replay_throttle_span(&self, event: &PdfRenderThrottleEvent) {
        let highlight_page = self.pdf_render_state.highlighted_page == Some(event.page_index);
        let span = tracing::span!(
            Level::TRACE,
            "PdfRenderThrottle",
            budget_plan = "shell.performance_budget",
            page = (event.page_index + 1),
            highlight_page = highlight_page,
            kind = ?event.kind,
            reason = event.reason.as_str(),
            overlay_budget_pages = self.pdf_render_state.overlay_budget_pages(),
        );
        let _enter = span.enter();
        trace!(event = ?event, "Replayed throttle span for QA");
    }

    fn replay_scheduler_event(&self, event: &SchedulerEvent) {
        let highlight_page = match &event.kind {
            SchedulerEventKind::RetryOverlay { highlight_page, .. } => *highlight_page,
            _ => None,
        };
        let span = tracing::span!(
            Level::TRACE,
            "PdfSchedulerReplay",
            budget_plan = "shell.performance_budget",
            kind = ?event.kind,
            highlight_page = highlight_page.map(|page| page + 1),
        );
        let _enter = span.enter();
        trace!(event = %event.kind.describe(), "Replayed scheduler event for QA");
    }

    fn regression_snapshot_event_links(
        &self,
        snapshot: &RegressionSnapshot,
    ) -> RegressionSnapshotEventLinks {
        let render_events = self
            .pdf_render_state
            .recent_render_events()
            .iter()
            .filter(|event| Self::matches_snapshot_page(snapshot, event.page_index))
            .filter(|event| {
                Self::within_snapshot_window(
                    snapshot.timestamp,
                    event.timestamp,
                    REGRESSION_EVENT_WINDOW,
                )
            })
            .cloned()
            .collect();
        let throttle_events = self
            .pdf_render_state
            .recent_throttle_events()
            .iter()
            .filter(|event| Self::matches_snapshot_page(snapshot, event.page_index))
            .filter(|event| {
                Self::within_snapshot_window(
                    snapshot.timestamp,
                    event.timestamp,
                    REGRESSION_EVENT_WINDOW,
                )
            })
            .cloned()
            .collect();
        let status_entries = self
            .status_log
            .iter()
            .filter(|entry| {
                Self::within_snapshot_window(
                    snapshot.timestamp,
                    entry.timestamp,
                    REGRESSION_EVENT_WINDOW,
                )
            })
            .cloned()
            .collect();
        RegressionSnapshotEventLinks {
            render_events,
            throttle_events,
            status_entries,
        }
    }

    fn regression_snapshot_timeline_entries(
        &self,
        snapshot: &RegressionSnapshot,
        event_links: &RegressionSnapshotEventLinks,
    ) -> Vec<RegressionSnapshotTimelineEntry> {
        let mut entries = Vec::new();
        for alert in self
            .pdf_render_state
            .recent_overlay_pressure_alerts()
            .iter()
            .filter(|alert| {
                Self::within_snapshot_window(
                    snapshot.timestamp,
                    alert.timestamp,
                    REGRESSION_EVENT_WINDOW,
                )
            })
            .filter(|alert| {
                alert
                    .highlight_page
                    .map_or(true, |page| Self::matches_snapshot_page(snapshot, page))
            })
        {
            entries.push(RegressionSnapshotTimelineEntry {
                kind: RegressionSnapshotTimelineKind::OverlayAlert(alert.clone()),
                timestamp: alert.timestamp,
            });
        }
        for event in event_links.render_events.iter() {
            entries.push(RegressionSnapshotTimelineEntry {
                kind: RegressionSnapshotTimelineKind::PdfRenderEvent(event.clone()),
                timestamp: event.timestamp,
            });
        }
        for event in event_links.throttle_events.iter() {
            entries.push(RegressionSnapshotTimelineEntry {
                kind: RegressionSnapshotTimelineKind::PdfThrottleEvent(event.clone()),
                timestamp: event.timestamp,
            });
        }
        for status in event_links.status_entries.iter() {
            entries.push(RegressionSnapshotTimelineEntry {
                kind: RegressionSnapshotTimelineKind::Status(status.clone()),
                timestamp: status.timestamp,
            });
        }
        entries.sort_by_key(|entry| entry.timestamp);
        entries
    }

    fn matches_snapshot_page(snapshot: &RegressionSnapshot, page_index: usize) -> bool {
        snapshot
            .current_page
            .map_or(true, |page| page == page_index)
    }

    fn within_snapshot_window(snapshot_ts: Instant, event_ts: Instant, window: Duration) -> bool {
        if event_ts >= snapshot_ts {
            event_ts.duration_since(snapshot_ts) <= window
        } else {
            snapshot_ts.duration_since(event_ts) <= window
        }
    }

    fn log_render_throttle(
        &mut self,
        kind: PdfRenderThrottleKind,
        page_index: usize,
        highlight_page: bool,
        overlay_budget_pages: usize,
        reason: &'static str,
    ) {
        let event = PdfRenderThrottleEvent::new(kind, page_index, reason.to_string());
        self.pdf_render_state.record_throttle_event(event.clone());
        self.push_budget_timeline_entry(
            RegressionSnapshotTimelineKind::PdfThrottleEvent(event.clone()),
            event.timestamp,
        );
        let span = tracing::span!(
            Level::TRACE,
            "PdfRenderThrottle",
            budget_plan = "shell.performance_budget",
            page = (page_index + 1),
            highlight_page = highlight_page,
            kind = ?kind,
            reason = reason,
            overlay_budget_pages = overlay_budget_pages,
        );
        let _enter = span.enter();
        trace!(
            page = (page_index + 1),
            kind = ?kind,
            reason = reason,
            highlight_page = highlight_page,
            "PDF render stage throttled/skipped"
        );
    }

    fn resolve_sentence_anchor(
        snapshot: &ReaderSnapshot,
        sentence_idx: usize,
    ) -> (Option<usize>, AnchorFallback) {
        if sentence_idx >= snapshot.sentence_anchor_map.len() {
            return (None, AnchorFallback::Missing);
        }
        if let Some(anchor_idx) = snapshot.sentence_anchor_map[sentence_idx] {
            return (Some(anchor_idx), AnchorFallback::Exact);
        }
        let mut best_distance = usize::MAX;
        let mut candidate = None;
        for (candidate_idx, entry) in snapshot.sentence_anchor_map.iter().enumerate() {
            if let Some(anchor_idx) = entry {
                let distance = sentence_idx.abs_diff(candidate_idx);
                if distance < best_distance {
                    best_distance = distance;
                    candidate = Some(*anchor_idx);
                }
            }
        }
        if let Some(anchor_idx) = candidate {
            (Some(anchor_idx), AnchorFallback::Nearest)
        } else {
            (None, AnchorFallback::Missing)
        }
    }

    fn render_canonical_preview(&self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        CollapsingHeader::new("Canonical sentences preview")
            .id_source("canonical-preview")
            .default_open(false)
            .show(ui, |ui| {
                let total = snapshot.canonical_sentences.len();
                ui.label(format!("{} canonical sentences (showing first 5)", total));
                for (idx, canonical) in snapshot.canonical_sentences.iter().enumerate() {
                    if idx >= 5 {
                        ui.label("…");
                        break;
                    }
                    ui.label(RichText::new(format!("{}: {}", idx + 1, canonical)).small());
                }
            });
    }

    fn render_pdf_diagnostics(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        if snapshot.pretty_kind != PrettyKind::Pdf {
            return;
        }
        let focus_request = self.overlay_pressure_focus;
        let mut overlay_warning_rect: Option<Rect> = None;
        CollapsingHeader::new("PDF diagnostics")
            .id_source("pdf-diagnostics")
            .default_open(false)
            .open(if focus_request { Some(true) } else { None })
            .show(ui, |ui| {
                ui.label(format!(
                    "Page {}/{}",
                    snapshot.current_page + 1,
                    snapshot.total_pages
                ));
                if let Some(classification) = snapshot.pdf_classification.as_ref() {
                    ui.label(format!(
                        "Document class: {:?} ({:.2})",
                        classification.document_class, classification.confidence
                    ));
                    ui.label(format!(
                        "OCR recommendation: {:?}",
                        classification.ocr_recommendation
                    ));
                }
                if let Some(policy) = snapshot.pdf_runtime_policy.as_ref() {
                    ui.label(format!("Text policy: {:?}", policy.text_only_policy));
                    ui.label(format!(
                        "Highlight policy: {:?}",
                        policy.sentence_highlight_policy
                    ));
                    ui.label(format!("Search policy: {:?}", policy.search_policy));
                    ui.label(format!("Policy explanation: {}", policy.explanation));
                }
                if let Some(alignment) = snapshot.pdf_ocr_alignment.as_ref() {
                    ui.label(format!("OCR source: {:?}", alignment.source_kind));
                    ui.label(format!(
                        "Mapped sentences: {}/{}",
                        alignment.mapped_sentence_count, alignment.sentence_count
                    ));
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
                if let Some(pipeline) = snapshot.pdf_ocr_pipeline.as_ref() {
                    ui.label(format!("OCR engine: {:?}", pipeline.engine_policy));
                    if !pipeline.fallback_decisions.is_empty() {
                        ui.label(format!(
                            "Fallbacks: {}",
                            pipeline
                                .fallback_decisions
                                .iter()
                                .map(|decision| format!("{decision:?}"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                    if !pipeline.fallback_strategy_labels.is_empty() {
                        ui.label(format!(
                            "Fallback labels: {}",
                            pipeline.fallback_strategy_labels.join(", ")
                        ));
                    }
                }
                if let Some(plan) = &self.pdf_render_state.plan {
                    ui.separator();
                    ui.label("PDF viewport plan:");
                    ui.label(format!(
                        "Visible pages: {}",
                        Self::format_pdf_page_list(&self.pdf_render_state.visible_page_indexes)
                    ));
                    ui.label(format!(
                        "Canvas pages: {}",
                        Self::format_pdf_page_list(&plan.canvas_page_indexes)
                    ));
                    ui.label(format!(
                        "Text layers: {}",
                        Self::format_pdf_page_list(&plan.text_layer_page_indexes)
                    ));
                    ui.label(format!(
                        "Priority pages: {}",
                        Self::format_pdf_page_list(&plan.priority_page_indexes)
                    ));
                    ui.label(format!(
                        "Medium priority: {}",
                        Self::format_pdf_page_list(&plan.medium_priority_page_indexes)
                    ));
                    ui.label(format!(
                        "Low priority: {}",
                        Self::format_pdf_page_list(&plan.low_priority_page_indexes)
                    ));
                    ui.label(format!(
                        "Active TTS page: {}",
                        self.pdf_render_state
                            .active_tts_page_index
                            .map(|idx| idx + 1)
                            .unwrap_or(0)
                    ));
                    ui.label(format!(
                        "Jump target page: {}",
                        self.pdf_render_state
                            .jump_target_page_index
                            .map(|idx| idx + 1)
                            .unwrap_or(0)
                    ));
                    let canvas_plan_len = plan.canvas_page_indexes.len();
                    let text_plan_len = plan.text_layer_page_indexes.len();
                    self.render_pdf_preview(ui, snapshot);
                    ui.label(format!(
                        "Rendered canvases: {}/{}",
                        self.pdf_render_state.rendered_canvas_pages, canvas_plan_len
                    ));
                    ui.label(format!(
                        "Rendered text layers: {}/{}",
                        self.pdf_render_state.rendered_text_layers, text_plan_len
                    ));
                    ui.label(format!(
                        "Rendered overlays: {}/{}",
                        self.pdf_render_state.rendered_overlays,
                        self.pdf_render_state.overlay_budget_pages()
                    ));
                    if let Some(decision) = self.overlay_diagnostics.preview_decision() {
                        ui.label(format!(
                            "Preview overlay budget: {} pages (text layer: {}, allowed: {})",
                            decision.budget_pages,
                            if decision.highlight_page_has_text_layer {
                                "yes"
                            } else {
                                "no"
                            },
                            if decision.allowed { "yes" } else { "no" }
                        ));
                        ui.label(format!(
                            "Overlays drawn: {}/{}",
                            decision.overlays_drawn,
                            decision.budget_pages.max(1)
                        ));
                        if let Some(reason) = &decision.overlay_reason {
                            ui.label(format!("Overlay geometry reason: {}", reason));
                        }
                        ui.label(format!(
                            "Cached overlay rects: {}",
                            decision.overlay_rects_available
                        ));
                        if ui.button("Replay preview overlay span").clicked() {
                            self.replay_overlay_span("preview", decision);
                        }
                    }
                    if let Some((event, decision)) = self.overlay_diagnostics.last_jump_decision() {
                        ui.label(format!(
                            "Last JumpToSentence ({}): budget {} pages (allowed: {})",
                            event,
                            decision.budget_pages,
                            if decision.allowed { "hit" } else { "skipped" }
                        ));
                        ui.label(format!(
                            "Overlay count: {} (text layer present: {})",
                            decision.overlays_drawn,
                            if decision.highlight_page_has_text_layer {
                                "yes"
                            } else {
                                "no"
                            }
                        ));
                        if let Some(reason) = &decision.overlay_reason {
                            ui.label(format!("Overlay geometry reason: {}", reason));
                        }
                        ui.label(format!(
                            "Cached overlay rects: {}",
                            decision.overlay_rects_available
                        ));
                        if ui.button("Replay last overlay span").clicked() {
                            self.replay_overlay_span(event, decision);
                        }
                    }
                } else {
                    ui.label("PDF viewport scheduler idle.");
                }
                if let Some(decision) = &self.pdf_render_state.decision {
                    ui.label(format!(
                        "Text layer budget: {} pages ({} evicted)",
                        self.pdf_render_state.overlay_budget_pages(),
                        decision.evict_text_layer_page_indexes.len()
                    ));
                    if !decision.evict_canvas_page_indexes.is_empty() {
                        ui.label(format!(
                            "Canvas evictions: {}",
                            Self::format_pdf_page_list(&decision.evict_canvas_page_indexes)
                        ));
                    }
                    if !decision.evict_text_layer_page_indexes.is_empty() {
                        ui.label(format!(
                            "Text layer evictions: {}",
                            Self::format_pdf_page_list(&decision.evict_text_layer_page_indexes)
                        ));
                    }
                } else {
                    ui.label("No viewport eviction activity yet.");
                }
                if let Some(age) = self.pdf_render_state.updated_age() {
                    ui.label(format!(
                        "Scheduler refreshed {:.2}s ago.",
                        age.as_secs_f32()
                    ));
                }
                ui.label(format!(
                    "OCR overlay budget pages: {}",
                    self.pdf_render_state.overlay_budget_pages()
                ));
                ui.label(format!(
                    "Highlightable OCR sentences: {}",
                    snapshot
                        .pdf_ocr_alignment
                        .as_ref()
                        .map(|alignment| alignment.highlightable_sentence_count)
                        .unwrap_or(0)
                ));
                ui.separator();
                ui.label("Scheduler events:");
                let non_retry_events: Vec<_> = self
                    .scheduler_events
                    .iter()
                    .filter(|event| !matches!(event.kind, SchedulerEventKind::RetryOverlay { .. }))
                    .cloned()
                    .collect();
                if non_retry_events.is_empty() {
                    ui.label("(No scheduler events logged yet)");
                } else {
                    for event in non_retry_events.iter().rev() {
                        ui.horizontal(|ui| {
                            ui.label(Self::scheduler_event_badge(&event.kind));
                            ui.label(
                                RichText::new(format!(
                                    "{} ({:.1}s ago)",
                                    event.kind.describe(),
                                    event.age_secs()
                                ))
                                .small()
                                .weak(),
                            );
                            self.scheduler_event_controls(ui, event);
                        });
                    }
                }
                ui.separator();
                ui.label("Render throttle timeline:");
                let throttle_events = self.pdf_render_state.recent_throttle_events();
                if throttle_events.is_empty() {
                    ui.label("(No throttle events yet)");
                } else {
                    for event in throttle_events.iter().rev() {
                        ui.horizontal(|ui| {
                            ui.label(LanternLeafApp::throttle_badge(event.kind));
                            ui.label(
                                RichText::new(format!(
                                    "{} ({:.1}s ago)",
                                    event.describe(),
                                    event.age_secs()
                                ))
                                .small(),
                            );
                            if ui.button("Replay throttle span").clicked() {
                                self.replay_throttle_span(event);
                            }
                        });
                    }
                }
                ui.separator();
                ui.label("Render events:");
                let render_events = self.pdf_render_state.recent_render_events();
                if render_events.is_empty() {
                    ui.label("(No render activity yet)");
                } else {
                    for event in render_events.iter().rev() {
                        ui.label(
                            RichText::new(format!(
                                "{} ({:.1}s ago)",
                                event.describe(),
                                event.age_secs()
                            ))
                            .small()
                            .weak(),
                        );
                    }
                }
                ui.separator();
                ui.label("Native render traces:");
                let native_spans = self.pdf_render_state.recent_native_render_spans();
                if native_spans.is_empty() {
                    ui.label("(No native renders yet)");
                } else {
                    for span in native_spans.iter().rev() {
                        ui.label(
                            RichText::new(format!(
                                "{} ({:.2?} ago)",
                                span.describe(),
                                Instant::now().saturating_duration_since(span.timestamp)
                            ))
                            .small()
                            .weak(),
                        );
                    }
                }
                ui.separator();
                ui.label("Native render evictions:");
                let evictions = self.pdf_render_state.recent_native_evictions();
                if evictions.is_empty() {
                    ui.label("(No evictions yet)");
                } else {
                    for event in evictions.iter().rev() {
                        ui.label(
                            RichText::new(format!(
                                "{} ({:.2?} ago)",
                                event.describe(),
                                Instant::now().saturating_duration_since(event.timestamp)
                            ))
                            .small()
                            .weak(),
                        );
                    }
                }
                ui.separator();
                let warning_label = ui.label("Overlay pressure warnings:");
                overlay_warning_rect = Some(warning_label.rect);
                let overlay_warnings = self
                    .pdf_render_state
                    .recent_overlay_pressure_alerts()
                    .to_vec();
                if overlay_warnings.is_empty() {
                    ui.label("(No overlay pressure warnings yet)");
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Related tranches:");
                        ui.hyperlink_to("Reader Rendering Core", READER_RENDR_ROADMAP_URL);
                        ui.hyperlink_to("PDF Subsystem", PDF_SUBSYSTEM_ROADMAP_URL);
                        ui.hyperlink_to(
                            "Implementation prioritization",
                            PRIORITIZATION_ROADMAP_URL,
                        );
                    });
                    for alert in overlay_warnings.iter().rev() {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(self.overlay_pressure_badge(alert));
                                ui.label(
                                    RichText::new(format!(
                                        "{} ({:.1}s ago)",
                                        alert.describe(),
                                        alert.age_secs()
                                    ))
                                    .small()
                                    .weak(),
                                );
                                ui.label(
                                    RichText::new(format!("[span id: {}]", alert.id()))
                                        .small()
                                        .weak(),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label("Tranche link:");
                                ui.hyperlink_to(alert.tranche_label(), alert.tranche_url());
                            });
                            ui.horizontal(|ui| {
                                if ui.button("Replay pressure span").clicked() {
                                    self.replay_overlay_pressure_alert(alert);
                                }
                                if ui.button("Copy span data").clicked() {
                                    let summary = self.overlay_pressure_span_summary(alert);
                                    ui.ctx()
                                        .output_mut(|output| output.copied_text = summary.clone());
                                    trace!(span_summary = %summary, "Copied overlay pressure span for QA");
                                    self.log_qa_span_copy(alert, &summary);
                                }
                                if ui.button("Log QA JSON").clicked() {
                                    let summary = self.overlay_pressure_span_summary(alert);
                                    self.log_qa_span_copy(alert, &summary);
                                }
                            });
                        });
                    }
                }
                ui.separator();
                ui.label("Audio budget traces:");
                let audio_events = self.audio_diagnostics.recent_events().iter().cloned().collect::<Vec<_>>();
                if audio_events.is_empty() {
                    ui.label("(No audio JumpToSentence events yet)");
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Related tranches:");
                        ui.hyperlink_to("Audio & TTS integration", TTS_ROADMAP_URL);
                        ui.hyperlink_to("Reader Rendering Core", READER_RENDR_ROADMAP_URL);
                        ui.hyperlink_to("Implementation prioritization", PRIORITIZATION_ROADMAP_URL);
                    });
                    for event in audio_events.iter().rev() {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(Self::audio_event_badge(event));
                                ui.label(
                                    RichText::new(format!(
                                        "{} ({:.1}s ago) [span id: {}]",
                                        event.describe(),
                                        event.age_secs(),
                                        event.id
                                    ))
                                    .small()
                                    .weak(),
                                );
                            });
                            let highlight_label = event
                                .highlight_page
                                .map(|page| page + 1)
                                .map_or_else(|| "unknown".to_string(), |page| page.to_string());
                            let overlay_reason = event
                                .overlay_snapshot
                                .overlay_reason
                                .as_deref()
                                .unwrap_or("unspecified");
                            ui.horizontal(|ui| {
                                ui.label(format!(
                                    "Anchor: {} | auto_scroll: {} | highlight page: {}",
                                    event.fallback.label(),
                                    event.auto_scroll,
                                    highlight_label
                                ));
                            });
                            ui.horizontal(|ui| {
                                ui.label(format!(
                                    "Overlay budget: {} pages (drawn {}), allowed: {} | overlay reason: {}",
                                    event.overlay_snapshot.budget_pages,
                                    event.overlay_snapshot.overlays_drawn,
                                    event.overlay_snapshot.allowed,
                                    overlay_reason
                                ));
                            });
                            ui.horizontal(|ui| {
                                self.audio_event_controls(ui, event);
                                if ui
                                    .small_button("Focus overlays")
                                    .on_hover_text("Scroll diagnostics to overlay pressure warnings")
                                    .clicked()
                                {
                                    self.overlay_pressure_focus = true;
                                }
                            });
                        });
                    }
                }
                ui.separator();
                ui.label("Settings trace events:");
                let settings_events = self.settings_trace_events.clone();
                if settings_events.is_empty() {
                    ui.label("(No settings spans captured yet)");
                } else {
                    for event in settings_events.iter().rev() {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "{} ({:.1}s ago)",
                                    event.describe(),
                                    event.age_secs()
                                ))
                                .small()
                                .weak(),
                            );
                            ui.label(
                                RichText::new(format!("[span id: {}]", event.id))
                                    .small()
                                    .weak(),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.hyperlink_to("Settings roadmap", event.roadmap_url);
                            if ui.button("Replay settings span").clicked() {
                                self.replay_settings_event(event);
                            }
                            if ui.button("Copy QA JSON").clicked() {
                                let summary = event.summary.clone();
                                ui.ctx()
                                    .output_mut(|output| output.copied_text = summary.clone());
                                trace!(span_summary = %summary, "Copied settings span for QA");
                                self.log_settings_trace_copy(event);
                            }
                        });
                    }
                }
                ui.separator();
                ui.label("Persistence trace events:");
                let persistence_events = self.persistence_trace_events.clone();
                if persistence_events.is_empty() {
                    ui.label("(No persistence spans yet)");
                } else {
                    for event in persistence_events.iter().rev() {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "{} ({:.1}s ago)",
                                    event.describe(),
                                    event.age_secs()
                                ))
                                .small()
                                .weak(),
                            );
                            ui.label(
                                RichText::new(format!("[span id: {}]", event.id))
                                    .small()
                                    .weak(),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.hyperlink_to("Persistence roadmap", event.roadmap_url);
                            if ui.button("Replay persistence span").clicked() {
                                self.replay_persistence_event(event);
                            }
                            if ui.button("Copy QA JSON").clicked() {
                                let summary = event.describe();
                                ui.ctx()
                                    .output_mut(|output| output.copied_text = summary.clone());
                                trace!(span_summary = %summary, "Copied persistence span for QA");
                                self.log_persistence_trace_copy(event);
                            }
                        });
                    }
                }
                ui.separator();
                ui.label("Regression watchlist:");
                let regression_snapshots = self.regression_snapshots.clone();
                if regression_snapshots.is_empty() {
                    ui.label("(No regression snapshots captured yet)");
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Related QA resources:");
                        ui.hyperlink_to("QA checklist", QA_REGRESSION_URL);
                        ui.hyperlink_to("Settings/persistence roadmap (Tranche 6)", SETTINGS_ROADMAP_URL);
                    });
                    for snapshot in regression_snapshots.iter().rev() {
                        let event_links = self.regression_snapshot_event_links(snapshot);
                        let timeline_entries =
                            self.regression_snapshot_timeline_entries(snapshot, &event_links);
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{} ({:.1}s ago) [id {}]",
                                        snapshot.describe(),
                                        snapshot.age_secs(),
                                        snapshot.id
                                    ))
                                    .small()
                                    .weak(),
                                );
                            });
                            if let Some(source_path) = snapshot.source_path.as_ref() {
                                ui.horizontal(|ui| {
                                    ui.label("Source:");
                                    ui.label(
                                        RichText::new(source_path)
                                            .small()
                                            .weak()
                                            .monospace(),
                                    );
                                });
                            }
                            if let Some(page) = snapshot.current_page {
                                ui.label(format!("Page: {}", page + 1));
                            }
                            if let Some(sentence) = snapshot.highlighted_sentence {
                                ui.label(format!("Highlighted sentence: {}", sentence + 1));
                            }
                            if let Some(overlay) = snapshot.overlay_snapshot.as_ref() {
                                let overlay_reason = overlay
                                    .overlay_reason
                                    .as_deref()
                                    .unwrap_or("unknown");
                                ui.label(format!(
                                    "Overlay budget {} pages (reason {})",
                                    overlay.budget_pages, overlay_reason
                                ));
                            }
                            if !timeline_entries.is_empty() {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("Timeline:");
                                for entry in timeline_entries.iter() {
                                    let button = Button::new(entry.badge_label(snapshot.timestamp))
                                        .rounding(6.0)
                                        .fill(entry.badge_color());
                                    if ui.add(button).clicked() {
                                        let kind = entry.kind.clone();
                                        self.execute_timeline_kind(&kind);
                                        self.record_timeline_history(entry);
                                    }
                                }
                            });
                        }
                            ui.horizontal(|ui| {
                                ui.label("Related docs:");
                                ui.hyperlink_to("QA checklist", QA_REGRESSION_URL);
                                ui.hyperlink_to(
                                    snapshot.scenario.label(),
                                    snapshot.scenario.roadmap_url(),
                                );
                            });
                            ui.horizontal(|ui| {
                            if ui.button("Replay regression snapshot").clicked() {
                                self.replay_regression_snapshot(snapshot);
                            }
                            if ui.button("Copy QA JSON").clicked() {
                                let payload = self.regression_snapshot_payload(snapshot);
                                ui.ctx()
                                    .output_mut(|output| output.copied_text = payload.clone());
                                trace!(
                                    span_summary = %snapshot.describe(),
                                    "Copied regression snapshot QA JSON"
                                );
                                self.log_regression_snapshot_copy(snapshot, &payload);
                            }
                            if ui.button("Log QA JSON").clicked() {
                                let payload = self.regression_snapshot_payload(snapshot);
                                self.log_regression_snapshot_copy(snapshot, &payload);
                            }
                        });
                            if !event_links.render_events.is_empty() {
                                ui.label("Related PDF render spans:");
                                for event in event_links.render_events.iter() {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(event.describe())
                                                .small()
                                                .weak(),
                                        );
                                        if ui.button("Replay render span").clicked() {
                                            self.replay_pdf_render_event(event);
                                        }
                                    });
                                }
                            }
                            if !event_links.throttle_events.is_empty() {
                                ui.label("Related throttle spans:");
                                for event in event_links.throttle_events.iter() {
                                    ui.horizontal(|ui| {
                                        ui.label(Self::throttle_badge(event.kind));
                                        ui.label(
                                            RichText::new(event.describe())
                                                .small()
                                                .weak(),
                                        );
                                        if ui.button("Replay throttle span").clicked() {
                                            self.replay_throttle_span(event);
                                        }
                                    });
                                }
                            }
                            if let Some(overlay_decision) = snapshot.overlay_snapshot.clone() {
                                let scenario_label = snapshot.scenario.label();
                                ui.horizontal(|ui| {
                                    if ui.button("Replay overlay decision").clicked() {
                                        self.replay_overlay_span(
                                            scenario_label,
                                            overlay_decision.clone(),
                                        );
                                    }
                                });
                            }
                        });
                    }
                }
                if focus_request {
                    if let Some(rect) = overlay_warning_rect {
                        ui.scroll_to_rect(rect, Some(Align::Center));
                    }
                    ui.label(
                        RichText::new("Overlay pressure focus requested.")
                            .color(Color32::from_rgb(220, 200, 120))
                            .small()
                            .strong(),
                    );
                }
                let rejection_events: Vec<_> = self
                    .scheduler_events
                    .iter()
                    .filter(|event| matches!(event.kind, SchedulerEventKind::RetryOverlay { .. }))
                    .cloned()
                    .collect();
                if !rejection_events.is_empty() {
                    ui.separator();
                    ui.label("Overlay budget rejections:");
                    for event in rejection_events.iter().rev() {
                        if let SchedulerEventKind::RetryOverlay {
                            reason,
                            highlight_page,
                            budget_pages,
                            overlay_reason,
                        } = &event.kind
                        {
                            let highlight_label =
                                highlight_page.map(|page| page + 1).map_or("unknown".to_string(), |page| page.to_string());
                            let overlay_reason = overlay_reason
                                .as_deref()
                                .unwrap_or("unspecified");
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("BUDGET REJECTION")
                                            .color(Color32::from_rgb(220, 180, 120))
                                            .strong()
                                            .small(),
                                    );
                                    ui.label(
                                        RichText::new(format!(
                                            "{} ({:.1}s ago)",
                                            event.kind.describe(),
                                            event.age_secs()
                                        ))
                                        .small()
                                        .weak(),
                                    );
                                    ui.label(
                                        RichText::new(format!(
                                            "page {} | budget {} | reason {} | overlay {}",
                                            highlight_label, budget_pages, reason, overlay_reason
                                        ))
                                        .small()
                                        .weak(),
                                    );
                                });
                                ui.horizontal(|ui| {
                                    self.scheduler_event_controls(ui, event);
                                    if ui
                                        .small_button("Focus diagnostics")
                                        .on_hover_text("Scroll the diagnostics panel into view")
                                        .clicked()
                                    {
                                        self.overlay_pressure_focus = true;
                                    }
                                });
                            });
                        }
                    }
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("QA timeline archive:").strong());
                    if ui.button("Export JSON").clicked() {
                        self.handle_timeline_export(TimelineArchiveFormat::Json);
                    }
                    if ui.button("Export CSV").clicked() {
                        self.handle_timeline_export(TimelineArchiveFormat::Csv);
                    }
                });
                self.render_timeline_archive_imports(ui);
                self.render_pinned_timeline_entries(ui);
                if !self.pinned_timeline_entries.is_empty() {
                    ui.separator();
                }
                if self.timeline_history.is_empty() {
                    ui.label("(No QA timeline entries yet)");
                } else {
                    let timeline_history = self.timeline_history.clone();
                    for entry in timeline_history.iter().rev() {
                        let is_pinned = self.is_timeline_entry_pinned(entry);
                        ui.horizontal(|ui| {
                            let badge = Button::new(entry.badge_label(Instant::now()))
                                .rounding(6.0)
                                .fill(entry.badge_color());
                            if ui.add(badge).clicked() {
                                self.execute_timeline_kind(&entry.entry.kind);
                                self.record_timeline_history(&entry.entry);
                            }
                            ui.label(entry.details());
                            ui.hyperlink_to("QA link", entry.qa_url.as_str());
                            let age_secs = entry.entry.timestamp.elapsed().as_secs_f32();
                            ui.label(format!("{:.1}s ago", age_secs));
                            if is_pinned {
                                if ui.button("Unpin").clicked() {
                                    self.unpin_timeline_entry(entry);
                                }
                            } else if ui.button("Pin").clicked() {
                                self.pin_timeline_entry(entry);
                            }
                        });
                    }
                }
            });
    }

    fn throttle_badge(kind: PdfRenderThrottleKind) -> RichText {
        match kind {
            PdfRenderThrottleKind::Canvas => RichText::new("CANVAS")
                .color(Color32::from_rgb(150, 190, 230))
                .small()
                .strong(),
            PdfRenderThrottleKind::TextLayer => RichText::new("TEXT")
                .color(Color32::from_rgb(130, 210, 170))
                .small()
                .strong(),
            PdfRenderThrottleKind::Overlay => RichText::new("OVERLAY")
                .color(Color32::from_rgb(220, 170, 100))
                .small()
                .strong(),
        }
    }

    fn scheduler_timeline_entry(event: &SchedulerEvent) -> RegressionSnapshotTimelineEntry {
        RegressionSnapshotTimelineEntry {
            kind: RegressionSnapshotTimelineKind::SchedulerEvent(event.clone()),
            timestamp: event.timestamp,
        }
    }

    fn scheduler_event_badge(kind: &SchedulerEventKind) -> RichText {
        match kind {
            SchedulerEventKind::Eviction { .. } => RichText::new("EVICT")
                .color(Color32::from_rgb(170, 210, 170))
                .small()
                .strong(),
            SchedulerEventKind::RetryOverlay { .. } => RichText::new("RETRY")
                .color(Color32::from_rgb(230, 180, 110))
                .small()
                .strong(),
        }
    }

    fn scheduler_event_controls(&mut self, ui: &mut Ui, event: &SchedulerEvent) {
        let entry = Self::scheduler_timeline_entry(event);
        if ui.small_button("Replay scheduler event").clicked() {
            let kind = entry.kind.clone();
            self.execute_timeline_kind(&kind);
            self.record_timeline_history(&entry);
        }
        if ui.small_button("Copy QA JSON").clicked() {
            let summary = event.kind.describe();
            let payload = self.scheduler_event_payload(event, &summary);
            ui.ctx()
                .output_mut(|output| output.copied_text = payload.clone());
            trace!(event = %summary, "Copied scheduler event QA JSON");
            self.log_scheduler_event_copy(event, &summary);
        }
        if ui.small_button("Pin timeline entry").clicked() {
            let history_entry = TimelineHistoryEntry::from_entry(&entry);
            self.pin_timeline_entry(&history_entry);
        }
    }

    fn audio_timeline_entry(event: &AudioBudgetEvent) -> RegressionSnapshotTimelineEntry {
        RegressionSnapshotTimelineEntry {
            kind: RegressionSnapshotTimelineKind::AudioEvent(event.clone()),
            timestamp: event.timestamp,
        }
    }

    fn audio_event_badge(event: &AudioBudgetEvent) -> RichText {
        if event.overlay_snapshot.allowed {
            RichText::new("AUDIO OK")
                .color(Color32::from_rgb(110, 210, 190))
                .small()
                .strong()
        } else {
            RichText::new("AUDIO BLOCKED")
                .color(Color32::from_rgb(230, 150, 110))
                .small()
                .strong()
        }
    }

    fn audio_event_controls(&mut self, ui: &mut Ui, event: &AudioBudgetEvent) {
        let entry = Self::audio_timeline_entry(event);
        if ui.small_button("Replay audio span").clicked() {
            self.replay_audio_event(event);
            self.record_timeline_history(&entry);
        }
        if ui.small_button("Copy QA JSON").clicked() {
            let summary = event.describe();
            let payload = self.audio_event_payload(event, &summary);
            ui.ctx()
                .output_mut(|output| output.copied_text = payload.clone());
            trace!(span_summary = %summary, "Copied audio budget span for QA");
            self.log_qa_audio_copy(event, &summary);
        }
        if ui.small_button("Log QA JSON").clicked() {
            let summary = event.describe();
            self.log_qa_audio_copy(event, &summary);
        }
        if ui.small_button("Pin timeline entry").clicked() {
            let history_entry = TimelineHistoryEntry::from_entry(&entry);
            self.pin_timeline_entry(&history_entry);
        }
    }

    fn render_pdf_preview(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        if snapshot.total_pages == 0 {
            ui.label("PDF preview will appear once the document is ready.");
            return;
        }
        let plan = match self.pdf_render_state.plan.as_ref().cloned() {
            Some(plan) => plan,
            None => {
                ui.label("Viewport preview waiting for scheduler updates...");
                return;
            }
        };
        self.prepare_pdf_textures(ui.ctx());
        const MAX_PREVIEW_PAGES: usize = 6;
        let preview_size = Vec2::new(ui.available_width(), 180.0);
        let (preview_rect, _) = ui.allocate_exact_size(preview_size, Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(preview_rect, 8.0, Color32::from_gray(18));
        let content_rect = preview_rect.shrink(6.0);
        painter.rect_stroke(content_rect, 6.0, Stroke::new(1.0, Color32::from_gray(60)));

        let mut preview_pages = Vec::new();
        let mut push_page = |page: usize| {
            if page < snapshot.total_pages && !preview_pages.contains(&page) {
                preview_pages.push(page);
            }
        };
        for page in &self.pdf_render_state.visible_page_indexes {
            push_page(*page);
        }
        if let Some(page) = self.pdf_render_state.highlighted_page {
            push_page(page);
        }
        for page in &plan.priority_page_indexes {
            push_page(*page);
        }
        for page in &plan.canvas_page_indexes {
            push_page(*page);
        }
        push_page(snapshot.current_page);
        preview_pages.truncate(MAX_PREVIEW_PAGES);
        if preview_pages.is_empty() {
            preview_pages.push(
                snapshot
                    .current_page
                    .min(snapshot.total_pages.saturating_sub(1)),
            );
        }

        let columns = preview_pages.len();
        let gap = 8.0;
        let total_gap = gap * columns.saturating_sub(1) as f32;
        let raw_width = (content_rect.width() - total_gap).max(0.0);
        let page_width = (raw_width / columns as f32).max(28.0);
        let used_width = page_width * columns as f32 + total_gap;
        let mut current_x =
            content_rect.left() + (content_rect.width() - used_width).max(0.0) / 2.0;
        let font = FontId::new(11.0, FontFamily::Monospace);
        let highlight_page = self.pdf_render_state.highlighted_page;
        let overlay_budget = self.pdf_render_state.overlay_budget_pages();
        let highlight_page_text_ready = highlight_page
            .and_then(|page| {
                self.pdf_render_state
                    .surface_for_page(page)
                    .map(|surface| surface.text_layer_ready)
            })
            .unwrap_or(false);
        let overlays_allowed = highlight_page_text_ready && overlay_budget > 0;

        let mut canvas_drawn = 0;
        let mut text_drawn = 0;
        let mut overlays_drawn = 0;

        for &page in &preview_pages {
            let page_rect = Rect::from_min_max(
                Pos2::new(current_x, content_rect.top()),
                Pos2::new(current_x + page_width, content_rect.bottom()),
            );
            current_x += page_width + gap;
            let is_highlight_page = Some(page) == highlight_page;
            let is_priority = plan.priority_page_indexes.contains(&page);
            let (
                canvas_allowed,
                text_allowed,
                canvas_texture,
                text_texture,
                overlays_source,
                overlay_reason,
            ) = {
                let surface = self.pdf_render_state.surface_for_page(page);
                let canvas_allowed = surface.map(|surface| surface.canvas_ready).unwrap_or(false);
                let text_allowed = surface
                    .map(|surface| surface.text_layer_ready)
                    .unwrap_or(false);
                let canvas_texture = surface
                    .and_then(|surface| surface.canvas_texture.as_ref())
                    .map(|texture| texture.id());
                let text_texture = surface
                    .and_then(|surface| surface.text_layer_texture.as_ref())
                    .map(|texture| texture.id());
                let overlays_source = surface
                    .map(|surface| surface.overlay_rects.clone())
                    .unwrap_or_else(|| self.pdf_render_state.overlay_rects.clone());
                let overlay_reason = surface
                    .and_then(|surface| surface.overlay_reason.clone())
                    .or_else(|| self.pdf_render_state.overlay_alignment_reason.clone());
                (
                    canvas_allowed,
                    text_allowed,
                    canvas_texture,
                    text_texture,
                    overlays_source,
                    overlay_reason,
                )
            };
            let has_canvas_intent = plan.canvas_page_indexes.contains(&page);
            let has_text_intent = plan.text_layer_page_indexes.contains(&page);
            let uv_rect = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));

            if canvas_allowed {
                let canvas_span = tracing::span!(
                    Level::TRACE,
                    "PdfRenderCanvas",
                    budget_plan = "shell.performance_budget",
                    page = (page + 1),
                    highlight_page = is_highlight_page,
                    priority_page = is_priority,
                    text_layer_available = text_allowed,
                    overlay_budget_pages = overlay_budget,
                );
                let _canvas_enter = canvas_span.enter();
                let canvas_event = PdfRenderEvent::canvas(page, is_highlight_page, overlay_budget);
                self.pdf_render_state
                    .record_render_event(canvas_event.clone());
                self.push_budget_timeline_entry(
                    RegressionSnapshotTimelineKind::PdfRenderEvent(canvas_event.clone()),
                    canvas_event.timestamp,
                );
                canvas_drawn += 1;
            } else if has_canvas_intent {
                let reason = if self.pdf_render_state.is_canvas_evicted(page) {
                    "evicted_from_budget"
                } else {
                    "not_ready"
                };
                self.log_render_throttle(
                    PdfRenderThrottleKind::Canvas,
                    page,
                    is_highlight_page,
                    overlay_budget,
                    reason,
                );
            } else {
                self.log_render_throttle(
                    PdfRenderThrottleKind::Canvas,
                    page,
                    is_highlight_page,
                    overlay_budget,
                    "not_scheduled",
                );
            }

            let fill_color = if !canvas_allowed {
                Color32::from_gray(10)
            } else if is_highlight_page {
                Color32::from_rgb(38, 105, 170)
            } else if has_canvas_intent {
                Color32::from_rgb(25, 25, 25)
            } else {
                Color32::from_rgb(15, 15, 15)
            };
            let border_color = if is_priority {
                Color32::from_rgb(220, 190, 120)
            } else if canvas_allowed || has_canvas_intent {
                Color32::from_rgb(90, 150, 210)
            } else {
                Color32::from_gray(70)
            };
            painter.rect_filled(page_rect, 6.0, fill_color);
            painter.rect_stroke(
                page_rect,
                6.0,
                Stroke::new(if is_priority { 3.0 } else { 1.4 }, border_color),
            );
            if canvas_allowed {
                if let Some(texture) = canvas_texture {
                    painter.image(texture, page_rect, uv_rect, Color32::WHITE);
                }
            }
            let inner = page_rect.shrink(4.0);
            if text_allowed {
                text_drawn += 1;
                let text_span = tracing::span!(
                    Level::TRACE,
                    "PdfRenderTextLayer",
                    budget_plan = "shell.performance_budget",
                    page = (page + 1),
                    highlight_page = is_highlight_page,
                    overlay_budget_pages = overlay_budget,
                );
                let _text_enter = text_span.enter();
                trace!(
                    page = (page + 1),
                    highlight_page = is_highlight_page,
                    "Drawing text layer"
                );
                let text_event =
                    PdfRenderEvent::text_layer(page, is_highlight_page, overlay_budget);
                self.pdf_render_state
                    .record_render_event(text_event.clone());
                self.push_budget_timeline_entry(
                    RegressionSnapshotTimelineKind::PdfRenderEvent(text_event.clone()),
                    text_event.timestamp,
                );
                let text_layer_rect = inner.shrink(2.0);
                if let Some(texture) = text_texture {
                    painter.image(
                        texture,
                        text_layer_rect,
                        uv_rect,
                        Color32::from_white_alpha(200),
                    );
                } else {
                    painter.rect_filled(
                        text_layer_rect,
                        4.0,
                        Color32::from_rgba_unmultiplied(50, 170, 120, 90),
                    );
                    painter.rect_stroke(
                        text_layer_rect,
                        4.0,
                        Stroke::new(1.0, Color32::from_rgba_unmultiplied(140, 220, 180, 200)),
                    );
                }
            } else if has_text_intent {
                let reason = if self.pdf_render_state.is_text_layer_evicted(page) {
                    "budget_exhausted"
                } else {
                    "not_ready"
                };
                self.log_render_throttle(
                    PdfRenderThrottleKind::TextLayer,
                    page,
                    is_highlight_page,
                    overlay_budget,
                    reason,
                );
            }
            painter.text(
                Pos2::new(page_rect.center().x, page_rect.bottom() - 12.0),
                Align2::CENTER_BOTTOM,
                format!("Pg {}", page + 1),
                font.clone(),
                Color32::WHITE,
            );
            if Some(page) == highlight_page {
                let mut page_overlay_drawn = 0;
                if overlays_allowed {
                    for (idx, rect) in overlays_source.iter().enumerate() {
                        if idx >= overlay_budget {
                            break;
                        }
                        overlays_drawn += 1;
                        page_overlay_drawn += 1;
                        let overlay = Rect::from_min_max(
                            Pos2::new(
                                inner.left() + rect[0] * inner.width(),
                                inner.top() + rect[1] * inner.height(),
                            ),
                            Pos2::new(
                                inner.left() + rect[2] * inner.width(),
                                inner.top() + rect[3] * inner.height(),
                            ),
                        );
                        painter.rect_filled(
                            overlay,
                            2.0,
                            Color32::from_rgba_unmultiplied(255, 190, 80, 160),
                        );
                    }
                }
                if page_overlay_drawn > 0 {
                    let overlay_span = tracing::span!(
                        Level::TRACE,
                        "PdfRenderOverlay",
                        budget_plan = "shell.performance_budget",
                        page = (page + 1),
                        highlight_page = true,
                        overlays_drawn = page_overlay_drawn,
                        overlay_budget_pages = overlay_budget,
                        overlay_alignment_reason = ?overlay_reason.as_deref(),
                    );
                    let _overlay_enter = overlay_span.enter();
                    trace!(
                        page = (page + 1),
                        overlays = page_overlay_drawn,
                        "Rendered highlight overlays"
                    );
                    let overlay_event = PdfRenderEvent::overlay(
                        page,
                        page_overlay_drawn,
                        overlay_budget,
                        overlay_reason.clone(),
                    );
                    self.pdf_render_state
                        .record_render_event(overlay_event.clone());
                    self.push_budget_timeline_entry(
                        RegressionSnapshotTimelineKind::PdfRenderEvent(overlay_event.clone()),
                        overlay_event.timestamp,
                    );
                }
                if !highlight_page_text_ready && !self.pdf_render_state.overlay_rects.is_empty() {
                    self.log_render_throttle(
                        PdfRenderThrottleKind::Overlay,
                        page,
                        true,
                        overlay_budget,
                        "text_layer_missing",
                    );
                } else if highlight_page_text_ready && overlay_budget == 0 {
                    self.log_render_throttle(
                        PdfRenderThrottleKind::Overlay,
                        page,
                        true,
                        overlay_budget,
                        "budget_exhausted",
                    );
                } else if overlays_allowed
                    && !self.pdf_render_state.overlay_rects.is_empty()
                    && self.pdf_render_state.overlay_rects.len() > overlay_budget
                {
                    self.log_render_throttle(
                        PdfRenderThrottleKind::Overlay,
                        page,
                        true,
                        overlay_budget,
                        "budget_exhausted",
                    );
                }
            }
        }

        self.pdf_render_state
            .record_render_metrics(canvas_drawn, text_drawn, overlays_drawn);
        let overlay_snapshot = self.capture_overlay_decision();
        self.maybe_record_overlay_retry(&overlay_snapshot, snapshot);
        self.overlay_diagnostics
            .record_preview(overlay_snapshot.clone());
        let overlay_span = self.overlay_budget_span("preview", &overlay_snapshot);
        let _overlay_enter = overlay_span.enter();
        let preview_span = tracing::span!(
            Level::TRACE,
            "PdfPreviewRender",
            budget_plan = "shell.performance_budget",
            highlight_page = ?highlight_page,
            highlight_page_text_layer = highlight_page_text_ready,
            overlay_budget_pages = overlay_snapshot.budget_pages,
            overlay_budget_allowed = overlay_snapshot.allowed,
            overlay_rect_count = overlay_snapshot.overlay_rects_available,
            overlay_alignment_reason = ?overlay_snapshot.overlay_reason.as_deref(),
            canvas_drawn = canvas_drawn,
            text_drawn = text_drawn,
            overlays = overlays_drawn,
        );
        let _enter = preview_span.enter();
        trace!(
            preview_pages = ?preview_pages,
            canvas = canvas_drawn,
            text_layers = text_drawn,
            overlays = overlays_drawn,
            overlay_rects = self.pdf_render_state.overlay_rects.len(),
            overlay_reason = ?self.pdf_render_state.overlay_alignment_reason.as_deref(),
            "Rendered simplified PDF preview"
        );
    }

    fn prepare_pdf_textures(&mut self, ctx: &Context) {
        if self.pdf_render_state.plan.is_none() {
            return;
        }
        for idx in 0..self.pdf_render_state.viewport_surfaces.len() {
            let (page_index, canvas_ready, canvas_missing, text_ready, text_missing) = {
                let surface = &self.pdf_render_state.viewport_surfaces[idx];
                (
                    surface.page_index,
                    surface.canvas_ready,
                    surface.canvas_texture.is_none(),
                    surface.text_layer_ready,
                    surface.text_layer_texture.is_none(),
                )
            };
            if canvas_ready && canvas_missing {
                let image = self
                    .render_pdf_texture(page_index, RenderTarget::Canvas)
                    .unwrap_or_else(|| Self::build_canvas_color_image(page_index));
                let texture = ctx.load_texture(
                    format!("pdf-{}-{}", RenderTarget::Canvas.label(), page_index),
                    image,
                    TextureOptions::LINEAR,
                );
                self.pdf_render_state.viewport_surfaces[idx].canvas_texture = Some(texture);
            }
            if text_ready && text_missing {
                let image = self
                    .render_pdf_texture(page_index, RenderTarget::TextLayer)
                    .unwrap_or_else(|| Self::build_text_layer_color_image(page_index));
                let texture = ctx.load_texture(
                    format!("pdf-{}-{}", RenderTarget::TextLayer.label(), page_index),
                    image,
                    TextureOptions::LINEAR,
                );
                self.pdf_render_state.viewport_surfaces[idx].text_layer_texture = Some(texture);
            }
        }
    }

    fn build_canvas_color_image(page_index: usize) -> ColorImage {
        Self::build_placeholder_texture(
            page_index,
            PDF_CANVAS_TEXTURE_SIZE,
            [180, 190, 205],
            [60, 80, 110],
        )
    }

    fn build_text_layer_color_image(page_index: usize) -> ColorImage {
        Self::build_placeholder_texture(
            page_index,
            PDF_TEXT_TEXTURE_SIZE,
            [230, 230, 210],
            [80, 90, 70],
        )
    }

    fn build_placeholder_texture(
        page_index: usize,
        size: [usize; 2],
        base: [u8; 3],
        accent: [u8; 3],
    ) -> ColorImage {
        let (width, height) = (size[0], size[1]);
        let mut data = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            let stripe = (y + page_index * 3) % 32 < 18;
            for x in 0..width {
                let pattern = ((x * 3 + y * 5 + page_index * 7) % 256) as u8;
                let extra = if stripe { 24 } else { 0 };
                let r = base[0]
                    .saturating_add((pattern / 16) as u8)
                    .saturating_add(extra);
                let g = base[1]
                    .saturating_add((pattern / 20) as u8)
                    .saturating_add(extra / 2);
                let b = accent[2].saturating_add((pattern / 24) as u8);
                data.extend_from_slice(&[r, g, b, 255u8]);
            }
        }
        ColorImage::from_rgba_unmultiplied([width, height], &data)
    }

    fn render_pdf_texture(
        &mut self,
        page_index: usize,
        target: RenderTarget,
    ) -> Option<ColorImage> {
        let source_path = self.current_pdf_path.as_deref()?;
        let render_result = {
            let renderer = self.pdf_renderer.as_mut()?;
            let outcome = match target {
                RenderTarget::Canvas => renderer.render_canvas(source_path, page_index),
                RenderTarget::TextLayer => renderer.render_text_layer(source_path, page_index),
            };
            match outcome {
                Ok(outcome) => {
                    let render_span = NativeRenderSpan {
                        timestamp: Instant::now(),
                        target,
                        page_index,
                        duration: outcome.duration,
                        cache_hit: outcome.cache_hit,
                    };
                    let evictions = renderer.drain_eviction_events();
                    Ok((outcome, render_span, evictions))
                }
                Err(err) => Err(err),
            }
        };
        match render_result {
            Ok((outcome, render_span, evictions)) => {
                let span = tracing::span!(
                    Level::TRACE,
                    "PdfNativeRender",
                    budget_plan = "shell.performance_budget",
                    target = ?target,
                    page = page_index + 1,
                    cache_hit = render_span.cache_hit,
                    duration_ms = render_span.duration.as_secs_f32(),
                );
                let _enter = span.enter();
                self.capture_overlay_pressure_from_native_render_span(&render_span);
                self.pdf_render_state.record_native_render_span(render_span);
                for eviction in evictions {
                    self.capture_overlay_pressure_from_native_eviction(&eviction);
                    self.pdf_render_state.record_native_eviction(eviction);
                }
                Some(outcome.image)
            }
            Err(err) => {
                warn!(
                    pdf_path = %source_path.display(),
                    page = page_index + 1,
                    target = ?target,
                    error = ?err,
                    "native PDF renderer failed"
                );
                None
            }
        }
    }

    fn page_index_for_global_sentence(
        page_sentence_counts: &[usize],
        sentence_idx: Option<usize>,
    ) -> Option<usize> {
        let mut remaining = sentence_idx?;
        for (page_idx, &count) in page_sentence_counts.iter().enumerate() {
            if remaining < count {
                return Some(page_idx);
            }
            remaining = remaining.saturating_sub(count);
        }
        page_sentence_counts.len().checked_sub(1)
    }

    fn global_sentence_index(snapshot: &ReaderSnapshot, sentence_idx: usize) -> Option<usize> {
        let current_page = snapshot.current_page;
        let current_page_size = *snapshot.page_sentence_counts.get(current_page)?;
        if sentence_idx >= current_page_size {
            return None;
        }
        let page_offset = snapshot
            .page_sentence_counts
            .iter()
            .take(current_page)
            .sum::<usize>();
        page_offset.checked_add(sentence_idx)
    }

    fn format_pdf_page_list(pages: &[usize]) -> String {
        if pages.is_empty() {
            "none".to_string()
        } else {
            pages
                .iter()
                .map(|idx| (idx + 1).to_string())
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    fn sentence_highlight_color(snapshot: &ReaderSnapshot) -> Color32 {
        let highlight = if snapshot.settings.theme == config::ThemeMode::Day {
            snapshot.settings.day_highlight
        } else {
            snapshot.settings.night_highlight
        };
        Self::color32_from_highlight(highlight)
    }

    fn color32_from_highlight(color: config::HighlightColor) -> Color32 {
        fn to_byte(value: f32) -> u8 {
            let clamped = value.clamp(0.0, 1.0);
            (clamped * 255.0).round() as u8
        }
        Color32::from_rgba_unmultiplied(
            to_byte(color.r),
            to_byte(color.g),
            to_byte(color.b),
            to_byte(color.a),
        )
    }

    fn render_modals(&mut self, ctx: &Context, reader_snapshot: Option<&ReaderSnapshot>) {
        let mut show_safe_quit_modal = self.show_safe_quit_modal;
        let mut show_reader_confirm_modal = self.show_reader_confirm_modal;
        let mut safe_quit_confirmed = false;
        let mut return_confirmed = false;
        let mut close_safe_quit_modal = false;
        let mut close_reader_confirm_modal = false;
        let any_modal_open = show_safe_quit_modal || show_reader_confirm_modal;

        if any_modal_open {
            let screen = ctx.screen_rect();
            let layer_id = egui::LayerId::new(Order::Middle, Id::new("modal_overlay"));
            let painter = ctx.layer_painter(layer_id);
            painter.rect_filled(
                screen,
                0.0,
                Color32::from_rgba_unmultiplied(10, 10, 10, 160),
            );
        }

        egui::Window::new("Safe quit confirmation")
            .open(&mut show_safe_quit_modal)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Are you sure you want to quit?");
                ui.horizontal(|ui| {
                    if ui.button("Yes").clicked() {
                        safe_quit_confirmed = true;
                        close_safe_quit_modal = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close_safe_quit_modal = true;
                    }
                });
            });
        egui::Window::new("Reader close confirmation")
            .open(&mut show_reader_confirm_modal)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Return to starter after closing reader?");
                ui.horizontal(|ui| {
                    if ui.button("Confirm").clicked() {
                        return_confirmed = true;
                        close_reader_confirm_modal = true;
                    }
                    if ui.button("Dismiss").clicked() {
                        close_reader_confirm_modal = true;
                    }
                });
            });

        if close_safe_quit_modal {
            show_safe_quit_modal = false;
        }
        self.show_safe_quit_modal = show_safe_quit_modal;
        if safe_quit_confirmed {
            self.record_persistence_event(PersistenceTrigger::SafeQuit, "safe_quit_flow");
            self.record_regression_snapshot(
                RegressionScenario::BookmarkRestore {
                    trigger: PersistenceTrigger::SafeQuit,
                },
                reader_snapshot,
                None,
            );
            self.execute_command(AppCommand::SafeQuit);
        }
        if close_reader_confirm_modal {
            show_reader_confirm_modal = false;
        }
        self.show_reader_confirm_modal = show_reader_confirm_modal;
        if return_confirmed {
            self.record_persistence_event(PersistenceTrigger::SessionClose, "reader_close_flow");
            self.record_regression_snapshot(
                RegressionScenario::BookmarkRestore {
                    trigger: PersistenceTrigger::SessionClose,
                },
                reader_snapshot,
                None,
            );
            self.execute_command(AppCommand::ReturnToStarter);
        }
    }

    fn render_status(&mut self, ctx: &Context) {
        TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Status log:");
                for entry in &self.status_log {
                    ui.label(format!("{} ({:.1}s)", entry.message, entry.age_secs()));
                }
                if self.shell_state.screen_lock_active {
                    ui.colored_label(Color32::YELLOW, "Screen lock active");
                }
            });
            if !self.shell_state.notifications.is_empty() {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Notifications:");
                    for note in &self.shell_state.notifications {
                        let color = match note.level {
                            NotificationLevel::Info => Color32::LIGHT_GRAY,
                            NotificationLevel::Warn => Color32::YELLOW,
                            NotificationLevel::Error => Color32::RED,
                        };
                        ui.colored_label(color, &note.message);
                    }
                });
            }
        });
    }

    fn update_pdf_render_state(&mut self, snapshot: Option<&ReaderSnapshot>) {
        if let Some(snapshot) = snapshot {
            if snapshot.pretty_kind == PrettyKind::Pdf && snapshot.total_pages > 0 {
                self.current_pdf_path = Some(PathBuf::from(&snapshot.source_path));
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
                let plan = build_pdf_viewport_render_plan(&plan_input);
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
                            Some(1.0)
                        } else {
                            None
                        },
                        text_layer_zoom: if plan.text_layer_page_indexes.contains(&page_index) {
                            Some(1.0)
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
                }
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
                self.pdf_render_state.last_updated = Some(Instant::now());
                return;
            }
        }
        self.current_pdf_path = None;
        self.pdf_render_state.reset();
    }
}

impl eframe::App for LanternLeafApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.handle_tts_runtime_events();
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
        ctx.set_visuals(Visuals::dark());
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

#[derive(Default)]
struct PdfRenderState {
    plan: Option<PdfViewportRenderPlan>,
    decision: Option<PdfViewportBudgetDecision>,
    visible_page_indexes: Vec<usize>,
    active_tts_page_index: Option<usize>,
    jump_target_page_index: Option<usize>,
    last_updated: Option<Instant>,
    rendered_canvas_pages: usize,
    rendered_text_layers: usize,
    rendered_overlays: usize,
    highlighted_page: Option<usize>,
    highlighted_sentence_idx: Option<usize>,
    overlay_rects: Vec<[f32; 4]>,
    overlay_alignment_reason: Option<String>,
    overlay_alignment_source: Option<String>,
    overlay_alignment_rects: HashMap<usize, OverlayGeometryEntry>,
    render_events: Vec<PdfRenderEvent>,
    viewport_surfaces: Vec<PdfViewportSurface>,
    throttle_events: Vec<PdfRenderThrottleEvent>,
    native_render_spans: Vec<NativeRenderSpan>,
    native_eviction_events: Vec<NativeRenderEviction>,
    overlay_pressure_alerts: Vec<OverlayPressureAlert>,
    next_overlay_alert_id: usize,
}

impl PdfRenderState {
    fn reset(&mut self) {
        self.plan = None;
        self.decision = None;
        self.visible_page_indexes.clear();
        self.active_tts_page_index = None;
        self.jump_target_page_index = None;
        self.last_updated = None;
        self.rendered_canvas_pages = 0;
        self.rendered_text_layers = 0;
        self.rendered_overlays = 0;
        self.highlighted_page = None;
        self.highlighted_sentence_idx = None;
        self.overlay_rects.clear();
        self.overlay_alignment_reason = None;
        self.overlay_alignment_source = None;
        self.overlay_alignment_rects.clear();
        self.render_events.clear();
        self.viewport_surfaces.clear();
        self.throttle_events.clear();
        self.native_render_spans.clear();
        self.native_eviction_events.clear();
        self.overlay_pressure_alerts.clear();
        self.next_overlay_alert_id = 0;
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
    ) {
        if self.highlighted_page == Some(page_index)
            && self.highlighted_sentence_idx == sentence_idx
        {
            return;
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
        if let Some(surface) = self
            .viewport_surfaces
            .iter_mut()
            .find(|surface| surface.page_index == page_index)
        {
            surface.overlay_rects = self.overlay_rects.clone();
            surface.overlay_reason = self.overlay_alignment_reason.clone();
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
        source_path: &str,
        sentence_idx: usize,
    ) -> Option<OverlayGeometryEntry> {
        self.ensure_alignment_cache(source_path);
        self.overlay_alignment_rects.get(&sentence_idx).cloned()
    }

    fn ensure_alignment_cache(&mut self, source_path: &str) {
        if self.overlay_alignment_source.as_deref() == Some(source_path) {
            return;
        }
        self.overlay_alignment_source = Some(source_path.to_string());
        self.overlay_alignment_rects.clear();
        let path = Path::new(source_path);
        if let Some(artifact) = cache::load_pdf_ocr_alignment_artifact(path) {
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

    fn alignment_rects(alignment: &crate::cache::PdfOcrSentenceAlignment) -> Vec<[f32; 4]> {
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

#[derive(Clone)]
struct OverlayGeometryEntry {
    rects: Vec<[f32; 4]>,
    reason: Option<String>,
}

impl OverlayGeometryEntry {
    fn new(rects: Vec<[f32; 4]>, reason: Option<String>) -> Self {
        Self { rects, reason }
    }

    fn from_alignment(alignment: &crate::cache::PdfOcrSentenceAlignment) -> Option<Self> {
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
        Some(Self::new(rects, reason))
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
