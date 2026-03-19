use crate::contracts::BootstrapConfig;
use crate::logging::effect_span;
use crate::pipeline::{
    AppCommand, AppEvent, DispatchPlan, PlannedEffect, RuntimeEffect, apply_event, plan_command,
};
use crate::shortcuts::ShortcutRegistry;
use crate::state::AppState;
use std::{
    collections::HashMap,
    fmt, panic,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    Tts,
    SourceIngestion,
    Pdf,
    BrowserTabs,
    Calibre,
    Persistence,
    Logging,
    Other,
}

impl From<&RuntimeEffect> for TaskKind {
    fn from(effect: &RuntimeEffect) -> Self {
        use RuntimeEffect::*;
        match effect {
            ApplyReaderCommand { .. } | PrecomputeTtsPage => TaskKind::Tts,
            OpenSourcePath { .. }
            | OpenClipboard
            | OpenClipboardText { .. }
            | OpenBrowserTab { .. }
            | OpenBrowserTabBundle { .. }
            | RefreshBrowserTab { .. }
            | ReturnToStarter
            | CloseReaderSession
            | TogglePanel { .. } => TaskKind::SourceIngestion,
            LoadPdfBytes { .. }
            | LoadPdfRenderPrecomputed { .. }
            | LoadPdfSyncMap { .. }
            | PersistPdfSyncMap { .. } => TaskKind::Pdf,
            LoadCalibreBooks { .. }
            | LoadCalibreCachedBooks
            | OpenCalibreBook { .. }
            | EnsureCalibreThumbnail { .. } => TaskKind::Calibre,
            DeleteRecent { .. } | ListRecents { .. } | CloseRecentBrowserTab { .. } => {
                TaskKind::BrowserTabs
            }
            SetRuntimeLogLevel { .. } => TaskKind::Logging,
            FlushPersistence { .. } => TaskKind::Persistence,
            SafeQuit => TaskKind::Other,
            _ => TaskKind::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskPhase {
    Started,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct TaskProgress {
    pub request_id: u64,
    pub kind: TaskKind,
    pub phase: TaskPhase,
    pub percent: Option<f32>,
    pub message: Option<String>,
}

pub type TaskProgressSender = mpsc::Sender<TaskProgress>;
pub type TaskProgressReceiver = mpsc::Receiver<TaskProgress>;

#[derive(Clone)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Clone)]
pub struct TaskContext {
    pub request_id: u64,
    pub kind: TaskKind,
    pub cancellation: CancellationToken,
    pub progress: TaskProgressSender,
}

impl TaskContext {
    pub fn report(&self, phase: TaskPhase, percent: Option<f32>, message: Option<String>) {
        let _ = self.progress.send(TaskProgress {
            request_id: self.request_id,
            kind: self.kind,
            phase,
            percent,
            message,
        });
    }
}

pub struct TaskRuntime {
    registrations: Arc<Mutex<HashMap<u64, CancellationToken>>>,
    progress_tx: TaskProgressSender,
    progress_rx: Arc<Mutex<TaskProgressReceiver>>,
}

impl TaskRuntime {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            registrations: Arc::new(Mutex::new(HashMap::new())),
            progress_tx: tx,
            progress_rx: Arc::new(Mutex::new(rx)),
        }
    }

    pub fn spawn_task<F>(&self, planned: PlannedEffect, work: F)
    where
        F: FnOnce(TaskContext) + Send + 'static,
    {
        let cancellation = CancellationToken::new();
        self.registrations
            .lock()
            .unwrap()
            .insert(planned.request_id, cancellation.clone());

        let progress = self.progress_tx.clone();
        let kind = TaskKind::from(&planned.effect);
        let context = TaskContext {
            request_id: planned.request_id,
            kind,
            cancellation: cancellation.clone(),
            progress,
        };

        let span = effect_span(planned.request_id, &planned.effect);
        thread::spawn(move || {
            let _span_guard = span.enter();
            context.report(TaskPhase::Started, None, Some("task_spawned".to_string()));
            if context.cancellation.is_cancelled() {
                context.report(
                    TaskPhase::Cancelled,
                    None,
                    Some("cancelled_before_start".to_string()),
                );
                return;
            }
            let result = panic::catch_unwind(panic::AssertUnwindSafe(|| work(context.clone())));
            match result {
                Ok(_) => {
                    context.report(
                        TaskPhase::Completed,
                        None,
                        Some("task_completed".to_string()),
                    );
                }
                Err(err) => {
                    let message = if let Some(string) = err.downcast_ref::<&str>() {
                        string.to_string()
                    } else if let Some(string) = err.downcast_ref::<String>() {
                        string.clone()
                    } else {
                        "panic during task".to_string()
                    };
                    context.report(TaskPhase::Failed, None, Some(message));
                }
            }
        });
    }

    pub fn cancel(&self, request_id: u64) {
        if let Some(token) = self.registrations.lock().unwrap().get(&request_id) {
            token.cancel();
        }
    }

    pub fn collect_progress(&self) -> Vec<TaskProgress> {
        let receiver = self.progress_rx.lock().unwrap();
        let mut results = Vec::new();
        while let Ok(progress) = receiver.try_recv() {
            results.push(progress);
        }
        results
    }
}

pub struct ProgressBatcher {
    bucket: Vec<TaskProgress>,
}

impl ProgressBatcher {
    pub fn new() -> Self {
        Self { bucket: Vec::new() }
    }

    pub fn collect(&mut self, runtime: &TaskRuntime) {
        for progress in runtime.collect_progress() {
            if let Some(existing) = self.bucket.iter_mut().find(|entry| {
                entry.request_id == progress.request_id && entry.phase == progress.phase
            }) {
                *existing = progress;
            } else {
                self.bucket.push(progress);
            }
        }
    }

    pub fn drain(&mut self) -> Vec<TaskProgress> {
        std::mem::take(&mut self.bucket)
    }
}

impl fmt::Debug for TaskRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaskRuntime")
            .field("registrations", &self.registrations.lock().unwrap().len())
            .finish()
    }
}

