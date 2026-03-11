use super::{
    PdfBookmarkPolicy, PdfClassificationSummary, PdfDocumentClass, PdfEmbeddedTextTrustDiagnostics,
    PdfGeometryMode, PdfOcrRecommendation, PdfPageClass, PdfPageClassCount,
    PdfPageClassificationSummary, PdfProbeFeatureSummary, PdfProbePageSummary,
    PdfRuntimePolicySummary, PdfSearchPolicy, PdfSentenceHighlightPolicy, PdfSyncStrategy,
    PdfTextOnlyPolicy, SourceContent,
};
use crate::cache::{hash_dir, is_browser_tab_manifest, load_browser_tab_manifest};
use crate::cancellation::CancellationToken;
use anyhow::{Context, Result};
use epub::doc::EpubDoc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use std::time::UNIX_EPOCH;
use tracing::{info, warn};

const PANDOC_FILTER_REL_PATH: &str = "conf/pandoc/strip-nontext.lua";
const PANDOC_PIPELINE_REV: &str = "pandoc-clean-v1";
const QUACK_CHECK_CONFIG_REL_PATH: &str = "conf/quack-check.toml";
const QUACK_CHECK_PIPELINE_REV: &str = "quack-check-pdf-v5";
const QUACK_CHECK_TEXT_FILENAME_DEFAULT: &str = "transcript.txt";
const PDF_CLASSIFICATION_VERSION: u32 = 3;
const AVAILABILITY_LOG_EVERY: u64 = 20;

static LOAD_COUNT_TOTAL: AtomicU64 = AtomicU64::new(0);
static LOAD_COUNT_WITH_MARKDOWN: AtomicU64 = AtomicU64::new(0);

pub(super) fn load_source_content(
    path: &Path,
    cancel: Option<&CancellationToken>,
) -> Result<SourceContent> {
    let start = Instant::now();
    ensure_not_cancelled(cancel, "load_source_text_start")?;
    if is_browser_tab_manifest(path) {
        if let Err(err) = crate::cache::rehydrate_browser_tab_manifest_assets(path) {
            warn!(path = %path.display(), "Browser-tab asset rehydrate failed: {err}");
        }
        let manifest = load_browser_tab_manifest(path)
            .with_context(|| format!("Failed to load browser-tab manifest {}", path.display()))?;
        let tts_text = fs::read_to_string(&manifest.text_path).with_context(|| {
            format!(
                "Failed to read browser-tab text {}",
                manifest.text_path.display()
            )
        })?;
        let html = fs::read_to_string(&manifest.html_path).with_context(|| {
            format!(
                "Failed to read browser-tab html {}",
                manifest.html_path.display()
            )
        })?;
        let wrapped_html = wrap_browser_tab_html(&html, &manifest.url);
        info!(
            path = %path.display(),
            tab_id = manifest.tab_id,
            url = %manifest.url,
            html_truncated = manifest.html_truncated,
            text_truncated = manifest.text_truncated,
            "Loaded browser-tab dual-view payload"
        );
        return Ok(SourceContent {
            tts_text: if tts_text.trim().is_empty() {
                "No textual content found in this browser tab.".to_string()
            } else {
                tts_text
            },
            reading_markdown: None,
            reading_html: Some(wrapped_html),
            has_structured_markdown: true,
            pdf_geometry_mode: None,
            pdf_sync_strategy: None,
            pdf_classification: None,
            pdf_runtime_policy: None,
        });
    }

    if is_text_file(path) {
        info!(path = %path.display(), "Loading plain text content");
        let data = fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let tts_text = if data.trim().is_empty() {
            "No textual content found in this file.".to_string()
        } else {
            data
        };
        info!(
            total_chars = tts_text.len(),
            "Finished loading plain text content"
        );
        return Ok(SourceContent {
            tts_text,
            reading_markdown: None,
            reading_html: None,
            has_structured_markdown: false,
            pdf_geometry_mode: None,
            pdf_sync_strategy: None,
            pdf_classification: None,
            pdf_runtime_policy: None,
        });
    }

    if is_pdf(path) {
        return load_pdf_with_quack_check(path, cancel);
    }

    if is_native_html_source(path) {
        let tts_text = load_with_pandoc(path, "plain", cancel)?;
        let html = load_native_pretty_html(path, cancel)?;
        let reading_html = if html.trim().is_empty() {
            None
        } else {
            Some(html)
        };
        let has_pretty = reading_html
            .as_deref()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        let result = SourceContent {
            tts_text,
            reading_markdown: None,
            reading_html,
            has_structured_markdown: has_pretty,
            pdf_geometry_mode: None,
            pdf_sync_strategy: None,
            pdf_classification: None,
            pdf_runtime_policy: None,
        };
        info!(
            path = %path.display(),
            stage = "native_html_dual_convert",
            elapsed_ms = start.elapsed().as_millis(),
            has_structured_markdown = result.has_structured_markdown,
            "Completed source conversion stage"
        );
        return Ok(result);
    }

    if is_pandoc_dual_source(path) {
        let tts_text = load_with_pandoc(path, "plain", cancel)?;
        let markdown = load_with_pandoc(path, "gfm-raw_html-raw_attribute", cancel)?;
        let reading_markdown = if markdown.trim().is_empty() {
            None
        } else {
            Some(markdown)
        };
        let has_pretty = reading_markdown.is_some();
        let result = SourceContent {
            tts_text,
            reading_markdown,
            reading_html: None,
            has_structured_markdown: has_pretty,
            pdf_geometry_mode: None,
            pdf_sync_strategy: None,
            pdf_classification: None,
            pdf_runtime_policy: None,
        };
        info!(
            path = %path.display(),
            stage = "pandoc_dual_convert",
            elapsed_ms = start.elapsed().as_millis(),
            has_structured_markdown = result.has_structured_markdown,
            "Completed source conversion stage"
        );
        return Ok(result);
    }

    if is_markdown(path) {
        ensure_not_cancelled(cancel, "before_markdown_read")?;
        let data = fs::read_to_string(path)
            .with_context(|| format!("Failed to read markdown file at {}", path.display()))?;
        let tts_text = markdown_to_plain_text(&data);
        return Ok(SourceContent {
            tts_text,
            reading_markdown: Some(data),
            reading_html: None,
            has_structured_markdown: true,
            pdf_geometry_mode: None,
            pdf_sync_strategy: None,
            pdf_classification: None,
            pdf_runtime_policy: None,
        });
    }

    anyhow::bail!(
        "Unsupported source format for {}. Supported source types are .txt, .md, .markdown, .pdf, .html, .doc, .docx, and .epub.",
        path.display(),
    );
}

pub(super) fn source_type_label(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("lltab") => "browser_tab",
        Some("txt") => "txt",
        Some("md") | Some("markdown") => "markdown",
        Some("pdf") => "pdf",
        Some("epub") => "epub",
        Some("html") | Some("htm") => "html",
        Some("doc") => "doc",
        Some("docx") => "docx",
        _ => "unknown",
    }
}

fn markdown_to_plain_text(input: &str) -> String {
    match html2text::from_read(input.as_bytes(), 10_000) {
        Ok(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                "No textual content found in this file.".to_string()
            } else {
                text
            }
        }
        Err(_) => {
            if input.trim().is_empty() {
                "No textual content found in this file.".to_string()
            } else {
                input.to_string()
            }
        }
    }
}

fn wrap_browser_tab_html(html: &str, url: &str) -> String {
    let escaped_url = url
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(r#"<div data-ll-base-url="{escaped_url}" data-ll-browser-tab="1">{html}</div>"#)
}

fn is_text_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase()),
        Some(ext) if ext == "txt"
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

pub(super) fn is_epub(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase()),
        Some(ext) if ext == "epub"
    )
}

fn is_pandoc_dual_source(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase()),
        Some(ext) if ext == "doc" || ext == "docx"
    )
}

fn is_native_html_source(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase()),
        Some(ext) if ext == "epub" || ext == "html" || ext == "htm" || ext == "lltab"
    )
}

fn is_pdf(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase()),
        Some(ext) if ext == "pdf"
    )
}

fn load_pdf_with_quack_check(
    path: &Path,
    cancel: Option<&CancellationToken>,
) -> Result<SourceContent> {
    let start = Instant::now();
    ensure_not_cancelled(cancel, "before_pdf_quack_check")?;
    let config_path = quack_check_config_path()?;
    let config_sha256 = hash_file(&config_path).with_context(|| {
        format!(
            "Failed to hash quack-check config {}",
            config_path.display()
        )
    })?;
    let text_filename = quack_check_text_filename(&config_path)?;
    let signature = pdf_signature(path, &config_sha256, &text_filename)?;

    if let Some(cached) = try_read_pdf_cache(path, &signature)? {
        let tts_text = normalize_pdf_text_for_reader(&cached.text);
        info!(
            path = %path.display(),
            total_chars = tts_text.len(),
            extraction_mode = cached.extraction_mode,
            ocr_enabled = cached.ocr_enabled,
            page_count = cached.page_count,
            sampled_pages = cached.sampled_pages,
            reading_order_mode = cached.reading_order_mode,
            fallback_strategies = ?cached.fallback_strategies,
            chunk_ranges = cached.chunk_ranges.len(),
            pdf_geometry_mode = ?cached.pdf_geometry_mode,
            pdf_sync_strategy = ?cached.pdf_sync_strategy,
            pdf_document_class = ?cached
                .pdf_classification
                .as_ref()
                .map(|value| value.document_class),
            "Using cached quack-check PDF transcript"
        );
        return Ok(SourceContent {
            tts_text,
            reading_markdown: None,
            reading_html: None,
            has_structured_markdown: false,
            pdf_geometry_mode: Some(cached.pdf_geometry_mode),
            pdf_sync_strategy: Some(cached.pdf_sync_strategy),
            pdf_classification: cached.pdf_classification,
            pdf_runtime_policy: cached.pdf_runtime_policy,
        });
    }

    let (_, _, run_out_dir) = pdf_cache_paths(path);
    let run = crate::quack_check::run_pdf_to_text_with_cancel(
        &config_path,
        path,
        &run_out_dir,
        cancel.cloned(),
    )
    .with_context(|| {
        format!(
            "Failed to transcribe PDF with in-process quack-check module for {}",
            path.display()
        )
    })?;
    let report = load_quack_check_report(&run.job_dir).ok();
    let resolved = resolve_pdf_dual_view_content(&run.text, &run.markdown, report.as_ref());
    let tts_text = resolved.tts_text;
    let reading_markdown = resolved.reading_markdown;
    let extraction_mode = pdf_extraction_mode_label(report.as_ref());
    let ocr_enabled = report
        .as_ref()
        .map(|value| value.decision.do_ocr)
        .unwrap_or(false);
    let quality_tier = report
        .as_ref()
        .map(|value| format!("{:?}", value.decision.tier))
        .unwrap_or_else(|| "Unknown".to_string());

    write_pdf_cache(
        path,
        &signature,
        &tts_text,
        resolved
            .pdf_geometry_mode
            .unwrap_or(PdfGeometryMode::MixedTextTrust),
        resolved
            .pdf_sync_strategy
            .unwrap_or(PdfSyncStrategy::ParagraphFallback),
        extraction_mode,
        ocr_enabled,
        report.as_ref(),
        resolved.pdf_classification.as_ref(),
        resolved.pdf_runtime_policy.as_ref(),
    )?;
    info!(
        path = %path.display(),
        total_chars = tts_text.len(),
        markdown_chars = reading_markdown.as_ref().map(|v| v.len()).unwrap_or(0),
        extraction_mode,
        quality_tier,
        ocr_enabled,
        page_count = report.as_ref().map(|value| value.input.page_count).unwrap_or(0),
        sampled_pages = report.as_ref().map(|value| value.sample.sampled_pages).unwrap_or(0),
        reading_order_mode = derive_pdf_reading_order_mode(
            resolved
                .pdf_geometry_mode
                .unwrap_or(PdfGeometryMode::MixedTextTrust),
            ocr_enabled
        ),
        fallback_strategies = ?derive_pdf_fallback_strategies(
            report.as_ref(),
            resolved
                .pdf_geometry_mode
                .unwrap_or(PdfGeometryMode::MixedTextTrust),
            resolved
                .pdf_sync_strategy
                .unwrap_or(PdfSyncStrategy::ParagraphFallback)
        ),
        chunk_ranges = report.as_ref().map(|value| value.chunk_reports.len()).unwrap_or(0),
        pdf_geometry_mode = ?resolved.pdf_geometry_mode,
        pdf_sync_strategy = ?resolved.pdf_sync_strategy,
        pdf_document_class = ?resolved
            .pdf_classification
            .as_ref()
            .map(|value| value.document_class),
        pdf_ocr_recommendation = ?resolved
            .pdf_classification
            .as_ref()
            .map(|value| value.ocr_recommendation),
        pdf_highlight_policy = ?resolved
            .pdf_runtime_policy
            .as_ref()
            .map(|value| value.sentence_highlight_policy),
        job_id = %run.job_id,
        job_dir = %run.job_dir.display(),
        elapsed_ms = start.elapsed().as_millis(),
        "Finished quack-check PDF transcription"
    );
    Ok(SourceContent {
        tts_text,
        reading_html: None,
        has_structured_markdown: reading_markdown.is_some(),
        reading_markdown,
        pdf_geometry_mode: resolved.pdf_geometry_mode,
        pdf_sync_strategy: resolved.pdf_sync_strategy,
        pdf_classification: resolved.pdf_classification,
        pdf_runtime_policy: resolved.pdf_runtime_policy,
    })
}

