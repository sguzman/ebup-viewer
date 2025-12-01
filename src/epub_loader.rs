//! EPUB loading utilities.
//!
//! This module is intentionally small: it knows how to open an EPUB, walk
//! through its spine, strip basic markup, and return a single `String` of text.
//! Keeping it isolated makes it easy to swap out or enhance parsing later
//! (e.g., extracting a table of contents or preserving styling).

use anyhow::{Context, Result};
use epub::doc::EpubDoc;
use std::path::Path;
use tracing::{debug, info, warn};

/// Load an EPUB from disk and return its text content as a single string.
pub fn load_epub_text(path: &Path) -> Result<String> {
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
                let plain = match html2text::from_read(chapter.as_bytes(), 80) {
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
