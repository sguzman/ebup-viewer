use super::*;
use lanternleaf_core::cache_service::CacheService;

#[tauri::command]
pub(crate) fn session_get_bootstrap(
    state: State<'_, Mutex<BackendState>>,
) -> Result<BootstrapState, BridgeError> {
    let guard = state
        .lock()
        .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
    Ok(bootstrap_state_from_backend(&guard))
}

#[tauri::command]
pub(crate) fn session_toggle_theme(
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
pub(crate) fn session_get_state(
    state: State<'_, Mutex<BackendState>>,
) -> Result<SessionState, BridgeError> {
    let guard = state
        .lock()
        .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
    Ok(to_session_state(&guard))
}

#[tauri::command]
pub(crate) fn session_return_to_starter(
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
pub(crate) fn panel_toggle_settings(
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
pub(crate) fn panel_toggle_stats(
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
pub(crate) fn panel_toggle_tts(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<SessionState, BridgeError> {
    apply_panel_toggle(&app, &state, "panel_toggle_tts", |panels| {
        panels.show_tts = !panels.show_tts;
    })
}

#[tauri::command]
pub(crate) fn recent_list(limit: Option<usize>) -> Vec<RecentBook> {
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
            browser_tab_id: recent.browser_tab_id,
            browser_window_id: recent.browser_window_id,
        })
        .collect()
}

#[tauri::command]
pub(crate) fn recent_delete(path: String) -> Result<(), BridgeError> {
    let source = PathBuf::from(path.trim());
    if source.as_os_str().is_empty() {
        return Err(bridge_error("invalid_input", "Path cannot be empty"));
    }
    let cache_service = lanternleaf_core::cache_service::FilesystemCacheService;
    cache_service
        .delete_recent_source_and_cache(&source)
        .map_err(|err| bridge_error("io_error", err))
}

#[tauri::command]
pub(crate) fn app_safe_quit(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<(), BridgeError> {
    finalize_shutdown_from_mutex(state.inner());
    app.exit(0);
    Ok(())
}