pub(super) fn resolve_pdf_dual_view_content(
    transcript_text: &str,
    markdown: &str,
    report: Option<&crate::quack_check::report::JobReport>,
) -> SourceContent {
    let tts_text = if transcript_text.trim().is_empty() {
        "No textual content found in this file.".to_string()
    } else {
        normalize_pdf_text_for_reader(transcript_text)
    };
    let reading_markdown = if markdown.trim().is_empty() {
        None
    } else {
        Some(markdown.to_string())
    };
    let pdf_classification = classify_pdf_runtime(report, transcript_text, markdown);
    let (pdf_geometry_mode, pdf_sync_strategy) = derive_pdf_runtime_metadata(
        pdf_classification.as_ref(),
        report,
        transcript_text,
        markdown,
    );
    let pdf_runtime_policy = derive_pdf_runtime_policy(
        pdf_classification.as_ref(),
        pdf_geometry_mode,
        pdf_sync_strategy,
        transcript_text,
    );
    SourceContent {
        tts_text,
        reading_html: None,
        has_structured_markdown: reading_markdown.is_some(),
        reading_markdown,
        pdf_geometry_mode: Some(pdf_geometry_mode),
        pdf_sync_strategy: Some(pdf_sync_strategy),
        pdf_classification,
        pdf_runtime_policy: Some(pdf_runtime_policy),
    }
}

fn load_native_pretty_html(path: &Path, cancel: Option<&CancellationToken>) -> Result<String> {
    if is_epub(path) {
        return load_epub_native_html(path, cancel);
    }
    ensure_not_cancelled(cancel, "before_native_html_read")?;
    let html = fs::read_to_string(path)
        .with_context(|| format!("Failed to read HTML source at {}", path.display()))?;
    ensure_not_cancelled(cancel, "after_native_html_read")?;
    if html.trim().is_empty() {
        Ok("<p>No structured HTML content found in this file.</p>".to_string())
    } else {
        Ok(html)
    }
}

fn load_epub_native_html(path: &Path, cancel: Option<&CancellationToken>) -> Result<String> {
    ensure_not_cancelled(cancel, "before_epub_native_html_open")?;
    let mut doc =
        EpubDoc::new(path).with_context(|| format!("Failed to open EPUB at {}", path.display()))?;
    let mut style_blocks = Vec::new();
    let mut css_resources: Vec<(String, String)> = doc
        .resources
        .iter()
        .map(|(id, item)| (id.clone(), item.mime.clone()))
        .filter(|(_, mime)| mime.eq_ignore_ascii_case("text/css"))
        .collect();
    css_resources.sort_by(|a, b| a.0.cmp(&b.0));
    for (id, _) in css_resources {
        if let Some((bytes, _)) = doc.get_resource(&id)
            && let Ok(css) = String::from_utf8(bytes)
        {
            let trimmed = css.trim();
            if !trimmed.is_empty() {
                style_blocks.push(trimmed.to_string());
            }
        }
    }
    let mut sections = Vec::new();
    let mut chapter_idx = 0usize;
    loop {
        ensure_not_cancelled(cancel, "during_epub_native_html_loop")?;
        let Some((chapter_html, _mime)) = doc.get_current_str() else {
            break;
        };
        let trimmed = chapter_html.trim();
        if !trimmed.is_empty() {
            sections.push(format!(
                "<section data-ll-epub-chapter=\"{chapter_idx}\">{trimmed}</section>"
            ));
            chapter_idx = chapter_idx.saturating_add(1);
        }
        if !doc.go_next() {
            break;
        }
    }
    if sections.is_empty() {
        Ok("<p>No structured HTML content found in this EPUB.</p>".to_string())
    } else {
        let styles = if style_blocks.is_empty() {
            String::new()
        } else {
            format!("<style>{}</style>\n", style_blocks.join("\n\n"))
        };
        Ok(format!("{styles}{}", sections.join("\n")))
    }
}

pub(super) fn normalize_pdf_text_for_reader(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut paragraph = String::new();

    for line in normalized.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush_pdf_paragraph(&mut out, &mut paragraph);
            continue;
        }

        if paragraph.is_empty() {
            paragraph.push_str(trimmed);
            continue;
        }

        if paragraph.ends_with('-')
            && trimmed
                .chars()
                .next()
                .map(|c| c.is_ascii_lowercase())
                .unwrap_or(false)
        {
            paragraph.pop();
            paragraph.push_str(trimmed);
        } else {
            paragraph.push(' ');
            paragraph.push_str(trimmed);
        }
    }

    flush_pdf_paragraph(&mut out, &mut paragraph);
    out.trim().to_string()
}

fn flush_pdf_paragraph(out: &mut String, paragraph: &mut String) {
    if paragraph.trim().is_empty() {
        paragraph.clear();
        return;
    }
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(paragraph.trim());
    paragraph.clear();
}

fn pdf_extraction_mode_label(
    report: Option<&crate::quack_check::report::JobReport>,
) -> &'static str {
    let Some(report) = report else {
        return "unknown";
    };
    match (report.decision.tier.clone(), report.decision.do_ocr) {
        (crate::quack_check::policy::QualityTier::HighText, false) => "embedded_text",
        (crate::quack_check::policy::QualityTier::MixedText, false) => "mixed_embedded_text",
        (crate::quack_check::policy::QualityTier::MixedText, true) => "mixed_text_with_ocr",
        (crate::quack_check::policy::QualityTier::Scan, true) => "ocr_scan",
        (crate::quack_check::policy::QualityTier::Scan, false) => "scan_without_ocr",
        (crate::quack_check::policy::QualityTier::HighText, true) => "high_text_with_ocr",
    }
}

fn derive_pdf_reading_order_mode(pdf_geometry_mode: PdfGeometryMode, ocr_enabled: bool) -> String {
    match (pdf_geometry_mode, ocr_enabled) {
        (PdfGeometryMode::HighTextTrust, false) => "embedded_text_order".to_string(),
        (PdfGeometryMode::HighTextTrust, true) => "embedded_text_order_with_ocr".to_string(),
        (PdfGeometryMode::MixedTextTrust, false) => "normalized_extracted_order".to_string(),
        (PdfGeometryMode::MixedTextTrust, true) => {
            "normalized_extracted_order_with_ocr".to_string()
        }
        (PdfGeometryMode::OcrRequired, _) => "ocr_text_order".to_string(),
        (PdfGeometryMode::RenderOnlyNoSync, _) => "render_only".to_string(),
    }
}

fn derive_pdf_fallback_strategies(
    report: Option<&crate::quack_check::report::JobReport>,
    pdf_geometry_mode: PdfGeometryMode,
    pdf_sync_strategy: PdfSyncStrategy,
) -> Vec<String> {
    let mut strategies = Vec::new();
    if !matches!(pdf_geometry_mode, PdfGeometryMode::HighTextTrust) {
        strategies.push("low_quality_extraction".to_string());
    }
    if matches!(
        pdf_sync_strategy,
        PdfSyncStrategy::ParagraphFallback | PdfSyncStrategy::RenderOnly
    ) {
        strategies.push("sentence_sync_degraded".to_string());
    }
    if report.map(|value| value.decision.do_ocr).unwrap_or(false) {
        strategies.push("ocr_enabled".to_string());
    }
    if report
        .map(|value| value.chunk_reports.len() > 1)
        .unwrap_or(false)
    {
        strategies.push("multi_chunk_page_ranges".to_string());
    }
    let has_native_fallback = report
        .map(|value| {
            value.chunk_reports.iter().any(|chunk| {
                chunk.warnings.iter().any(|warning| {
                    warning.contains("fell back to docling")
                        || warning.contains("native_text failed")
                })
            })
        })
        .unwrap_or(false);
    if has_native_fallback {
        strategies.push("native_text_to_docling_fallback".to_string());
    }
    strategies.sort();
    strategies.dedup();
    strategies
}

