//! EPUB loading utilities.
//!
//! This module is intentionally small: it knows how to open an EPUB, walk
//! through its spine, strip basic markup, and return a single `String` of text.
//! Keeping it isolated makes it easy to swap out or enhance parsing later
//! (e.g., extracting a table of contents or preserving styling).

use anyhow::{Context, Result};
use epub::doc::EpubDoc;
use std::fs;
use std::path::Path;
use std::process::Command;
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

    if is_markdown(path) {
        match load_with_pandoc(path) {
            Ok(text) => return Ok(text),
            Err(err) => {
                warn!(
                    path = %path.display(),
                    "Pandoc markdown conversion failed, falling back to raw markdown: {err}"
                );
                let data = fs::read_to_string(path).with_context(|| {
                    format!("Failed to read markdown file at {}", path.display())
                })?;
                return Ok(data);
            }
        }
    }

    if !is_epub(path) {
        return load_with_pandoc(path);
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
                // Use a very large width so we do not bake in hard line breaks—let the UI handle wrapping.
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
    let output = Command::new("pandoc")
        .arg(path)
        .arg("--to")
        .arg("plain")
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
    info!(
        path = %path.display(),
        total_chars = text.len(),
        "Finished pandoc conversion"
    );
    Ok(text)
}
