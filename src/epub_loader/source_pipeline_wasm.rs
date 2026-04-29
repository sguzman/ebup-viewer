use std::path::Path;
use anyhow::{Result, anyhow};
use super::SourceContent;
use crate::cancellation::CancellationToken;

pub(super) fn is_epub(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase()),
        Some(ext) if ext == "epub"
    )
}

pub(super) fn is_markdown(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase()),
        Some(ext) if ext == "md" || ext == "markdown"
    )
}

pub(super) fn source_type_label(path: &Path) -> &'static str {
    if is_epub(path) {
        "epub"
    } else if is_markdown(path) {
        "markdown"
    } else {
        "unknown"
    }
}

pub(super) fn load_source_content(
    _path: &Path,
    _cancel: Option<&CancellationToken>,
) -> Result<SourceContent> {
    Err(anyhow!("Source loading not supported on WASM"))
}