fn classify_pdf_runtime(
    report: Option<&crate::quack_check::report::JobReport>,
    transcript_text: &str,
    _markdown: &str,
) -> Option<PdfClassificationSummary> {
    let report = report?;
    let sample = &report.sample;
    let feature_summary = PdfProbeFeatureSummary {
        sampled_pages: sample.sampled_pages,
        text_page_ratio: sample.text_page_ratio,
        empty_text_page_ratio: sample.empty_text_page_ratio,
        sparse_text_page_ratio: sample.sparse_text_page_ratio,
        noisy_text_page_ratio: sample.noisy_text_page_ratio,
        repeated_header_ratio: sample.repeated_header_ratio,
        repeated_footer_ratio: sample.repeated_footer_ratio,
        image_page_ratio: sample.image_page_ratio,
        mixed_text_image_page_ratio: sample.mixed_text_image_page_ratio,
        full_page_raster_page_ratio: sample.full_page_raster_page_ratio,
        hidden_text_layer_page_ratio: sample.hidden_text_layer_page_ratio,
        invisible_text_layer_page_ratio: sample.invisible_text_layer_page_ratio,
        duplicate_text_page_ratio: sample.duplicate_text_page_ratio,
        stacked_duplicate_text_page_ratio: sample.stacked_duplicate_text_page_ratio,
        avg_chars_per_page: sample.avg_chars_per_page,
        garbage_ratio: sample.garbage_ratio,
        whitespace_ratio: sample.whitespace_ratio,
    };
    let page_classes: Vec<PdfPageClassificationSummary> =
        sample.pages.iter().map(classify_pdf_sample_page).collect();
    let trust_diagnostics = derive_pdf_trust_diagnostics(sample);

    let clean_count = count_page_class(&page_classes, PdfPageClass::EmbeddedClean);
    let noisy_count = count_page_class(&page_classes, PdfPageClass::EmbeddedNoisy);
    let sparse_count = count_page_class(&page_classes, PdfPageClass::EmbeddedSparse);
    let image_only_count = count_page_class(&page_classes, PdfPageClass::ImageOnlyNoText);
    let hidden_overlay_count = count_page_class(&page_classes, PdfPageClass::HiddenOcrOverlay);
    let layout_hostile_count = count_page_class(&page_classes, PdfPageClass::LayoutHostile);
    let weak_scan_count = count_page_class(&page_classes, PdfPageClass::ScanWithWeakOcr);
    let sampled = page_classes.len().max(1) as f32;
    let transcript_chars = transcript_text.trim().chars().count() as f32;
    let transcript_chars_per_page = transcript_chars / report.input.page_count.max(1) as f32;
    let scanish_ratio =
        (image_only_count + hidden_overlay_count + weak_scan_count) as f32 / sampled;
    let clean_ratio = clean_count as f32 / sampled;
    let sparse_ratio = sparse_count as f32 / sampled;
    let hostile_ratio = (noisy_count + layout_hostile_count) as f32 / sampled;
    let mixed_image_ratio = feature_summary.mixed_text_image_page_ratio;
    let full_page_raster_ratio = feature_summary.full_page_raster_page_ratio;
    let hidden_overlay_signal = feature_summary.hidden_text_layer_page_ratio.max(
        if trust_diagnostics.hidden_text_layer_suspected {
            0.6
        } else {
            0.0
        },
    );
    let invisible_text_signal = feature_summary.invisible_text_layer_page_ratio.max(
        if trust_diagnostics.invisible_text_suspected {
            0.6
        } else {
            0.0
        },
    );
    let stacked_duplicate_signal = feature_summary.stacked_duplicate_text_page_ratio.max(
        if trust_diagnostics.stacked_duplicate_text_suspected {
            0.5
        } else {
            0.0
        },
    );

    let (document_class, confidence, mut reasons) = if transcript_text.trim().is_empty() {
        (
            PdfDocumentClass::ImageOnlyNoText,
            0.98,
            vec![
                "transcript_text_empty".to_string(),
                "sampled_pages_have_no_usable_text".to_string(),
            ],
        )
    } else if hidden_overlay_signal >= 0.40
        || invisible_text_signal >= 0.35
        || stacked_duplicate_signal >= 0.30
        || (full_page_raster_ratio >= 0.35 && hidden_overlay_count as f32 / sampled >= 0.25)
    {
        (
            PdfDocumentClass::HiddenOcrOverlay,
            0.82,
            vec![
                "many_pages_have_sparse_overlay_like_text".to_string(),
                "raster_coverage_and_hidden_text_signals_align".to_string(),
            ],
        )
    } else if scanish_ratio >= 0.65 || full_page_raster_ratio >= 0.55 {
        let class = if transcript_chars_per_page >= 450.0 && sample.garbage_ratio <= 0.03 {
            PdfDocumentClass::ScanWithGoodOcr
        } else {
            PdfDocumentClass::ScanWithWeakOcr
        };
        (
            class,
            0.84,
            vec![
                "most_sampled_pages_lack_trustworthy_embedded_text".to_string(),
                format!("ocr_enabled={}", report.decision.do_ocr),
            ],
        )
    } else if clean_ratio >= 0.70
        && hostile_ratio <= 0.15
        && sparse_ratio <= 0.20
        && trust_diagnostics.block_coherence >= 0.72
        && trust_diagnostics.coordinate_sanity >= 0.65
        && trust_diagnostics.reading_order_stability >= 0.65
    {
        (
            PdfDocumentClass::EmbeddedClean,
            0.88,
            vec![
                "sampled_pages_show_dense_low-garbage_text".to_string(),
                "trust_diagnostics_support_exact_sync".to_string(),
            ],
        )
    } else if mixed_image_ratio >= 0.30 && clean_ratio >= 0.20 {
        (
            PdfDocumentClass::HybridMixedDocument,
            0.8,
            vec![
                "sampled_pages_mix_image_heavy_and_embedded_text_signals".to_string(),
                "borderline_pdf_kept_in_explicit_mixed_class".to_string(),
            ],
        )
    } else if sparse_ratio >= 0.50 {
        (
            PdfDocumentClass::EmbeddedSparse,
            0.73,
            vec!["many_sampled_pages_have_only_sparse_text".to_string()],
        )
    } else if layout_hostile_count as f32 / sampled >= 0.35
        || feature_summary.repeated_header_ratio >= 0.50
        || feature_summary.repeated_footer_ratio >= 0.50
    {
        (
            PdfDocumentClass::LayoutHostileDocument,
            0.71,
            vec![
                "layout_signals_suggest_unstable_reading_order".to_string(),
                "header_footer_repetition_or_short_line_density_detected".to_string(),
            ],
        )
    } else if distinct_page_class_kinds(&page_classes) >= 3 {
        (
            PdfDocumentClass::HybridMixedDocument,
            0.76,
            vec!["sampled_pages_span_multiple_quality_classes".to_string()],
        )
    } else {
        (
            PdfDocumentClass::EmbeddedNoisy,
            0.68,
            vec![
                "embedded_text_exists_but_quality_is_not_clean".to_string(),
                "paragraph_level_fallback_recommended".to_string(),
            ],
        )
    };

    reasons.push(format!("sampled_page_count={}", page_classes.len()));
    reasons.push(format!(
        "class_distribution={}",
        describe_page_distribution(&page_classes)
    ));
    reasons.extend(trust_diagnostics.rationale.iter().take(3).cloned());
    reasons.sort();
    reasons.dedup();

    let ocr_recommendation = match document_class {
        PdfDocumentClass::EmbeddedClean => PdfOcrRecommendation::NotNeeded,
        PdfDocumentClass::EmbeddedNoisy | PdfDocumentClass::EmbeddedSparse => {
            if trust_diagnostics.coordinate_sanity < 0.48 || trust_diagnostics.block_coherence < 0.5
            {
                PdfOcrRecommendation::GeometryOnly
            } else {
                PdfOcrRecommendation::NotNeeded
            }
        }
        PdfDocumentClass::HiddenOcrOverlay => PdfOcrRecommendation::GeometryOnly,
        PdfDocumentClass::HybridMixedDocument | PdfDocumentClass::LayoutHostileDocument => {
            if transcript_chars_per_page < 180.0 {
                PdfOcrRecommendation::RequiredForText
            } else {
                PdfOcrRecommendation::GeometryOnly
            }
        }
        PdfDocumentClass::ScanWithGoodOcr | PdfDocumentClass::ScanWithWeakOcr => {
            PdfOcrRecommendation::RequiredForText
        }
        PdfDocumentClass::ImageOnlyNoText => {
            if report.decision.do_ocr && transcript_text.trim().is_empty() {
                PdfOcrRecommendation::UnlikelyToHelp
            } else {
                PdfOcrRecommendation::RequiredForText
            }
        }
    };
    let ocr_replace_threshold_met = trust_diagnostics.ocr_replace_confidence >= 0.74;
    let ocr_augment_threshold_met = trust_diagnostics.ocr_augment_confidence >= 0.58;
    if matches!(
        ocr_recommendation,
        PdfOcrRecommendation::RequiredForText | PdfOcrRecommendation::GeometryOnly
    ) {
        reasons.push(format!(
            "ocr_replace_threshold_met={ocr_replace_threshold_met}"
        ));
        reasons.push(format!(
            "ocr_augment_threshold_met={ocr_augment_threshold_met}"
        ));
    }

    Some(PdfClassificationSummary {
        document_class,
        confidence,
        ocr_recommendation,
        reasons,
        feature_summary,
        trust_diagnostics,
        class_distribution: page_class_distribution(&page_classes),
        page_classes,
    })
}

fn classify_pdf_sample_page(
    page: &crate::quack_check::probe::ProbePageStats,
) -> PdfPageClassificationSummary {
    let features = PdfProbePageSummary {
        page_index: page.page_index,
        char_count: page.char_count,
        token_count: page.token_count,
        line_count: page.line_count,
        whitespace_ratio: page.whitespace_ratio,
        garbage_ratio: page.garbage_ratio,
        punctuation_ratio: page.punctuation_ratio,
        digit_ratio: page.digit_ratio,
        non_latin_ratio: page.non_latin_ratio,
        alpha_char_ratio: page.alpha_char_ratio,
        uppercase_char_ratio: page.uppercase_char_ratio,
        alpha_token_ratio: page.alpha_token_ratio,
        avg_token_length: page.avg_token_length,
        short_line_ratio: page.short_line_ratio,
        repeated_line_ratio: page.repeated_line_ratio,
        hyphenated_line_ratio: page.hyphenated_line_ratio,
        image_object_count: page.image_object_count,
        image_coverage_ratio: page.image_coverage_ratio,
        duplicate_text_ratio: page.duplicate_text_ratio,
        block_coherence: page.block_coherence,
        coordinate_sanity: page.coordinate_sanity,
        reading_order_stability: page.reading_order_stability,
        hidden_text_layer_suspected: page.hidden_text_layer_suspected,
        invisible_text_suspected: page.invisible_text_suspected,
        duplicate_text_suspected: page.duplicate_text_suspected,
        stacked_duplicate_text_suspected: page.stacked_duplicate_text_suspected,
        mixed_text_image_suspected: page.mixed_text_image_suspected,
        full_page_raster_suspected: page.full_page_raster_suspected,
        first_line: page.first_line.clone(),
        last_line: page.last_line.clone(),
    };

    let avg_line_length = if page.line_count == 0 {
        0.0
    } else {
        page.char_count as f32 / page.line_count as f32
    };
    let looks_corrupt = page.garbage_ratio >= 0.04
        || page.non_latin_ratio >= 0.35
        || page.alpha_token_ratio <= 0.38
        || page.avg_token_length >= 14.0;
    let looks_layout_hostile = (page.short_line_ratio >= 0.65 && page.line_count >= 8)
        || page.repeated_line_ratio >= 0.25
        || page.hyphenated_line_ratio >= 0.22
        || (avg_line_length <= 18.0 && page.line_count >= 10)
        || page.block_coherence <= 0.42
        || page.reading_order_stability <= 0.4;
    let looks_hidden_overlay = page.hidden_text_layer_suspected
        || page.invisible_text_suspected
        || page.stacked_duplicate_text_suspected
        || (page.char_count <= 80
            && page.token_count <= 18
            && page.alpha_token_ratio >= 0.55
            && page.short_line_ratio <= 0.60);
    let looks_scan = page.full_page_raster_suspected
        || (page.image_coverage_ratio >= 0.82 && page.char_count <= 140);

    let (class, confidence, reasons) = if page.char_count == 0 {
        (
            PdfPageClass::ImageOnlyNoText,
            0.99,
            vec!["no_extracted_text_detected".to_string()],
        )
    } else if looks_hidden_overlay {
        let mut reasons = vec![
            "very_sparse_text_layer_detected".to_string(),
            "overlay_like_alpha_token_mix".to_string(),
            format!("image_coverage_ratio={:.3}", page.image_coverage_ratio),
        ];
        if page.invisible_text_suspected {
            reasons.push("invisible_or_zero_opacity_text_suspected".to_string());
        }
        if page.stacked_duplicate_text_suspected {
            reasons.push("duplicated_text_stacked_over_image_content".to_string());
        }
        (PdfPageClass::HiddenOcrOverlay, 0.82, reasons)
    } else if looks_scan {
        (
            PdfPageClass::ScanWithWeakOcr,
            0.79,
            vec![
                "full_page_raster_or_high_image_coverage_detected".to_string(),
                "text_density_is_too_thin_for_embedded_sync".to_string(),
            ],
        )
    } else if looks_corrupt {
        (
            PdfPageClass::EmbeddedNoisy,
            0.81,
            vec![
                "garbled_or_non_coherent_text_ratio_high".to_string(),
                "token_continuity_is_unstable".to_string(),
            ],
        )
    } else if page.char_count < 120 || page.token_count < 25 {
        (
            PdfPageClass::EmbeddedSparse,
            0.74,
            vec!["sparse_text_density".to_string()],
        )
    } else if looks_layout_hostile {
        (
            PdfPageClass::LayoutHostile,
            0.76,
            vec![
                "short_line_density_suggests_layout_hostility".to_string(),
                "line_coherence_and_paragraph_reconstruction_are_weak".to_string(),
            ],
        )
    } else if page.char_count < 260 && (page.digit_ratio >= 0.12 || page.alpha_token_ratio <= 0.55)
    {
        (
            PdfPageClass::ScanWithWeakOcr,
            0.67,
            vec![
                "weak_ocr_like_text_density".to_string(),
                "token_quality_is_too_thin_for_exact_sync".to_string(),
            ],
        )
    } else if page.alpha_token_ratio >= 0.72
        && page.avg_token_length >= 3.0
        && page.avg_token_length <= 9.5
        && page.short_line_ratio <= 0.45
        && page.repeated_line_ratio <= 0.12
        && page.hyphenated_line_ratio <= 0.18
    {
        (
            PdfPageClass::EmbeddedClean,
            0.9,
            vec![
                "dense_low-garbage_embedded_text".to_string(),
                "token_and_line_coherence_support_exact_sync".to_string(),
            ],
        )
    } else {
        (
            PdfPageClass::EmbeddedNoisy,
            0.64,
            vec![
                "embedded_text_exists_but_trust_signals_are_mixed".to_string(),
                "prefer_degraded_sync".to_string(),
            ],
        )
    };

    PdfPageClassificationSummary {
        page_index: page.page_index,
        class,
        confidence,
        reasons,
        features,
    }
}

