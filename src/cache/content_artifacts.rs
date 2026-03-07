use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

use super::{
    CONTENT_LAYOUT_VERSION, CONTENT_LAYOUT_VERSION_FILE, CONTENT_READING_HTML_FILE,
    CONTENT_READING_MARKDOWN_FILE, CONTENT_TTS_TEXT_FILE, hash_dir,
};

pub(super) fn persist_dual_view_artifacts(
    source_path: &Path,
    tts_text: &str,
    reading_markdown: Option<&str>,
    reading_html: Option<&str>,
) {
    ensure_content_layout(source_path);
    let tts_path = hash_dir(source_path).join(CONTENT_TTS_TEXT_FILE);
    if let Some(parent) = tts_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match fs::write(&tts_path, tts_text) {
        Ok(()) => {
            debug!(
                path = %tts_path.display(),
                chars = tts_text.len(),
                "Persisted cached tts_text artifact"
            );
        }
        Err(err) => warn!(path = %tts_path.display(), "Failed to persist tts_text artifact: {err}"),
    }

    let markdown_path = hash_dir(source_path).join(CONTENT_READING_MARKDOWN_FILE);
    match reading_markdown {
        Some(markdown) => match fs::write(&markdown_path, markdown) {
            Ok(()) => debug!(
                path = %markdown_path.display(),
                chars = markdown.len(),
                "Persisted cached reading_markdown artifact"
            ),
            Err(err) => warn!(
                path = %markdown_path.display(),
                "Failed to persist reading_markdown artifact: {err}"
            ),
        },
        None => {
            let _ = fs::remove_file(&markdown_path);
        }
    }

    let html_path = hash_dir(source_path).join(CONTENT_READING_HTML_FILE);
    match reading_html {
        Some(html) => match fs::write(&html_path, html) {
            Ok(()) => debug!(
                path = %html_path.display(),
                chars = html.len(),
                "Persisted cached reading_html artifact"
            ),
            Err(err) => warn!(
                path = %html_path.display(),
                "Failed to persist reading_html artifact: {err}"
            ),
        },
        None => {
            let _ = fs::remove_file(&html_path);
        }
    }
}

pub(super) fn persist_sentence_anchor_map(source_path: &Path, page: usize, anchors: &[Option<usize>]) {
    ensure_content_layout(source_path);
    let map_dir = hash_dir(source_path)
        .join("content")
        .join("sentence-anchor-map");
    if fs::create_dir_all(&map_dir).is_err() {
        return;
    }
    let map_path = map_dir.join(format!("page-{page:05}.toml"));
    #[derive(serde::Serialize)]
    struct AnchorMap<'a> {
        anchors: &'a [i64],
    }
    let encoded: Vec<i64> = anchors
        .iter()
        .map(|value| value.map(|v| v as i64).unwrap_or(-1))
        .collect();
    match toml::to_string(&AnchorMap { anchors: &encoded }) {
        Ok(serialized) => {
            if let Err(err) = fs::write(&map_path, serialized) {
                warn!(path = %map_path.display(), "Failed to persist sentence anchor map: {err}");
            } else {
                debug!(
                    path = %map_path.display(),
                    count = anchors.len(),
                    "Persisted sentence anchor map"
                );
            }
        }
        Err(err) => warn!("Failed to serialize sentence anchor map: {err}"),
    }
}

pub(super) fn load_sentence_anchor_map(source_path: &Path, page: usize) -> Option<Vec<Option<usize>>> {
    let map_path = hash_dir(source_path)
        .join("content")
        .join("sentence-anchor-map")
        .join(format!("page-{page:05}.toml"));
    let raw = fs::read_to_string(&map_path).ok()?;
    #[derive(serde::Deserialize)]
    struct AnchorMap {
        anchors: Vec<i64>,
    }
    let parsed: AnchorMap = toml::from_str(&raw).ok()?;
    Some(
        parsed
            .anchors
            .into_iter()
            .map(|value| (value >= 0).then_some(value as usize))
            .collect(),
    )
}

pub(super) fn tts_dir(source_path: &Path) -> PathBuf {
    hash_dir(source_path).join("tts")
}

pub(super) fn normalized_dir(source_path: &Path) -> PathBuf {
    hash_dir(source_path).join("normalized")
}

fn ensure_content_layout(source_path: &Path) {
    let hash_root = hash_dir(source_path);
    let version_path = hash_root.join(CONTENT_LAYOUT_VERSION_FILE);
    let current = fs::read_to_string(&version_path).ok();
    if current
        .as_deref()
        .map(str::trim)
        .map(|value| value == CONTENT_LAYOUT_VERSION)
        .unwrap_or(false)
    {
        return;
    }

    let content_dir = hash_root.join("content");
    if let Err(err) = fs::create_dir_all(&content_dir) {
        warn!(path = %content_dir.display(), "Failed to create content cache layout directory: {err}");
        return;
    }

    migrate_legacy_content_files(&hash_root);

    if let Some(parent) = version_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(err) = fs::write(&version_path, CONTENT_LAYOUT_VERSION) {
        warn!(path = %version_path.display(), "Failed to persist content layout version: {err}");
    } else {
        debug!(
            path = %version_path.display(),
            version = CONTENT_LAYOUT_VERSION,
            "Initialized content cache layout version"
        );
    }
}

fn migrate_legacy_content_files(hash_root: &Path) {
    let legacy_plain = hash_root.join("source-plain.txt");
    let new_plain = hash_root.join(CONTENT_TTS_TEXT_FILE);
    if legacy_plain.exists() && !new_plain.exists() {
        if let Err(err) = fs::rename(&legacy_plain, &new_plain) {
            warn!(
                from = %legacy_plain.display(),
                to = %new_plain.display(),
                "Failed to migrate legacy plain text cache file: {err}"
            );
        } else {
            debug!(
                from = %legacy_plain.display(),
                to = %new_plain.display(),
                "Migrated legacy plain text cache file"
            );
        }
    }

    let legacy_markdown = hash_root.join("source-markdown.txt");
    let new_markdown = hash_root.join(CONTENT_READING_MARKDOWN_FILE);
    if legacy_markdown.exists() && !new_markdown.exists() {
        if let Err(err) = fs::rename(&legacy_markdown, &new_markdown) {
            warn!(
                from = %legacy_markdown.display(),
                to = %new_markdown.display(),
                "Failed to migrate legacy markdown cache file: {err}"
            );
        } else {
            debug!(
                from = %legacy_markdown.display(),
                to = %new_markdown.display(),
                "Migrated legacy markdown cache file"
            );
        }
    }

    let legacy_html = hash_root.join("source-html.html");
    let new_html = hash_root.join(CONTENT_READING_HTML_FILE);
    if legacy_html.exists() && !new_html.exists() {
        if let Err(err) = fs::rename(&legacy_html, &new_html) {
            warn!(
                from = %legacy_html.display(),
                to = %new_html.display(),
                "Failed to migrate legacy HTML cache file: {err}"
            );
        } else {
            debug!(
                from = %legacy_html.display(),
                to = %new_html.display(),
                "Migrated legacy HTML cache file"
            );
        }
    }
}
