#[cfg(not(target_arch = "wasm32"))]
use sha2::{Digest, Sha256};
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

use super::bookmarks_config::PdfRect;
use super::{
    CONTENT_LAYOUT_VERSION, CONTENT_LAYOUT_VERSION_FILE, CONTENT_READING_HTML_FILE,
    CONTENT_READING_MARKDOWN_FILE, CONTENT_TTS_TEXT_FILE, hash_dir,
};
use crate::epub_loader::{
    PdfClassificationSummary, PdfGeometryMode, PdfOcrGeometryQualityClass, PdfOcrSourceKind,
    PdfRuntimePolicySummary, PdfSyncStrategy,
};

const CONTENT_PDF_SYNC_META_FILE: &str = "content/pdf-sync-meta.toml";
const CONTENT_PDF_SENTENCE_MAP_FILE: &str = "content/pdf-sentence-map.toml";
const CONTENT_PDF_OCR_ALIGNMENT_FILE: &str = "content/pdf-ocr-alignment.toml";
const CONTENT_PDF_RENDER_PRECOMPUTE_FILE: &str = "content/pdf-render-precompute.toml";
const PDF_SYNC_META_CLASSIFICATION_VERSION: u32 = 3;
pub const PDF_OCR_ALIGNMENT_VERSION: u32 = 2;
const PDF_RENDER_PRECOMPUTE_VERSION: u32 = 1;

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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PdfOcrSentenceAlignment {
    pub sentence_idx: usize,
    #[serde(default)]
    pub sentence_text_hash: String,
    pub page_idx: Option<usize>,
    #[serde(default)]
    pub rects: Vec<PdfRect>,
    #[serde(default)]
    pub line_rects: Vec<PdfRect>,
    #[serde(default)]
    pub block_rects: Vec<PdfRect>,
    pub confidence_tier: String,
    pub fallback_reason: String,
    #[serde(default)]
    pub token_lineage: Vec<String>,
    pub score: f32,
    #[serde(default)]
    pub crosses_column_boundaries: bool,
    #[serde(default)]
    pub cross_column_confident: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PdfOcrPageAlignmentBucket {
    pub page_idx: usize,
    #[serde(default)]
    pub sentence_indexes: Vec<usize>,
    pub highlightable_sentence_count: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PdfOcrTokenGeometry {
    pub token_id: String,
    pub page_idx: usize,
    pub block_idx: usize,
    pub line_idx: usize,
    pub reading_order_idx: usize,
    pub text: String,
    pub rect: PdfRect,
    pub confidence: f32,
    pub source_kind: PdfOcrSourceKind,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PdfOcrLineGeometry {
    pub line_id: String,
    pub page_idx: usize,
    pub block_idx: usize,
    pub reading_order_idx: usize,
    pub text: String,
    pub rect: PdfRect,
    pub confidence: f32,
    #[serde(default)]
    pub token_ids: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PdfOcrBlockGeometry {
    pub block_id: String,
    pub page_idx: usize,
    pub reading_order_idx: usize,
    pub text: String,
    pub rect: PdfRect,
    pub confidence: f32,
    #[serde(default)]
    pub line_ids: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PdfOcrPageGeometry {
    pub page_idx: usize,
    pub confidence: f32,
    pub build_ms: u32,
    pub reading_order_mode: String,
    #[serde(default)]
    pub block_ids: Vec<String>,
    #[serde(default)]
    pub line_ids: Vec<String>,
    #[serde(default)]
    pub token_ids: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PdfOcrAlignmentArtifact {
    pub version: u32,
    pub quality_class: PdfOcrGeometryQualityClass,
    pub source_kind: PdfOcrSourceKind,
    pub sentence_count: usize,
    pub mapped_sentence_count: usize,
    pub rect_mapped_sentence_count: usize,
    pub line_mapped_sentence_count: usize,
    pub block_mapped_sentence_count: usize,
    pub page_only_sentence_count: usize,
    pub unmappable_sentence_count: usize,
    pub highlightable_sentence_count: usize,
    pub token_lineage_available: bool,
    pub deterministic: bool,
    pub reused_alignment_count: usize,
    pub rebuilt_alignment_count: usize,
    pub alignment_build_ms: u32,
    #[serde(default)]
    pub page_build_ms: Vec<u32>,
    #[serde(default)]
    pub chunk_build_ms: Vec<u32>,
    pub cross_column_alignment_count: usize,
    pub cross_column_confident_alignment_count: usize,
    #[serde(default)]
    pub degraded_reasons: Vec<String>,
    pub explanation: String,
    #[serde(default)]
    pub page_buckets: Vec<PdfOcrPageAlignmentBucket>,
    #[serde(default)]
    pub blocks: Vec<PdfOcrBlockGeometry>,
    #[serde(default)]
    pub lines: Vec<PdfOcrLineGeometry>,
    #[serde(default)]
    pub tokens: Vec<PdfOcrTokenGeometry>,
    #[serde(default)]
    pub page_geometry: Vec<PdfOcrPageGeometry>,
    #[serde(default)]
    pub alignments: Vec<PdfOcrSentenceAlignment>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PdfSentencePageHint {
    pub page_idx: Option<usize>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PdfRenderPrecomputedState {
    pub version: u32,
    #[serde(default)]
    pub page_texts: Vec<String>,
    #[serde(default)]
    pub sentence_page_hints: Vec<PdfSentencePageHint>,
    #[serde(default)]
    pub source: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PdfSentenceMap {
    locations: Vec<PdfSentenceLocation>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PdfOcrAlignmentEnvelope {
    artifact: PdfOcrAlignmentArtifact,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PdfRenderPrecomputedEnvelope {
    artifact: PdfRenderPrecomputedState,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn stable_sentence_text_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.trim().as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(target_arch = "wasm32")]
pub fn stable_sentence_text_hash(text: &str) -> String {
    format!("{:x}", text.len())
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(target_arch = "wasm32")]
pub(super) fn persist_dual_view_artifacts(
    _source_path: &Path,
    _tts_text: &str,
    _reading_markdown: Option<&str>,
    _reading_html: Option<&str>,
) {
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
pub(super) fn persist_sentence_anchor_map(
    _source_path: &Path,
    _page: usize,
    _anchors: &[Option<usize>],
) {
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
pub(super) fn load_sentence_anchor_map(
    _source_path: &Path,
    _page: usize,
) -> Option<Vec<Option<usize>>> {
    None
}





pub(super) fn tts_dir(source_path: &Path) -> PathBuf {
    hash_dir(source_path).join("tts")
}

pub(super) fn normalized_dir(source_path: &Path) -> PathBuf {
    hash_dir(source_path).join("normalized")
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
pub(super) fn persist_pdf_sync_meta(
    _source_path: &Path,
    _pdf_geometry_mode: PdfGeometryMode,
    _pdf_sync_strategy: PdfSyncStrategy,
    _pdf_classification: Option<&PdfClassificationSummary>,
    _pdf_runtime_policy: Option<&PdfRuntimePolicySummary>,
) {
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
pub(super) fn load_pdf_sync_meta(_source_path: &Path) -> Option<PdfSyncMeta> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
pub(super) fn persist_pdf_sentence_map(_source_path: &Path, _locations: &[PdfSentenceLocation]) {
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
pub(super) fn load_pdf_sentence_map(_source_path: &Path) -> Option<Vec<PdfSentenceLocation>> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn persist_pdf_ocr_alignment_artifact(
    source_path: &Path,
    artifact: &PdfOcrAlignmentArtifact,
) {
    ensure_content_layout(source_path);
    let artifact_path = hash_dir(source_path).join(CONTENT_PDF_OCR_ALIGNMENT_FILE);
    if let Some(parent) = artifact_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut persisted = artifact.clone();
    persisted.version = PDF_OCR_ALIGNMENT_VERSION;
    let serialized = match toml::to_string(&PdfOcrAlignmentEnvelope {
        artifact: persisted.clone(),
    }) {
        Ok(value) => value,
        Err(err) => {
            warn!("Failed to serialize PDF OCR alignment artifact: {err}");
            return;
        }
    };
    if let Err(err) = fs::write(&artifact_path, serialized) {
        warn!(
            path = %artifact_path.display(),
            "Failed to persist PDF OCR alignment artifact: {err}"
        );
    } else {
        debug!(
            path = %artifact_path.display(),
            sentence_count = persisted.sentence_count,
            mapped_sentence_count = persisted.mapped_sentence_count,
            quality_class = ?persisted.quality_class,
            source_kind = ?persisted.source_kind,
            "Persisted PDF OCR alignment artifact"
        );
    }
}

#[cfg(target_arch = "wasm32")]
pub(super) fn persist_pdf_ocr_alignment_artifact(
    _source_path: &Path,
    _artifact: &PdfOcrAlignmentArtifact,
) {
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn load_pdf_ocr_alignment_artifact(
    source_path: &Path,
) -> Option<PdfOcrAlignmentArtifact> {
    let artifact_path = hash_dir(source_path).join(CONTENT_PDF_OCR_ALIGNMENT_FILE);
    let raw = match fs::read_to_string(&artifact_path) {
        Ok(value) => value,
        Err(err) => {
            debug!(
                path = %artifact_path.display(),
                "PDF OCR alignment artifact unavailable: {err}"
            );
            return None;
        }
    };
    let parsed: PdfOcrAlignmentEnvelope = match toml::from_str(&raw) {
        Ok(value) => value,
        Err(err) => {
            warn!(
                path = %artifact_path.display(),
                "PDF OCR alignment artifact was corrupt; removing stale artifact so it can be rebuilt: {err}"
            );
            let _ = fs::remove_file(&artifact_path);
            return None;
        }
    };
    if parsed.artifact.version != PDF_OCR_ALIGNMENT_VERSION {
        warn!(
            path = %artifact_path.display(),
            cached_version = parsed.artifact.version,
            required_version = PDF_OCR_ALIGNMENT_VERSION,
            "PDF OCR alignment artifact version changed; removing stale artifact so it can be rebuilt"
        );
        let _ = fs::remove_file(&artifact_path);
        return None;
    }
    debug!(
        path = %artifact_path.display(),
        sentence_count = parsed.artifact.sentence_count,
        mapped_sentence_count = parsed.artifact.mapped_sentence_count,
        quality_class = ?parsed.artifact.quality_class,
        "Loaded cached PDF OCR alignment artifact"
    );
    Some(parsed.artifact)
}

#[cfg(target_arch = "wasm32")]
pub(super) fn load_pdf_ocr_alignment_artifact(
    _source_path: &Path,
) -> Option<PdfOcrAlignmentArtifact> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn persist_pdf_render_precomputed_state(
    source_path: &Path,
    artifact: &PdfRenderPrecomputedState,
) {
    ensure_content_layout(source_path);
    let path = hash_dir(source_path).join(CONTENT_PDF_RENDER_PRECOMPUTE_FILE);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let envelope = PdfRenderPrecomputedEnvelope {
        artifact: PdfRenderPrecomputedState {
            version: PDF_RENDER_PRECOMPUTE_VERSION,
            ..artifact.clone()
        },
    };
    println!("DEBUG: persisting precompute to path: {}", path.display());
    match toml::to_string(&envelope) {
        Ok(serialized) => match fs::write(&path, serialized) {
            Ok(()) => debug!(
                path = %path.display(),
                page_count = envelope.artifact.page_texts.len(),
                sentence_hint_count = envelope.artifact.sentence_page_hints.len(),
                "Persisted PDF render precompute artifact"
            ),
            Err(err) => warn!(
                path = %path.display(),
                "Failed to persist PDF render precompute artifact: {err}"
            ),
        },
        Err(err) => warn!(
            path = %path.display(),
            "Failed to serialize PDF render precompute artifact: {err}"
        ),
    }
}

#[cfg(target_arch = "wasm32")]
pub(super) fn persist_pdf_render_precomputed_state(
    _source_path: &Path,
    _artifact: &PdfRenderPrecomputedState,
) {
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn load_pdf_render_precomputed_state(
    source_path: &Path,
) -> Option<PdfRenderPrecomputedState> {
    let path = hash_dir(source_path).join(CONTENT_PDF_RENDER_PRECOMPUTE_FILE);
    let raw = fs::read_to_string(&path).ok()?;
    let parsed = toml::from_str::<PdfRenderPrecomputedEnvelope>(&raw).ok()?;
    if parsed.artifact.version != PDF_RENDER_PRECOMPUTE_VERSION {
        debug!(
            path = %path.display(),
            found = parsed.artifact.version,
            expected = PDF_RENDER_PRECOMPUTE_VERSION,
            "Ignoring stale PDF render precompute artifact version"
        );
        return None;
    }
    Some(parsed.artifact)
}

#[cfg(target_arch = "wasm32")]
pub(super) fn load_pdf_render_precomputed_state(
    _source_path: &Path,
) -> Option<PdfRenderPrecomputedState> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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
