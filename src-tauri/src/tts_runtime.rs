use super::*;

#[derive(Debug, Clone)]
pub(crate) struct TtsRequestRuntime {
    pub(crate) request_id: u64,
    pub(crate) cancel_token: cancellation::CancellationToken,
    pub(crate) pause_requested: Arc<AtomicBool>,
}

impl TtsRequestRuntime {
    pub(crate) fn set_paused(&self, paused: bool) {
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

pub(crate) fn cancel_tts_request(state: &mut BackendState) {
    if let Some(runtime) = state.tts_request.take() {
        runtime.cancel_token.cancel();
    }
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
                match engine.as_ref().expect("engine initialized").prepare_batch(
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
            match engine.as_ref().expect("engine initialized").prepare_batch(
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
            let next_engine = engine.as_ref().expect("engine initialized").clone();

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
        let playback = match engine.as_ref().expect("engine initialized").play_files(
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

pub(crate) fn sync_tts_runtime_after_reader_change(
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

pub(crate) fn should_sync_tts_after_reader_command(command: &session::SessionCommand) -> bool {
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

pub(crate) fn apply_reader_command_with_sync(
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

pub(crate) fn apply_reader_command(
    app: &tauri::AppHandle,
    state: &State<'_, Mutex<BackendState>>,
    command: session::SessionCommand,
) -> Result<session::ReaderSnapshot, BridgeError> {
    let should_sync_tts = should_sync_tts_after_reader_command(&command);
    apply_reader_command_with_sync(app, state, command, should_sync_tts)
}
