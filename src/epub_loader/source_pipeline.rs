use super::{PdfGeometryMode, PdfSyncStrategy, SourceContent};
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
const QUACK_CHECK_PIPELINE_REV: &str = "quack-check-pdf-v2";
const QUACK_CHECK_TEXT_FILENAME_DEFAULT: &str = "transcript.txt";
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
            "Using cached quack-check PDF transcript"
        );
        return Ok(SourceContent {
            tts_text,
            reading_markdown: None,
            reading_html: None,
            has_structured_markdown: false,
            pdf_geometry_mode: Some(cached.pdf_geometry_mode),
            pdf_sync_strategy: Some(cached.pdf_sync_strategy),
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
    )?;
    info!(
        path = %path.display(),
        total_chars = tts_text.len(),
        markdown_chars = reading_markdown.as_ref().map(|v| v.len()).unwrap_or(0),
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
    let (pdf_geometry_mode, pdf_sync_strategy) =
        derive_pdf_runtime_metadata(report, transcript_text, markdown);
    SourceContent {
        tts_text,
        reading_html: None,
        has_structured_markdown: reading_markdown.is_some(),
        reading_markdown,
        pdf_geometry_mode: Some(pdf_geometry_mode),
        pdf_sync_strategy: Some(pdf_sync_strategy),
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

fn normalize_pdf_text_for_reader(input: &str) -> String {
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
    {
        info!(
            path = %path.display(),
            "PDF transcript cache miss: signature changed, rebuilding artifacts"
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
    Ok(Some(PdfCachedLoad {
        text,
        pdf_geometry_mode: cached_meta
            .pdf_geometry_mode
            .unwrap_or(PdfGeometryMode::MixedTextTrust),
        pdf_sync_strategy: cached_meta
            .pdf_sync_strategy
            .unwrap_or(PdfSyncStrategy::ParagraphFallback),
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
    let meta_toml =
        toml::to_string(&signature).context("Failed to serialize PDF transcript cache metadata")?;
    fs::write(&meta_path, meta_toml).with_context(|| {
        format!(
            "Failed to write PDF transcript cache metadata at {}",
            meta_path.display()
        )
    })?;

    Ok(())
}

fn derive_pdf_runtime_metadata(
    report: Option<&crate::quack_check::report::JobReport>,
    transcript_text: &str,
    markdown: &str,
) -> (PdfGeometryMode, PdfSyncStrategy) {
    if transcript_text.trim().is_empty() {
        return (PdfGeometryMode::RenderOnlyNoSync, PdfSyncStrategy::RenderOnly);
    }
    let Some(report) = report else {
        return if markdown.trim().is_empty() {
            (PdfGeometryMode::MixedTextTrust, PdfSyncStrategy::ParagraphFallback)
        } else {
            (PdfGeometryMode::HighTextTrust, PdfSyncStrategy::SentenceSpans)
        };
    };

    match report.decision.tier {
        crate::quack_check::policy::QualityTier::HighText => {
            (PdfGeometryMode::HighTextTrust, PdfSyncStrategy::SentenceSpans)
        }
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

fn load_quack_check_report(
    job_dir: &Path,
) -> Result<crate::quack_check::report::JobReport> {
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
