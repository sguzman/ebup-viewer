//! Source loading utilities.
//!
//! The loader converts supported book formats to plain text and also extracts
//! image assets for rendering in the reading pane.

use crate::cache::hash_dir;
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
use std::time::UNIX_EPOCH;
use tracing::{debug, info, warn};

static RE_MARKDOWN_IMAGE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").expect("valid markdown image regex"));
const PANDOC_FILTER_REL_PATH: &str = "conf/pandoc/strip-nontext.lua";
const PANDOC_PIPELINE_REV: &str = "pandoc-clean-v1";
const QUACK_CHECK_CONFIG_REL_PATH: &str = "conf/quack-check.toml";
const QUACK_CHECK_PIPELINE_REV: &str = "quack-check-pdf-v2";
const QUACK_CHECK_TEXT_FILENAME_DEFAULT: &str = "transcript.txt";

#[derive(Debug, Clone)]
pub struct BookImage {
    pub path: PathBuf,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct LoadedBook {
    pub text: String,
    pub images: Vec<BookImage>,
}

/// Load a supported source file and return plain text plus extracted image paths.
pub fn load_book_content(path: &Path) -> Result<LoadedBook> {
    let text = load_source_text(path)?;
    let images = match collect_images(path) {
        Ok(images) => images,
        Err(err) => {
            warn!(path = %path.display(), "Image extraction failed: {err}");
            Vec::new()
        }
    };
    info!(
        path = %path.display(),
        image_count = images.len(),
        "Source load complete"
    );
    Ok(LoadedBook { text, images })
}

fn load_source_text(path: &Path) -> Result<String> {
    if is_text_file(path) {
        info!(path = %path.display(), "Loading plain text content");
        let data = fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let text = if data.trim().is_empty() {
            "No textual content found in this file.".to_string()
        } else {
            data
        };
        info!(
            total_chars = text.len(),
            "Finished loading plain text content"
        );
        return Ok(text);
    }

    if is_pdf(path) {
        return load_pdf_with_quack_check(path);
    }

    match load_with_pandoc(path) {
        Ok(text) => return Ok(text),
        Err(err) => {
            warn!(
                path = %path.display(),
                "Pandoc conversion failed; attempting format-specific fallback: {err}"
            );
        }
    }

    if is_markdown(path) {
        let data = fs::read_to_string(path)
            .with_context(|| format!("Failed to read markdown file at {}", path.display()))?;
        return Ok(data);
    }

    if !is_epub(path) {
        anyhow::bail!(
            "Unsupported source format for {}. Supported source types are .epub, .pdf, .md, .markdown, and .txt (other formats require successful pandoc conversion).",
            path.display(),
        );
    }

    info!(path = %path.display(), "Loading EPUB content");
    let mut doc =
        EpubDoc::new(path).with_context(|| format!("Failed to open EPUB at {}", path.display()))?;

    let mut combined = String::new();
    let mut chapters = 0usize;

    loop {
        match doc.get_current_str() {
            Some((chapter, _mime)) => {
                chapters += 1;
                if !combined.is_empty() {
                    combined.push_str("\n\n");
                }
                // Use a lightweight HTML-to-text pass to remove most markup; fall back to raw chapter on errors.
                // Use a very large width so we do not bake in hard line breaks; let the UI handle wrapping.
                let plain = match html2text::from_read(chapter.as_bytes(), 10_000) {
                    Ok(clean) => clean,
                    Err(err) => {
                        warn!(chapter = chapters, "html2text failed: {err}");
                        chapter
                    }
                };
                debug!(
                    chapter = chapters,
                    added_chars = plain.len(),
                    "Parsed chapter"
                );
                combined.push_str(&plain);
            }
            None => break,
        }

        if !doc.go_next() {
            break;
        }
    }

    if combined.trim().is_empty() {
        combined.push_str("No textual content found in this EPUB.");
    }

    info!(
        chapters,
        total_chars = combined.len(),
        "Finished loading EPUB content"
    );
    Ok(combined)
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

fn is_pdf(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase()),
        Some(ext) if ext == "pdf"
    )
}

fn load_pdf_with_quack_check(path: &Path) -> Result<String> {
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
        info!(
            path = %path.display(),
            total_chars = cached.len(),
            "Using cached quack-check PDF transcript"
        );
        return Ok(normalize_pdf_text_for_reader(&cached));
    }

    let (_, _, run_out_dir) = pdf_cache_paths(path);
    let run = crate::quack_check::run_pdf_to_text(&config_path, path, &run_out_dir).with_context(
        || {
            format!(
                "Failed to transcribe PDF with in-process quack-check module for {}",
                path.display()
            )
        },
    )?;
    let text = if run.text.trim().is_empty() {
        "No textual content found in this file.".to_string()
    } else {
        normalize_pdf_text_for_reader(&run.text)
    };

    write_pdf_cache(path, &signature, &text)?;
    info!(
        path = %path.display(),
        total_chars = text.len(),
        job_id = %run.job_id,
        job_dir = %run.job_dir.display(),
        "Finished quack-check PDF transcription"
    );
    Ok(text)
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

