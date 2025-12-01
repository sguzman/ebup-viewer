//! Simple cache to remember the last opened page per EPUB file.
//!
//! Files are stored under `.cache/` using a hash of the EPUB path as the
//! filename to avoid filesystem issues. The format is a tiny TOML file with a
//! single `page` field.

use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const CACHE_DIR: &str = ".cache";

/// Load the cached page for a given EPUB path, if present.
pub fn load_last_page(epub_path: &Path) -> Option<usize> {
    let path = bookmark_path(epub_path);
    let data = fs::read_to_string(path).ok()?;
    let value: CacheEntry = toml::from_str(&data).ok()?;
    Some(value.page)
}

/// Persist the current page for a given EPUB path. Errors are ignored to keep
/// the UI responsive.
pub fn save_last_page(epub_path: &Path, page: usize) {
    let path = bookmark_path(epub_path);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let entry = CacheEntry { page };
    if let Ok(contents) = toml::to_string(&entry) {
        if let Ok(mut file) = fs::File::create(path) {
            let _ = file.write_all(contents.as_bytes());
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    page: usize,
}

fn hash_dir(epub_path: &Path) -> PathBuf {
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
