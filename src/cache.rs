//! Simple cache to remember the last opened page per EPUB file, along with
//! finer-grained resume data (sentence + scroll position).
//!
//! Files are stored under `.cache/` using a hash of the source file contents
//! as the directory name so path aliases do not fragment the cache. The format
//! is a tiny TOML file with a `page` field plus optional `sentence_idx`,
//! `sentence_text`, and `scroll_y` for resuming inside the page.

use crate::config::{AppConfig, parse_config, serialize_config};
use epub::doc::EpubDoc;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;
use tracing::{debug, warn};

pub const CACHE_DIR: &str = ".cache";
const SOURCE_PATH_FILE: &str = "source-path.txt";
static CONTENT_DIGEST_CACHE: OnceLock<Mutex<HashMap<PathBuf, SourceDigestEntry>>> = OnceLock::new();

#[derive(Clone)]
struct SourceDigestEntry {
    len: u64,
    modified_unix_secs: u64,
    digest: String,
}

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

#[derive(Debug, Clone)]
pub struct RecentBook {
    pub source_path: PathBuf,
    pub display_title: String,
    pub thumbnail_path: Option<PathBuf>,
    pub last_opened_unix_secs: u64,
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
    let hash = source_content_hash(epub_path).unwrap_or_else(|| {
        // Fallback for unreadable paths keeps cache functions non-fatal.
        let mut hasher = Sha256::new();
        hasher.update(epub_path.as_os_str().to_string_lossy().as_bytes());
        format!("{:x}", hasher.finalize())
    });
    Path::new(CACHE_DIR).join(hash)
}

fn source_content_hash(path: &Path) -> Option<String> {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let metadata = fs::metadata(&canonical).ok()?;
    let len = metadata.len();
    let modified_unix_secs = metadata
        .modified()
        .ok()
        .and_then(|ts| ts.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let cache = CONTENT_DIGEST_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some(entry) = guard.get(&canonical) {
            if entry.len == len && entry.modified_unix_secs == modified_unix_secs {
                return Some(entry.digest.clone());
            }
        }
    }

    let mut file = fs::File::open(&canonical).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = [0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buf).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    let digest = format!("{:x}", hasher.finalize());

    if let Ok(mut guard) = cache.lock() {
        guard.insert(
            canonical,
            SourceDigestEntry {
                len,
                modified_unix_secs,
                digest: digest.clone(),
            },
        );
    }

    Some(digest)
}

fn bookmark_path(epub_path: &Path) -> PathBuf {
    hash_dir(epub_path).join("bookmark.toml")
}

pub fn remember_source_path(source_path: &Path) {
    let hint_path = hash_dir(source_path).join(SOURCE_PATH_FILE);
    if let Some(parent) = hint_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let canonical = fs::canonicalize(source_path).unwrap_or_else(|_| source_path.to_path_buf());
    let payload = canonical.to_string_lossy().to_string();
    if let Err(err) = fs::write(&hint_path, payload) {
        warn!(path = %hint_path.display(), "Failed to persist source path hint: {err}");
    }
}

pub fn persist_clipboard_text_source(text: &str) -> Result<PathBuf, String> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return Err("clipboard text is empty".to_string());
    }

    let mut hasher = Sha256::new();
    hasher.update(trimmed.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    let short = &digest[..16];
    let dir = Path::new(CACHE_DIR).join("clipboard");
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let path = dir.join(format!("clipboard-{short}.txt"));

    if !path.exists() {
        fs::write(&path, trimmed).map_err(|err| err.to_string())?;
    }

    Ok(path)
}

pub fn list_recent_books(limit: usize) -> Vec<RecentBook> {
    let Ok(entries) = fs::read_dir(CACHE_DIR) else {
        return Vec::new();
    };

    let mut books: Vec<RecentBook> = entries
        .flatten()
        .filter_map(|entry| {
            let Ok(file_type) = entry.file_type() else {
                return None;
            };
            if !file_type.is_dir() {
                return None;
            }
            let source_hint_path = entry.path().join(SOURCE_PATH_FILE);
            let source_path_raw = fs::read_to_string(&source_hint_path).ok()?;
            let source_path = PathBuf::from(source_path_raw.trim());
            if source_path.as_os_str().is_empty() || !source_path.exists() {
                return None;
            }
            let last_opened_unix_secs = fs::metadata(&source_hint_path)
                .ok()
                .and_then(|meta| meta.modified().ok())
                .and_then(|ts| ts.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let display_title = infer_recent_title(&source_path);
            let thumbnail_path = infer_recent_thumbnail(&source_path);
            Some(RecentBook {
                source_path,
                display_title,
                thumbnail_path,
                last_opened_unix_secs,
            })
        })
        .collect();

    books.sort_by(|a, b| b.last_opened_unix_secs.cmp(&a.last_opened_unix_secs));
    books.dedup_by(|a, b| a.source_path == b.source_path);
    if limit > 0 && books.len() > limit {
        books.truncate(limit);
    }
    books
}

pub fn tts_dir(epub_path: &Path) -> PathBuf {
    hash_dir(epub_path).join("tts")
}

pub fn normalized_dir(epub_path: &Path) -> PathBuf {
    hash_dir(epub_path).join("normalized")
}

fn infer_recent_title(source_path: &Path) -> String {
    if source_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .map(|name| name == "clipboard")
        .unwrap_or(false)
    {
        return "Clipboard Text".to_string();
    }

    if source_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("epub"))
        .unwrap_or(false)
        && let Ok(doc) = EpubDoc::new(source_path)
        && let Some(title) = doc.get_title()
    {
        let trimmed = title.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            source_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("book")
        })
        .to_string()
}

fn infer_recent_thumbnail(source_path: &Path) -> Option<PathBuf> {
    if !source_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("epub"))
        .unwrap_or(false)
    {
        return None;
    }

    let thumb_path = hash_dir(source_path).join("thumbs").join("cover-thumb.jpg");
    if thumb_path.exists() {
        return Some(thumb_path);
    }

    let mut doc = EpubDoc::new(source_path).ok()?;
    let (cover, _mime) = doc.get_cover()?;
    write_thumbnail_file(&thumb_path, &cover).ok()?;
    Some(thumb_path)
}

fn write_thumbnail_file(path: &Path, raw_image: &[u8]) -> Result<(), String> {
    let image = image::load_from_memory(raw_image).map_err(|err| err.to_string())?;
    let thumb = image.resize(68, 100, FilterType::Triangle);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let mut encoded = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(Cursor::new(&mut encoded), 80);
    encoder
        .encode_image(&thumb)
        .map_err(|err| err.to_string())?;
    fs::write(path, encoded).map_err(|err| err.to_string())?;
    Ok(())
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
    match parse_config(&data) {
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
    if let Ok(contents) = serialize_config(config) {
        if let Err(err) = fs::write(&path, contents) {
            warn!(path = %path.display(), "Failed to save EPUB config: {err}");
        } else {
            debug!(path = %path.display(), "Persisted EPUB config");
        }
    }
}