/// Orchestrates the Rust-native app runtime foundation, exposing the shared state, tracing-aware
/// command planning, and configured shortcut registry that the future egui shell will consume.
#[derive(Clone)]
pub struct AppRuntime {
    state: Arc<Mutex<AppState>>,
    task_runtime: TaskRuntime,
    progress_batcher: Mutex<ProgressBatcher>,
    next_request_id: AtomicU64,
    shortcuts: ShortcutRegistry,
}

impl Default for AppRuntime {
    fn default() -> Self {
        Self::new(ShortcutRegistry::default())
    }
}

impl AppRuntime {
    /// Build the runtime using the provided shortcut registry.
    pub fn new(shortcuts: ShortcutRegistry) -> Self {
        Self {
            state: Arc::new(Mutex::new(AppState::default())),
            task_runtime: TaskRuntime::new(),
            progress_batcher: Mutex::new(ProgressBatcher::new()),
            next_request_id: AtomicU64::new(1),
            shortcuts,
        }
    }

    /// Builds a runtime preconfigured with the bootstrap shortcut map.
    pub fn with_bootstrap_config(config: &BootstrapConfig) -> Self {
        Self::new(ShortcutRegistry::with_bootstrap_config(config))
    }

    /// Access the shared shortcut registry.
    pub fn shortcut_registry(&self) -> ShortcutRegistry {
        self.shortcuts.clone()
    }

