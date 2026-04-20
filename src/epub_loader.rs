//! Source loading utilities.
//!
//! The loader converts supported book formats to plain text and also extracts
//! image assets for rendering in the reading pane.

use crate::cache::{hash_dir, is_browser_tab_manifest, load_browser_tab_manifest};
use crate::cancellation::CancellationToken;
use anyhow::{Context, Result};
use epub::doc::EpubDoc;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::{info, warn};

static RE_MARKDOWN_IMAGE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").expect("valid markdown image regex"));
static RE_HTML_IMG_SRC: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?is)<img\b[^>]*?\bsrc\s*=\s*["']([^"']+)["'][^>]*>"#)
        .expect("valid html image src regex")
});
static RE_HTML_SVG_IMAGE_HREF: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?is)<image\b[^>]*?\b(?:xlink:href|href)\s*=\s*["']([^"']+)["'][^>]*>"#)
        .expect("valid svg image href regex")
});

#[path = "epub_loader/source_pipeline.rs"]
mod source_pipeline;

use source_pipeline::{
    ensure_not_cancelled, is_epub, is_markdown, load_source_content, record_markdown_availability,
    source_type_label,
};
use ts_rs::TS;

#[derive(Debug, Clone)]
pub struct BookImage {
    pub path: PathBuf,
    pub source_ref: String,
    pub label: String,
    pub char_offset: usize,
}

