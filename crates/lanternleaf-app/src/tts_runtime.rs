use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use lanternleaf_core::{cache, cancellation, config, normalizer, session, tts};
use tracing::{info, trace, warn};

const TTS_PROGRESS_POLL_INTERVAL: Duration = Duration::from_millis(8);
const TTS_PREPARE_SENTENCE_WINDOW: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtsRuntimeMode {
    Real,
    Simulated,
}

#[derive(Debug, Clone)]
pub enum TtsCommand {
    Play,
    Pause,
    TogglePlayPause,
    PlayFromPageStart,
    PlayFromHighlight,
    SeekNext,
    SeekPrev,
    RepeatSentence,
    Stop,
    ApplySettings { patch: session::ReaderSettingsPatch },
}

impl TtsCommand {
    pub fn from_session_command(command: &session::SessionCommand) -> Option<Self> {
        match command {
            session::SessionCommand::TtsPlay => Some(Self::Play),
            session::SessionCommand::TtsPause => Some(Self::Pause),
            session::SessionCommand::TtsTogglePlayPause => Some(Self::TogglePlayPause),
            session::SessionCommand::TtsPlayFromPageStart => Some(Self::PlayFromPageStart),
            session::SessionCommand::TtsPlayFromHighlight => Some(Self::PlayFromHighlight),
            session::SessionCommand::TtsSeekNext => Some(Self::SeekNext),
            session::SessionCommand::TtsSeekPrev => Some(Self::SeekPrev),
            session::SessionCommand::TtsRepeatSentence => Some(Self::RepeatSentence),
            session::SessionCommand::TtsStop => Some(Self::Stop),
            session::SessionCommand::ApplySettings { patch } => {
                if patch_has_tts_fields(patch) {
                    Some(Self::ApplySettings { patch: patch.clone() })
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn to_session_command(&self) -> session::SessionCommand {
        match self {
            Self::Play => session::SessionCommand::TtsPlay,
            Self::Pause => session::SessionCommand::TtsPause,
            Self::TogglePlayPause => session::SessionCommand::TtsTogglePlayPause,
            Self::PlayFromPageStart => session::SessionCommand::TtsPlayFromPageStart,
            Self::PlayFromHighlight => session::SessionCommand::TtsPlayFromHighlight,
            Self::SeekNext => session::SessionCommand::TtsSeekNext,
            Self::SeekPrev => session::SessionCommand::TtsSeekPrev,
            Self::RepeatSentence => session::SessionCommand::TtsRepeatSentence,
            Self::Stop => session::SessionCommand::TtsStop,
            Self::ApplySettings { patch } => {
                session::SessionCommand::ApplySettings { patch: patch.clone() }
            }
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Play => "tts.play",
            Self::Pause => "tts.pause",
            Self::TogglePlayPause => "tts.toggle_play_pause",
            Self::PlayFromPageStart => "tts.play_page_start",
            Self::PlayFromHighlight => "tts.play_from_highlight",
            Self::SeekNext => "tts.seek_next",
            Self::SeekPrev => "tts.seek_prev",
            Self::RepeatSentence => "tts.repeat_sentence",
            Self::Stop => "tts.stop",
            Self::ApplySettings { .. } => "tts.apply_settings",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtsRuntimeEventKind {
    StateChanged,
    Progress,
    Queued,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct TtsRuntimeEvent {
    pub request_id: u64,
    pub action: String,
    pub kind: TtsRuntimeEventKind,
    pub snapshot: Option<session::ReaderSnapshot>,
    pub playback: Option<crate::contracts::ReaderPlaybackState>,
    pub tts: Option<session::ReaderTtsView>,
    pub message: Option<String>,
    pub cursor: Option<TtsCursor>,
}

#[derive(Debug, Clone)]
pub struct TtsPlaybackSnapshot {
    pub state: session::TtsPlaybackState,
    pub current_sentence_idx: Option<usize>,
    pub total_sentences: usize,
    pub progress_pct: f64,
    pub speed: f32,
    pub volume: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct TtsCursor {
    pub audio_idx: Option<usize>,
    pub display_idx: Option<usize>,
    pub page: usize,
}

#[derive(Debug)]
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

#[derive(Default)]
struct TtsEventBatcher {
    bucket: Vec<TtsRuntimeEvent>,
}

impl TtsEventBatcher {
    fn collect(&mut self, rx: &mpsc::Receiver<TtsRuntimeEvent>) {
        while let Ok(event) = rx.try_recv() {
            if matches!(event.kind, TtsRuntimeEventKind::Progress | TtsRuntimeEventKind::StateChanged)
            {
                if let Some(existing) = self.bucket.iter_mut().find(|entry| {
                    entry.request_id == event.request_id && entry.kind == event.kind
                }) {
                    *existing = event;
                    continue;
                }
            }
            self.bucket.push(event);
        }
    }

    fn drain(&mut self) -> Vec<TtsRuntimeEvent> {
        std::mem::take(&mut self.bucket)
    }
}

#[derive(Clone)]
pub struct TtsRuntime {
    mode: TtsRuntimeMode,
    normalizer: normalizer::TextNormalizer,
    panels: Arc<Mutex<session::PanelState>>,
    session: Arc<Mutex<Option<session::ReaderSession>>>,
    request: Arc<Mutex<Option<TtsRequestRuntime>>>,
    next_request_id: Arc<AtomicU64>,
    event_tx: mpsc::Sender<TtsRuntimeEvent>,
    event_rx: Arc<Mutex<mpsc::Receiver<TtsRuntimeEvent>>>,
    event_batcher: Arc<Mutex<TtsEventBatcher>>,
}

impl TtsRuntime {
    pub fn new(normalizer: normalizer::TextNormalizer) -> Self {
        Self::new_with_mode(normalizer, TtsRuntimeMode::Real)
    }

    pub fn new_with_mode(normalizer: normalizer::TextNormalizer, mode: TtsRuntimeMode) -> Self {
        let (event_tx, event_rx) = mpsc::channel();
        Self {
            mode,
            normalizer,
            panels: Arc::new(Mutex::new(session::PanelState::default())),
            session: Arc::new(Mutex::new(None)),
            request: Arc::new(Mutex::new(None)),
            next_request_id: Arc::new(AtomicU64::new(1)),
            event_tx,
            event_rx: Arc::new(Mutex::new(event_rx)),
            event_batcher: Arc::new(Mutex::new(TtsEventBatcher::default())),
        }
    }

    pub fn set_session(&self, session: Option<session::ReaderSession>) {
        if session.is_none() {
            self.cancel_request();
        }
        if let Ok(mut guard) = self.session.lock() {
            *guard = session;
        }
    }

    pub fn set_panels(&self, panels: session::PanelState) {
        if let Ok(mut guard) = self.panels.lock() {
            *guard = panels;
        }
    }

    pub fn snapshot(&self) -> Option<session::ReaderSnapshot> {
        let Ok(mut session_guard) = self.session.lock() else {
            return None;
        };
        let reader = session_guard.as_mut()?;
        let panels = panels_snapshot(&self.panels);
        Some(reader.snapshot(panels, &self.normalizer))
    }

    pub fn apply_command(&self, command: TtsCommand) -> Option<session::ReaderSnapshot> {
        trace!(tts_command = command.label(), "Applying TTS command");
        let mut should_sync_tts = true;
        match command {
            TtsCommand::Play => {
                should_sync_tts = !self.maybe_resume_playback();
            }
            TtsCommand::Pause => {
                self.pause_playback();
                should_sync_tts = false;
            }
            TtsCommand::TogglePlayPause => {
                should_sync_tts = !self.maybe_toggle_playback();
            }
            _ => {}
        }

        let command_for_session = command.to_session_command();
        let action = command_for_session.action();
        let sync_after_command = should_sync_tts_after_reader_command(&command_for_session);
        let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
        let snapshot = {
            let mut guard = self.session.lock().ok()?;
            let reader = match guard.as_mut() {
                Some(reader) => reader,
                None => {
                    warn!(
                        request_id,
                        action,
                        "Ignoring TTS command because no reader session is active"
                    );
                    self.emit_event(TtsRuntimeEvent {
                        request_id,
                        action: action.to_string(),
                        kind: TtsRuntimeEventKind::Failed,
                        snapshot: None,
                        playback: None,
                        tts: None,
                        message: Some("no reader session".to_string()),
                        cursor: None,
                    });
                    return None;
                }
            };
            let panels = panels_snapshot(&self.panels);
            let event = reader.apply_command(command_for_session, panels, &self.normalizer);
            event.snapshot
        };

        let cursor = cursor_from_snapshot(&snapshot);
        self.emit_event(TtsRuntimeEvent {
            request_id,
            action: action.to_string(),
            kind: TtsRuntimeEventKind::StateChanged,
            snapshot: Some(snapshot.clone()),
            playback: Some(reader_playback_state_from_snapshot(&snapshot)),
            tts: Some(snapshot.tts.clone()),
            message: None,
            cursor,
        });

        if should_sync_tts && sync_after_command {
            self.sync_tts_runtime_after_reader_change();
        }

        Some(snapshot)
    }

    pub fn collect_events(&self) -> Vec<TtsRuntimeEvent> {
        let rx_guard = match self.event_rx.lock() {
            Ok(guard) => guard,
            Err(_) => return Vec::new(),
        };
        let mut batcher = match self.event_batcher.lock() {
            Ok(guard) => guard,
            Err(_) => return Vec::new(),
        };
        batcher.collect(&rx_guard);
        batcher.drain()
    }

    fn emit_event(&self, event: TtsRuntimeEvent) {
        let _ = self.event_tx.send(event);
    }

    fn cancel_request(&self) {
        if let Ok(mut guard) = self.request.lock() {
            if let Some(runtime) = guard.take() {
                runtime.cancel_token.cancel();
                self.emit_event(TtsRuntimeEvent {
                    request_id: runtime.request_id,
                    action: "reader_tts_runtime_cancelled".to_string(),
                    kind: TtsRuntimeEventKind::Cancelled,
                    snapshot: None,
                    playback: None,
                    tts: None,
                    message: None,
                    cursor: None,
                });
            }
        }
    }

    fn pause_playback(&self) {
        let behavior = self
            .session
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|reader| reader.config.tts_pause_resume_behavior))
            .unwrap_or_default();
        match behavior {
            config::TtsPauseResumeBehavior::RestartSentence => self.cancel_request(),
            _ => {
                if let Ok(guard) = self.request.lock() {
                    if let Some(runtime) = guard.as_ref() {
                        runtime.set_paused(true);
                    }
                }
            }
        }
    }

    fn maybe_resume_playback(&self) -> bool {
        let (behavior, paused, can_resume) = {
            let Ok(mut guard) = self.session.lock() else {
                return false;
            };
            let reader = match guard.as_mut() {
                Some(reader) => reader,
                None => return false,
            };
            let panels = panels_snapshot(&self.panels);
            let state = reader.snapshot(panels, &self.normalizer).tts.state;
            (
                reader.config.tts_pause_resume_behavior,
                state == session::TtsPlaybackState::Paused,
                self.request.lock().ok().and_then(|guard| guard.as_ref().map(|_| ())).is_some(),
            )
        };

        if behavior == config::TtsPauseResumeBehavior::ResumeFromPausePoint && paused && can_resume {
            if let Ok(guard) = self.request.lock() {
                if let Some(runtime) = guard.as_ref() {
                    runtime.set_paused(false);
                    return true;
                }
            }
        }
        false
    }

    fn maybe_toggle_playback(&self) -> bool {
        let (behavior, tts_state, can_resume) = {
            let Ok(mut guard) = self.session.lock() else {
                return false;
            };
            let reader = match guard.as_mut() {
                Some(reader) => reader,
                None => return false,
            };
            let panels = panels_snapshot(&self.panels);
            let state = reader.snapshot(panels, &self.normalizer).tts.state;
            (
                reader.config.tts_pause_resume_behavior,
                state,
                self.request.lock().ok().and_then(|guard| guard.as_ref().map(|_| ())).is_some(),
            )
        };

        match tts_state {
            session::TtsPlaybackState::Playing => {
                if behavior == config::TtsPauseResumeBehavior::RestartSentence {
                    self.cancel_request();
                } else if let Ok(guard) = self.request.lock() {
                    if let Some(runtime) = guard.as_ref() {
                        runtime.set_paused(true);
                    }
                }
                true
            }
            session::TtsPlaybackState::Paused => {
                if behavior == config::TtsPauseResumeBehavior::ResumeFromPausePoint && can_resume {
                    if let Ok(guard) = self.request.lock() {
                        if let Some(runtime) = guard.as_ref() {
                            runtime.set_paused(false);
                            return true;
                        }
                    }
                }
                false
            }
            session::TtsPlaybackState::Idle => false,
        }
    }

    fn sync_tts_runtime_after_reader_change(&self) {
        let plan = self.build_tts_playback_plan();
        if plan.is_none() {
            self.cancel_request();
            return;
        }
        let plan = plan.expect("plan exists");
        self.cancel_request();
        let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
        let cancel_token = cancellation::CancellationToken::new();
        let pause_requested = Arc::new(AtomicBool::new(false));
        {
            let mut guard = match self.request.lock() {
                Ok(guard) => guard,
                Err(_) => return,
            };
            *guard = Some(TtsRequestRuntime {
                request_id,
                cancel_token: cancel_token.clone(),
                pause_requested: pause_requested.clone(),
            });
        }

        info!(
            request_id,
            page = plan.page + 1,
            sentence_idx = plan.start_idx,
            sentence_count = plan.sentences.len(),
            "Starting TTS runtime playback job"
        );

        let ctx = TtsRuntimeContext {
            mode: self.mode,
            normalizer: self.normalizer.clone(),
            panels: self.panels.clone(),
            session: self.session.clone(),
            request: self.request.clone(),
            event_tx: self.event_tx.clone(),
        };

        thread::spawn(move || {
            run_tts_runtime_loop(ctx, request_id, cancel_token, pause_requested);
        });
    }

    fn build_tts_playback_plan(&self) -> Option<TtsPlaybackPlan> {
        let mut guard = self.session.lock().ok()?;
        let reader = guard.as_mut()?;
        let panels = panels_snapshot(&self.panels);
        let snapshot = reader.snapshot(panels, &self.normalizer);
        if snapshot.tts.state != session::TtsPlaybackState::Playing {
            return None;
        }
        let (audio_sentences, start_idx) = reader.current_tts_audio_slice(&self.normalizer);
        if audio_sentences.is_empty() {
            return None;
        }
        trace!(
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
}

#[derive(Clone)]
struct TtsRuntimeContext {
    mode: TtsRuntimeMode,
    normalizer: normalizer::TextNormalizer,
    panels: Arc<Mutex<session::PanelState>>,
    session: Arc<Mutex<Option<session::ReaderSession>>>,
    request: Arc<Mutex<Option<TtsRequestRuntime>>>,
    event_tx: mpsc::Sender<TtsRuntimeEvent>,
}

fn run_tts_runtime_loop(
    ctx: TtsRuntimeContext,
    runtime_request_id: u64,
    cancel_token: cancellation::CancellationToken,
    pause_requested: Arc<AtomicBool>,
) {
    struct PrefetchedBatch {
        source_path: PathBuf,
        page: usize,
        start_idx: usize,
        prepared: Vec<PreparedSentence>,
    }

    struct PendingPrefetch {
        source_path: PathBuf,
        page: usize,
        start_idx: usize,
        handle: thread::JoinHandle<Result<Vec<PreparedSentence>, String>>,
    }

    let mut engine: Option<tts::TtsEngine> = None;
    let mut ready_prefetch: Option<PrefetchedBatch> = None;

    loop {
        if cancel_token.is_cancelled() {
            break;
        }

        let Some(plan) = collect_tts_playback_plan(&ctx, runtime_request_id) else {
            break;
        };
        if plan.start_idx >= plan.sentences.len() {
            break;
        }

        if ctx.mode == TtsRuntimeMode::Real && engine.is_none() {
            let built_engine = match tts::TtsEngine::new(plan.model_path.clone(), plan.espeak_path.clone()) {
                Ok(engine) => engine,
                Err(err) => {
                    transition_tts_runtime_to_paused(&ctx, runtime_request_id, "reader_tts_runtime_error", &format!("Failed to initialize Piper TTS engine: {err}"));
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
                prepare_tts_batch(&ctx, &plan, plan.start_idx, chunk_end, engine.as_ref())
                    .unwrap_or_else(|err| {
                        transition_tts_runtime_to_paused(
                            &ctx,
                            runtime_request_id,
                            "reader_tts_runtime_error",
                            &format!("Failed to prepare TTS audio batch: {err}"),
                        );
                        Vec::new()
                    })
            }
        } else {
            prepare_tts_batch(&ctx, &plan, plan.start_idx, chunk_end, engine.as_ref()).unwrap_or_else(|err| {
                transition_tts_runtime_to_paused(
                    &ctx,
                    runtime_request_id,
                    "reader_tts_runtime_error",
                    &format!("Failed to prepare TTS audio batch: {err}"),
                );
                Vec::new()
            })
        };

        if cancel_token.is_cancelled() {
            break;
        }
        if prepared.is_empty() {
            transition_tts_runtime_to_paused(
                &ctx,
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
            let next_engine = engine.as_ref().cloned();
            let next_ctx = ctx.clone();
            let next_plan = plan.clone();
            Some(PendingPrefetch {
                source_path: plan.source_path.clone(),
                page: plan.page,
                start_idx: next_chunk_start,
                handle: thread::spawn(move || {
                    prepare_tts_batch(
                        &next_ctx,
                        &next_plan,
                        next_chunk_start,
                        next_chunk_end,
                        next_engine.as_ref(),
                    )
                    .map_err(|err| err.to_string())
                }),
            })
        } else {
            None
        };

        let playback = match build_playback(&ctx, &plan, &prepared, engine.as_ref()) {
            Ok(playback) => playback,
            Err(err) => {
                if cancel_token.is_cancelled() {
                    break;
                }
                transition_tts_runtime_to_paused(
                    &ctx,
                    runtime_request_id,
                    "reader_tts_runtime_error",
                    &format!("Failed to start Piper playback: {err}"),
                );
                break;
            }
        };

        emit_queued_event(&ctx, runtime_request_id, &plan, &prepared);

        let mut continue_playback = true;
        for duration in playback.sentence_durations.iter().copied() {
            let mut remaining = duration.saturating_add(plan.pause_after);
            let mut last_tick = Instant::now();
            loop {
                if cancel_token.is_cancelled() {
                    playback.stop();
                    clear_tts_request_if_current(&ctx, runtime_request_id);
                    emit_terminal_event(
                        &ctx,
                        runtime_request_id,
                        TtsRuntimeEventKind::Cancelled,
                        "reader_tts_runtime_cancelled",
                        None,
                    );
                    return;
                }

                if pause_requested.load(Ordering::SeqCst) {
                    if !playback.is_paused() {
                        playback.pause();
                    }
                    last_tick = Instant::now();
                    thread::sleep(TTS_PROGRESS_POLL_INTERVAL);
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
                thread::sleep(TTS_PROGRESS_POLL_INTERVAL);
            }

            if cancel_token.is_cancelled() {
                playback.stop();
                clear_tts_request_if_current(&ctx, runtime_request_id);
                emit_terminal_event(
                    &ctx,
                    runtime_request_id,
                    TtsRuntimeEventKind::Cancelled,
                    "reader_tts_runtime_cancelled",
                    None,
                );
                return;
            }

            if !advance_tts_runtime_cursor(&ctx, runtime_request_id) {
                continue_playback = false;
                break;
            }
        }

        loop {
            if cancel_token.is_cancelled() {
                playback.stop();
                clear_tts_request_if_current(&ctx, runtime_request_id);
                emit_terminal_event(
                    &ctx,
                    runtime_request_id,
                    TtsRuntimeEventKind::Cancelled,
                    "reader_tts_runtime_cancelled",
                    None,
                );
                return;
            }

            if pause_requested.load(Ordering::SeqCst) {
                if !playback.is_paused() {
                    playback.pause();
                }
                thread::sleep(TTS_PROGRESS_POLL_INTERVAL);
                continue;
            }

            if playback.is_paused() {
                playback.play();
            }

            if playback.queued_sources() == 0 {
                break;
            }

            thread::sleep(TTS_PROGRESS_POLL_INTERVAL);
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

    clear_tts_request_if_current(&ctx, runtime_request_id);
    let kind = if cancel_token.is_cancelled() {
        TtsRuntimeEventKind::Cancelled
    } else {
        TtsRuntimeEventKind::Completed
    };
    let action = if kind == TtsRuntimeEventKind::Cancelled {
        "reader_tts_runtime_cancelled"
    } else {
        "reader_tts_runtime_complete"
    };
    emit_terminal_event(&ctx, runtime_request_id, kind, action, None);
}

fn collect_tts_playback_plan(
    ctx: &TtsRuntimeContext,
    runtime_request_id: u64,
) -> Option<TtsPlaybackPlan> {
    let mut guard = ctx.session.lock().ok()?;
    let Some(reader) = guard.as_mut() else {
        return None;
    };
    let current_request_id = ctx
        .request
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|runtime| runtime.request_id));
    if current_request_id != Some(runtime_request_id) {
        return None;
    }
    let panels = panels_snapshot(&ctx.panels);
    let snapshot = reader.snapshot(panels, &ctx.normalizer);
    if snapshot.tts.state != session::TtsPlaybackState::Playing {
        return None;
    }
    let (audio_sentences, start_idx) = reader.current_tts_audio_slice(&ctx.normalizer);
    if audio_sentences.is_empty() {
        return None;
    }
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

fn clear_tts_request_if_current(ctx: &TtsRuntimeContext, runtime_request_id: u64) {
    if let Ok(mut guard) = ctx.request.lock() {
        let current_request_id = guard.as_ref().map(|runtime| runtime.request_id);
        if current_request_id == Some(runtime_request_id) {
            *guard = None;
        }
    }
}

fn transition_tts_runtime_to_paused(
    ctx: &TtsRuntimeContext,
    runtime_request_id: u64,
    action: &str,
    message: &str,
) {
    let event_payload = {
        let mut guard = match ctx.session.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        let current_request_id = ctx
            .request
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|runtime| runtime.request_id));
        if current_request_id != Some(runtime_request_id) {
            return;
        }
        let reader = match guard.as_mut() {
            Some(reader) => reader,
            None => return,
        };
        let panels = panels_snapshot(&ctx.panels);
        let event = reader.apply_command(session::SessionCommand::TtsPause, panels, &ctx.normalizer);
        persist_reader_progress(reader, "tts_runtime_pause");
        Some((event.snapshot, reader.source_path.clone()))
    };

    if let Some((snapshot, source_path)) = event_payload {
        warn!(
            runtime_request_id,
            source = %source_path.display(),
            error = %message,
            "TTS runtime transitioned to paused"
        );
        emit_snapshot_event(ctx, runtime_request_id, action, snapshot, TtsRuntimeEventKind::Failed, Some(message.to_string()));
    }
}

fn advance_tts_runtime_cursor(ctx: &TtsRuntimeContext, runtime_request_id: u64) -> bool {
    let event_payload = {
        let mut guard = match ctx.session.lock() {
            Ok(guard) => guard,
            Err(_) => return false,
        };
        let current_request_id = ctx
            .request
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|runtime| runtime.request_id));
        if current_request_id != Some(runtime_request_id) {
            return false;
        }
        let reader = match guard.as_mut() {
            Some(reader) => reader,
            None => return false,
        };
        let panels = panels_snapshot(&ctx.panels);
        let current_snapshot = reader.snapshot(panels, &ctx.normalizer);
        if current_snapshot.tts.state != session::TtsPlaybackState::Playing {
            return false;
        }
        let event = reader.apply_command(session::SessionCommand::TtsSeekNext, panels, &ctx.normalizer);
        persist_reader_progress(reader, "tts_runtime_step");
        Some(event.snapshot)
    };

    if let Some(snapshot) = event_payload {
        emit_snapshot_event(
            ctx,
            runtime_request_id,
            "reader_tts_runtime_step",
            snapshot.clone(),
            TtsRuntimeEventKind::Progress,
            None,
        );
        snapshot.tts.state == session::TtsPlaybackState::Playing
    } else {
        false
    }
}

fn persist_reader_progress(reader: &mut session::ReaderSession, reason: &'static str) {
    let bookmark = reader.to_bookmark();
    let source_path = reader.source_path.clone();
    trace!(
        path = %reader.source_path.display(),
        page = reader.current_page + 1,
        reason,
        "Persisting active reader progress"
    );
    let _ = cache::save_bookmark(source_path.as_path(), &bookmark);
}

#[derive(Debug, Clone)]
struct PreparedSentence {
    path: Option<PathBuf>,
    duration: Duration,
}

fn prepare_tts_batch(
    ctx: &TtsRuntimeContext,
    plan: &TtsPlaybackPlan,
    chunk_start: usize,
    chunk_end: usize,
    engine: Option<&tts::TtsEngine>,
) -> Result<Vec<PreparedSentence>, String> {
    let chunk_sentences = plan.sentences[chunk_start..chunk_end].to_vec();
    match ctx.mode {
        TtsRuntimeMode::Simulated => Ok(chunk_sentences
            .iter()
            .map(|sentence| PreparedSentence {
                path: None,
                duration: simulated_sentence_duration(sentence, plan.speed),
            })
            .collect()),
        TtsRuntimeMode::Real => {
            let engine = engine.ok_or_else(|| "TTS engine missing".to_string())?;
            let cache_root = cache::hash_dir(&plan.source_path).join("tts");
            let prepared = engine
                .prepare_batch(
                    cache_root,
                    chunk_sentences,
                    0,
                    plan.threads,
                    plan.progress_log_interval,
                )
                .map_err(|err| err.to_string())?;
            Ok(prepared
                .into_iter()
                .map(|(path, duration)| PreparedSentence {
                    path: Some(path),
                    duration,
                })
                .collect())
        }
    }
}

struct PlaybackHandle {
    kind: PlaybackKind,
    sentence_durations: Vec<Duration>,
}

enum PlaybackKind {
    Real(tts::TtsPlayback),
    Simulated {
        paused: Arc<AtomicBool>,
        queued: Arc<AtomicUsize>,
    },
}

impl PlaybackHandle {
    fn pause(&self) {
        match &self.kind {
            PlaybackKind::Real(playback) => playback.pause(),
            PlaybackKind::Simulated { paused, .. } => {
                paused.store(true, Ordering::SeqCst);
            }
        }
    }

    fn play(&self) {
        match &self.kind {
            PlaybackKind::Real(playback) => playback.play(),
            PlaybackKind::Simulated { paused, .. } => {
                paused.store(false, Ordering::SeqCst);
            }
        }
    }

    fn is_paused(&self) -> bool {
        match &self.kind {
            PlaybackKind::Real(playback) => playback.is_paused(),
            PlaybackKind::Simulated { paused, .. } => paused.load(Ordering::SeqCst),
        }
    }

    fn stop(self) {
        match self.kind {
            PlaybackKind::Real(playback) => playback.stop(),
            PlaybackKind::Simulated { queued, .. } => {
                queued.store(0, Ordering::SeqCst);
            }
        }
    }

    fn queued_sources(&self) -> usize {
        match &self.kind {
            PlaybackKind::Real(playback) => playback.queued_sources(),
            PlaybackKind::Simulated { queued, .. } => queued.load(Ordering::SeqCst),
        }
    }
}

fn build_playback(
    ctx: &TtsRuntimeContext,
    plan: &TtsPlaybackPlan,
    prepared: &[PreparedSentence],
    engine: Option<&tts::TtsEngine>,
) -> Result<PlaybackHandle, String> {
    match ctx.mode {
        TtsRuntimeMode::Simulated => {
            let sentence_durations = prepared.iter().map(|item| item.duration).collect::<Vec<_>>();
            let queued = Arc::new(AtomicUsize::new(sentence_durations.len()));
            Ok(PlaybackHandle {
                kind: PlaybackKind::Simulated {
                    paused: Arc::new(AtomicBool::new(false)),
                    queued,
                },
                sentence_durations,
            })
        }
        TtsRuntimeMode::Real => {
            let engine = engine.ok_or_else(|| "TTS engine missing".to_string())?;
            let files: Vec<PathBuf> = prepared
                .iter()
                .filter_map(|item| item.path.clone())
                .collect();
            let playback = engine
                .play_files(&files, plan.pause_after, plan.speed, plan.volume, false)
                .map_err(|err| err.to_string())?;
            let sentence_durations = playback.sentence_durations().to_vec();
            Ok(PlaybackHandle {
                kind: PlaybackKind::Real(playback),
                sentence_durations,
            })
        }
    }
}

fn emit_snapshot_event(
    ctx: &TtsRuntimeContext,
    request_id: u64,
    action: &str,
    snapshot: session::ReaderSnapshot,
    kind: TtsRuntimeEventKind,
    message: Option<String>,
) {
    let playback = reader_playback_state_from_snapshot(&snapshot);
    let cursor = cursor_from_snapshot(&snapshot);
    let event = TtsRuntimeEvent {
        request_id,
        action: action.to_string(),
        kind,
        snapshot: Some(snapshot.clone()),
        playback: Some(playback),
        tts: Some(snapshot.tts.clone()),
        message,
        cursor,
    };
    let _ = ctx.event_tx.send(event);
}

fn emit_terminal_event(
    ctx: &TtsRuntimeContext,
    request_id: u64,
    kind: TtsRuntimeEventKind,
    action: &str,
    message: Option<String>,
) {
    let event = TtsRuntimeEvent {
        request_id,
        action: action.to_string(),
        kind,
        snapshot: None,
        playback: None,
        tts: None,
        message,
        cursor: None,
    };
    let _ = ctx.event_tx.send(event);
}

fn emit_queued_event(ctx: &TtsRuntimeContext, request_id: u64, plan: &TtsPlaybackPlan, prepared: &[PreparedSentence]) {
    let event = TtsRuntimeEvent {
        request_id,
        action: "reader_tts_runtime_queue".to_string(),
        kind: TtsRuntimeEventKind::Queued,
        snapshot: None,
        playback: None,
        tts: None,
        message: Some(format!(
            "queued {} sentences for page {}",
            prepared.len(),
            plan.page + 1
        )),
        cursor: None,
    };
    let _ = ctx.event_tx.send(event);
}

fn cursor_from_snapshot(snapshot: &session::ReaderSnapshot) -> Option<TtsCursor> {
    Some(TtsCursor {
        audio_idx: snapshot.tts.current_sentence_idx,
        display_idx: snapshot.highlighted_sentence_idx,
        page: snapshot.current_page,
    })
}

fn reader_playback_state_from_snapshot(
    reader: &session::ReaderSnapshot,
) -> crate::contracts::ReaderPlaybackState {
    crate::contracts::ReaderPlaybackState {
        source_path: reader.source_path.clone(),
        current_page: reader.current_page,
        highlighted_sentence_idx: reader.highlighted_sentence_idx,
        tts: reader.tts.clone(),
        stats: reader.stats.clone(),
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

fn patch_has_tts_fields(patch: &session::ReaderSettingsPatch) -> bool {
    patch.tts_speed.is_some()
        || patch.tts_volume.is_some()
        || patch.pause_after_sentence.is_some()
        || patch.auto_scroll_tts.is_some()
}

fn panels_snapshot(panels: &Mutex<session::PanelState>) -> session::PanelState {
    panels.lock().map(|guard| *guard).unwrap_or_default()
}

fn simulated_sentence_duration(sentence: &str, speed: f32) -> Duration {
    let words = sentence.split_whitespace().count().max(1) as f32;
    let base_wpm = 180.0;
    let effective_wpm = (base_wpm * speed.max(0.25)).max(60.0);
    let seconds = words / (effective_wpm / 60.0);
    Duration::from_secs_f32(seconds.max(0.15))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_session(page_sentences: &[&[&str]]) -> session::ReaderSession {
        let pages: Vec<String> = page_sentences
            .iter()
            .map(|sentences| sentences.join(" "))
            .collect();
        let raw_page_sentences: Vec<Vec<String>> = page_sentences
            .iter()
            .map(|sentences| sentences.iter().map(|s| s.to_string()).collect())
            .collect();
        let page_word_counts: Vec<usize> = pages
            .iter()
            .map(|page| page.split_whitespace().count())
            .collect();
        let page_sentence_counts: Vec<usize> = raw_page_sentences.iter().map(Vec::len).collect();

        session::ReaderSession {
            source_path: PathBuf::from("/tmp/test.epub"),
            source_name: "test.epub".to_string(),
            tts_text: pages.join("\n\n"),
            reading_markdown: None,
            reading_html: None,
            has_structured_markdown: false,
            pdf_geometry_mode: None,
            pdf_sync_strategy: None,
            pdf_classification: None,
            pdf_runtime_policy: None,
            pdf_ocr_alignment: None,
            pdf_ocr_pipeline: None,
            images: Vec::new(),
            config: config::AppConfig::default(),
            pages,
            markdown_pages: Vec::new(),
            raw_page_sentences,
            sentence_anchor_maps: Vec::new(),
            page_word_counts,
            page_sentence_counts,
            current_page: 0,
            highlighted_display_idx: Some(0),
            highlighted_audio_idx: None,
            text_only_mode: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            selected_search_match: None,
            tts_state: session::TtsPlaybackState::Paused,
            current_plan_page: None,
            current_plan: None,
        }
    }

    #[test]
    fn tts_command_updates_snapshot_and_state() {
        let normalizer = normalizer::TextNormalizer::default();
        let runtime = TtsRuntime::new_with_mode(normalizer, TtsRuntimeMode::Simulated);
        runtime.set_session(Some(build_test_session(&[&["A.", "B."]] )));

        let snapshot = runtime.apply_command(TtsCommand::Play).expect("snapshot");
        assert_eq!(snapshot.tts.state, session::TtsPlaybackState::Playing);
    }

    #[test]
    fn tts_runtime_emits_progress_events() {
        let normalizer = normalizer::TextNormalizer::default();
        let runtime = TtsRuntime::new_with_mode(normalizer, TtsRuntimeMode::Simulated);
        runtime.set_session(Some(build_test_session(&[&["A.", "B.", "C."]] )));

        let _ = runtime.apply_command(TtsCommand::Play);
        thread::sleep(Duration::from_millis(80));
        let events = runtime.collect_events();
        assert!(events.iter().any(|event| event.kind == TtsRuntimeEventKind::Progress));
    }

    #[test]
    fn tts_runtime_cancels_on_clear_session() {
        let normalizer = normalizer::TextNormalizer::default();
        let runtime = TtsRuntime::new_with_mode(normalizer, TtsRuntimeMode::Simulated);
        runtime.set_session(Some(build_test_session(&[&["A.", "B."]] )));

        let _ = runtime.apply_command(TtsCommand::Play);
        runtime.set_session(None);
        thread::sleep(Duration::from_millis(10));
        let events = runtime.collect_events();
        assert!(events.iter().any(|event| event.kind == TtsRuntimeEventKind::Completed || event.kind == TtsRuntimeEventKind::Cancelled));
    }
}