fn derive_pdf_trust_diagnostics(
    sample: &crate::quack_check::probe::ProbeSampleStats,
) -> PdfEmbeddedTextTrustDiagnostics {
    let page_count = sample.pages.len().max(1) as f32;
    let avg_block_coherence = sample
        .pages
        .iter()
        .map(|page| page.block_coherence)
        .sum::<f32>()
        / page_count;
    let avg_coordinate_sanity = sample
        .pages
        .iter()
        .map(|page| page.coordinate_sanity)
        .sum::<f32>()
        / page_count;
    let avg_reading_order_stability = sample
        .pages
        .iter()
        .map(|page| page.reading_order_stability)
        .sum::<f32>()
        / page_count;
    let duplicate_text_suppression_needed = sample.duplicate_text_page_ratio >= 0.22
        || sample
            .pages
            .iter()
            .any(|page| page.duplicate_text_suspected || page.duplicate_text_ratio >= 0.2);
    let hidden_text_layer_suspected = sample.hidden_text_layer_page_ratio >= 0.2
        || sample
            .pages
            .iter()
            .any(|page| page.hidden_text_layer_suspected);
    let invisible_text_suspected = sample.invisible_text_layer_page_ratio >= 0.15
        || sample
            .pages
            .iter()
            .any(|page| page.invisible_text_suspected);
    let stacked_duplicate_text_suspected = sample.stacked_duplicate_text_page_ratio >= 0.15
        || sample
            .pages
            .iter()
            .any(|page| page.stacked_duplicate_text_suspected);
    let ocr_replace_confidence = ((sample.full_page_raster_page_ratio * 0.35)
        + (sample.hidden_text_layer_page_ratio * 0.35)
        + (sample.invisible_text_layer_page_ratio * 0.2)
        + (sample.stacked_duplicate_text_page_ratio * 0.1))
        .clamp(0.0, 1.0);
    let ocr_augment_confidence = ((1.0 - avg_coordinate_sanity) * 0.35
        + (1.0 - avg_block_coherence) * 0.25
        + sample.mixed_text_image_page_ratio * 0.25
        + sample.duplicate_text_page_ratio * 0.15)
        .clamp(0.0, 1.0);
    let ocr_confidence_threshold_met =
        ocr_replace_confidence >= 0.74 || ocr_augment_confidence >= 0.58;
    let mut rationale = Vec::new();
    rationale.push(format!("block_coherence={avg_block_coherence:.3}"));
    rationale.push(format!("coordinate_sanity={avg_coordinate_sanity:.3}"));
    rationale.push(format!(
        "reading_order_stability={avg_reading_order_stability:.3}"
    ));
    if hidden_text_layer_suspected {
        rationale.push("hidden_text_layer_suspected=true".to_string());
    }
    if invisible_text_suspected {
        rationale.push("invisible_text_suspected=true".to_string());
    }
    if stacked_duplicate_text_suspected {
        rationale.push("stacked_duplicate_text_suspected=true".to_string());
    }
    if duplicate_text_suppression_needed {
        rationale.push("duplicate_text_suppression_needed=true".to_string());
    }
    if sample.full_page_raster_page_ratio >= 0.25 {
        rationale.push(format!(
            "full_page_raster_ratio={:.3}",
            sample.full_page_raster_page_ratio
        ));
    }
    if sample.mixed_text_image_page_ratio >= 0.2 {
        rationale.push(format!(
            "mixed_text_image_ratio={:.3}",
            sample.mixed_text_image_page_ratio
        ));
    }
    rationale.push(format!(
        "ocr_replace_confidence={ocr_replace_confidence:.3}"
    ));
    rationale.push(format!(
        "ocr_augment_confidence={ocr_augment_confidence:.3}"
    ));
    PdfEmbeddedTextTrustDiagnostics {
        block_coherence: avg_block_coherence,
        coordinate_sanity: avg_coordinate_sanity,
        reading_order_stability: avg_reading_order_stability,
        duplicate_text_suppression_needed,
        hidden_text_layer_suspected,
        invisible_text_suspected,
        stacked_duplicate_text_suspected,
        full_page_raster_ratio: sample.full_page_raster_page_ratio,
        mixed_text_image_ratio: sample.mixed_text_image_page_ratio,
        ocr_replace_confidence,
        ocr_augment_confidence,
        ocr_confidence_threshold_met,
        rationale,
    }
}

fn count_page_class(pages: &[PdfPageClassificationSummary], class: PdfPageClass) -> usize {
    pages.iter().filter(|page| page.class == class).count()
}

fn distinct_page_class_kinds(pages: &[PdfPageClassificationSummary]) -> usize {
    let mut seen = Vec::new();
    for page in pages {
        if !seen.contains(&page.class) {
            seen.push(page.class);
        }
    }
    seen.len()
}

fn page_class_distribution(pages: &[PdfPageClassificationSummary]) -> Vec<PdfPageClassCount> {
    let order = [
        PdfPageClass::EmbeddedClean,
        PdfPageClass::EmbeddedNoisy,
        PdfPageClass::EmbeddedSparse,
        PdfPageClass::HiddenOcrOverlay,
        PdfPageClass::ScanWithWeakOcr,
        PdfPageClass::ImageOnlyNoText,
        PdfPageClass::LayoutHostile,
    ];
    order
        .into_iter()
        .filter_map(|class| {
            let count = count_page_class(pages, class) as u32;
            (count > 0).then_some(PdfPageClassCount { class, count })
        })
        .collect()
}

fn describe_page_distribution(pages: &[PdfPageClassificationSummary]) -> String {
    page_class_distribution(pages)
        .into_iter()
        .map(|entry| format!("{:?}:{}", entry.class, entry.count))
        .collect::<Vec<_>>()
        .join(",")
}

fn load_with_pandoc(
    path: &Path,
    target: &str,
    cancel: Option<&CancellationToken>,
) -> Result<String> {
    let start = Instant::now();
    ensure_not_cancelled(cancel, "before_pandoc")?;
    info!(
        path = %path.display(),
        target,
        "Converting source with pandoc"
    );

    let signature = source_signature(path, target)?;
    if let Some(cached) = try_read_pandoc_cache(path, target, &signature)? {
        info!(path = %path.display(), target, "Using cached pandoc conversion");
        return Ok(cached);
    }

    let filter_path = pandoc_filter_path()?;
    let output = Command::new("pandoc")
        .arg(path)
        .arg("--to")
        .arg(target)
        .arg("--wrap=none")
        .arg("--columns=100000")
        .arg("--strip-comments")
        .arg("--eol=lf")
        .args(if target == "plain" {
            vec![
                "--lua-filter".to_string(),
                filter_path.to_string_lossy().to_string(),
            ]
        } else {
            Vec::new()
        })
        .output()
        .with_context(|| format!("Failed to start pandoc for {}", path.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "pandoc conversion to {target} failed for {}: {}",
            path.display(),
            stderr.trim()
        );
    }

    let text = String::from_utf8(output.stdout)
        .with_context(|| format!("pandoc returned non-UTF8 text for {}", path.display()))?;
    ensure_not_cancelled(cancel, "after_pandoc")?;
    let text = if text.trim().is_empty() {
        "No textual content found in this file.".to_string()
    } else {
        text
    };

    if let Err(err) = write_pandoc_cache(path, target, &signature, &text) {
        warn!(path = %path.display(), "Failed to cache pandoc text output: {err}");
    }

    info!(
        path = %path.display(),
        target,
        total_chars = text.len(),
        elapsed_ms = start.elapsed().as_millis(),
        "Finished pandoc conversion"
    );
    Ok(text)
}

pub(super) fn record_markdown_availability(path: &Path, has_structured_markdown: bool) {
    let total = LOAD_COUNT_TOTAL.fetch_add(1, Ordering::Relaxed) + 1;
    let with_markdown = if has_structured_markdown {
        LOAD_COUNT_WITH_MARKDOWN.fetch_add(1, Ordering::Relaxed) + 1
    } else {
        LOAD_COUNT_WITH_MARKDOWN.load(Ordering::Relaxed)
    };
    if total % AVAILABILITY_LOG_EVERY == 0 {
        let ext = path
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or("<none>")
            .to_ascii_lowercase();
        let availability_pct = if total == 0 {
            0.0
        } else {
            (with_markdown as f64 / total as f64) * 100.0
        };
        info!(
            total_sources = total,
            sources_with_markdown = with_markdown,
            availability_pct = (availability_pct * 100.0).round() / 100.0,
            latest_source_ext = %ext,
            "Markdown availability summary"
        );
    }
}

