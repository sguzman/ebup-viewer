use super::*;

#[tauri::command]
pub(crate) async fn browser_tabs_health(
    state: State<'_, Mutex<BackendState>>,
) -> Result<browser_tabs::BrowsrHealth, BridgeError> {
    let cfg = {
        let guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        guard.base_config.clone()
    };
    if !cfg.browser_tabs_enabled {
        return Err(bridge_error(
            "browser_tabs_disabled",
            "Browser tabs import is disabled in config",
        ));
    }
    let client = browsr_client_from_config(&cfg)?;
    client
        .health()
        .await
        .map_err(|err| bridge_error("browsr_unavailable", err.to_string()))
}

#[tauri::command]
pub(crate) async fn browser_tabs_list_windows(
    state: State<'_, Mutex<BackendState>>,
) -> Result<Vec<browser_tabs::BrowserWindow>, BridgeError> {
    let cfg = {
        let guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        guard.base_config.clone()
    };
    if !cfg.browser_tabs_enabled {
        return Err(bridge_error(
            "browser_tabs_disabled",
            "Browser tabs import is disabled in config",
        ));
    }
    let client = browsr_client_from_config(&cfg)?;
    client
        .list_windows()
        .await
        .map_err(|err| bridge_error("browsr_request_failed", err.to_string()))
}

#[tauri::command]
pub(crate) async fn browser_tabs_list_tabs(
    state: State<'_, Mutex<BackendState>>,
    window_id: Option<u64>,
    query: Option<String>,
    refresh: Option<bool>,
) -> Result<Vec<browser_tabs::BrowserTab>, BridgeError> {
    let cfg = {
        let guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        guard.base_config.clone()
    };
    if !cfg.browser_tabs_enabled {
        return Err(bridge_error(
            "browser_tabs_disabled",
            "Browser tabs import is disabled in config",
        ));
    }
    let client = browsr_client_from_config(&cfg)?;
    client
        .list_tabs(window_id, query.as_deref(), refresh.unwrap_or(false))
        .await
        .map_err(|err| bridge_error("browsr_request_failed", err.to_string()))
}

#[tauri::command]
pub(crate) async fn source_open_browser_tab(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
    tab_id: u64,
    window_id: Option<u64>,
) -> Result<OpenSourceResult, BridgeError> {
    let cfg = {
        let guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        guard.base_config.clone()
    };
    if !cfg.browser_tabs_enabled {
        return Err(bridge_error(
            "browser_tabs_disabled",
            "Browser tabs import is disabled in config",
        ));
    }
    let client = browsr_client_from_config(&cfg)?;
    let tab_meta = client
        .list_tabs(window_id, None, false)
        .await
        .ok()
        .and_then(|tabs| tabs.into_iter().find(|tab| tab.id == tab_id));
    let snapshot = client
        .snapshot_tab(tab_id)
        .await
        .map_err(|err| bridge_error("browsr_snapshot_failed", err.to_string()))?;
    let source_path = cache::persist_browser_tab_source(&snapshot, tab_meta.as_ref())
        .map_err(|err| bridge_error("browser_tab_cache_error", err))?;
    info!(
        tab_id,
        window_id,
        source_path = %source_path.display(),
        title = %snapshot.title,
        url = %snapshot.url,
        html_truncated = snapshot.truncation.html.truncated,
        text_truncated = snapshot.truncation.text.truncated,
        "Persisted browser-tab snapshot source"
    );
    open_resolved_source(&app, &state, source_path).await
}

#[tauri::command]
pub(crate) async fn source_refresh_browser_tab(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
    path: String,
) -> Result<OpenSourceResult, BridgeError> {
    let source_path = resolve_source_path(&path)?;
    let manifest = cache::load_browser_tab_manifest(&source_path).ok_or_else(|| {
        bridge_error(
            "invalid_input",
            format!(
                "Source is not a browser-tab manifest: {}",
                source_path.display()
            ),
        )
    })?;
    let cfg = {
        let guard = state
            .lock()
            .map_err(|_| bridge_error("lock_poisoned", "Backend state lock poisoned"))?;
        guard.base_config.clone()
    };
    if !cfg.browser_tabs_enabled {
        return Err(bridge_error(
            "browser_tabs_disabled",
            "Browser tabs import is disabled in config",
        ));
    }
    let client = browsr_client_from_config(&cfg)?;
    let tab_meta = client
        .list_tabs(manifest.window_id, None, true)
        .await
        .ok()
        .and_then(|tabs| tabs.into_iter().find(|tab| tab.id == manifest.tab_id));
    let snapshot = client
        .snapshot_tab(manifest.tab_id)
        .await
        .map_err(|err| bridge_error("browsr_snapshot_failed", err.to_string()))?;
    let refreshed_source_path = cache::persist_browser_tab_source(&snapshot, tab_meta.as_ref())
        .map_err(|err| bridge_error("browser_tab_cache_error", err))?;
    info!(
        tab_id = manifest.tab_id,
        source_path = %refreshed_source_path.display(),
        title = %snapshot.title,
        url = %snapshot.url,
        html_truncated = snapshot.truncation.html.truncated,
        text_truncated = snapshot.truncation.text.truncated,
        "Refreshed browser-tab snapshot source"
    );
    open_resolved_source(&app, &state, refreshed_source_path).await
}
