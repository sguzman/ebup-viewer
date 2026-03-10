use super::*;
use tracing::{info, warn};

async fn lookup_browser_tab_metadata(
    client: &browser_tabs::BrowsrClient,
    tab_id: u64,
    window_id: Option<u64>,
    refresh: bool,
) -> Option<browser_tabs::BrowserTab> {
    client
        .list_tabs(window_id, None, refresh)
        .await
        .ok()
        .and_then(|tabs| tabs.into_iter().find(|tab| tab.id == tab_id))
}

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
    let tab_meta = lookup_browser_tab_metadata(&client, tab_id, window_id, false).await;
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
pub(crate) async fn source_open_browser_tab_bundle(
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
    let tab_meta = lookup_browser_tab_metadata(&client, tab_id, window_id, false).await;
    let waited = client
        .start_import_bundle_and_wait(tab_id)
        .await
        .map_err(|err| bridge_error("browsr_import_bundle_failed", err.to_string()))?;
    let completed_job = waited.result.job;
    info!(
        job_id = %completed_job.job_id,
        tab_id,
        window_id,
        status = %completed_job.status,
        "Completed browser-tab bundle import wait request"
    );
    let manifest = match completed_job.status.as_str() {
        "completed" => waited.result.manifest.ok_or_else(|| {
            bridge_error(
                "browsr_import_bundle_failed",
                format!(
                    "Import bundle {} completed without an attached manifest",
                    completed_job.job_id
                ),
            )
        })?,
        "failed" | "cancelled" => {
            let message = completed_job
                .error
                .as_ref()
                .and_then(|value| value.message.clone())
                .unwrap_or_else(|| {
                    format!(
                        "bundle import {} for tab {}",
                        completed_job.status, completed_job.tab_id
                    )
                });
            return Err(bridge_error("browsr_import_bundle_failed", message));
        }
        other => {
            return Err(bridge_error(
                "browsr_import_bundle_failed",
                format!("Unexpected terminal import bundle status: {other}"),
            ));
        }
    };
    let document = manifest.bundle.document.as_ref().ok_or_else(|| {
        bridge_error(
            "browsr_import_bundle_failed",
            format!(
                "Import bundle {} did not include a document payload",
                completed_job.job_id
            ),
        )
    })?;
    let mut assets = Vec::new();
    for asset_ref in manifest
        .bundle
        .assets
        .iter()
        .filter(|asset| asset.body_available && !asset.url.trim().is_empty())
    {
        match client
            .get_import_bundle_asset(&completed_job.job_id, &asset_ref.asset_id)
            .await
        {
            Ok(asset) => assets.push(asset),
            Err(err) => {
                warn!(
                    job_id = %completed_job.job_id,
                    tab_id,
                    asset_id = %asset_ref.asset_id,
                    url = %asset_ref.url,
                    "Skipping bundle asset fetch failure: {err}"
                );
            }
        }
    }
    let bundle_capture = browser_tabs::BrowserTabBundleCapture {
        tab_id,
        title: manifest
            .bundle
            .tab
            .title
            .clone()
            .or_else(|| tab_meta.as_ref().map(|value| value.title.clone()))
            .unwrap_or_else(|| format!("Browser tab {tab_id}")),
        url: manifest
            .bundle
            .tab
            .url
            .clone()
            .or_else(|| tab_meta.as_ref().map(|value| value.url.clone()))
            .unwrap_or_default(),
        captured_at: completed_job.updated_at.clone(),
        html: document.html.clone().unwrap_or_default(),
        text: document.text.clone(),
        selection: document.selection.clone(),
        assets,
    };
    let source_path = cache::persist_browser_tab_bundle_source(&bundle_capture, tab_meta.as_ref())
        .map_err(|err| bridge_error("browser_tab_cache_error", err))?;
    info!(
        job_id = %completed_job.job_id,
        tab_id,
        source_path = %source_path.display(),
        title = %bundle_capture.title,
        url = %bundle_capture.url,
        asset_count = bundle_capture.assets.len(),
        selection_chars = bundle_capture.selection.as_ref().map(|value| value.len()).unwrap_or(0),
        "Persisted browser-tab bundle source"
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
    let tab_meta =
        lookup_browser_tab_metadata(&client, manifest.tab_id, manifest.window_id, true).await;
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

#[tauri::command]
pub(crate) async fn recent_close_browser_tab(
    state: State<'_, Mutex<BackendState>>,
    path: String,
) -> Result<(), BridgeError> {
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
    client
        .close_tab(manifest.tab_id)
        .await
        .map_err(|err| bridge_error("browsr_close_failed", err.to_string()))?;
    info!(
        source_path = %source_path.display(),
        tab_id = manifest.tab_id,
        window_id = manifest.window_id,
        "Closed browser tab for recent imported source"
    );
    Ok(())
}