pub(super) fn ensure_not_cancelled(
    cancel: Option<&CancellationToken>,
    stage: &'static str,
) -> Result<()> {
    if let Some(token) = cancel {
        token.check_cancelled(stage)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PandocCacheMeta {
    source_len: u64,
    source_modified_unix_secs: Option<u64>,
    #[serde(default)]
    pipeline_rev: String,
    #[serde(default)]
    target: String,
    #[serde(default)]
    filter_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PdfCacheMeta {
    source_len: u64,
    source_modified_unix_secs: Option<u64>,
    #[serde(default)]
    pipeline_rev: String,
    #[serde(default)]
    quack_config_sha256: String,
    #[serde(default)]
    quack_text_filename: String,
    #[serde(default)]
    pdf_geometry_mode: Option<PdfGeometryMode>,
    #[serde(default)]
    pdf_sync_strategy: Option<PdfSyncStrategy>,
    #[serde(default)]
    pdf_classification: Option<PdfClassificationSummary>,
    #[serde(default)]
    pdf_runtime_policy: Option<PdfRuntimePolicySummary>,
    #[serde(default)]
    pdf_classification_version: u32,
    #[serde(default)]
    extraction_mode: String,
    #[serde(default)]
    ocr_enabled: bool,
    #[serde(default)]
    page_count: u32,
    #[serde(default)]
    sampled_pages: u32,
    #[serde(default)]
    reading_order_mode: String,
    #[serde(default)]
    fallback_strategies: Vec<String>,
    #[serde(default)]
    chunk_ranges: Vec<PdfChunkRangeMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PdfChunkRangeMeta {
    start_page: u32,
    end_page: u32,
    #[serde(default)]
    warnings: Vec<String>,
    #[serde(default)]
    meta: serde_json::Value,
}

#[derive(Debug, Default, Deserialize)]
struct QuackCheckConfigToml {
    output: Option<QuackCheckOutputToml>,
}

#[derive(Debug, Default, Deserialize)]
struct QuackCheckOutputToml {
    text_filename: Option<String>,
}

fn source_signature(path: &Path, target: &str) -> Result<PandocCacheMeta> {
    let meta = fs::metadata(path)
        .with_context(|| format!("Failed to read source metadata for {}", path.display()))?;

    let modified = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());

    let filter_sha256 = if target == "plain" {
        let filter_path = pandoc_filter_path()?;
        hash_file(&filter_path)
            .with_context(|| format!("Failed to hash pandoc filter at {}", filter_path.display()))?
    } else {
        String::new()
    };

    Ok(PandocCacheMeta {
        source_len: meta.len(),
        source_modified_unix_secs: modified,
        pipeline_rev: PANDOC_PIPELINE_REV.to_string(),
        target: target.to_string(),
        filter_sha256,
    })
}

fn pdf_signature(path: &Path, config_sha256: &str, text_filename: &str) -> Result<PdfCacheMeta> {
    let meta = fs::metadata(path)
        .with_context(|| format!("Failed to read source metadata for {}", path.display()))?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());

    Ok(PdfCacheMeta {
        source_len: meta.len(),
        source_modified_unix_secs: modified,
        pipeline_rev: QUACK_CHECK_PIPELINE_REV.to_string(),
        quack_config_sha256: config_sha256.to_string(),
        quack_text_filename: text_filename.to_string(),
        pdf_geometry_mode: None,
        pdf_sync_strategy: None,
        pdf_classification: None,
        pdf_runtime_policy: None,
        pdf_classification_version: PDF_CLASSIFICATION_VERSION,
        extraction_mode: String::new(),
        ocr_enabled: false,
        page_count: 0,
        sampled_pages: 0,
        reading_order_mode: String::new(),
        fallback_strategies: Vec::new(),
        chunk_ranges: Vec::new(),
    })
}

fn pandoc_cache_paths(path: &Path, target: &str) -> (PathBuf, PathBuf) {
    let dir = hash_dir(path);
    let suffix = if target == "plain" {
        "plain"
    } else {
        "markdown"
    };
    (
        dir.join(format!("source-{suffix}.txt")),
        dir.join(format!("source-{suffix}.meta.toml")),
    )
}

fn pdf_cache_paths(path: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let dir = hash_dir(path).join("pdf");
    (
        dir.join("source-plain.txt"),
        dir.join("source-plain.meta.toml"),
        dir.join("quack-check-out"),
    )
}

fn try_read_pandoc_cache(
    path: &Path,
    target: &str,
    signature: &PandocCacheMeta,
) -> Result<Option<String>> {
    let (text_path, meta_path) = pandoc_cache_paths(path, target);

    let meta_str = match fs::read_to_string(&meta_path) {
        Ok(v) => v,
        Err(err) => {
            info!(
                path = %path.display(),
                target,
                meta_path = %meta_path.display(),
                "Pandoc cache miss: metadata unavailable ({err})"
            );
            return Ok(None);
        }
    };

    let cached_meta: PandocCacheMeta = match toml::from_str(&meta_str) {
        Ok(v) => v,
        Err(err) => {
            warn!(
                path = %path.display(),
                target,
                meta_path = %meta_path.display(),
                "Pandoc cache metadata corrupt; rebuilding artifacts: {err}"
            );
            return Ok(None);
        }
    };

    if cached_meta.source_len != signature.source_len
        || cached_meta.source_modified_unix_secs != signature.source_modified_unix_secs
        || cached_meta.pipeline_rev != signature.pipeline_rev
        || cached_meta.target != signature.target
        || cached_meta.filter_sha256 != signature.filter_sha256
    {
        info!(
            path = %path.display(),
            target,
            "Pandoc cache miss: signature changed, rebuilding artifacts"
        );
        return Ok(None);
    }

    let text = match fs::read_to_string(&text_path) {
        Ok(value) => value,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            info!(
                path = %path.display(),
                target,
                cache_text_path = %text_path.display(),
                "Pandoc cache metadata exists but text payload is missing; treating as cache miss"
            );
            return Ok(None);
        }
        Err(err) => {
            return Err(err).with_context(|| {
                format!(
                    "Failed to read pandoc cache text at {}",
                    text_path.display()
                )
            });
        }
    };
    Ok(Some(text))
}

struct PdfCachedLoad {
    text: String,
    pdf_geometry_mode: PdfGeometryMode,
    pdf_sync_strategy: PdfSyncStrategy,
    pdf_classification: Option<PdfClassificationSummary>,
    pdf_runtime_policy: Option<PdfRuntimePolicySummary>,
    extraction_mode: String,
    ocr_enabled: bool,
    page_count: u32,
    sampled_pages: u32,
    reading_order_mode: String,
    fallback_strategies: Vec<String>,
    chunk_ranges: Vec<PdfChunkRangeMeta>,
}

fn try_read_pdf_cache(path: &Path, signature: &PdfCacheMeta) -> Result<Option<PdfCachedLoad>> {
    let (text_path, meta_path, _) = pdf_cache_paths(path);
    let meta_str = match fs::read_to_string(&meta_path) {
        Ok(v) => v,
        Err(err) => {
            info!(
                path = %path.display(),
                meta_path = %meta_path.display(),
                "PDF transcript cache miss: metadata unavailable ({err})"
            );
            return Ok(None);
        }
    };

    let cached_meta: PdfCacheMeta = match toml::from_str(&meta_str) {
        Ok(v) => v,
        Err(err) => {
            warn!(
                path = %path.display(),
                meta_path = %meta_path.display(),
                "PDF transcript cache metadata corrupt; rebuilding artifacts: {err}"
            );
            return Ok(None);
        }
    };

    if cached_meta.source_len != signature.source_len
        || cached_meta.source_modified_unix_secs != signature.source_modified_unix_secs
        || cached_meta.pipeline_rev != signature.pipeline_rev
        || cached_meta.quack_config_sha256 != signature.quack_config_sha256
        || cached_meta.quack_text_filename != signature.quack_text_filename
        || cached_meta.pdf_classification_version != PDF_CLASSIFICATION_VERSION
    {
        info!(
            path = %path.display(),
            cached_classification_version = cached_meta.pdf_classification_version,
            required_classification_version = PDF_CLASSIFICATION_VERSION,
            "PDF transcript cache miss: signature or classification version changed, rebuilding artifacts"
        );
        return Ok(None);
    }

    let text = match fs::read_to_string(&text_path) {
        Ok(value) => value,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            info!(
                path = %path.display(),
                cache_text_path = %text_path.display(),
                "PDF transcript cache metadata exists but text payload is missing; rebuilding artifacts"
            );
            return Ok(None);
        }
        Err(err) => {
            return Err(err).with_context(|| {
                format!(
                    "Failed to read PDF transcript cache text at {}",
                    text_path.display()
                )
            });
        }
    };
    info!(
        path = %path.display(),
        cache_text_path = %text_path.display(),
        meta_path = %meta_path.display(),
        total_chars = text.len(),
        pdf_geometry_mode = ?cached_meta.pdf_geometry_mode,
        pdf_sync_strategy = ?cached_meta.pdf_sync_strategy,
        pdf_document_class = ?cached_meta
            .pdf_classification
            .as_ref()
            .map(|value| value.document_class),
        pdf_highlight_policy = ?cached_meta
            .pdf_runtime_policy
            .as_ref()
            .map(|value| value.sentence_highlight_policy),
        extraction_mode = cached_meta.extraction_mode,
        ocr_enabled = cached_meta.ocr_enabled,
        page_count = cached_meta.page_count,
        sampled_pages = cached_meta.sampled_pages,
        reading_order_mode = cached_meta.reading_order_mode,
        fallback_strategies = ?cached_meta.fallback_strategies,
        chunk_ranges = cached_meta.chunk_ranges.len(),
        "PDF transcript cache hit"
    );
    Ok(Some(PdfCachedLoad {
        text,
        pdf_geometry_mode: cached_meta
            .pdf_geometry_mode
            .unwrap_or(PdfGeometryMode::MixedTextTrust),
        pdf_sync_strategy: cached_meta
            .pdf_sync_strategy
            .unwrap_or(PdfSyncStrategy::ParagraphFallback),
        pdf_classification: cached_meta.pdf_classification,
        pdf_runtime_policy: cached_meta.pdf_runtime_policy,
        extraction_mode: cached_meta.extraction_mode,
        ocr_enabled: cached_meta.ocr_enabled,
        page_count: cached_meta.page_count,
        sampled_pages: cached_meta.sampled_pages,
        reading_order_mode: cached_meta.reading_order_mode,
        fallback_strategies: cached_meta.fallback_strategies,
        chunk_ranges: cached_meta.chunk_ranges,
    }))
}

fn write_pandoc_cache(
    path: &Path,
    target: &str,
    signature: &PandocCacheMeta,
    text: &str,
) -> Result<()> {
    let (text_path, meta_path) = pandoc_cache_paths(path, target);
    if let Some(parent) = text_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create cache dir {}", parent.display()))?;
    }

    fs::write(&text_path, text).with_context(|| {
        format!(
            "Failed to write pandoc cache text at {}",
            text_path.display()
        )
    })?;

    let meta_toml =
        toml::to_string(signature).context("Failed to serialize pandoc cache metadata")?;
    fs::write(&meta_path, meta_toml).with_context(|| {
        format!(
            "Failed to write pandoc cache metadata at {}",
            meta_path.display()
        )
    })?;

    Ok(())
}

