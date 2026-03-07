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
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use std::time::UNIX_EPOCH;
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
const PANDOC_FILTER_REL_PATH: &str = "conf/pandoc/strip-nontext.lua";
const PANDOC_PIPELINE_REV: &str = "pandoc-clean-v1";
const QUACK_CHECK_CONFIG_REL_PATH: &str = "conf/quack-check.toml";
const QUACK_CHECK_PIPELINE_REV: &str = "quack-check-pdf-v2";
const QUACK_CHECK_TEXT_FILENAME_DEFAULT: &str = "transcript.txt";
const AVAILABILITY_LOG_EVERY: u64 = 20;

static LOAD_COUNT_TOTAL: AtomicU64 = AtomicU64::new(0);
static LOAD_COUNT_WITH_MARKDOWN: AtomicU64 = AtomicU64::new(0);

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
    pub images: Vec<BookImage>,
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
        image_count = images.len(),
        elapsed_ms = start.elapsed().as_millis(),
        "Source load complete"
    );
    Ok(LoadedBook {
        tts_text: content.tts_text,
        reading_markdown: content.reading_markdown,
        reading_html: content.reading_html,
        has_structured_markdown: content.has_structured_markdown,
        images,
    })
}

#[derive(Debug, Clone)]
struct SourceContent {
    tts_text: String,
    reading_markdown: Option<String>,
    reading_html: Option<String>,
    has_structured_markdown: bool,
}

fn load_source_content(path: &Path, cancel: Option<&CancellationToken>) -> Result<SourceContent> {
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
        });
    }

    anyhow::bail!(
        "Unsupported source format for {}. Supported source types are .txt, .md, .markdown, .pdf, .html, .doc, .docx, and .epub.",
        path.display(),
    );
}

fn source_type_label(path: &Path) -> &'static str {
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

fn is_markdown(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase()),
        Some(ext) if ext == "md" || ext == "markdown"
    )
}

fn is_epub(path: &Path) -> bool {
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
        Some(ext)
            if ext == "doc" || ext == "docx"
    )
}

fn is_native_html_source(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase()),
        Some(ext) if ext == "epub" || ext == "html" || ext == "htm"
            || ext == "lltab"
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
        let tts_text = normalize_pdf_text_for_reader(&cached);
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
    let resolved = resolve_pdf_dual_view_content(&run.text, &run.markdown);
    let tts_text = resolved.tts_text;
    let reading_markdown = resolved.reading_markdown;

    write_pdf_cache(path, &signature, &tts_text)?;
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
    })
}

fn resolve_pdf_dual_view_content(transcript_text: &str, markdown: &str) -> SourceContent {
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
    SourceContent {
        tts_text,
        reading_html: None,
        has_structured_markdown: reading_markdown.is_some(),
        reading_markdown,
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
    // PDF text extraction often preserves physical line wraps. Unwrap lines inside
    // paragraphs so pagination/highlighting tracks prose flow instead of scan layout.
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

fn record_markdown_availability(path: &Path, has_structured_markdown: bool) {
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

fn ensure_not_cancelled(cancel: Option<&CancellationToken>, stage: &'static str) -> Result<()> {
    if let Some(token) = cancel {
        token.check_cancelled(stage)?;
    }
    Ok(())
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

fn try_read_pdf_cache(path: &Path, signature: &PdfCacheMeta) -> Result<Option<String>> {
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
    Ok(Some(text))
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

fn write_pdf_cache(path: &Path, signature: &PdfCacheMeta, text: &str) -> Result<()> {
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

    let meta_toml =
        toml::to_string(signature).context("Failed to serialize PDF transcript cache metadata")?;
    fs::write(&meta_path, meta_toml).with_context(|| {
        format!(
            "Failed to write PDF transcript cache metadata at {}",
            meta_path.display()
        )
    })?;

    Ok(())
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

fn project_root() -> PathBuf {
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
        let structured =
            resolve_pdf_dual_view_content("Line one.\nLine two.", "# Heading\n\nLine one.");
        assert!(structured.has_structured_markdown);
        assert!(structured.reading_markdown.is_some());
        assert!(structured.tts_text.contains("Line one."));

        let scan_fallback = resolve_pdf_dual_view_content("Scanned OCR text", "   ");
        assert!(!scan_fallback.has_structured_markdown);
        assert!(scan_fallback.reading_markdown.is_none());
        assert!(scan_fallback.tts_text.contains("Scanned OCR text"));
    }

    #[test]
    fn project_root_finds_workspace_conf_directory() {
        let root = project_root();
        assert!(
            root.join("conf").exists(),
            "expected conf directory at {}",
            root.display()
        );
    }
}