fn load_with_pandoc(path: &Path) -> Result<String> {
    info!(
        path = %path.display(),
        "Converting source to plain text with pandoc"
    );

    let signature = source_signature(path)?;
    if let Some(cached) = try_read_pandoc_cache(path, &signature)? {
        info!(path = %path.display(), "Using cached pandoc plain-text conversion");
        return Ok(cached);
    }

    let filter_path = pandoc_filter_path()?;
    let output = Command::new("pandoc")
        .arg(path)
        .arg("--to")
        .arg("plain")
        .arg("--wrap=none")
        .arg("--columns=100000")
        .arg("--strip-comments")
        .arg("--eol=lf")
        .arg("--lua-filter")
        .arg(&filter_path)
        .output()
        .with_context(|| format!("Failed to start pandoc for {}", path.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "pandoc conversion failed for {}: {}",
            path.display(),
            stderr.trim()
        );
    }

    let text = String::from_utf8(output.stdout)
        .with_context(|| format!("pandoc returned non-UTF8 text for {}", path.display()))?;
    let text = if text.trim().is_empty() {
        "No textual content found in this file.".to_string()
    } else {
        text
    };

    if let Err(err) = write_pandoc_cache(path, &signature, &text) {
        warn!(path = %path.display(), "Failed to cache pandoc text output: {err}");
    }

    info!(
        path = %path.display(),
        total_chars = text.len(),
        "Finished pandoc conversion"
    );
    Ok(text)
}

fn collect_images(path: &Path) -> Result<Vec<BookImage>> {
    if is_markdown(path) {
        return collect_markdown_images(path);
    }
    if is_epub(path) {
        return collect_epub_images(path);
    }
    Ok(Vec::new())
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
            label,
        });
    }

    Ok(images)
}

fn collect_epub_images(path: &Path) -> Result<Vec<BookImage>> {
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

    let mut images = Vec::new();
    let mut seen = HashSet::new();

    for (idx, (id, resource_path, mime)) in entries.into_iter().enumerate() {
        let Some((bytes, _)) = doc.get_resource(&id) else {
            continue;
        };

        let image_hash = short_hash(&bytes);
        if !seen.insert(image_hash.clone()) {
            continue;
        }

        let extension = resource_path
            .extension()
            .and_then(|s| s.to_str())
            .filter(|ext| !ext.is_empty())
            .map(|ext| ext.to_ascii_lowercase())
            .or_else(|| extension_from_mime(&mime).map(str::to_string))
            .unwrap_or_else(|| "img".to_string());
        let file_name = format!("img-{idx:04}-{image_hash}.{extension}");
        let output = image_dir.join(file_name);

        if !output.exists() {
            fs::write(&output, &bytes)
                .with_context(|| format!("Failed to write extracted image {}", output.display()))?;
        }

        let label = resource_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("image")
            .to_string();

        images.push(BookImage {
            path: output,
            label,
        });
    }

    Ok(images)
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
        "image/png" | "image/jpeg" | "image/jpg" | "image/gif" | "image/webp" | "image/bmp"
    )
}

fn extension_from_mime(mime: &str) -> Option<&'static str> {
    match mime.to_ascii_lowercase().as_str() {
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/bmp" => Some("bmp"),
        _ => None,
    }
}

fn short_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let full = format!("{:x}", hasher.finalize());
    full.chars().take(12).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PandocCacheMeta {
    source_len: u64,
    source_modified_unix_secs: Option<u64>,
    #[serde(default)]
    pipeline_rev: String,
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

fn source_signature(path: &Path) -> Result<PandocCacheMeta> {
    let meta = fs::metadata(path)
        .with_context(|| format!("Failed to read source metadata for {}", path.display()))?;

    let modified = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());

    let filter_path = pandoc_filter_path()?;
    let filter_sha256 = hash_file(&filter_path)
        .with_context(|| format!("Failed to hash pandoc filter at {}", filter_path.display()))?;

    Ok(PandocCacheMeta {
        source_len: meta.len(),
        source_modified_unix_secs: modified,
        pipeline_rev: PANDOC_PIPELINE_REV.to_string(),
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

fn pandoc_cache_paths(path: &Path) -> (PathBuf, PathBuf) {
    let dir = hash_dir(path);
    (
        dir.join("source-plain.txt"),
        dir.join("source-plain.meta.toml"),
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

fn try_read_pandoc_cache(path: &Path, signature: &PandocCacheMeta) -> Result<Option<String>> {
    let (text_path, meta_path) = pandoc_cache_paths(path);

    let meta_str = match fs::read_to_string(&meta_path) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };

    let cached_meta: PandocCacheMeta = match toml::from_str(&meta_str) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };

    if cached_meta.source_len != signature.source_len
        || cached_meta.source_modified_unix_secs != signature.source_modified_unix_secs
        || cached_meta.pipeline_rev != signature.pipeline_rev
        || cached_meta.filter_sha256 != signature.filter_sha256
    {
        return Ok(None);
    }

    let text = fs::read_to_string(&text_path).with_context(|| {
        format!(
            "Failed to read pandoc cache text at {}",
            text_path.display()
        )
    })?;
    Ok(Some(text))
}

fn try_read_pdf_cache(path: &Path, signature: &PdfCacheMeta) -> Result<Option<String>> {
    let (text_path, meta_path, _) = pdf_cache_paths(path);
    let meta_str = match fs::read_to_string(&meta_path) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };

    let cached_meta: PdfCacheMeta = match toml::from_str(&meta_str) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };

    if cached_meta.source_len != signature.source_len
        || cached_meta.source_modified_unix_secs != signature.source_modified_unix_secs
        || cached_meta.pipeline_rev != signature.pipeline_rev
        || cached_meta.quack_config_sha256 != signature.quack_config_sha256
        || cached_meta.quack_text_filename != signature.quack_text_filename
    {
        return Ok(None);
    }

    let text = fs::read_to_string(&text_path).with_context(|| {
        format!(
            "Failed to read PDF transcript cache text at {}",
            text_path.display()
        )
    })?;
    Ok(Some(text))
}

fn write_pandoc_cache(path: &Path, signature: &PandocCacheMeta, text: &str) -> Result<()> {
    let (text_path, meta_path) = pandoc_cache_paths(path);
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

    let rooted = Path::new(env!("CARGO_MANIFEST_DIR")).join(PANDOC_FILTER_REL_PATH);
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
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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