fn write_pdf_cache(
    path: &Path,
    signature: &PdfCacheMeta,
    text: &str,
    pdf_geometry_mode: PdfGeometryMode,
    pdf_sync_strategy: PdfSyncStrategy,
    extraction_mode: &str,
    ocr_enabled: bool,
    report: Option<&crate::quack_check::report::JobReport>,
    pdf_classification: Option<&PdfClassificationSummary>,
    pdf_runtime_policy: Option<&PdfRuntimePolicySummary>,
) -> Result<()> {
    let (text_path, meta_path, _) = pdf_cache_paths(path);
    if let Some(parent) = text_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create cache dir {}", parent.display()))?;
    }

    fs::write(&text_path, text).with_context(|| {
        format!(
            "Failed to write PDF transcript cache text at {}",
            text_path.display()
        )
    })?;

    let mut signature = signature.clone();
    signature.pdf_geometry_mode = Some(pdf_geometry_mode);
    signature.pdf_sync_strategy = Some(pdf_sync_strategy);
    signature.pdf_classification = pdf_classification.cloned();
    signature.pdf_runtime_policy = pdf_runtime_policy.cloned();
    signature.pdf_classification_version = PDF_CLASSIFICATION_VERSION;
    signature.extraction_mode = extraction_mode.to_string();
    signature.ocr_enabled = ocr_enabled;
    signature.page_count = report.map(|value| value.input.page_count).unwrap_or(0);
    signature.sampled_pages = report.map(|value| value.sample.sampled_pages).unwrap_or(0);
    signature.reading_order_mode = derive_pdf_reading_order_mode(pdf_geometry_mode, ocr_enabled);
    signature.fallback_strategies =
        derive_pdf_fallback_strategies(report, pdf_geometry_mode, pdf_sync_strategy);
    signature.chunk_ranges = report
        .map(|value| {
            value
                .chunk_reports
                .iter()
                .map(|chunk| PdfChunkRangeMeta {
                    start_page: chunk.start_page,
                    end_page: chunk.end_page,
                    warnings: chunk.warnings.clone(),
                    meta: chunk.meta.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    let meta_toml =
        toml::to_string(&signature).context("Failed to serialize PDF transcript cache metadata")?;
    fs::write(&meta_path, meta_toml).with_context(|| {
        format!(
            "Failed to write PDF transcript cache metadata at {}",
            meta_path.display()
        )
    })?;

    info!(
        path = %path.display(),
        cache_text_path = %text_path.display(),
        meta_path = %meta_path.display(),
        total_chars = text.len(),
        ?pdf_geometry_mode,
        ?pdf_sync_strategy,
        pdf_document_class = ?signature
            .pdf_classification
            .as_ref()
            .map(|value| value.document_class),
        pdf_highlight_policy = ?signature
            .pdf_runtime_policy
            .as_ref()
            .map(|value| value.sentence_highlight_policy),
        extraction_mode,
        ocr_enabled,
        page_count = signature.page_count,
        sampled_pages = signature.sampled_pages,
        reading_order_mode = signature.reading_order_mode,
        fallback_strategies = ?signature.fallback_strategies,
        chunk_ranges = signature.chunk_ranges.len(),
        "Persisted PDF transcript cache artifacts"
    );

    Ok(())
}

fn derive_pdf_runtime_metadata(
    classification: Option<&PdfClassificationSummary>,
    report: Option<&crate::quack_check::report::JobReport>,
    transcript_text: &str,
    markdown: &str,
) -> (PdfGeometryMode, PdfSyncStrategy) {
    if transcript_text.trim().is_empty() {
        return (
            PdfGeometryMode::RenderOnlyNoSync,
            PdfSyncStrategy::RenderOnly,
        );
    }
    let Some(report) = report else {
        return if markdown.trim().is_empty() {
            (
                PdfGeometryMode::MixedTextTrust,
                PdfSyncStrategy::ParagraphFallback,
            )
        } else {
            (
                PdfGeometryMode::HighTextTrust,
                PdfSyncStrategy::SentenceSpans,
            )
        };
    };

    if let Some(classification) = classification {
        return match classification.document_class {
            PdfDocumentClass::EmbeddedClean => (
                PdfGeometryMode::HighTextTrust,
                PdfSyncStrategy::SentenceSpans,
            ),
            PdfDocumentClass::EmbeddedNoisy
            | PdfDocumentClass::EmbeddedSparse
            | PdfDocumentClass::HybridMixedDocument
            | PdfDocumentClass::LayoutHostileDocument => (
                PdfGeometryMode::MixedTextTrust,
                PdfSyncStrategy::ParagraphFallback,
            ),
            PdfDocumentClass::HiddenOcrOverlay
            | PdfDocumentClass::ScanWithGoodOcr
            | PdfDocumentClass::ScanWithWeakOcr => (
                PdfGeometryMode::OcrRequired,
                if transcript_text.trim().is_empty() {
                    PdfSyncStrategy::RenderOnly
                } else {
                    PdfSyncStrategy::ParagraphFallback
                },
            ),
            PdfDocumentClass::ImageOnlyNoText => (
                PdfGeometryMode::RenderOnlyNoSync,
                PdfSyncStrategy::RenderOnly,
            ),
        };
    }

    match report.decision.tier {
        crate::quack_check::policy::QualityTier::HighText => (
            PdfGeometryMode::HighTextTrust,
            PdfSyncStrategy::SentenceSpans,
        ),
        crate::quack_check::policy::QualityTier::MixedText => (
            PdfGeometryMode::MixedTextTrust,
            PdfSyncStrategy::ParagraphFallback,
        ),
        crate::quack_check::policy::QualityTier::Scan => (
            PdfGeometryMode::OcrRequired,
            if transcript_text.trim().is_empty() {
                PdfSyncStrategy::RenderOnly
            } else {
                PdfSyncStrategy::ParagraphFallback
            },
        ),
    }
}

fn derive_pdf_runtime_policy(
    classification: Option<&PdfClassificationSummary>,
    pdf_geometry_mode: PdfGeometryMode,
    pdf_sync_strategy: PdfSyncStrategy,
    transcript_text: &str,
) -> PdfRuntimePolicySummary {
    let has_text = !transcript_text.trim().is_empty();
    let document_class = classification.map(|value| value.document_class);
    let text_only_policy = if !has_text {
        match document_class {
            Some(PdfDocumentClass::ScanWithGoodOcr)
            | Some(PdfDocumentClass::ScanWithWeakOcr)
            | Some(PdfDocumentClass::HiddenOcrOverlay) => PdfTextOnlyPolicy::OcrRequired,
            _ => PdfTextOnlyPolicy::Disabled,
        }
    } else if matches!(
        pdf_sync_strategy,
        PdfSyncStrategy::SentenceSpans | PdfSyncStrategy::ParagraphFallback
    ) {
        PdfTextOnlyPolicy::FullText
    } else {
        PdfTextOnlyPolicy::LimitedText
    };
    let sentence_highlight_policy = match pdf_sync_strategy {
        PdfSyncStrategy::SentenceSpans => PdfSentenceHighlightPolicy::ExactSentence,
        PdfSyncStrategy::ParagraphFallback => PdfSentenceHighlightPolicy::ParagraphFallback,
        PdfSyncStrategy::RenderOnly => PdfSentenceHighlightPolicy::Disabled,
    };
    let search_policy = match text_only_policy {
        PdfTextOnlyPolicy::FullText => PdfSearchPolicy::FullText,
        PdfTextOnlyPolicy::LimitedText | PdfTextOnlyPolicy::OcrRequired => {
            PdfSearchPolicy::LimitedText
        }
        PdfTextOnlyPolicy::Disabled => PdfSearchPolicy::Disabled,
    };
    let bookmark_policy = if has_text {
        PdfBookmarkPolicy::CanonicalText
    } else {
        PdfBookmarkPolicy::PageOnly
    };

    let mut degraded_reasons = Vec::new();
    if !matches!(
        sentence_highlight_policy,
        PdfSentenceHighlightPolicy::ExactSentence
    ) {
        degraded_reasons.push("sentence_sync_not_exact".to_string());
    }
    if matches!(text_only_policy, PdfTextOnlyPolicy::OcrRequired) {
        degraded_reasons.push("ocr_needed_for_text_ownership".to_string());
    }
    if matches!(text_only_policy, PdfTextOnlyPolicy::Disabled) {
        degraded_reasons.push("no_usable_text_available".to_string());
    }
    if matches!(pdf_geometry_mode, PdfGeometryMode::MixedTextTrust) {
        degraded_reasons.push("embedded_text_trust_is_mixed".to_string());
    }
    if matches!(pdf_geometry_mode, PdfGeometryMode::OcrRequired) {
        degraded_reasons.push("native_pdf_sync_is_ocr_gated".to_string());
    }
    if matches!(pdf_geometry_mode, PdfGeometryMode::RenderOnlyNoSync) {
        degraded_reasons.push("render_only_mode".to_string());
    }
    if let Some(classification) = classification {
        degraded_reasons.extend(classification.reasons.iter().take(2).cloned());
    }
    degraded_reasons.sort();
    degraded_reasons.dedup();

    let explanation = match sentence_highlight_policy {
        PdfSentenceHighlightPolicy::ExactSentence => {
            "Exact sentence sync is enabled for this PDF.".to_string()
        }
        PdfSentenceHighlightPolicy::ParagraphFallback => {
            "This PDF is readable, but highlight sync is degraded to paragraph/page-level fallbacks."
                .to_string()
        }
        PdfSentenceHighlightPolicy::Disabled => {
            "This PDF is currently render-only for native view sync.".to_string()
        }
    };

    PdfRuntimePolicySummary {
        text_only_policy,
        sentence_highlight_policy,
        search_policy,
        bookmark_policy,
        tts_allowed: has_text,
        pretty_sync_enabled: !matches!(
            sentence_highlight_policy,
            PdfSentenceHighlightPolicy::Disabled
        ),
        exact_sentence_sync: matches!(
            sentence_highlight_policy,
            PdfSentenceHighlightPolicy::ExactSentence
        ),
        explanation,
        degraded_reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Debug, serde::Deserialize)]
    struct FixtureFile {
        fixtures: Vec<ClassificationFixture>,
    }

    #[derive(Debug, serde::Deserialize)]
    struct ClassificationFixture {
        id: String,
        label: String,
        document_class: PdfDocumentClass,
        ocr_recommendation: PdfOcrRecommendation,
        text_only_policy: PdfTextOnlyPolicy,
        sentence_highlight_policy: PdfSentenceHighlightPolicy,
        search_policy: PdfSearchPolicy,
        page_classes: Vec<PdfPageClass>,
        sample: crate::quack_check::probe::ProbeSampleStats,
        pages: Vec<crate::quack_check::probe::ProbePageStats>,
    }

    fn load_classification_fixtures() -> Vec<ClassificationFixture> {
        let raw = include_str!("../../tests/fixtures/pdf-classification-fixtures.toml");
        let parsed: FixtureFile = toml::from_str(raw).expect("fixture toml should parse");
        parsed.fixtures
    }

    fn sample_report() -> crate::quack_check::report::JobReport {
        crate::quack_check::report::JobReport {
            input: crate::quack_check::probe::ProbeInput {
                path: "/tmp/test.pdf".to_string(),
                file_bytes: 1024,
                page_count: 12,
            },
            sample: crate::quack_check::probe::ProbeSampleStats {
                sampled_pages: 6,
                avg_chars_per_page: 1200,
                garbage_ratio: 0.02,
                whitespace_ratio: 0.18,
                text_page_ratio: 1.0,
                empty_text_page_ratio: 0.0,
                sparse_text_page_ratio: 0.0,
                noisy_text_page_ratio: 0.0,
                repeated_header_ratio: 0.5,
                repeated_footer_ratio: 0.5,
                image_page_ratio: 0.0,
                mixed_text_image_page_ratio: 0.0,
                full_page_raster_page_ratio: 0.0,
                hidden_text_layer_page_ratio: 0.0,
                invisible_text_layer_page_ratio: 0.0,
                duplicate_text_page_ratio: 0.0,
                stacked_duplicate_text_page_ratio: 0.0,
                pages: vec![
                    crate::quack_check::probe::ProbePageStats {
                        page_index: 1,
                        char_count: 1400,
                        token_count: 240,
                        line_count: 32,
                        whitespace_ratio: 0.17,
                        garbage_ratio: 0.01,
                        punctuation_ratio: 0.09,
                        digit_ratio: 0.02,
                        non_latin_ratio: 0.0,
                        alpha_char_ratio: 0.72,
                        uppercase_char_ratio: 0.04,
                        alpha_token_ratio: 0.88,
                        avg_token_length: 5.2,
                        short_line_ratio: 0.22,
                        repeated_line_ratio: 0.03,
                        hyphenated_line_ratio: 0.04,
                        image_object_count: 0,
                        image_coverage_ratio: 0.0,
                        duplicate_text_ratio: 0.03,
                        block_coherence: 0.92,
                        coordinate_sanity: 0.9,
                        reading_order_stability: 0.88,
                        hidden_text_layer_suspected: false,
                        invisible_text_suspected: false,
                        duplicate_text_suspected: false,
                        stacked_duplicate_text_suspected: false,
                        mixed_text_image_suspected: false,
                        full_page_raster_suspected: false,
                        first_line: "chapter #".to_string(),
                        last_line: "publisher footer".to_string(),
                    },
                    crate::quack_check::probe::ProbePageStats {
                        page_index: 2,
                        char_count: 1380,
                        token_count: 230,
                        line_count: 31,
                        whitespace_ratio: 0.18,
                        garbage_ratio: 0.01,
                        punctuation_ratio: 0.08,
                        digit_ratio: 0.01,
                        non_latin_ratio: 0.0,
                        alpha_char_ratio: 0.73,
                        uppercase_char_ratio: 0.04,
                        alpha_token_ratio: 0.87,
                        avg_token_length: 5.1,
                        short_line_ratio: 0.21,
                        repeated_line_ratio: 0.03,
                        hyphenated_line_ratio: 0.04,
                        image_object_count: 0,
                        image_coverage_ratio: 0.0,
                        duplicate_text_ratio: 0.03,
                        block_coherence: 0.91,
                        coordinate_sanity: 0.89,
                        reading_order_stability: 0.87,
                        hidden_text_layer_suspected: false,
                        invisible_text_suspected: false,
                        duplicate_text_suspected: false,
                        stacked_duplicate_text_suspected: false,
                        mixed_text_image_suspected: false,
                        full_page_raster_suspected: false,
                        first_line: "chapter #".to_string(),
                        last_line: "publisher footer".to_string(),
                    },
                ],
            },
            decision: crate::quack_check::policy::PolicyDecision {
                tier: crate::quack_check::policy::QualityTier::MixedText,
                chosen_engine: "native_text".to_string(),
                do_ocr: true,
            },
            chunk_reports: vec![
                crate::quack_check::report::ChunkReport {
                    chunk_index: 0,
                    start_page: 1,
                    end_page: 6,
                    ok: true,
                    warnings: vec!["native_text failed; fell back to docling".to_string()],
                    meta: serde_json::json!({
                        "engine": "docling",
                        "use_page_range": true,
                        "applied_flags": ["generate_parsed_pages"]
                    }),
                },
                crate::quack_check::report::ChunkReport {
                    chunk_index: 1,
                    start_page: 7,
                    end_page: 12,
                    ok: true,
                    warnings: vec![],
                    meta: serde_json::json!({
                        "engine": "docling",
                        "use_page_range": false,
                        "applied_flags": []
                    }),
                },
            ],
        }
    }

    fn unique_pdf_path() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("lanternleaf_pdf_pipeline_{nanos}.pdf"))
    }

    #[test]
    fn derive_pdf_fallback_strategies_captures_quality_and_engine_fallbacks() {
        let report = sample_report();

        let strategies = derive_pdf_fallback_strategies(
            Some(&report),
            PdfGeometryMode::MixedTextTrust,
            PdfSyncStrategy::ParagraphFallback,
        );

        assert!(strategies.contains(&"low_quality_extraction".to_string()));
        assert!(strategies.contains(&"sentence_sync_degraded".to_string()));
        assert!(strategies.contains(&"ocr_enabled".to_string()));
        assert!(strategies.contains(&"multi_chunk_page_ranges".to_string()));
        assert!(strategies.contains(&"native_text_to_docling_fallback".to_string()));
    }

    #[test]
    fn derive_pdf_reading_order_mode_tracks_extraction_path() {
        assert_eq!(
            derive_pdf_reading_order_mode(PdfGeometryMode::HighTextTrust, false),
            "embedded_text_order"
        );
        assert_eq!(
            derive_pdf_reading_order_mode(PdfGeometryMode::MixedTextTrust, true),
            "normalized_extracted_order_with_ocr"
        );
        assert_eq!(
            derive_pdf_reading_order_mode(PdfGeometryMode::OcrRequired, true),
            "ocr_text_order"
        );
    }

    #[test]
    fn pdf_cache_roundtrip_preserves_chunk_page_ranges_and_meta() {
        let path = unique_pdf_path();
        fs::write(&path, b"pdf").expect("write source");
        let signature = pdf_signature(&path, "cfg", "transcript.txt").expect("signature");
        let report = sample_report();

        write_pdf_cache(
            &path,
            &signature,
            "Alpha. Beta.",
            PdfGeometryMode::MixedTextTrust,
            PdfSyncStrategy::ParagraphFallback,
            "mixed_text_with_ocr",
            true,
            Some(&report),
            classify_pdf_runtime(Some(&report), "Alpha. Beta.", "## pretty").as_ref(),
            None,
        )
        .expect("write pdf cache");

        let cached = try_read_pdf_cache(&path, &signature)
            .expect("read pdf cache")
            .expect("cached value");
        assert_eq!(cached.page_count, 12);
        assert_eq!(cached.sampled_pages, 6);
        assert_eq!(cached.chunk_ranges.len(), 2);
        assert_eq!(cached.chunk_ranges[0].start_page, 1);
        assert_eq!(cached.chunk_ranges[0].end_page, 6);
        assert_eq!(
            cached
                .pdf_classification
                .as_ref()
                .map(|value| value.document_class),
            Some(PdfDocumentClass::LayoutHostileDocument)
        );
        assert_eq!(
            cached.chunk_ranges[0].meta["applied_flags"],
            serde_json::json!(["generate_parsed_pages"])
        );

        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(hash_dir(&path));
    }

    #[test]
    fn classification_rollup_prefers_scan_when_sampled_pages_are_image_only() {
        let mut report = sample_report();
        report.sample.pages = vec![
            crate::quack_check::probe::ProbePageStats {
                page_index: 1,
                char_count: 0,
                token_count: 0,
                line_count: 0,
                whitespace_ratio: 0.0,
                garbage_ratio: 0.0,
                punctuation_ratio: 0.0,
                digit_ratio: 0.0,
                non_latin_ratio: 0.0,
                alpha_char_ratio: 0.0,
                uppercase_char_ratio: 0.0,
                alpha_token_ratio: 0.0,
                avg_token_length: 0.0,
                short_line_ratio: 0.0,
                repeated_line_ratio: 0.0,
                hyphenated_line_ratio: 0.0,
                image_object_count: 1,
                image_coverage_ratio: 0.96,
                duplicate_text_ratio: 0.0,
                block_coherence: 0.0,
                coordinate_sanity: 0.05,
                reading_order_stability: 0.0,
                hidden_text_layer_suspected: false,
                invisible_text_suspected: false,
                duplicate_text_suspected: false,
                stacked_duplicate_text_suspected: false,
                mixed_text_image_suspected: false,
                full_page_raster_suspected: true,
                first_line: String::new(),
                last_line: String::new(),
            },
            crate::quack_check::probe::ProbePageStats {
                page_index: 2,
                char_count: 12,
                token_count: 2,
                line_count: 1,
                whitespace_ratio: 0.1,
                garbage_ratio: 0.0,
                punctuation_ratio: 0.0,
                digit_ratio: 0.0,
                non_latin_ratio: 0.0,
                alpha_char_ratio: 0.75,
                uppercase_char_ratio: 0.02,
                alpha_token_ratio: 1.0,
                avg_token_length: 5.5,
                short_line_ratio: 0.0,
                repeated_line_ratio: 0.0,
                hyphenated_line_ratio: 0.0,
                image_object_count: 1,
                image_coverage_ratio: 0.9,
                duplicate_text_ratio: 0.0,
                block_coherence: 0.1,
                coordinate_sanity: 0.12,
                reading_order_stability: 0.1,
                hidden_text_layer_suspected: true,
                invisible_text_suspected: true,
                duplicate_text_suspected: false,
                stacked_duplicate_text_suspected: false,
                mixed_text_image_suspected: false,
                full_page_raster_suspected: true,
                first_line: String::new(),
                last_line: String::new(),
            },
        ];
        report.sample.empty_text_page_ratio = 0.5;
        report.sample.sparse_text_page_ratio = 0.5;
        report.sample.text_page_ratio = 0.5;
        report.sample.image_page_ratio = 1.0;
        report.sample.full_page_raster_page_ratio = 1.0;
        report.sample.hidden_text_layer_page_ratio = 0.5;
        report.sample.invisible_text_layer_page_ratio = 0.5;

        let classification =
            classify_pdf_runtime(Some(&report), "Recovered OCR text.", "").expect("classification");
        assert_eq!(
            classification.document_class,
            PdfDocumentClass::ScanWithWeakOcr
        );
        assert_eq!(
            classification.ocr_recommendation,
            PdfOcrRecommendation::RequiredForText
        );
    }

    #[test]
    fn classification_marks_short_repetitive_lines_as_layout_hostile() {
        let mut report = sample_report();
        report.sample.pages = vec![crate::quack_check::probe::ProbePageStats {
            page_index: 1,
            char_count: 420,
            token_count: 90,
            line_count: 28,
            whitespace_ratio: 0.16,
            garbage_ratio: 0.0,
            punctuation_ratio: 0.03,
            digit_ratio: 0.01,
            non_latin_ratio: 0.0,
            alpha_char_ratio: 0.74,
            uppercase_char_ratio: 0.06,
            alpha_token_ratio: 0.92,
            avg_token_length: 4.7,
            short_line_ratio: 0.82,
            repeated_line_ratio: 0.31,
            hyphenated_line_ratio: 0.28,
            image_object_count: 0,
            image_coverage_ratio: 0.0,
            duplicate_text_ratio: 0.31,
            block_coherence: 0.32,
            coordinate_sanity: 0.44,
            reading_order_stability: 0.28,
            hidden_text_layer_suspected: false,
            invisible_text_suspected: false,
            duplicate_text_suspected: true,
            stacked_duplicate_text_suspected: false,
            mixed_text_image_suspected: false,
            full_page_raster_suspected: false,
            first_line: "item".to_string(),
            last_line: "item".to_string(),
        }];

        let page = classify_pdf_sample_page(&report.sample.pages[0]);
        assert_eq!(page.class, PdfPageClass::LayoutHostile);

        let classification =
            classify_pdf_runtime(Some(&report), "Recovered text still exists.", "")
                .expect("classification");
        assert_eq!(
            classification.document_class,
            PdfDocumentClass::LayoutHostileDocument
        );
    }

    #[test]
    fn classification_prefers_hidden_overlay_when_raster_and_sparse_text_align() {
        let mut report = sample_report();
        report.sample.pages = vec![crate::quack_check::probe::ProbePageStats {
            page_index: 1,
            char_count: 42,
            token_count: 9,
            line_count: 2,
            whitespace_ratio: 0.12,
            garbage_ratio: 0.0,
            punctuation_ratio: 0.01,
            digit_ratio: 0.0,
            non_latin_ratio: 0.0,
            alpha_char_ratio: 0.78,
            uppercase_char_ratio: 0.02,
            alpha_token_ratio: 1.0,
            avg_token_length: 4.5,
            short_line_ratio: 0.0,
            repeated_line_ratio: 0.0,
            hyphenated_line_ratio: 0.0,
            image_object_count: 1,
            image_coverage_ratio: 0.91,
            duplicate_text_ratio: 0.0,
            block_coherence: 0.22,
            coordinate_sanity: 0.15,
            reading_order_stability: 0.14,
            hidden_text_layer_suspected: true,
            invisible_text_suspected: true,
            duplicate_text_suspected: false,
            stacked_duplicate_text_suspected: false,
            mixed_text_image_suspected: false,
            full_page_raster_suspected: true,
            first_line: "alpha beta".to_string(),
            last_line: "gamma".to_string(),
        }];
        report.sample.hidden_text_layer_page_ratio = 1.0;
        report.sample.image_page_ratio = 1.0;
        report.sample.full_page_raster_page_ratio = 1.0;

        let classification = classify_pdf_runtime(Some(&report), "Recovered text from OCR.", "")
            .expect("classification");
        assert_eq!(
            classification.document_class,
            PdfDocumentClass::HiddenOcrOverlay
        );
        assert!(classification.trust_diagnostics.hidden_text_layer_suspected);
    }

    #[test]
    fn classification_keeps_borderline_image_text_mix_in_hybrid_class() {
        let mut report = sample_report();
        report.sample.pages = vec![
            crate::quack_check::probe::ProbePageStats {
                page_index: 1,
                char_count: 1200,
                token_count: 220,
                line_count: 30,
                whitespace_ratio: 0.18,
                garbage_ratio: 0.01,
                punctuation_ratio: 0.06,
                digit_ratio: 0.01,
                non_latin_ratio: 0.0,
                alpha_char_ratio: 0.74,
                uppercase_char_ratio: 0.04,
                alpha_token_ratio: 0.9,
                avg_token_length: 5.3,
                short_line_ratio: 0.18,
                repeated_line_ratio: 0.02,
                hyphenated_line_ratio: 0.03,
                image_object_count: 2,
                image_coverage_ratio: 0.33,
                duplicate_text_ratio: 0.02,
                block_coherence: 0.88,
                coordinate_sanity: 0.8,
                reading_order_stability: 0.78,
                hidden_text_layer_suspected: false,
                invisible_text_suspected: false,
                duplicate_text_suspected: false,
                stacked_duplicate_text_suspected: false,
                mixed_text_image_suspected: true,
                full_page_raster_suspected: false,
                first_line: "chapter one".to_string(),
                last_line: "body".to_string(),
            },
            crate::quack_check::probe::ProbePageStats {
                page_index: 2,
                char_count: 0,
                token_count: 0,
                line_count: 0,
                whitespace_ratio: 0.0,
                garbage_ratio: 0.0,
                punctuation_ratio: 0.0,
                digit_ratio: 0.0,
                non_latin_ratio: 0.0,
                alpha_char_ratio: 0.0,
                uppercase_char_ratio: 0.0,
                alpha_token_ratio: 0.0,
                avg_token_length: 0.0,
                short_line_ratio: 0.0,
                repeated_line_ratio: 0.0,
                hyphenated_line_ratio: 0.0,
                image_object_count: 1,
                image_coverage_ratio: 0.95,
                duplicate_text_ratio: 0.0,
                block_coherence: 0.0,
                coordinate_sanity: 0.1,
                reading_order_stability: 0.0,
                hidden_text_layer_suspected: false,
                invisible_text_suspected: false,
                duplicate_text_suspected: false,
                stacked_duplicate_text_suspected: false,
                mixed_text_image_suspected: false,
                full_page_raster_suspected: true,
                first_line: String::new(),
                last_line: String::new(),
            },
        ];
        report.sample.image_page_ratio = 1.0;
        report.sample.mixed_text_image_page_ratio = 0.5;
        report.sample.full_page_raster_page_ratio = 0.5;
        report.sample.text_page_ratio = 0.5;
        report.sample.empty_text_page_ratio = 0.5;

        let classification =
            classify_pdf_runtime(Some(&report), "Text exists in some chapters.", "")
                .expect("classification");
        assert_eq!(
            classification.document_class,
            PdfDocumentClass::HybridMixedDocument
        );
        assert!(
            classification
                .reasons
                .iter()
                .any(|reason| reason.contains("borderline_pdf_kept_in_explicit_mixed_class"))
        );
    }

    #[test]
    fn trust_diagnostics_flag_duplicate_suppression_when_page_text_repeats() {
        let mut report = sample_report();
        report.sample.pages[0].duplicate_text_ratio = 0.34;
        report.sample.pages[0].duplicate_text_suspected = true;
        report.sample.pages[0].block_coherence = 0.38;
        report.sample.duplicate_text_page_ratio = 0.5;

        let classification = classify_pdf_runtime(Some(&report), "Repeated text still loads.", "")
            .expect("classification");
        assert!(
            classification
                .trust_diagnostics
                .duplicate_text_suppression_needed
        );
        assert!(
            classification
                .trust_diagnostics
                .rationale
                .iter()
                .any(|reason| reason.contains("duplicate_text_suppression_needed"))
        );
    }

    #[test]
    fn classification_fixture_matrix_matches_expected_contracts() {
        let fixtures = load_classification_fixtures();
        assert!(
            fixtures.len() >= 8,
            "expected a substantial classification fixture matrix"
        );

        for fixture in fixtures {
            let report = crate::quack_check::report::JobReport {
                input: crate::quack_check::probe::ProbeInput {
                    path: format!("/fixtures/{}.pdf", fixture.id),
                    file_bytes: 1024,
                    page_count: fixture.pages.len() as u32,
                },
                sample: crate::quack_check::probe::ProbeSampleStats {
                    pages: fixture.pages.clone(),
                    ..fixture.sample.clone()
                },
                decision: crate::quack_check::policy::PolicyDecision {
                    tier: crate::quack_check::policy::QualityTier::MixedText,
                    chosen_engine: "docling".to_string(),
                    do_ocr: !matches!(fixture.ocr_recommendation, PdfOcrRecommendation::NotNeeded),
                },
                chunk_reports: Vec::new(),
            };
            let transcript_text =
                if matches!(fixture.document_class, PdfDocumentClass::ImageOnlyNoText) {
                    ""
                } else {
                    "Fixture transcript text."
                };
            let classification = classify_pdf_runtime(Some(&report), transcript_text, "")
                .unwrap_or_else(|| {
                    panic!("classification should exist for fixture {}", fixture.id)
                });
            let policy = derive_pdf_runtime_policy(
                Some(&classification),
                derive_pdf_runtime_metadata(
                    Some(&classification),
                    Some(&report),
                    transcript_text,
                    "",
                )
                .0,
                derive_pdf_runtime_metadata(
                    Some(&classification),
                    Some(&report),
                    transcript_text,
                    "",
                )
                .1,
                transcript_text,
            );

            assert_eq!(
                classification.document_class, fixture.document_class,
                "document class mismatch for fixture {} ({})",
                fixture.id, fixture.label
            );
            assert_eq!(
                classification.ocr_recommendation, fixture.ocr_recommendation,
                "ocr recommendation mismatch for fixture {}",
                fixture.id
            );
            assert_eq!(
                policy.text_only_policy, fixture.text_only_policy,
                "text-only policy mismatch for fixture {}",
                fixture.id
            );
            assert_eq!(
                policy.sentence_highlight_policy, fixture.sentence_highlight_policy,
                "highlight policy mismatch for fixture {}",
                fixture.id
            );
            assert_eq!(
                policy.search_policy, fixture.search_policy,
                "search policy mismatch for fixture {}",
                fixture.id
            );
            let actual_page_classes = classification
                .page_classes
                .iter()
                .map(|page| page.class)
                .collect::<Vec<_>>();
            assert_eq!(
                actual_page_classes, fixture.page_classes,
                "page classes mismatch for fixture {}",
                fixture.id
            );
        }
    }

    #[test]
    fn runtime_policy_uses_exact_sync_for_clean_embedded_text() {
        let classification = PdfClassificationSummary {
            document_class: PdfDocumentClass::EmbeddedClean,
            confidence: 0.9,
            ocr_recommendation: PdfOcrRecommendation::NotNeeded,
            reasons: vec!["clean".to_string()],
            feature_summary: PdfProbeFeatureSummary {
                sampled_pages: 1,
                text_page_ratio: 1.0,
                empty_text_page_ratio: 0.0,
                sparse_text_page_ratio: 0.0,
                noisy_text_page_ratio: 0.0,
                repeated_header_ratio: 0.0,
                repeated_footer_ratio: 0.0,
                image_page_ratio: 0.0,
                mixed_text_image_page_ratio: 0.0,
                full_page_raster_page_ratio: 0.0,
                hidden_text_layer_page_ratio: 0.0,
                invisible_text_layer_page_ratio: 0.0,
                duplicate_text_page_ratio: 0.0,
                stacked_duplicate_text_page_ratio: 0.0,
                avg_chars_per_page: 1400,
                garbage_ratio: 0.01,
                whitespace_ratio: 0.18,
            },
            trust_diagnostics: PdfEmbeddedTextTrustDiagnostics {
                block_coherence: 0.92,
                coordinate_sanity: 0.91,
                reading_order_stability: 0.89,
                duplicate_text_suppression_needed: false,
                hidden_text_layer_suspected: false,
                invisible_text_suspected: false,
                stacked_duplicate_text_suspected: false,
                full_page_raster_ratio: 0.0,
                mixed_text_image_ratio: 0.0,
                ocr_replace_confidence: 0.0,
                ocr_augment_confidence: 0.0,
                ocr_confidence_threshold_met: false,
                rationale: vec!["clean".to_string()],
            },
            page_classes: Vec::new(),
            class_distribution: vec![PdfPageClassCount {
                class: PdfPageClass::EmbeddedClean,
                count: 1,
            }],
        };

        let policy = derive_pdf_runtime_policy(
            Some(&classification),
            PdfGeometryMode::HighTextTrust,
            PdfSyncStrategy::SentenceSpans,
            "Alpha. Beta.",
        );

        assert_eq!(policy.text_only_policy, PdfTextOnlyPolicy::FullText);
        assert_eq!(
            policy.sentence_highlight_policy,
            PdfSentenceHighlightPolicy::ExactSentence
        );
        assert_eq!(policy.search_policy, PdfSearchPolicy::FullText);
        assert!(policy.tts_allowed);
        assert!(policy.exact_sentence_sync);
    }

    #[test]
    fn runtime_policy_gates_text_ownership_when_ocr_is_required() {
        let classification = PdfClassificationSummary {
            document_class: PdfDocumentClass::ScanWithWeakOcr,
            confidence: 0.8,
            ocr_recommendation: PdfOcrRecommendation::RequiredForText,
            reasons: vec!["scan".to_string()],
            feature_summary: PdfProbeFeatureSummary {
                sampled_pages: 1,
                text_page_ratio: 0.2,
                empty_text_page_ratio: 0.8,
                sparse_text_page_ratio: 0.2,
                noisy_text_page_ratio: 0.0,
                repeated_header_ratio: 0.0,
                repeated_footer_ratio: 0.0,
                image_page_ratio: 1.0,
                mixed_text_image_page_ratio: 0.0,
                full_page_raster_page_ratio: 1.0,
                hidden_text_layer_page_ratio: 1.0,
                invisible_text_layer_page_ratio: 1.0,
                duplicate_text_page_ratio: 0.0,
                stacked_duplicate_text_page_ratio: 0.0,
                avg_chars_per_page: 40,
                garbage_ratio: 0.0,
                whitespace_ratio: 0.1,
            },
            trust_diagnostics: PdfEmbeddedTextTrustDiagnostics {
                block_coherence: 0.1,
                coordinate_sanity: 0.12,
                reading_order_stability: 0.08,
                duplicate_text_suppression_needed: false,
                hidden_text_layer_suspected: true,
                invisible_text_suspected: true,
                stacked_duplicate_text_suspected: false,
                full_page_raster_ratio: 1.0,
                mixed_text_image_ratio: 0.0,
                ocr_replace_confidence: 0.82,
                ocr_augment_confidence: 0.41,
                ocr_confidence_threshold_met: true,
                rationale: vec!["scan".to_string()],
            },
            page_classes: Vec::new(),
            class_distribution: vec![PdfPageClassCount {
                class: PdfPageClass::ScanWithWeakOcr,
                count: 1,
            }],
        };

        let policy = derive_pdf_runtime_policy(
            Some(&classification),
            PdfGeometryMode::OcrRequired,
            PdfSyncStrategy::RenderOnly,
            "",
        );

        assert_eq!(policy.text_only_policy, PdfTextOnlyPolicy::OcrRequired);
        assert_eq!(
            policy.sentence_highlight_policy,
            PdfSentenceHighlightPolicy::Disabled
        );
        assert_eq!(policy.bookmark_policy, PdfBookmarkPolicy::PageOnly);
        assert!(!policy.tts_allowed);
        assert!(!policy.pretty_sync_enabled);
    }
}