#[derive(Debug, Clone)]
pub struct LoadedBook {
    pub tts_text: String,
    pub reading_markdown: Option<String>,
    pub reading_html: Option<String>,
    pub has_structured_markdown: bool,
    pub pdf_geometry_mode: Option<PdfGeometryMode>,
    pub pdf_sync_strategy: Option<PdfSyncStrategy>,
    pub pdf_classification: Option<PdfClassificationSummary>,
    pub pdf_runtime_policy: Option<PdfRuntimePolicySummary>,
    pub pdf_ocr_pipeline: Option<PdfOcrPipelineSummary>,
    pub images: Vec<BookImage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PdfGeometryMode {
    HighTextTrust,
    MixedTextTrust,
    OcrRequired,
    RenderOnlyNoSync,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PdfSyncStrategy {
    SentenceSpans,
    ParagraphFallback,
    RenderOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PdfPageClass {
    EmbeddedClean,
    EmbeddedNoisy,
    EmbeddedSparse,
    HiddenOcrOverlay,
    ScanWithWeakOcr,
    ImageOnlyNoText,
    LayoutHostile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PdfDocumentClass {
    EmbeddedClean,
    EmbeddedNoisy,
    EmbeddedSparse,
    HiddenOcrOverlay,
    ScanWithGoodOcr,
    ScanWithWeakOcr,
    ImageOnlyNoText,
    HybridMixedDocument,
    LayoutHostileDocument,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PdfOcrRecommendation {
    NotNeeded,
    GeometryOnly,
    RequiredForText,
    UnlikelyToHelp,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct PdfProbePageSummary {
    pub page_index: u32,
    pub char_count: u32,
    pub token_count: u32,
    pub line_count: u32,
    pub whitespace_ratio: f32,
    pub garbage_ratio: f32,
    pub punctuation_ratio: f32,
    pub digit_ratio: f32,
    pub non_latin_ratio: f32,
    pub alpha_char_ratio: f32,
    pub uppercase_char_ratio: f32,
    pub alpha_token_ratio: f32,
    pub avg_token_length: f32,
    pub short_line_ratio: f32,
    pub repeated_line_ratio: f32,
    pub hyphenated_line_ratio: f32,
    pub image_object_count: u32,
    pub image_coverage_ratio: f32,
    pub duplicate_text_ratio: f32,
    pub block_coherence: f32,
    pub coordinate_sanity: f32,
    pub reading_order_stability: f32,
    pub hidden_text_layer_suspected: bool,
    pub invisible_text_suspected: bool,
    pub duplicate_text_suspected: bool,
    pub stacked_duplicate_text_suspected: bool,
    pub mixed_text_image_suspected: bool,
    pub full_page_raster_suspected: bool,
    pub first_line: String,
    pub last_line: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct PdfProbeFeatureSummary {
    pub sampled_pages: u32,
    pub text_page_ratio: f32,
    pub empty_text_page_ratio: f32,
    pub sparse_text_page_ratio: f32,
    pub noisy_text_page_ratio: f32,
    pub repeated_header_ratio: f32,
    pub repeated_footer_ratio: f32,
    pub image_page_ratio: f32,
    pub mixed_text_image_page_ratio: f32,
    pub full_page_raster_page_ratio: f32,
    pub hidden_text_layer_page_ratio: f32,
    pub invisible_text_layer_page_ratio: f32,
    pub duplicate_text_page_ratio: f32,
    pub stacked_duplicate_text_page_ratio: f32,
    pub avg_chars_per_page: u32,
    pub garbage_ratio: f32,
    pub whitespace_ratio: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct PdfEmbeddedTextTrustDiagnostics {
    pub block_coherence: f32,
    pub coordinate_sanity: f32,
    pub reading_order_stability: f32,
    pub duplicate_text_suppression_needed: bool,
    pub hidden_text_layer_suspected: bool,
    pub invisible_text_suspected: bool,
    pub stacked_duplicate_text_suspected: bool,
    pub full_page_raster_ratio: f32,
    pub mixed_text_image_ratio: f32,
    pub ocr_replace_confidence: f32,
    pub ocr_augment_confidence: f32,
    pub ocr_confidence_threshold_met: bool,
    #[serde(default)]
    pub rationale: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct PdfPageClassificationSummary {
    pub page_index: u32,
    pub class: PdfPageClass,
    pub confidence: f32,
    #[serde(default)]
    pub reasons: Vec<String>,
    pub features: PdfProbePageSummary,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct PdfPageClassCount {
    pub class: PdfPageClass,
    #[ts(type = "number")]
    pub count: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct PdfClassificationSummary {
    pub document_class: PdfDocumentClass,
    pub confidence: f32,
    pub ocr_recommendation: PdfOcrRecommendation,
    #[serde(default)]
    pub reasons: Vec<String>,
    pub feature_summary: PdfProbeFeatureSummary,
    pub trust_diagnostics: PdfEmbeddedTextTrustDiagnostics,
    #[serde(default)]
    pub page_classes: Vec<PdfPageClassificationSummary>,
    #[serde(default)]
    pub class_distribution: Vec<PdfPageClassCount>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PdfTextOnlyPolicy {
    FullText,
    LimitedText,
    OcrRequired,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PdfSentenceHighlightPolicy {
    ExactSentence,
    ParagraphFallback,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PdfSearchPolicy {
    FullText,
    LimitedText,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PdfBookmarkPolicy {
    CanonicalText,
    PageOnly,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct PdfRuntimePolicySummary {
    pub text_only_policy: PdfTextOnlyPolicy,
    pub sentence_highlight_policy: PdfSentenceHighlightPolicy,
    pub search_policy: PdfSearchPolicy,
    pub bookmark_policy: PdfBookmarkPolicy,
    pub tts_allowed: bool,
    pub pretty_sync_enabled: bool,
    pub exact_sentence_sync: bool,
    pub explanation: String,
    #[serde(default)]
    pub degraded_reasons: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PdfOcrGeometryQualityClass {
    OcrHighTrust,
    OcrMixedTrust,
    OcrTextOnly,
    OcrFailedOrUnusable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PdfOcrSourceKind {
    EmbeddedText,
    OcrText,
    MixedMergedText,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct PdfOcrAlignmentSummary {
    pub quality_class: PdfOcrGeometryQualityClass,
    pub source_kind: PdfOcrSourceKind,
    pub sentence_count: u32,
    pub mapped_sentence_count: u32,
    pub rect_mapped_sentence_count: u32,
    pub line_mapped_sentence_count: u32,
    pub block_mapped_sentence_count: u32,
    pub page_only_sentence_count: u32,
    pub unmappable_sentence_count: u32,
    pub highlightable_sentence_count: u32,
    pub token_lineage_available: bool,
    pub deterministic: bool,
    pub coverage_ratio: f32,
    pub reused_alignment_count: u32,
    pub rebuilt_alignment_count: u32,
    pub cached_page_bucket_count: u32,
    pub alignment_build_ms: u32,
    pub geometry_block_count: u32,
    pub geometry_line_count: u32,
    pub geometry_token_count: u32,
    pub page_timing_count: u32,
    pub chunk_timing_count: u32,
    pub max_page_build_ms: u32,
    pub max_chunk_build_ms: u32,
    pub cross_column_alignment_count: u32,
    pub cross_column_confident_alignment_count: u32,
    pub exact_sentence_rate: f32,
    pub degraded_fallback_rate: f32,
    pub page_only_rate: f32,
    pub unmappable_rate: f32,
    #[serde(default)]
    pub degraded_reasons: Vec<String>,
    pub explanation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PdfOcrEnginePolicy {
    EmbeddedTextOnly,
    OcrOnly,
    HybridEmbeddedOcrMerge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PdfOcrFallbackDecision {
    NativeTextToOcrFallback,
    OcrRetryMoreAggressive,
    OcrTextOnlyWithoutGeometry,
    RenderOnlyNoSync,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct PdfOcrPipelineSummary {
    pub engine_policy: PdfOcrEnginePolicy,
    #[serde(default)]
    pub fallback_decisions: Vec<PdfOcrFallbackDecision>,
    pub ocr_enabled: bool,
    pub page_count: u32,
    pub sampled_pages: u32,
    pub chunk_count: u32,
    pub reading_order_mode: String,
    #[serde(default)]
    pub normalization_summary: PdfOcrNormalizationSummary,
    #[serde(default)]
    pub page_reading_order: Vec<PdfOcrPageReadingOrderDecision>,
    #[serde(default)]
    pub fallback_strategy_labels: Vec<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct PdfOcrNormalizationSummary {
    pub canonical_text_derived_from_ocr: bool,
    pub page_sentence_provenance_available: bool,
    pub token_trail_available: bool,
    pub broken_line_join_count: u32,
    pub hyphen_recovery_count: u32,
    pub ligature_replacement_count: u32,
    pub unicode_normalization_count: u32,
    pub repeated_header_suppression_count: u32,
    pub repeated_footer_suppression_count: u32,
    pub margin_sidenote_suppression_count: u32,
    pub table_cell_normalization_count: u32,
    pub footnote_marker_adjustment_count: u32,
    pub punctuation_repair_count: u32,
    pub dropped_noise_line_count: u32,
    pub merged_line_count: u32,
    #[serde(default)]
    pub trace_notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PdfOcrPageLayoutClass {
    SingleColumn,
    StrongTwoColumn,
    MixedColumnCaptionBand,
    BottomFootnoteBand,
    OuterMarginSidenotes,
    TableLike,
    RotatedPage,
    RotatedBlocks,
    FigureCaptionSeparated,
    Fallback,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct PdfOcrPageReadingOrderDecision {
    pub page_index: u32,
    pub layout_class: PdfOcrPageLayoutClass,
    pub confidence: f32,
    #[serde(default)]
    pub reasons: Vec<String>,
}

/// Load a supported source file and return plain text plus extracted image paths.
pub fn load_book_content(path: &Path) -> Result<LoadedBook> {
    load_book_content_with_cancel(path, None)
}

/// Load a supported source file with an optional cooperative cancellation token.
pub fn load_book_content_with_cancel(
    path: &Path,
    cancel: Option<&CancellationToken>,
) -> Result<LoadedBook> {
    let start = Instant::now();
    let source_type = source_type_label(path);
    ensure_not_cancelled(cancel, "load_book_content_start")?;
    let content = load_source_content(path, cancel)?;
    crate::cache::persist_dual_view_artifacts(
        path,
        &content.tts_text,
        content.reading_markdown.as_deref(),
        content.reading_html.as_deref(),
    );
    if let (Some(pdf_geometry_mode), Some(pdf_sync_strategy)) =
        (content.pdf_geometry_mode, content.pdf_sync_strategy)
    {
        crate::cache::persist_pdf_sync_meta(
            path,
            pdf_geometry_mode,
            pdf_sync_strategy,
            content.pdf_classification.as_ref(),
            content.pdf_runtime_policy.as_ref(),
        );
    }
    record_markdown_availability(path, content.has_structured_markdown);
    ensure_not_cancelled(cancel, "after_load_source_text")?;
    let images = match collect_images(path) {
        Ok(images) => images,
        Err(err) => {
            warn!(path = %path.display(), "Image extraction failed: {err}");
            Vec::new()
        }
    };
    info!(
        path = %path.display(),
        source_type,
        pretty_kind = if content.reading_html.is_some() {
            "html"
        } else if content.reading_markdown.is_some() {
            "markdown"
        } else {
            "none"
        },
        has_structured_markdown = content.has_structured_markdown,
        markdown_chars = content.reading_markdown.as_ref().map(|v| v.len()).unwrap_or(0),
        html_chars = content.reading_html.as_ref().map(|v| v.len()).unwrap_or(0),
        tts_chars = content.tts_text.len(),
        pdf_geometry_mode = ?content.pdf_geometry_mode,
        pdf_sync_strategy = ?content.pdf_sync_strategy,
        pdf_document_class = ?content
            .pdf_classification
            .as_ref()
            .map(|value| value.document_class),
        pdf_ocr_recommendation = ?content
            .pdf_classification
            .as_ref()
            .map(|value| value.ocr_recommendation),
        pdf_runtime_policy = ?content
            .pdf_runtime_policy
            .as_ref()
            .map(|value| value.sentence_highlight_policy),
        pdf_ocr_engine_policy = ?content
            .pdf_ocr_pipeline
            .as_ref()
            .map(|value| value.engine_policy),
        image_count = images.len(),
        elapsed_ms = start.elapsed().as_millis(),
        "Source load complete"
    );
    Ok(LoadedBook {
        tts_text: content.tts_text,
        reading_markdown: content.reading_markdown,
        reading_html: content.reading_html,
        has_structured_markdown: content.has_structured_markdown,
        pdf_geometry_mode: content.pdf_geometry_mode,
        pdf_sync_strategy: content.pdf_sync_strategy,
        pdf_classification: content.pdf_classification,
        pdf_runtime_policy: content.pdf_runtime_policy,
        pdf_ocr_pipeline: content.pdf_ocr_pipeline,
        images,
    })
}

#[derive(Debug, Clone)]
struct SourceContent {
    tts_text: String,
    reading_markdown: Option<String>,
    reading_html: Option<String>,
    has_structured_markdown: bool,
    pdf_geometry_mode: Option<PdfGeometryMode>,
    pdf_sync_strategy: Option<PdfSyncStrategy>,
    pdf_classification: Option<PdfClassificationSummary>,
    pdf_runtime_policy: Option<PdfRuntimePolicySummary>,
    pdf_ocr_pipeline: Option<PdfOcrPipelineSummary>,
}

fn collect_images(path: &Path) -> Result<Vec<BookImage>> {
    let start = Instant::now();
    if is_browser_tab_manifest(path) {
        let images = collect_browser_tab_assets(path)?;
        info!(
            path = %path.display(),
            source_type = "browser_tab",
            image_count = images.len(),
            elapsed_ms = start.elapsed().as_millis(),
            "Image extraction completed"
        );
        return Ok(images);
    }
    if is_markdown(path) {
        let images = collect_markdown_images(path)?;
        info!(
            path = %path.display(),
            source_type = "markdown",
            image_count = images.len(),
            elapsed_ms = start.elapsed().as_millis(),
            "Image extraction completed"
        );
        return Ok(images);
    }
    if is_epub(path) {
        let images = collect_epub_images(path)?;
        info!(
            path = %path.display(),
            source_type = "epub",
            image_count = images.len(),
            elapsed_ms = start.elapsed().as_millis(),
            "Image extraction completed"
        );
        return Ok(images);
    }
    info!(
        path = %path.display(),
        source_type = source_type_label(path),
        elapsed_ms = start.elapsed().as_millis(),
        "Image extraction skipped for source type"
    );
    Ok(Vec::new())
}

fn collect_browser_tab_assets(path: &Path) -> Result<Vec<BookImage>> {
    let manifest = load_browser_tab_manifest(path)
        .with_context(|| format!("Failed to load browser-tab manifest {}", path.display()))?;
    if manifest.assets.is_empty() {
        return Ok(Vec::new());
    }
    let text_len = fs::read_to_string(&manifest.text_path)
        .map(|value| value.len())
        .unwrap_or(0)
        .max(1);
    let total = manifest.assets.len();
    Ok(manifest
        .assets
        .iter()
        .enumerate()
        .map(|(idx, asset)| BookImage {
            path: asset.local_path.clone(),
            source_ref: asset.raw_path.clone(),
            label: Path::new(&asset.raw_path)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("asset")
                .to_string(),
            char_offset: ((idx + 1) * text_len) / (total + 1),
        })
        .collect())
}

fn collect_markdown_images(path: &Path) -> Result<Vec<BookImage>> {
    let data = fs::read_to_string(path)
        .with_context(|| format!("Failed to read markdown file at {}", path.display()))?;
    let mut images = Vec::new();
    let mut seen = HashSet::new();
    let base_dir = path.parent().unwrap_or(Path::new("."));

    for captures in RE_MARKDOWN_IMAGE.captures_iter(&data) {
        let alt = captures
            .get(1)
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();
        let Some(raw_target) = captures.get(2).map(|m| m.as_str()) else {
            continue;
        };
        let Some(local_target) = normalize_markdown_image_target(raw_target) else {
            continue;
        };

        let candidate = base_dir.join(local_target);
        if !candidate.exists() {
            continue;
        }

        let canonical = fs::canonicalize(&candidate).unwrap_or(candidate);
        if !seen.insert(canonical.clone()) {
            continue;
        }

        let label = if !alt.is_empty() {
            alt
        } else {
            canonical
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("image")
                .to_string()
        };
        images.push(BookImage {
            path: canonical,
            source_ref: raw_target.to_string(),
            label,
            char_offset: captures.get(0).map(|m| m.start()).unwrap_or(0),
        });
    }

    Ok(images)
}

fn collect_epub_images(path: &Path) -> Result<Vec<BookImage>> {
    #[derive(Debug, Clone)]
    struct ExtractedImage {
        output: PathBuf,
        source_ref: String,
        label: String,
    }

    let mut doc =
        EpubDoc::new(path).with_context(|| format!("Failed to open EPUB at {}", path.display()))?;
    let mut entries: Vec<(String, PathBuf, String)> = doc
        .resources
        .iter()
        .map(|(id, item)| (id.clone(), item.path.clone(), item.mime.clone()))
        .filter(|(_, _, mime)| is_supported_image_mime(mime))
        .collect();
    entries.sort_by(|a, b| a.1.cmp(&b.1));

    let image_dir = hash_dir(path).join("images");
    fs::create_dir_all(&image_dir)
        .with_context(|| format!("Failed to create image cache dir {}", image_dir.display()))?;

    let mut extracted = Vec::new();
    let mut path_lookup: std::collections::HashMap<String, ExtractedImage> =
        std::collections::HashMap::new();
    let mut basename_lookup: std::collections::HashMap<String, ExtractedImage> =
        std::collections::HashMap::new();

    for (_idx, (id, resource_path, mime)) in entries.into_iter().enumerate() {
        let Some((bytes, _)) = doc.get_resource(&id) else {
            continue;
        };

        let default_extension = extension_from_mime(&mime).map(str::to_string);
        let output = epub_resource_output_path(&image_dir, &resource_path, default_extension);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create image cache dir {}", parent.display())
            })?;
        }
        if !output.exists() {
            fs::write(&output, &bytes)
                .with_context(|| format!("Failed to write extracted image {}", output.display()))?;
        }

        let label = resource_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("image")
            .to_string();

        let image = ExtractedImage {
            output: output.clone(),
            source_ref: resource_path.to_string_lossy().to_string(),
            label: label.clone(),
        };
        extracted.push(image.clone());

        let normalized_key = normalize_epub_path_key(resource_path.to_string_lossy().as_ref());
        path_lookup.insert(normalized_key, image.clone());
        if let Some(base_name) = resource_path.file_name().and_then(|s| s.to_str()) {
            let base_key = normalize_epub_path_key(base_name);
            basename_lookup
                .entry(base_key)
                .or_insert_with(|| image.clone());
        }
    }

    if extracted.is_empty() {
        return Ok(Vec::new());
    }

    let mut images = Vec::new();
    let mut chapter_idx = 0usize;
    let mut chapter_start = 0usize;
    let mut seen_anchors = HashSet::new();

    loop {
        let Some((chapter, _mime)) = doc.get_current_str() else {
            break;
        };

        if chapter_idx > 0 {
            chapter_start += 2;
        }

        let chapter_len = match html2text::from_read(chapter.as_bytes(), 10_000) {
            Ok(clean) => clean.len(),
            Err(_) => chapter.len(),
        };

        let mut chapter_images = Vec::new();
        for captures in RE_HTML_IMG_SRC.captures_iter(&chapter) {
            let Some(raw_src) = captures.get(1).map(|m| m.as_str()) else {
                continue;
            };
            let src = raw_src
                .split('#')
                .next()
                .unwrap_or(raw_src)
                .split('?')
                .next()
                .unwrap_or(raw_src)
                .trim();
            if src.is_empty() {
                continue;
            }

            let normalized_src = normalize_epub_path_key(src);
            let resolved = path_lookup.get(&normalized_src).cloned().or_else(|| {
                Path::new(src)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(normalize_epub_path_key)
                    .and_then(|base| basename_lookup.get(&base).cloned())
            });

            if let Some(image) = resolved {
                chapter_images.push(image);
            }
        }
        for captures in RE_HTML_SVG_IMAGE_HREF.captures_iter(&chapter) {
            let Some(raw_src) = captures.get(1).map(|m| m.as_str()) else {
                continue;
            };
            let src = raw_src
                .split('#')
                .next()
                .unwrap_or(raw_src)
                .split('?')
                .next()
                .unwrap_or(raw_src)
                .trim();
            if src.is_empty() {
                continue;
            }
            let normalized_src = normalize_epub_path_key(src);
            let resolved = path_lookup.get(&normalized_src).cloned().or_else(|| {
                Path::new(src)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(normalize_epub_path_key)
                    .and_then(|base| basename_lookup.get(&base).cloned())
            });
            if let Some(image) = resolved {
                chapter_images.push(image);
            }
        }

        for (idx, image) in chapter_images.iter().enumerate() {
            let pos_in_chapter = if chapter_len == 0 {
                0
            } else {
                ((idx + 1) * chapter_len) / (chapter_images.len() + 1)
            };
            let char_offset = chapter_start.saturating_add(pos_in_chapter);
            let anchor_key = format!("{}:{char_offset}", image.output.to_string_lossy());
            if !seen_anchors.insert(anchor_key) {
                continue;
            }
            images.push(BookImage {
                path: image.output.clone(),
                source_ref: image.source_ref.clone(),
                label: image.label.clone(),
                char_offset,
            });
        }

        chapter_start = chapter_start.saturating_add(chapter_len);
        chapter_idx = chapter_idx.saturating_add(1);
        if !doc.go_next() {
            break;
        }
    }

    Ok(images)
}

fn normalize_epub_path_key(raw: &str) -> String {
    let trimmed = raw.trim().trim_matches('/');
    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch == '\\' {
            out.push('/');
        } else {
            out.push(ch.to_ascii_lowercase());
        }
    }
    out
}

fn normalize_markdown_image_target(raw: &str) -> Option<&str> {
    let trimmed = raw.trim().trim_matches('<').trim_matches('>');
    if trimmed.is_empty() {
        return None;
    }
    let target = trimmed
        .split_whitespace()
        .next()
        .unwrap_or(trimmed)
        .trim_matches('"')
        .trim_matches('\'');
    if target.is_empty() {
        return None;
    }
    if target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("data:")
        || target.starts_with("mailto:")
        || target.starts_with('#')
    {
        return None;
    }

    let target = target.split('#').next().unwrap_or(target);
    let target = target.split('?').next().unwrap_or(target);
    if target.is_empty() {
        None
    } else {
        Some(target)
    }
}

fn is_supported_image_mime(mime: &str) -> bool {
    matches!(
        mime.to_ascii_lowercase().as_str(),
        "image/png"
            | "image/jpeg"
            | "image/jpg"
            | "image/gif"
            | "image/webp"
            | "image/bmp"
            | "image/svg+xml"
    )
}

fn extension_from_mime(mime: &str) -> Option<&'static str> {
    match mime.to_ascii_lowercase().as_str() {
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/bmp" => Some("bmp"),
        "image/svg+xml" => Some("svg"),
        _ => None,
    }
}

fn epub_resource_output_path(
    image_dir: &Path,
    resource_path: &Path,
    default_extension: Option<String>,
) -> PathBuf {
    let mut out = image_dir.to_path_buf();
    for component in resource_path.components() {
        if let std::path::Component::Normal(segment) = component {
            out.push(segment);
        }
    }
    let missing_ext = out.extension().and_then(|s| s.to_str()).is_none();
    if missing_ext && let Some(ext) = default_extension {
        out.set_extension(ext);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser_tabs::{
        BrowserTab, BrowserTabSnapshot, SnapshotTruncation, SnapshotTruncationEntry,
    };
    use crate::cache::{delete_recent_source_and_cache, persist_browser_tab_source};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_file(name: &str, extension: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "lanternleaf_epub_loader_{name}_{nanos}.{extension}"
        ))
    }

    #[test]
    fn load_book_content_honors_cancellation_token() {
        let path = unique_temp_file("cancelled_txt", "txt");
        fs::write(&path, "hello world").expect("write txt fixture");
        let token = CancellationToken::new();
        token.cancel();

        let err = load_book_content_with_cancel(&path, Some(&token))
            .expect_err("cancelled load should return an error");
        assert!(
            err.to_string().contains("operation cancelled"),
            "unexpected error: {err}"
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn markdown_source_emits_markdown_and_tts_text() {
        let path = unique_temp_file("markdown_contract", "md");
        fs::write(&path, "# Title\n\nThis is **markdown** content.").expect("write md fixture");

        let loaded = load_book_content(&path).expect("markdown should load");
        assert!(loaded.has_structured_markdown);
        assert!(loaded.reading_markdown.is_some());
        assert!(loaded.tts_text.contains("Title"));
        assert!(loaded.tts_text.contains("markdown"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn text_source_falls_back_without_markdown() {
        let path = unique_temp_file("text_contract", "txt");
        fs::write(&path, "plain text source").expect("write txt fixture");

        let loaded = load_book_content(&path).expect("text should load");
        assert!(!loaded.has_structured_markdown);
        assert!(loaded.reading_markdown.is_none());
        assert_eq!(loaded.tts_text, "plain text source");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn html_source_emits_native_html_without_markdown_fallback() {
        let path = unique_temp_file("html_contract", "html");
        fs::write(
            &path,
            "<html><body><h1>Heading</h1><p>Paragraph</p><img src=\"cover.png\"/></body></html>",
        )
        .expect("write html fixture");

        let loaded = load_book_content(&path).expect("html should load");
        assert!(loaded.reading_html.is_some());
        assert!(loaded.reading_markdown.is_none());
        assert!(loaded.has_structured_markdown);
        assert!(loaded.tts_text.contains("Heading"));
        assert!(loaded.tts_text.contains("Paragraph"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn html_source_tts_text_drops_non_text_noise_from_plain_conversion() {
        let path = unique_temp_file("html_plain_cleanup", "html");
        fs::write(
            &path,
            r#"
            <html>
              <head>
                <style>body { color: red; }</style>
                <script>console.log("nope")</script>
              </head>
              <body>
                <h1>Readable Heading</h1>
                <p>First readable paragraph.</p>
                <figure><img src="cover.png" alt="Cover"/><figcaption>Ignored figure caption</figcaption></figure>
                <table><tr><td>Ignore table text</td></tr></table>
                <p>Second readable paragraph.</p>
              </body>
            </html>
            "#,
        )
        .expect("write html cleanup fixture");

        let loaded = load_book_content(&path).expect("html should load");
        assert!(loaded.reading_html.is_some());
        assert!(loaded.tts_text.contains("Readable Heading"));
        assert!(loaded.tts_text.contains("First readable paragraph."));
        assert!(loaded.tts_text.contains("Second readable paragraph."));
        assert!(!loaded.tts_text.contains("console.log"));
        assert!(!loaded.tts_text.contains("body { color: red; }"));
        assert!(!loaded.tts_text.contains("Ignore table text"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn browser_tab_manifest_loads_native_html_and_plain_text() {
        let snapshot = BrowserTabSnapshot {
            tab_id: 909,
            title: "Imported Article".to_string(),
            url: "https://example.com/articles/start".to_string(),
            lang: Some("en".to_string()),
            ready_state: Some("complete".to_string()),
            captured_at: Some("2026-03-06T20:00:00Z".to_string()),
            html: Some(
                r#"<article><p><img src="./cover.jpg"/>Hello article.</p></article>"#.to_string(),
            ),
            text: Some("Hello article.".to_string()),
            selection: None,
            truncation: SnapshotTruncation {
                html: SnapshotTruncationEntry::default(),
                text: SnapshotTruncationEntry::default(),
                selection: SnapshotTruncationEntry::default(),
            },
        };
        let tab = BrowserTab {
            id: 909,
            window_id: 21,
            index: Some(0),
            active: Some(true),
            audible: Some(false),
            pinned: Some(false),
            status: Some("complete".to_string()),
            title: "Imported Article".to_string(),
            url: snapshot.url.clone(),
            fav_icon_url: None,
            last_accessed: Some(0.0),
        };
        let manifest_path =
            persist_browser_tab_source(&snapshot, Some(&tab)).expect("persist browser tab source");

        let loaded = load_book_content(&manifest_path).expect("load browser tab manifest");
        assert!(loaded.has_structured_markdown);
        assert!(loaded.reading_html.is_some());
        assert!(loaded.reading_markdown.is_none());
        assert_eq!(loaded.tts_text, "Hello article.");
        assert!(
            loaded
                .reading_html
                .as_deref()
                .unwrap_or_default()
                .contains(r#"data-ll-base-url="https://example.com/articles/start""#)
        );

        delete_recent_source_and_cache(&manifest_path).expect("cleanup browser tab source");
    }

    #[test]
    fn resolve_pdf_content_marks_markdown_only_when_present() {
        let structured = source_pipeline::resolve_pdf_dual_view_content(
            "Line one.\nLine two.",
            "# Heading\n\nLine one.",
            None,
        );
        assert!(structured.has_structured_markdown);
        assert!(structured.reading_markdown.is_some());
        assert!(structured.tts_text.contains("Line one."));

        let scan_fallback =
            source_pipeline::resolve_pdf_dual_view_content("Scanned OCR text", "   ", None);
        assert!(!scan_fallback.has_structured_markdown);
        assert!(scan_fallback.reading_markdown.is_none());
        assert!(scan_fallback.tts_text.contains("Scanned OCR text"));
    }

    #[test]
    fn normalize_pdf_text_for_reader_collapses_wrapped_lines_and_hyphenation() {
        let normalized = source_pipeline::normalize_pdf_text_for_reader(
            "Alpha para-\n graph line\nnext line\n\nBeta block",
        );
        assert_eq!(normalized, "Alpha paragraph line next line.\n\nBeta block");
    }

    #[test]
    fn normalize_pdf_text_for_reader_preserves_paragraph_breaks() {
        let normalized =
            source_pipeline::normalize_pdf_text_for_reader("First line\nSecond line\n\nThird line");
        assert_eq!(normalized, "First line Second line.\n\nThird line");
    }

    #[test]
    fn project_root_finds_workspace_conf_directory() {
        let root = source_pipeline::project_root();
        assert!(
            root.join("conf").exists(),
            "expected conf directory at {}",
            root.display()
        );
    }
}
