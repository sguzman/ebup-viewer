//! EPUB loading utilities.
//!
//! This module is intentionally small: it knows how to open an EPUB, walk
//! through its spine, strip basic markup, and return a single `String` of text.
//! Keeping it isolated makes it easy to swap out or enhance parsing later
//! (e.g., extracting a table of contents or preserving styling).

use crate::cache::hash_dir;
use anyhow::{Context, Result};
use epub::doc::EpubDoc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;
use tracing::{debug, info, warn};

/// Load an EPUB from disk and return its text content as a single string.
pub fn load_epub_text(path: &Path) -> Result<String> {
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
            "Unsupported source format without successful pandoc conversion: {}",
            path.display()
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

    let output = Command::new("pandoc")
        .arg(path)
        .arg("--to")
        .arg("plain")
        .arg("--wrap=none")
        .arg("--columns=100000")
        .arg("--strip-comments")
        .arg("--eol=lf")
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PandocCacheMeta {
    source_len: u64,
    source_modified_unix_secs: Option<u64>,
}

fn source_signature(path: &Path) -> Result<PandocCacheMeta> {
    let meta = fs::metadata(path)
        .with_context(|| format!("Failed to read source metadata for {}", path.display()))?;

    let modified = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());

    Ok(PandocCacheMeta {
        source_len: meta.len(),
        source_modified_unix_secs: modified,
    })
}

fn pandoc_cache_paths(path: &Path) -> (PathBuf, PathBuf) {
    let dir = hash_dir(path);
    (
        dir.join("source-plain.txt"),
        dir.join("source-plain.meta.toml"),
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
