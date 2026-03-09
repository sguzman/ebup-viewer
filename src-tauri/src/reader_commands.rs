use super::*;

#[tauri::command]
pub(crate) fn reader_load_pdf_bytes(path: String) -> Result<Vec<u8>, BridgeError> {
    let source_path = PathBuf::from(path.trim());
    if source_path.as_os_str().is_empty() {
        return Err(bridge_error("invalid_input", "Path cannot be empty"));
    }
    tracing::debug!(path = %source_path.display(), "Loading native PDF bytes for renderer");
    fs::read(&source_path).map_err(|err| {
        bridge_error(
            "io_error",
            format!("Failed to read PDF bytes at {}: {err}", source_path.display()),
        )
    })
}

#[tauri::command]
pub(crate) fn reader_persist_pdf_sync_map(
    path: String,
    locations: Vec<cache::PdfSentenceLocation>,
) -> Result<(), BridgeError> {
    let source_path = PathBuf::from(&path);
    tracing::debug!(
        path = %source_path.display(),
        count = locations.len(),
        "Persisting PDF sentence sync map from native PDF renderer"
    );
    cache::persist_pdf_sentence_map(&source_path, &locations);
    Ok(())
}

#[tauri::command]
pub(crate) fn reader_load_pdf_sync_map(
    path: String,
) -> Result<Vec<cache::PdfSentenceLocation>, BridgeError> {
    let source_path = PathBuf::from(path.trim());
    if source_path.as_os_str().is_empty() {
        return Err(bridge_error("invalid_input", "Path cannot be empty"));
    }
    let locations = cache::load_pdf_sentence_map(&source_path).unwrap_or_default();
    tracing::debug!(
        path = %source_path.display(),
        count = locations.len(),
        "Loaded cached PDF sentence sync map for native PDF renderer"
    );
    Ok(locations)
}

#[tauri::command]
pub(crate) fn reader_get_snapshot(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::GetSnapshot)
}

#[tauri::command]
pub(crate) fn reader_next_page(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::NextPage)
}

#[tauri::command]
pub(crate) fn reader_prev_page(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::PrevPage)
}

#[tauri::command]
pub(crate) fn reader_set_page(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
    page: usize,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::SetPage { page })
}

#[tauri::command]
pub(crate) fn reader_sentence_click(
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
pub(crate) fn reader_next_sentence(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::NextSentence)
}

#[tauri::command]
pub(crate) fn reader_prev_sentence(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::PrevSentence)
}

#[tauri::command]
pub(crate) fn reader_toggle_text_only(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::ToggleTextOnly)
}

#[tauri::command]
pub(crate) fn reader_apply_settings(
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
pub(crate) fn reader_search_set_query(
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
pub(crate) fn reader_search_next(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::SearchNext)
}

#[tauri::command]
pub(crate) fn reader_search_prev(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::SearchPrev)
}

#[tauri::command]
pub(crate) fn reader_tts_play(
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

        if behavior == config::TtsPauseResumeBehavior::ResumeFromPausePoint
            && paused
            && let Some(runtime) = guard.tts_request.as_ref()
        {
            runtime.set_paused(false);
            should_sync_tts = false;
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
pub(crate) fn reader_tts_pause(
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
pub(crate) fn reader_tts_toggle_play_pause(
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
                if behavior == config::TtsPauseResumeBehavior::ResumeFromPausePoint
                    && let Some(runtime) = guard.tts_request.as_ref()
                {
                    runtime.set_paused(false);
                    should_sync_tts = false;
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
pub(crate) fn reader_tts_play_from_page_start(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::TtsPlayFromPageStart)
}

#[tauri::command]
pub(crate) fn reader_tts_play_from_highlight(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::TtsPlayFromHighlight)
}

#[tauri::command]
pub(crate) fn reader_tts_seek_next(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::TtsSeekNext)
}

#[tauri::command]
pub(crate) fn reader_tts_seek_prev(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::TtsSeekPrev)
}

#[tauri::command]
pub(crate) fn reader_tts_repeat_sentence(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<session::ReaderSnapshot, BridgeError> {
    apply_reader_command(&app, &state, session::SessionCommand::TtsRepeatSentence)
}

#[tauri::command]
pub(crate) fn reader_tts_precompute_page(
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
pub(crate) fn reader_close_session(
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
