use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

use super::bookmarks_config::PdfRect;
use super::{
    CONTENT_LAYOUT_VERSION, CONTENT_LAYOUT_VERSION_FILE, CONTENT_READING_HTML_FILE,
    CONTENT_READING_MARKDOWN_FILE, CONTENT_TTS_TEXT_FILE, hash_dir,
};
use crate::epub_loader::{
    PdfClassificationSummary, PdfGeometryMode, PdfRuntimePolicySummary, PdfSyncStrategy,
};

const CONTENT_PDF_SYNC_META_FILE: &str = "content/pdf-sync-meta.toml";
const CONTENT_PDF_SENTENCE_MAP_FILE: &str = "content/pdf-sentence-map.toml";
const PDF_SYNC_META_CLASSIFICATION_VERSION: u32 = 2;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct PdfSyncMeta {
    pub pdf_geometry_mode: PdfGeometryMode,
    pub pdf_sync_strategy: PdfSyncStrategy,
    #[serde(default)]
    pub pdf_classification: Option<PdfClassificationSummary>,
    #[serde(default)]
    pub pdf_runtime_policy: Option<PdfRuntimePolicySummary>,
    #[serde(default)]
    pub pdf_classification_version: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PdfSentenceLocation {
    pub sentence_idx: usize,
    pub page_idx: Option<usize>,
    #[serde(default)]
    pub rects: Vec<PdfRect>,
    #[serde(default)]
    pub line_rects: Vec<PdfRect>,
    #[serde(default)]
    pub block_rects: Vec<PdfRect>,
    pub confidence: String,
    pub reason: String,
    pub score: f32,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PdfSentenceMap {
    locations: Vec<PdfSentenceLocation>,
}

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

pub(super) fn persist_sentence_anchor_map(
    source_path: &Path,
    page: usize,
    anchors: &[Option<usize>],
) {
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

pub(super) fn load_sentence_anchor_map(
    source_path: &Path,
    page: usize,
) -> Option<Vec<Option<usize>>> {
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

pub(super) fn persist_pdf_sync_meta(
    source_path: &Path,
    pdf_geometry_mode: PdfGeometryMode,
    pdf_sync_strategy: PdfSyncStrategy,
    pdf_classification: Option<&PdfClassificationSummary>,
    pdf_runtime_policy: Option<&PdfRuntimePolicySummary>,
) {
    ensure_content_layout(source_path);
    let meta_path = hash_dir(source_path).join(CONTENT_PDF_SYNC_META_FILE);
    if let Some(parent) = meta_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let serialized = match toml::to_string(&PdfSyncMeta {
        pdf_geometry_mode,
        pdf_sync_strategy,
        pdf_classification: pdf_classification.cloned(),
        pdf_runtime_policy: pdf_runtime_policy.cloned(),
        pdf_classification_version: PDF_SYNC_META_CLASSIFICATION_VERSION,
    }) {
        Ok(value) => value,
        Err(err) => {
            warn!("Failed to serialize PDF sync metadata: {err}");
            return;
        }
    };
    if let Err(err) = fs::write(&meta_path, serialized) {
        warn!(path = %meta_path.display(), "Failed to persist PDF sync metadata: {err}");
    } else {
        debug!(
            path = %meta_path.display(),
            ?pdf_geometry_mode,
            ?pdf_sync_strategy,
            pdf_document_class = ?pdf_classification.map(|value| value.document_class),
            pdf_highlight_policy = ?pdf_runtime_policy.map(|value| value.sentence_highlight_policy),
            "Persisted PDF sync metadata"
        );
    }
}

pub(super) fn load_pdf_sync_meta(source_path: &Path) -> Option<PdfSyncMeta> {
    let meta_path = hash_dir(source_path).join(CONTENT_PDF_SYNC_META_FILE);
    let raw = match fs::read_to_string(&meta_path) {
        Ok(value) => value,
        Err(err) => {
            debug!(
                path = %meta_path.display(),
                "PDF sync metadata unavailable: {err}"
            );
            return None;
        }
    };
    let parsed: PdfSyncMeta = match toml::from_str(&raw) {
        Ok(value) => value,
        Err(err) => {
            warn!(
                path = %meta_path.display(),
                "PDF sync metadata was corrupt; removing stale artifact so it can be rebuilt: {err}"
            );
            let _ = fs::remove_file(&meta_path);
            return None;
        }
    };
    if parsed.pdf_classification_version != PDF_SYNC_META_CLASSIFICATION_VERSION {
        warn!(
            path = %meta_path.display(),
            cached_classification_version = parsed.pdf_classification_version,
            required_classification_version = PDF_SYNC_META_CLASSIFICATION_VERSION,
            "PDF sync metadata classification version changed; removing stale artifact so it can be rebuilt"
        );
        let _ = fs::remove_file(&meta_path);
        return None;
    }
    debug!(
        path = %meta_path.display(),
        ?parsed.pdf_geometry_mode,
        ?parsed.pdf_sync_strategy,
        pdf_document_class = ?parsed
            .pdf_classification
            .as_ref()
            .map(|value| value.document_class),
        classification_version = parsed.pdf_classification_version,
        "Loaded cached PDF sync metadata"
    );
    Some(parsed)
}

pub(super) fn persist_pdf_sentence_map(source_path: &Path, locations: &[PdfSentenceLocation]) {
    ensure_content_layout(source_path);
    let map_path = hash_dir(source_path).join(CONTENT_PDF_SENTENCE_MAP_FILE);
    if let Some(parent) = map_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut merged_locations = load_pdf_sentence_map(source_path).unwrap_or_default();
    let mut replaced = 0usize;
    let mut inserted = 0usize;
    for location in locations {
        if let Some(existing) = merged_locations
            .iter_mut()
            .find(|candidate| candidate.sentence_idx == location.sentence_idx)
        {
            *existing = location.clone();
            replaced += 1;
        } else {
            merged_locations.push(location.clone());
            inserted += 1;
        }
    }
    merged_locations.sort_by_key(|location| location.sentence_idx);
    let serialized = match toml::to_string(&PdfSentenceMap {
        locations: merged_locations.clone(),
    }) {
        Ok(value) => value,
        Err(err) => {
            warn!("Failed to serialize PDF sentence map: {err}");
            return;
        }
    };
    if let Err(err) = fs::write(&map_path, serialized) {
        warn!(path = %map_path.display(), "Failed to persist PDF sentence map: {err}");
    } else {
        debug!(
            path = %map_path.display(),
            incoming_count = locations.len(),
            merged_count = merged_locations.len(),
            replaced,
            inserted,
            "Persisted merged PDF sentence map"
        );
    }
}

pub(super) fn load_pdf_sentence_map(source_path: &Path) -> Option<Vec<PdfSentenceLocation>> {
    let map_path = hash_dir(source_path).join(CONTENT_PDF_SENTENCE_MAP_FILE);
    let raw = match fs::read_to_string(&map_path) {
        Ok(value) => value,
        Err(err) => {
            debug!(
                path = %map_path.display(),
                "PDF sentence map unavailable: {err}"
            );
            return None;
        }
    };
    let parsed: PdfSentenceMap = match toml::from_str(&raw) {
        Ok(value) => value,
        Err(err) => {
            warn!(
                path = %map_path.display(),
                "PDF sentence map was corrupt; removing stale artifact so it can be rebuilt: {err}"
            );
            let _ = fs::remove_file(&map_path);
            return None;
        }
    };
    debug!(
        path = %map_path.display(),
        count = parsed.locations.len(),
        "Loaded cached PDF sentence map"
    );
    Some(parsed.locations)
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
    if let Some(previous) = current.as_deref().map(str::trim)
        && !previous.is_empty()
    {
        debug!(
            path = %hash_root.display(),
            previous_version = previous,
            next_version = CONTENT_LAYOUT_VERSION,
            "Migrating cached content layout version"
        );
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