fn load_quack_check_report(job_dir: &Path) -> Result<crate::quack_check::report::JobReport> {
    let report_path = job_dir.join("final").join("report.json");
    let raw = fs::read_to_string(&report_path).with_context(|| {
        format!(
            "Failed to read quack-check report at {}",
            report_path.display()
        )
    })?;
    serde_json::from_str(&raw).with_context(|| {
        format!(
            "Failed to parse quack-check report JSON at {}",
            report_path.display()
        )
    })
}

fn hash_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path)
        .with_context(|| format!("Failed to read file for hashing: {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn pandoc_filter_path() -> Result<PathBuf> {
    let relative = PathBuf::from(PANDOC_FILTER_REL_PATH);
    if relative.exists() {
        return Ok(relative);
    }

    let rooted = project_root().join(PANDOC_FILTER_REL_PATH);
    if rooted.exists() {
        return Ok(rooted);
    }

    anyhow::bail!(
        "pandoc Lua filter not found at {} or {}",
        relative.display(),
        rooted.display()
    );
}

fn quack_check_config_path() -> Result<PathBuf> {
    if let Some(value) = std::env::var_os("QUACK_CHECK_CONFIG") {
        let candidate = PathBuf::from(value);
        if candidate.exists() {
            return Ok(candidate);
        }
        anyhow::bail!(
            "QUACK_CHECK_CONFIG is set but file does not exist: {}",
            candidate.display()
        );
    }

    let relative = PathBuf::from(QUACK_CHECK_CONFIG_REL_PATH);
    if relative.exists() {
        return Ok(relative);
    }

    let rooted = project_root().join(QUACK_CHECK_CONFIG_REL_PATH);
    if rooted.exists() {
        return Ok(rooted);
    }

    anyhow::bail!(
        "quack-check config not found at {} or {}",
        relative.display(),
        rooted.display()
    );
}

pub(super) fn project_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest_dir.ancestors() {
        let candidate = ancestor.to_path_buf();
        if candidate.join("conf").exists() {
            return candidate;
        }
    }
    manifest_dir
}

fn quack_check_text_filename(config_path: &Path) -> Result<String> {
    let raw = fs::read_to_string(config_path).with_context(|| {
        format!(
            "Failed to read quack-check config {}",
            config_path.display()
        )
    })?;
    let parsed: QuackCheckConfigToml = toml::from_str(&raw).with_context(|| {
        format!(
            "Invalid quack-check config TOML at {}",
            config_path.display()
        )
    })?;
    let name = parsed
        .output
        .and_then(|out| out.text_filename)
        .unwrap_or_else(|| QUACK_CHECK_TEXT_FILENAME_DEFAULT.to_string());
    let trimmed = name.trim();
    if trimmed.is_empty() {
        Ok(QUACK_CHECK_TEXT_FILENAME_DEFAULT.to_string())
    } else {
        Ok(trimmed.to_string())
    }
}
