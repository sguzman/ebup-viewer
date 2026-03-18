use crate::logging::effect_span;
use crate::pipeline::{PlannedEffect, RuntimeEffect};
use std::{
    collections::HashMap,
    fmt, panic,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{DispatchPlan, RuntimeEffect};
    use std::time::Duration;

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
}
