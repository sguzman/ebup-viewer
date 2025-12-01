//! Simple cache to remember the last opened page per EPUB file, along with
//! finer-grained resume data (sentence + scroll position).
//!
//! Files are stored under `.cache/` using a hash of the EPUB path as the
//! filename to avoid filesystem issues. The format is a tiny TOML file with a
//! `page` field plus optional `sentence_idx`, `sentence_text`, and `scroll_y`
//! for resuming inside the page.

use crate::config::AppConfig;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

pub const CACHE_DIR: &str = ".cache";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Bookmark {
    pub page: usize,
    #[serde(default)]
    pub sentence_idx: Option<usize>,
    #[serde(default)]
    pub sentence_text: Option<String>,
    #[serde(default = "default_scroll")]
    pub scroll_y: f32,
}

fn default_scroll() -> f32 {
    0.0
}

/// Load the cached bookmark for a given EPUB path, if present.
pub fn load_bookmark(epub_path: &Path) -> Option<Bookmark> {
    let path = bookmark_path(epub_path);
    let data = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) => {
            debug!(
                path = %path.display(),
                "No cached last page found or unreadable: {err}"
            );
            return None;
        }
    };
    let value: CacheEntry = toml::from_str(&data).ok()?;
    debug!(page = value.page, "Loaded last page bookmark");
    Some(Bookmark {
        page: value.page,
        sentence_idx: value.sentence_idx,
        sentence_text: value.sentence_text,
        scroll_y: value.scroll_y.unwrap_or_else(default_scroll),
    })
}

/// Persist the current bookmark for a given EPUB path. Errors are ignored to
/// keep the UI responsive.
pub fn save_bookmark(epub_path: &Path, bookmark: &Bookmark) {
    let path = bookmark_path(epub_path);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let entry = CacheEntry {
        page: bookmark.page,
        sentence_idx: bookmark.sentence_idx,
        sentence_text: bookmark.sentence_text.clone(),
        scroll_y: Some(bookmark.scroll_y),
    };
    if let Ok(contents) = toml::to_string(&entry) {
        if let Ok(mut file) = fs::File::create(path) {
            if let Err(err) = file.write_all(contents.as_bytes()) {
                warn!("Failed to persist last page: {err}");
            } else {
                debug!(page = bookmark.page, "Saved last page bookmark");
            }
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    page: usize,
    #[serde(default)]
    sentence_idx: Option<usize>,
    #[serde(default)]
    sentence_text: Option<String>,
    #[serde(default)]
    scroll_y: Option<f32>,
}

pub fn hash_dir(epub_path: &Path) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(epub_path.as_os_str().to_string_lossy().as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    Path::new(CACHE_DIR).join(hash)
}

fn bookmark_path(epub_path: &Path) -> PathBuf {
    hash_dir(epub_path).join("bookmark.toml")
}

pub fn tts_dir(epub_path: &Path) -> PathBuf {
    hash_dir(epub_path).join("tts")
}

pub fn load_epub_config(epub_path: &Path) -> Option<AppConfig> {
    let path = hash_dir(epub_path).join("config.toml");
    let data = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) => {
            debug!(
                path = %path.display(),
                "No cached EPUB config found or unreadable: {err}"
            );
            return None;
        }
    };
    match toml::from_str(&data) {
        Ok(cfg) => {
            debug!("Loaded cached EPUB config");
            Some(cfg)
        }
        Err(err) => {
            warn!("Cached EPUB config invalid: {err}");
            None
        }
    }
}

pub fn save_epub_config(epub_path: &Path, config: &AppConfig) {
    let dir = hash_dir(epub_path);
    let path = dir.join("config.toml");
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(contents) = toml::to_string(config) {
        if let Err(err) = fs::write(&path, contents) {
            warn!(path = %path.display(), "Failed to save EPUB config: {err}");
        } else {
            debug!(path = %path.display(), "Persisted EPUB config");
        }
    }
}