    /// Retrieves a fresh plan request id.
    pub fn next_request_id(&self) -> u64 {
        self.next_request_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Plans a command using `pipeline::plan_command` and the current snapshot of state.
    pub fn plan_command(&self, command: AppCommand) -> DispatchPlan {
        let request_id = self.next_request_id();
        let guard = self.state.lock().unwrap();
        plan_command(&guard, request_id, command)
    }

    /// Applies an event to the shared `AppState`.
    pub fn apply_event(&self, event: AppEvent) {
        let mut guard = self.state.lock().unwrap();
        apply_event(&mut guard, event);
    }

    /// Returns a cloned snapshot of the current `AppState`.
    pub fn state_snapshot(&self) -> AppState {
        let guard = self.state.lock().unwrap();
        guard.clone()
    }

    /// Collects task progress updates via the internal batcher.
    pub fn collect_progress(&self) -> Vec<TaskProgress> {
        let mut batcher = self.progress_batcher.lock().unwrap();
        batcher.collect(&self.task_runtime);
        batcher.drain()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{AppCommand, DispatchPlan, ReaderCommand, RuntimeEffect};
    use std::time::Duration;

    use crate::contracts::BootstrapConfig;
    use crate::pipeline::ReaderCommand;
    use crate::shortcuts::{ShortcutAction, ShortcutScope};
    use config::{FontFamily, FontWeight, HighlightColor, ThemeMode};
    use lanternleaf_core::session::{self, SessionCommand};

    #[test]
    fn cancellation_token_detects_cancel() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn progress_batcher_coalesces_same_phase() {
        let runtime = TaskRuntime::new();
        let plan = DispatchPlan {
            request_id: 1,
            action: "reader_tts_play",
            local_events: Vec::new(),
            effects: vec![PlannedEffect {
                request_id: 1,
                effect: RuntimeEffect::LoadCalibreCachedBooks,
            }],
        };
        runtime.spawn_task(plan.effects[0].clone(), |context| {
            context.report(
                TaskPhase::InProgress,
                Some(0.1),
                Some("halfway".to_string()),
            );
            thread::sleep(Duration::from_millis(10));
            context.report(
                TaskPhase::InProgress,
                Some(0.2),
                Some("progressing".to_string()),
            );
        });
        thread::sleep(Duration::from_millis(50));
        let mut batcher = ProgressBatcher::new();
        batcher.collect(&runtime);
        assert!(
            batcher
                .bucket
                .iter()
                .any(|entry| entry.phase == TaskPhase::InProgress && entry.percent == Some(0.2))
        );
    }

    #[test]
    fn runtime_collect_progress_returns_entries() {
        let runtime = TaskRuntime::new();
        let plan = DispatchPlan {
            request_id: 2,
            action: "reader_next_page",
            local_events: Vec::new(),
            effects: vec![PlannedEffect {
                request_id: 2,
                effect: RuntimeEffect::LoadBootstrap,
            }],
        };
        runtime.spawn_task(plan.effects[0].clone(), |context| {
            context.report(TaskPhase::InProgress, None, Some("working".to_string()));
        });
        thread::sleep(Duration::from_millis(20));
        let results = runtime.collect_progress();
        assert!(!results.is_empty());
    }

    fn sample_bootstrap_config() -> BootstrapConfig {
        BootstrapConfig {
            theme: ThemeMode::Day,
            font_family: FontFamily::Lexend,
            font_weight: FontWeight::Normal,
            day_highlight: HighlightColor {
                r: 0.1,
                g: 0.2,
                b: 0.3,
                a: 0.4,
            },
            night_highlight: HighlightColor {
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
        }
    }

    #[test]
    fn runtime_request_ids_increment() {
        let runtime = AppRuntime::default();
        let plan_one = runtime.plan_command(AppCommand::Bootstrap);
        let plan_two = runtime.plan_command(AppCommand::RefreshRecents { limit: None });
        assert_eq!(plan_two.request_id, plan_one.request_id + 1);
    }

    #[test]
    fn runtime_shortcuts_expose_configured_actions() {
        let runtime = AppRuntime::with_bootstrap_config(&sample_bootstrap_config());
        let shortcuts = runtime.shortcut_registry();
        let matches = shortcuts.matches("space", ShortcutScope::Reader);
        assert!(matches.iter().any(|binding| {
            matches!(
                binding.action,
                ShortcutAction::Command(AppCommand::Reader(ReaderCommand::Session(
                    SessionCommand::TtsTogglePlayPause
                )))
            )
        }));
    }
}
