use super::*;

#[tauri::command]
pub(crate) async fn source_open_path(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
    path: String,
) -> Result<OpenSourceResult, BridgeError> {
    let source = resolve_source_path(&path)?;
    open_resolved_source(&app, &state, source).await
}

#[tauri::command]
pub(crate) async fn source_open_clipboard_text(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
    text: String,
) -> Result<OpenSourceResult, BridgeError> {
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        return Err(bridge_error("invalid_input", "clipboard text is empty"));
    }
    let path = cache::persist_clipboard_text_source(&trimmed)
        .map_err(|err| bridge_error("invalid_input", err))?;
    open_resolved_source(&app, &state, path).await
}

#[tauri::command]
pub(crate) async fn source_open_clipboard(
    app: tauri::AppHandle,
    state: State<'_, Mutex<BackendState>>,
) -> Result<OpenSourceResult, BridgeError> {
    info!("Opening source from system clipboard");
    let app_for_read = app.clone();
    let text = tauri::async_runtime::spawn_blocking(move || {
        read_clipboard_text_with_fallback(&app_for_read)
    })
    .await
    .map_err(|err| {
        bridge_error(
            "clipboard_error",
            format!("Clipboard worker task failed: {err}"),
        )
    })?
    .map_err(|err| bridge_error("clipboard_error", err))?;
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        warn!("Clipboard read succeeded but text was empty");
        return Err(bridge_error("invalid_input", "clipboard text is empty"));
    }
    let path = cache::persist_clipboard_text_source(&trimmed)
        .map_err(|err| bridge_error("invalid_input", err))?;
    open_resolved_source(&app, &state, path).await
}
