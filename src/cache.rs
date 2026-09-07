//! Simple cache to remember the last opened page per EPUB file, along with
//! finer-grained resume data (sentence + scroll position).
//!
//! Files are stored under `.cache/lantern-leaf/` using a hash of the source file contents
//! as the directory name so path aliases do not fragment the cache. The format
//! is a tiny TOML file with a `page` field plus optional `sentence_idx`,
//! `sentence_text`, and `scroll_y` for resuming inside the page.

#[path = "cache/bookmarks_config.rs"]
mod bookmarks_config;
#[path = "cache/browser_tab_cache.rs"]
mod browser_tab_cache;
#[path = "cache/content_artifacts.rs"]
mod content_artifacts;

#[cfg(not(target_arch = "wasm32"))]
use crate::workspace::workspace_root_from_cwd;
#[cfg(not(target_arch = "wasm32"))]
use epub::doc::EpubDoc;
#[cfg(not(target_arch = "wasm32"))]
use image::codecs::jpeg::JpegEncoder;
#[cfg(not(target_arch = "wasm32"))]
use image::imageops::FilterType;
#[cfg(not(target_arch = "wasm32"))]
use crate::config::AppConfig;
#[cfg(not(target_arch = "wasm32"))]
use sha2::{Digest, Sha256};
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
use std::io::Cursor;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
#[cfg(not(target_arch = "wasm32"))]
use std::thread;
use std::time::{Duration, UNIX_EPOCH};
use tracing::{debug, trace, warn};

pub const CACHE_DIR: &str = ".cache";
const CACHE_APP_SUBDIR: &str = "lantern-leaf";
pub const CACHE_DIR_ENV: &str = "LANTERNLEAF_CACHE_DIR";
const SOURCE_PATH_FILE: &str = "source-path.txt";
const CONTENT_LAYOUT_VERSION: &str = "dual-view-v3";
const CONTENT_LAYOUT_VERSION_FILE: &str = "content/layout-version.txt";
const CONTENT_TTS_TEXT_FILE: &str = "content/tts-text.txt";
const CONTENT_READING_MARKDOWN_FILE: &str = "content/reading-markdown.md";
const CONTENT_READING_HTML_FILE: &str = "content/reading-html.html";
const BROWSER_TABS_SUBDIR: &str = "browser-tabs";
const BROWSER_TAB_MANIFEST_FILE: &str = "browser-tab.lltab";
const BROWSER_TAB_HTML_FILE: &str = "snapshot.html";
const BROWSER_TAB_RAW_HTML_FILE: &str = "snapshot-raw.html";
const BROWSER_TAB_TEXT_FILE: &str = "snapshot.txt";
const BROWSER_TAB_ASSETS_SUBDIR: &str = "assets";
const BROWSER_TAB_MANIFEST_VERSION: u32 = 4;
const BROWSER_TAB_FETCH_USER_AGENT: &str =
    "LanternLeaf/2026.03 (browser-tab-import; local desktop reader)";
static CONTENT_DIGEST_CACHE: OnceLock<Mutex<HashMap<PathBuf, SourceDigestEntry>>> = OnceLock::new();
static CACHE_LAYOUT_INIT: OnceLock<()> = OnceLock::new();

#[derive(Clone)]
struct SourceDigestEntry {
    len: u64,
    modified_unix_nanos: u128,
    digest: String,
}

pub use bookmarks_config::Bookmark;
pub use bookmarks_config::PdfRect;
pub use browser_tab_cache::{
    BrowserTabAsset, BrowserTabSourceManifest, is_browser_tab_manifest, load_browser_tab_manifest,
    persist_browser_tab_bundle_source, persist_browser_tab_source,
    rehydrate_browser_tab_manifest_assets,
};
pub use content_artifacts::{
    PDF_OCR_ALIGNMENT_VERSION, PdfOcrAlignmentArtifact, PdfOcrBlockGeometry, PdfOcrLineGeometry,
    PdfOcrPageAlignmentBucket, PdfOcrPageGeometry, PdfOcrSentenceAlignment, PdfOcrTokenGeometry,
    PdfRenderPrecomputedState, PdfSentenceLocation, PdfSentencePageHint, stable_sentence_text_hash,
};

#[derive(Debug, Clone)]
pub struct RecentBook {
    pub source_path: PathBuf,
    pub display_title: String,
    pub snippet: String,
    pub thumbnail_path: Option<PathBuf>,
    pub last_opened_unix_secs: u64,
    pub browser_tab_id: Option<u64>,
    pub browser_window_id: Option<u64>,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn cache_root() -> PathBuf {
    let workspace_root = workspace_root_from_cwd();
    let configured_root = resolve_configured_cache_root(
        workspace_root.as_deref(),
        std::env::var_os(CACHE_DIR_ENV).as_deref(),
    );
    let app_root = app_cache_root(&configured_root);
    ensure_cache_layout(&configured_root, &app_root);
    trace!(
        cache_root = %app_root.display(),
        cache_config_root = %configured_root.display(),
        workspace = ?workspace_root,
        "Resolved LanternLeaf cache directory"
    );
    app_root
}

#[cfg(target_arch = "wasm32")]
pub fn cache_root() -> PathBuf {
    PathBuf::from("/virtual/cache")
}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_configured_cache_root(
    workspace_root: Option<&Path>,
    env_override: Option<&std::ffi::OsStr>,
) -> PathBuf {
    if let Some(value) = env_override.map(PathBuf::from) {
        if value.is_absolute() {
            return value;
        }
        if let Some(root) = workspace_root {
            return root.join(value);
        }
        return value;
    }

    if let Some(root) = workspace_root {
        root.join(CACHE_DIR)
    } else {
        PathBuf::from(CACHE_DIR)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn app_cache_root(configured_root: &Path) -> PathBuf {
    if configured_root
        .file_name()
        .map(|name| name == std::ffi::OsStr::new(CACHE_APP_SUBDIR))
        .unwrap_or(false)
    {
        configured_root.to_path_buf()
    } else {
        configured_root.join(CACHE_APP_SUBDIR)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn ensure_cache_layout(configured_root: &Path, app_root: &Path) {
    CACHE_LAYOUT_INIT.get_or_init(|| {
        if let Err(err) = fs::create_dir_all(app_root) {
            warn!(
                path = %app_root.display(),
                "Failed to create cache root directory: {err}"
            );
            return;
        }
        migrate_legacy_cache_layout(configured_root, app_root);
    });
}

#[cfg(target_arch = "wasm32")]
fn ensure_cache_layout(_configured_root: &Path, _app_root: &Path) {}

#[cfg(not(target_arch = "wasm32"))]
fn migrate_legacy_cache_layout(configured_root: &Path, app_root: &Path) {
    if configured_root == app_root {
        return;
    }

    let Ok(entries) = fs::read_dir(configured_root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name();
        if file_name == std::ffi::OsStr::new(CACHE_APP_SUBDIR) {
            continue;
        }
        let Some(name_str) = file_name.to_str() else {
            continue;
        };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !should_migrate_legacy_cache_entry(name_str, file_type.is_dir()) {
            continue;
        }
        let target = app_root.join(&file_name);
        if target.exists() {
            continue;
        }
        if let Err(err) = fs::rename(&path, &target) {
            warn!(
                from = %path.display(),
                to = %target.display(),
                "Failed to migrate cache entry: {err}"
            );
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn should_migrate_legacy_cache_entry(name: &str, is_dir: bool) -> bool {
    matches!(
        name,
        "calibre-books.toml"
            | "calibre-downloads"
            | "calibre-thumbs"
            | "clipboard"
            | "test-sources"
            | "_cover_test.bin"
            | "_thumb_test.bin"
    ) || name.starts_with("quack-check-")
        || (is_dir && is_sha256_dir_name(name))
}

#[cfg(not(target_arch = "wasm32"))]
fn is_sha256_dir_name(name: &str) -> bool {
    name.len() == 64 && name.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Load the cached bookmark for a given EPUB path, if present.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_bookmark(epub_path: &Path) -> Option<Bookmark> {
    bookmarks_config::load_bookmark(epub_path)
}

#[cfg(target_arch = "wasm32")]
pub fn load_bookmark(_epub_path: &Path) -> Option<Bookmark> {
    None
}

/// Persist the current bookmark for a given EPUB path. Errors are ignored to
/// keep the UI responsive.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_bookmark(epub_path: &Path, bookmark: &Bookmark) {
    bookmarks_config::save_bookmark(epub_path, bookmark)
}

#[cfg(target_arch = "wasm32")]
pub fn save_bookmark(_epub_path: &Path, _bookmark: &Bookmark) {
}

#[cfg(not(target_arch = "wasm32"))]
pub fn source_hash(path: &Path) -> String {
    source_content_hash(path).unwrap_or_else(|| {
        let mut hasher = Sha256::new();
        hasher.update(path.as_os_str().to_string_lossy().as_bytes());
        format!("{:x}", hasher.finalize())
    })
}

#[cfg(target_arch = "wasm32")]
pub fn source_hash(path: &Path) -> String {
    format!("{:x}", path.to_string_lossy().len())
}

pub fn hash_dir(epub_path: &Path) -> PathBuf {
    cache_root().join(source_hash(epub_path))
}

#[cfg(not(target_arch = "wasm32"))]
fn source_content_hash(path: &Path) -> Option<String> {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let metadata = fs::metadata(&canonical).ok()?;
    let len = metadata.len();
    let modified_unix_nanos = metadata
        .modified()
        .ok()
        .and_then(|ts| ts.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    let cache = CONTENT_DIGEST_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some(entry) = guard.get(&canonical) {
            if entry.len == len && entry.modified_unix_nanos == modified_unix_nanos {
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
                modified_unix_nanos,
                digest: digest.clone(),
            },
        );
    }

    Some(digest)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn persist_dual_view_artifacts(
    source_path: &Path,
    tts_text: &str,
    reading_markdown: Option<&str>,
    reading_html: Option<&str>,
) {
    content_artifacts::persist_dual_view_artifacts(
        source_path,
        tts_text,
        reading_markdown,
        reading_html,
    )
}

#[cfg(target_arch = "wasm32")]
pub fn persist_dual_view_artifacts(
    _source_path: &Path,
    _tts_text: &str,
    _reading_markdown: Option<&str>,
    _reading_html: Option<&str>,
) {
}

pub fn persist_sentence_anchor_map(source_path: &Path, page: usize, anchors: &[Option<usize>]) {
    content_artifacts::persist_sentence_anchor_map(source_path, page, anchors)
}

pub fn load_sentence_anchor_map(source_path: &Path, page: usize) -> Option<Vec<Option<usize>>> {
    content_artifacts::load_sentence_anchor_map(source_path, page)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn persist_pdf_sync_meta(
    source_path: &Path,
    pdf_geometry_mode: crate::epub_loader::PdfGeometryMode,
    pdf_sync_strategy: crate::epub_loader::PdfSyncStrategy,
    pdf_classification: Option<&crate::epub_loader::PdfClassificationSummary>,
    pdf_runtime_policy: Option<&crate::epub_loader::PdfRuntimePolicySummary>,
) {
    content_artifacts::persist_pdf_sync_meta(
        source_path,
        pdf_geometry_mode,
        pdf_sync_strategy,
        pdf_classification,
        pdf_runtime_policy,
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_pdf_sync_meta(source_path: &Path) -> Option<content_artifacts::PdfSyncMeta> {
    content_artifacts::load_pdf_sync_meta(source_path)
}

#[cfg(target_arch = "wasm32")]
pub fn load_pdf_sync_meta(_source_path: &Path) -> Option<content_artifacts::PdfSyncMeta> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub fn persist_pdf_sentence_map(source_path: &Path, locations: &[PdfSentenceLocation]) {
    content_artifacts::persist_pdf_sentence_map(source_path, locations)
}

#[cfg(target_arch = "wasm32")]
pub fn persist_pdf_sentence_map(_source_path: &Path, _locations: &[PdfSentenceLocation]) {
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_pdf_sentence_map(source_path: &Path) -> Option<Vec<PdfSentenceLocation>> {
    content_artifacts::load_pdf_sentence_map(source_path)
}

#[cfg(target_arch = "wasm32")]
pub fn load_pdf_sentence_map(_source_path: &Path) -> Option<Vec<PdfSentenceLocation>> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub fn persist_pdf_ocr_alignment_artifact(source_path: &Path, artifact: &PdfOcrAlignmentArtifact) {
    content_artifacts::persist_pdf_ocr_alignment_artifact(source_path, artifact)
}

#[cfg(target_arch = "wasm32")]
pub fn persist_pdf_ocr_alignment_artifact(_source_path: &Path, _artifact: &PdfOcrAlignmentArtifact) {
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_pdf_ocr_alignment_artifact(source_path: &Path) -> Option<PdfOcrAlignmentArtifact> {
    content_artifacts::load_pdf_ocr_alignment_artifact(source_path)
}

#[cfg(target_arch = "wasm32")]
pub fn load_pdf_ocr_alignment_artifact(_source_path: &Path) -> Option<PdfOcrAlignmentArtifact> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub fn persist_pdf_render_precomputed_state(
    source_path: &Path,
    artifact: &PdfRenderPrecomputedState,
) {
    content_artifacts::persist_pdf_render_precomputed_state(source_path, artifact)
}

#[cfg(target_arch = "wasm32")]
pub fn persist_pdf_render_precomputed_state(
    _source_path: &Path,
    _artifact: &PdfRenderPrecomputedState,
) {
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_pdf_render_precomputed_state(source_path: &Path) -> Option<PdfRenderPrecomputedState> {
    content_artifacts::load_pdf_render_precomputed_state(source_path)
}

#[cfg(target_arch = "wasm32")]
pub fn load_pdf_render_precomputed_state(_source_path: &Path) -> Option<PdfRenderPrecomputedState> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
pub fn remember_source_path(_source_path: &Path) {
}

#[cfg(not(target_arch = "wasm32"))]
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
    let dir = cache_root().join("clipboard");
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let path = dir.join(format!("clipboard-{short}.txt"));

    if !path.exists() {
        fs::write(&path, trimmed).map_err(|err| err.to_string())?;
    }

    Ok(path)
}

#[cfg(target_arch = "wasm32")]
pub fn persist_clipboard_text_source(_text: &str) -> Result<PathBuf, String> {
    Err("Clipboard source not supported on WASM".to_string())
}

#[cfg(target_arch = "wasm32")]
pub fn delete_recent_source_and_cache(_source_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn delete_recent_source_and_cache(source_path: &Path) -> Result<(), String> {
    let canonical_source =
        fs::canonicalize(source_path).unwrap_or_else(|_| source_path.to_path_buf());
    let cache_path = hash_dir(source_path);
    debug!(
        source_path = %source_path.display(),
        canonical_source = %canonical_source.display(),
        cache_path = %cache_path.display(),
        "Deleting recent source and cached reader artifacts"
    );
    if is_browser_tab_manifest(source_path) {
        let browser_tab_dir = source_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| source_path.to_path_buf());
        delete_path_if_present(&browser_tab_dir)?;
    } else {
        delete_path_if_present(source_path)?;
    }
    delete_dir_if_present(&cache_path)?;
    delete_recent_entry_dirs_for_source(&canonical_source)?;
    if canonical_source != source_path {
        delete_recent_entry_dirs_for_source(source_path)?;
    }

    debug!(
        source_path = %source_path.display(),
        cache_path = %cache_path.display(),
        "Finished deleting recent source and cached reader artifacts"
    );

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn delete_recent_entry_dirs_for_source(source_path: &Path) -> Result<(), String> {
    let Ok(entries) = fs::read_dir(cache_root()) else {
        return Ok(());
    };
    let target = source_path.to_string_lossy().to_string();
    for entry in entries.flatten() {
        let entry_path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let source_hint_path = entry_path.join(SOURCE_PATH_FILE);
        let Ok(raw) = fs::read_to_string(&source_hint_path) else {
            continue;
        };
        if raw.trim() != target {
            continue;
        }
        delete_dir_if_present(&entry_path)?;
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn delete_path_if_present(path: &Path) -> Result<(), String> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            debug!(path = %path.display(), "Delete skipped: source path already missing");
            return Ok(());
        }
        Err(err) => {
            warn!(path = %path.display(), "Delete failed while reading metadata: {err}");
            return Err(err.to_string());
        }
    };

    let remove_result = if metadata.is_dir() {
        remove_dir_all_with_retries(path)
    } else {
        fs::remove_file(path)
    };
    match remove_result {
        Ok(()) => {
            debug!(
                path = %path.display(),
                is_dir = metadata.is_dir(),
                "Deleted recent source path"
            );
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            debug!(
                path = %path.display(),
                is_dir = metadata.is_dir(),
                "Delete raced with another remover; source already missing"
            );
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
            warn!(
                path = %path.display(),
                is_dir = metadata.is_dir(),
                "Source delete remained busy after retries; leaving for next cleanup pass: {err}"
            );
            Ok(())
        }
        Err(err) => {
            warn!(
                path = %path.display(),
                is_dir = metadata.is_dir(),
                "Delete failed while removing source path: {err}"
            );
            Err(err.to_string())
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn delete_dir_if_present(path: &Path) -> Result<(), String> {
    match remove_dir_all_with_retries(path) {
        Ok(()) => {
            debug!(path = %path.display(), "Deleted cache directory");
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            debug!(path = %path.display(), "Delete skipped: cache directory already missing");
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
            warn!(
                path = %path.display(),
                "Cache delete remained busy after retries; leaving for next cleanup pass: {err}"
            );
            Ok(())
        }
        Err(err) => {
            warn!(path = %path.display(), "Delete failed while removing cache directory: {err}");
            Err(err.to_string())
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn remove_dir_all_with_retries(path: &Path) -> Result<(), std::io::Error> {
    const MAX_RETRIES: u32 = 4;
    for attempt in 0..=MAX_RETRIES {
        match fs::remove_dir_all(path) {
            Ok(()) => return Ok(()),
            Err(err)
                if err.kind() == std::io::ErrorKind::DirectoryNotEmpty && attempt < MAX_RETRIES =>
            {
                let retry_in_ms = 25 * u64::from(attempt + 1);
                warn!(
                    path = %path.display(),
                    attempt = attempt + 1,
                    max_attempts = MAX_RETRIES + 1,
                    retry_in_ms,
                    "Directory still had concurrent writes during delete; retrying"
                );
                thread::sleep(Duration::from_millis(retry_in_ms));
            }
            Err(err) => return Err(err),
        }
    }
    unreachable!("retry loop always returns on success or terminal error");
}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_existing_recent_source_path(source_path: &Path) -> Option<PathBuf> {
    if source_path.as_os_str().is_empty() {
        return None;
    }
    if source_path.exists() {
        return Some(source_path.to_path_buf());
    }

    let mut components: Vec<&std::ffi::OsStr> = Vec::new();
    for component in source_path.components() {
        components.push(component.as_os_str());
    }

    let cache_idx = components
        .iter()
        .position(|segment| *segment == std::ffi::OsStr::new(CACHE_DIR))?;

    let cache_root = cache_root();
    let mut candidate = cache_root.clone();
    for segment in components.iter().skip(cache_idx + 1) {
        if candidate == cache_root && *segment == std::ffi::OsStr::new(CACHE_APP_SUBDIR) {
            continue;
        }
        candidate.push(segment);
    }

    if candidate.exists() {
        Some(candidate)
    } else {
        None
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(not(target_arch = "wasm32"))]
#[cfg(not(target_arch = "wasm32"))]
pub fn list_recent_books(limit: usize) -> Vec<RecentBook> {
    let Ok(entries) = fs::read_dir(cache_root()) else {
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
            let Some(source_path) = resolve_existing_recent_source_path(&source_path) else {
                let _ = delete_dir_if_present(&entry.path());
                return None;
            };

            // Self-heal stale source hint paths after workspace/project moves.
            let current_hint = source_path_raw.trim();
            let resolved_hint = source_path.to_string_lossy();
            if current_hint != resolved_hint {
                let _ = fs::write(&source_hint_path, resolved_hint.as_ref());
            }

            let last_opened_unix_secs = fs::metadata(&source_hint_path)
                .ok()
                .and_then(|meta| meta.modified().ok())
                .and_then(|ts| ts.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let browser_tab_manifest = load_browser_tab_manifest(&source_path);
            let display_title = infer_recent_title(&source_path);
            let snippet = infer_recent_snippet(&source_path, &display_title);
            let thumbnail_path = infer_recent_thumbnail(&source_path);
            Some(RecentBook {
                source_path,
                display_title,
                snippet,
                thumbnail_path,
                last_opened_unix_secs,
                browser_tab_id: browser_tab_manifest
                    .as_ref()
                    .map(|manifest| manifest.tab_id),
                browser_window_id: browser_tab_manifest.and_then(|manifest| manifest.window_id),
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

#[cfg(target_arch = "wasm32")]
pub fn list_recent_books(_limit: usize) -> Vec<RecentBook> {
    Vec::new()
}
#[cfg(not(target_arch = "wasm32"))]
#[cfg(not(target_arch = "wasm32"))]
pub fn tts_dir(epub_path: &Path) -> PathBuf {
    content_artifacts::tts_dir(epub_path)
}

#[cfg(target_arch = "wasm32")]
pub fn tts_dir(_epub_path: &Path) -> PathBuf {
    PathBuf::from("/virtual/tts")
}

#[cfg(not(target_arch = "wasm32"))]
pub fn normalized_dir(epub_path: &Path) -> PathBuf {
    content_artifacts::normalized_dir(epub_path)
}

#[cfg(target_arch = "wasm32")]
pub fn normalized_dir(_epub_path: &Path) -> PathBuf {
    PathBuf::from("/virtual/normalized")
}

#[cfg(not(target_arch = "wasm32"))]
fn infer_recent_title(source_path: &Path) -> String {
    if let Some(manifest) = load_browser_tab_manifest(source_path) {
        let trimmed = manifest.title.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
        let url = manifest.url.trim();
        if !url.is_empty() {
            return url.to_string();
        }
    }

    if source_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .map(|name| name == "clipboard")
        .unwrap_or(false)
    {
        if let Some(title) = infer_clipboard_recent_title(source_path) {
            return title;
        }
    }

    if source_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false)
        && let Some(title) = infer_pdf_recent_title(source_path)
    {
        return title;
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

#[cfg(not(target_arch = "wasm32"))]
fn infer_recent_snippet(source_path: &Path, display_title: &str) -> String {
    let preview_lines = infer_recent_preview_lines(source_path);
    if preview_lines.is_empty() {
        return String::new();
    }

    let normalized_title = normalize_preview_line(display_title);
    let mut context_parts = Vec::new();
    for line in preview_lines {
        if normalize_preview_line(&line) != normalized_title {
            context_parts.push(line);
        }
    }

    if context_parts.is_empty() {
        return String::new();
    }

    // Keep this as a single line in the UI but include broad context from many lines.
    truncate_preview_line(&context_parts.join(" "), 640)
}

#[cfg(not(target_arch = "wasm32"))]
fn infer_clipboard_recent_title(source_path: &Path) -> Option<String> {
    let contents = fs::read_to_string(source_path).ok()?;
    let first_non_empty_line = contents.lines().find_map(|line| {
        let compact = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if compact.is_empty() {
            None
        } else {
            Some(compact)
        }
    })?;
    const MAX_TITLE_CHARS: usize = 96;
    let char_count = first_non_empty_line.chars().count();
    if char_count <= MAX_TITLE_CHARS {
        return Some(first_non_empty_line);
    }
    let mut truncated = first_non_empty_line
        .chars()
        .take(MAX_TITLE_CHARS - 3)
        .collect::<String>();
    truncated = truncated.trim_end().to_string();
    Some(format!("{truncated}..."))
}

#[cfg(not(target_arch = "wasm32"))]
fn infer_recent_preview_lines(source_path: &Path) -> Vec<String> {
    if let Some(manifest) = load_browser_tab_manifest(source_path) {
        return preview_lines_from_text(
            &fs::read_to_string(manifest.text_path).unwrap_or_default(),
        );
    }

    if source_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .map(|name| name == "clipboard")
        .unwrap_or(false)
    {
        return preview_lines_from_text(&fs::read_to_string(source_path).unwrap_or_default());
    }

    if source_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false)
    {
        return preview_lines_from_text(&cached_recent_pdf_text(source_path).unwrap_or_default());
    }

    if source_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            ext.eq_ignore_ascii_case("txt")
                || ext.eq_ignore_ascii_case("md")
                || ext.eq_ignore_ascii_case("markdown")
        })
        .unwrap_or(false)
    {
        return preview_lines_from_text(&fs::read_to_string(source_path).unwrap_or_default());
    }

    if source_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("epub"))
        .unwrap_or(false)
        && let Ok(mut doc) = EpubDoc::new(source_path)
        && let Some((chapter, _mime)) = doc.get_current_str()
    {
        let plain = match html2text::from_read(chapter.as_bytes(), 10_000) {
            Ok(text) => text,
            Err(err) => {
                warn!("Failed to convert EPUB preview HTML to text: {err}");
                chapter
            }
        };
        return preview_lines_from_text(&plain);
    }

    Vec::new()
}

#[cfg(not(target_arch = "wasm32"))]
fn preview_lines_from_text(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let normalized = normalize_preview_line(line);
            if normalized.is_empty() {
                None
            } else {
                Some(normalized)
            }
        })
        .take(128)
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn normalize_preview_line(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(not(target_arch = "wasm32"))]
fn truncate_preview_line(line: &str, max_chars: usize) -> String {
    let char_count = line.chars().count();
    if char_count <= max_chars {
        return line.to_string();
    }
    let mut truncated = line
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    truncated = truncated.trim_end().to_string();
    format!("{truncated}...")
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(not(target_arch = "wasm32"))]
fn infer_pdf_recent_title(source_path: &Path) -> Option<String> {
    let preview_lines = preview_lines_from_text(&cached_recent_pdf_text(source_path)?);
    let first_line = preview_lines
        .into_iter()
        .find(|line| line.chars().count() >= 8)?;
    Some(truncate_preview_line(&first_line, 96))
}

#[cfg(not(target_arch = "wasm32"))]
fn cached_recent_pdf_text(source_path: &Path) -> Option<String> {
    let tts_text_path = hash_dir(source_path).join(CONTENT_TTS_TEXT_FILE);
    fs::read_to_string(tts_text_path)
        .ok()
        .filter(|text| !text.trim().is_empty())
}

#[cfg(not(target_arch = "wasm32"))]
fn infer_recent_thumbnail(source_path: &Path) -> Option<PathBuf> {
    if is_browser_tab_manifest(source_path) {
        return None;
    }

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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
pub fn load_epub_config(epub_path: &Path) -> Option<crate::config::AppConfig> {
    bookmarks_config::load_epub_config(epub_path)
}

#[cfg(target_arch = "wasm32")]
pub fn load_epub_config(_epub_path: &Path) -> Option<crate::config::AppConfig> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_epub_config(epub_path: &Path, config: &crate::config::AppConfig) {
    bookmarks_config::save_epub_config(epub_path, config)
}

#[cfg(target_arch = "wasm32")]
pub fn save_epub_config(_epub_path: &Path, _config: &crate::config::AppConfig) {
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::browser_tabs::{
        BrowserTab, BrowserTabSnapshot, SnapshotTruncation, SnapshotTruncationEntry,
    };
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_source_path(ext: &str) -> PathBuf {
        let counter = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let mut p = std::env::temp_dir();
        p.push(format!(
            "lanternleaf_test_source_{}_{}.{}",
            std::process::id(),
            format!("{nanos}_{counter}"),
            ext
        ));
        p
    }

    fn write_source_file(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        let payload = format!(
            "cache-test-payload-{}-{}-{}",
            std::process::id(),
            TEST_COUNTER.fetch_add(1, Ordering::Relaxed),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        fs::write(path, payload).expect("write source");
    }

    fn cleanup_source_and_cache(path: &Path) {
        let cache_path = hash_dir(path);
        let _ = fs::remove_file(path);
        let _ = fs::remove_dir_all(cache_path);
    }

    #[test]
    fn cache_root_uses_env_override_when_present() {
        let override_path = std::env::temp_dir().join(format!(
            "lanternleaf_cache_root_override_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));

        let configured_root = resolve_configured_cache_root(
            None,
            Some(override_path.as_os_str()),
        );
        assert_eq!(configured_root, override_path);
        assert_eq!(app_cache_root(&configured_root), override_path.join(CACHE_APP_SUBDIR));
    }

    #[test]
    fn bookmark_roundtrip_preserves_sentence_and_scroll() {
        let source = unique_source_path("epub");
        write_source_file(&source);

        let bookmark = Bookmark {
            page: 42,
            sentence_idx: Some(7),
            sentence_text: Some("A saved sentence".to_string()),
            scroll_y: 0.37,
            pdf_page_idx: Some(3),
            pdf_rects: vec![PdfRect {
                left: 0.11,
                top: 0.22,
                width: 0.33,
                height: 0.04,
            }],
            pdf_line_rects: vec![PdfRect {
                left: 0.11,
                top: 0.22,
                width: 0.33,
                height: 0.04,
            }],
            pdf_block_rects: vec![PdfRect {
                left: 0.1,
                top: 0.2,
                width: 0.35,
                height: 0.08,
            }],
            pdf_confidence: Some("exact".to_string()),
            pdf_reason: Some("exact_geometry".to_string()),
            pdf_quality_class: Some(crate::epub_loader::PdfOcrGeometryQualityClass::OcrHighTrust),
            pdf_sentence_text_hash: Some(stable_sentence_text_hash("A saved sentence")),
            pdf_token_lineage: vec!["page:3".to_string()],
        };

        save_bookmark(&source, &bookmark);
        let loaded = load_bookmark(&source).expect("bookmark should load");

        assert_eq!(loaded.page, 42);
        assert_eq!(loaded.sentence_idx, Some(7));
        assert_eq!(loaded.sentence_text.as_deref(), Some("A saved sentence"));
        assert!((loaded.scroll_y - 0.37).abs() < f32::EPSILON);
        assert_eq!(loaded.pdf_page_idx, Some(3));
        assert_eq!(
            loaded.pdf_rects,
            vec![PdfRect {
                left: 0.11,
                top: 0.22,
                width: 0.33,
                height: 0.04,
            }]
        );
        assert_eq!(loaded.pdf_confidence.as_deref(), Some("exact"));
        assert_eq!(loaded.pdf_reason.as_deref(), Some("exact_geometry"));
        assert_eq!(
            loaded.pdf_quality_class,
            Some(crate::epub_loader::PdfOcrGeometryQualityClass::OcrHighTrust)
        );
        assert_eq!(
            loaded.pdf_sentence_text_hash.as_deref(),
            Some(stable_sentence_text_hash("A saved sentence").as_str())
        );
        assert_eq!(loaded.pdf_token_lineage, vec!["page:3".to_string()]);

        cleanup_source_and_cache(&source);
    }

    #[test]
    fn load_bookmark_defaults_scroll_for_legacy_cache_entries() {
        let source = unique_source_path("epub");
        write_source_file(&source);

        let path = hash_dir(&source).join("bookmark.toml");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create cache dir");
        }
        fs::write(
            &path,
            r#"
page = 5
sentence_idx = 2
sentence_text = "legacy bookmark entry"
"#,
        )
        .expect("write legacy bookmark");

        let loaded = load_bookmark(&source).expect("legacy bookmark should load");
        assert_eq!(loaded.page, 5);
        assert_eq!(loaded.sentence_idx, Some(2));
        assert_eq!(
            loaded.sentence_text.as_deref(),
            Some("legacy bookmark entry")
        );
        assert!((loaded.scroll_y - 0.0).abs() < f32::EPSILON);
        assert_eq!(loaded.pdf_page_idx, None);
        assert!(loaded.pdf_rects.is_empty());
        assert!(loaded.pdf_line_rects.is_empty());
        assert!(loaded.pdf_block_rects.is_empty());
        assert_eq!(loaded.pdf_confidence, None);
        assert_eq!(loaded.pdf_reason, None);
        assert_eq!(loaded.pdf_quality_class, None);
        assert_eq!(loaded.pdf_sentence_text_hash, None);
        assert!(loaded.pdf_token_lineage.is_empty());

        cleanup_source_and_cache(&source);
    }

    #[test]
    fn pdf_sync_meta_roundtrip_preserves_geometry_mode_and_strategy() {
        let source = unique_source_path("pdf");
        write_source_file(&source);

        persist_pdf_sync_meta(
            &source,
            crate::epub_loader::PdfGeometryMode::MixedTextTrust,
            crate::epub_loader::PdfSyncStrategy::ParagraphFallback,
            None,
            None,
        );
        let loaded = load_pdf_sync_meta(&source).expect("pdf sync meta should load");
        assert_eq!(
            loaded.pdf_geometry_mode,
            crate::epub_loader::PdfGeometryMode::MixedTextTrust
        );
        assert_eq!(
            loaded.pdf_sync_strategy,
            crate::epub_loader::PdfSyncStrategy::ParagraphFallback
        );

        cleanup_source_and_cache(&source);
    }

    #[test]
    fn pdf_sync_meta_roundtrip_preserves_runtime_policy() {
        let source = unique_source_path("pdf");
        write_source_file(&source);
        let policy = crate::epub_loader::PdfRuntimePolicySummary {
            text_only_policy: crate::epub_loader::PdfTextOnlyPolicy::LimitedText,
            sentence_highlight_policy:
                crate::epub_loader::PdfSentenceHighlightPolicy::ParagraphFallback,
            search_policy: crate::epub_loader::PdfSearchPolicy::LimitedText,
            bookmark_policy: crate::epub_loader::PdfBookmarkPolicy::PageOnly,
            tts_allowed: true,
            pretty_sync_enabled: true,
            exact_sentence_sync: false,
            explanation: "Degraded sync".to_string(),
            degraded_reasons: vec!["sentence_sync_not_exact".to_string()],
        };

        persist_pdf_sync_meta(
            &source,
            crate::epub_loader::PdfGeometryMode::MixedTextTrust,
            crate::epub_loader::PdfSyncStrategy::ParagraphFallback,
            None,
            Some(&policy),
        );
        let loaded = load_pdf_sync_meta(&source).expect("pdf sync meta should load");
        assert_eq!(
            loaded
                .pdf_runtime_policy
                .as_ref()
                .map(|value| value.sentence_highlight_policy),
            Some(crate::epub_loader::PdfSentenceHighlightPolicy::ParagraphFallback)
        );
        assert_eq!(
            loaded
                .pdf_runtime_policy
                .as_ref()
                .map(|value| value.explanation.as_str()),
            Some("Degraded sync")
        );

        cleanup_source_and_cache(&source);
    }

    #[test]
    fn pdf_sentence_map_roundtrip_preserves_locations() {
        let source = unique_source_path("pdf");
        write_source_file(&source);

        let locations = vec![
            PdfSentenceLocation {
                sentence_idx: 0,
                page_idx: Some(1),
                rects: vec![PdfRect {
                    left: 0.2,
                    top: 0.1,
                    width: 0.5,
                    height: 0.03,
                }],
                line_rects: vec![PdfRect {
                    left: 0.2,
                    top: 0.1,
                    width: 0.5,
                    height: 0.03,
                }],
                block_rects: vec![PdfRect {
                    left: 0.2,
                    top: 0.1,
                    width: 0.5,
                    height: 0.03,
                }],
                confidence: "exact".to_string(),
                reason: "exact_geometry".to_string(),
                score: 1.0,
            },
            PdfSentenceLocation {
                sentence_idx: 1,
                page_idx: Some(1),
                rects: vec![],
                line_rects: vec![],
                block_rects: vec![],
                confidence: "page".to_string(),
                reason: "page_location_only".to_string(),
                score: 0.2,
            },
        ];

        persist_pdf_sentence_map(&source, &locations);
        let loaded = load_pdf_sentence_map(&source).expect("pdf sentence map should load");
        assert_eq!(loaded, locations);

        cleanup_source_and_cache(&source);
    }

    #[test]
    fn pdf_sentence_map_persist_merges_new_sentence_ranges() {
        let source = unique_source_path("pdf");
        write_source_file(&source);

        persist_pdf_sentence_map(
            &source,
            &[PdfSentenceLocation {
                sentence_idx: 2,
                page_idx: Some(3),
                rects: vec![],
                line_rects: vec![],
                block_rects: vec![],
                confidence: "fallback".to_string(),
                reason: "paragraph_fallback".to_string(),
                score: 0.51,
            }],
        );
        persist_pdf_sentence_map(
            &source,
            &[PdfSentenceLocation {
                sentence_idx: 0,
                page_idx: Some(1),
                rects: vec![],
                line_rects: vec![],
                block_rects: vec![],
                confidence: "exact".to_string(),
                reason: "exact_geometry".to_string(),
                score: 1.0,
            }],
        );

        let loaded = load_pdf_sentence_map(&source).expect("merged pdf sentence map should load");
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].sentence_idx, 0);
        assert_eq!(loaded[1].sentence_idx, 2);

        cleanup_source_and_cache(&source);
    }

    #[test]
    fn pdf_ocr_alignment_artifact_roundtrip_preserves_summary_and_alignments() {
        let source = unique_source_path("pdf");
        write_source_file(&source);

        let artifact = PdfOcrAlignmentArtifact {
            version: 0,
            quality_class: crate::epub_loader::PdfOcrGeometryQualityClass::OcrMixedTrust,
            source_kind: crate::epub_loader::PdfOcrSourceKind::OcrText,
            sentence_count: 3,
            mapped_sentence_count: 2,
            rect_mapped_sentence_count: 1,
            line_mapped_sentence_count: 1,
            block_mapped_sentence_count: 0,
            page_only_sentence_count: 0,
            unmappable_sentence_count: 1,
            highlightable_sentence_count: 2,
            token_lineage_available: false,
            deterministic: true,
            reused_alignment_count: 1,
            rebuilt_alignment_count: 1,
            alignment_build_ms: 4,
            page_build_ms: vec![2, 2],
            chunk_build_ms: vec![4],
            cross_column_alignment_count: 0,
            cross_column_confident_alignment_count: 0,
            degraded_reasons: vec!["line_window_fuzzy_alignment".to_string()],
            explanation: "Test OCR alignment".to_string(),
            page_buckets: vec![PdfOcrPageAlignmentBucket {
                page_idx: 2,
                sentence_indexes: vec![1],
                highlightable_sentence_count: 1,
            }],
            blocks: vec![],
            lines: vec![],
            tokens: vec![],
            page_geometry: vec![],
            alignments: vec![PdfOcrSentenceAlignment {
                sentence_idx: 1,
                sentence_text_hash: stable_sentence_text_hash("Example sentence."),
                page_idx: Some(2),
                rects: vec![],
                line_rects: vec![PdfRect {
                    left: 0.1,
                    top: 0.2,
                    width: 0.3,
                    height: 0.04,
                }],
                block_rects: vec![],
                confidence_tier: "line_fallback".to_string(),
                fallback_reason: "line_window_fuzzy_alignment".to_string(),
                token_lineage: Vec::new(),
                score: 0.74,
                crosses_column_boundaries: false,
                cross_column_confident: false,
            }],
        };

        persist_pdf_ocr_alignment_artifact(&source, &artifact);
        let loaded = load_pdf_ocr_alignment_artifact(&source)
            .expect("pdf ocr alignment artifact should load");

        assert_eq!(loaded.version, 2);
        assert_eq!(loaded.quality_class, artifact.quality_class);
        assert_eq!(loaded.source_kind, artifact.source_kind);
        assert_eq!(loaded.mapped_sentence_count, artifact.mapped_sentence_count);
        assert_eq!(loaded.alignments, artifact.alignments);

        cleanup_source_and_cache(&source);
    }

    #[test]
    fn pdf_render_precomputed_state_roundtrip_preserves_page_texts_and_sentence_hints() {
        let source = unique_source_path("pdf");
        write_source_file(&source);

        let artifact = PdfRenderPrecomputedState {
            version: 0,
            page_texts: vec!["Page one".to_string(), "Page two".to_string()],
            sentence_page_hints: vec![
                PdfSentencePageHint { page_idx: Some(0) },
                PdfSentencePageHint { page_idx: Some(1) },
                PdfSentencePageHint { page_idx: None },
            ],
            source: "native_python_backend".to_string(),
        };

        persist_pdf_render_precomputed_state(&source, &artifact);
        let loaded = load_pdf_render_precomputed_state(&source)
            .expect("pdf render precompute artifact should load");

        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.page_texts, artifact.page_texts);
        assert_eq!(loaded.sentence_page_hints, artifact.sentence_page_hints);
        assert_eq!(loaded.source, artifact.source);

        cleanup_source_and_cache(&source);
    }

    #[test]
    fn load_pdf_sync_meta_removes_corrupt_artifact() {
        let source = unique_source_path("pdf");
        write_source_file(&source);

        let meta_path = hash_dir(&source).join("content").join("pdf-sync-meta.toml");
        if let Some(parent) = meta_path.parent() {
            fs::create_dir_all(parent).expect("create pdf sync meta dir");
        }
        fs::write(&meta_path, "not = [valid").expect("write corrupt pdf sync meta");

        let loaded = load_pdf_sync_meta(&source);
        assert!(loaded.is_none());
        assert!(
            !meta_path.exists(),
            "corrupt pdf sync meta should be removed"
        );

        cleanup_source_and_cache(&source);
    }

    #[test]
    fn delete_recent_source_and_cache_removes_pdf_sidecar_artifacts() {
        let source = unique_source_path("pdf");
        write_source_file(&source);
        persist_dual_view_artifacts(&source, "Alpha. Beta.", Some("# Alpha"), None);
        persist_pdf_sync_meta(
            &source,
            crate::epub_loader::PdfGeometryMode::HighTextTrust,
            crate::epub_loader::PdfSyncStrategy::SentenceSpans,
            None,
            None,
        );
        let meta_path = hash_dir(&source).join("content").join("pdf-sync-meta.toml");
        let tts_text_path = hash_dir(&source).join("content").join("tts-text.txt");
        assert!(
            meta_path.exists(),
            "pdf sync meta should exist before delete"
        );
        assert!(
            tts_text_path.exists(),
            "tts text artifact should exist before delete"
        );

        delete_recent_source_and_cache(&source).expect("delete source and cache");

        assert!(
            !meta_path.exists(),
            "pdf sync meta should be removed with the cache directory"
        );
        assert!(
            !tts_text_path.exists(),
            "tts text artifact should be removed with the cache directory"
        );
    }

    #[test]
    fn epub_config_roundtrip_preserves_reader_fields() {
        let source = unique_source_path("epub");
        write_source_file(&source);

        let mut cfg = AppConfig::default();
        cfg.font_size = 29;
        cfg.lines_per_page = 731;
        cfg.margin_horizontal = 123;
        cfg.pause_after_sentence = 0.19;
        cfg.tts_speed = 2.7;
        cfg.key_toggle_tts = "ctrl+alt+y".to_string();

        save_epub_config(&source, &cfg);
        let loaded = load_epub_config(&source).expect("config should load");

        assert_eq!(loaded.font_size, 29);
        assert_eq!(loaded.lines_per_page, 731);
        assert_eq!(loaded.margin_horizontal, 123);
        assert!((loaded.pause_after_sentence - 0.19).abs() < f32::EPSILON);
        assert!((loaded.tts_speed - 2.7).abs() < f32::EPSILON);
        assert_eq!(loaded.key_toggle_tts, "ctrl+alt+y");

        cleanup_source_and_cache(&source);
    }

    #[test]
    fn clipboard_recent_title_uses_first_non_empty_line() {
        let source = cache_root()
            .join("clipboard")
            .join(format!("clipboard-title-{}.txt", std::process::id()));
        if let Some(parent) = source.parent() {
            fs::create_dir_all(parent).expect("create clipboard cache dir");
        }
        fs::write(
            &source,
            "\n\n   \nFirst clipboard line with useful context\nSecond line",
        )
        .expect("write clipboard source");

        let title = infer_recent_title(&source);
        assert_eq!(title, "First clipboard line with useful context");

        let _ = fs::remove_file(&source);
    }

    #[test]
    fn delete_recent_source_and_cache_is_ok_when_paths_are_missing() {
        let source = unique_source_path("epub");
        cleanup_source_and_cache(&source);

        let result = delete_recent_source_and_cache(&source);
        assert!(result.is_ok());
    }

    #[test]
    fn delete_recent_source_and_cache_is_idempotent() {
        let source = unique_source_path("txt");
        write_source_file(&source);

        let cache_path = hash_dir(&source);
        fs::create_dir_all(&cache_path).expect("create cache dir");
        fs::write(cache_path.join("bookmark.toml"), "page = 1").expect("write cache marker");

        let first = delete_recent_source_and_cache(&source);
        assert!(first.is_ok());
        assert!(!source.exists());
        assert!(!cache_path.exists());

        let second = delete_recent_source_and_cache(&source);
        assert!(second.is_ok());
    }

    #[test]
    fn dual_view_artifacts_and_anchor_maps_roundtrip() {
        let source = unique_source_path("txt");
        write_source_file(&source);

        persist_dual_view_artifacts(
            &source,
            "tts text payload",
            Some("# Heading\n\nBody"),
            Some("<p>Pretty HTML</p>"),
        );
        let tts_path = hash_dir(&source).join(CONTENT_TTS_TEXT_FILE);
        let markdown_path = hash_dir(&source).join(CONTENT_READING_MARKDOWN_FILE);
        assert_eq!(
            fs::read_to_string(&tts_path).expect("read tts artifact"),
            "tts text payload"
        );
        assert!(
            fs::read_to_string(&markdown_path)
                .expect("read markdown artifact")
                .contains("Heading")
        );

        let anchors = vec![Some(0), Some(1), None, Some(3)];
        persist_sentence_anchor_map(&source, 2, &anchors);
        let loaded = load_sentence_anchor_map(&source, 2).expect("anchor map should load");
        assert_eq!(loaded, anchors);

        cleanup_source_and_cache(&source);
    }

    #[test]
    fn browser_tab_manifest_roundtrip_and_delete_removes_artifacts() {
        let snapshot = BrowserTabSnapshot {
            tab_id: 77,
            title: "Browser Article".to_string(),
            url: "https://example.com/articles/77".to_string(),
            lang: Some("en".to_string()),
            ready_state: Some("complete".to_string()),
            captured_at: Some("2026-03-06T20:00:00Z".to_string()),
            html: Some("<article><p>Hello browser tab</p></article>".to_string()),
            text: Some("Hello browser tab".to_string()),
            selection: None,
            truncation: SnapshotTruncation {
                html: SnapshotTruncationEntry::default(),
                text: SnapshotTruncationEntry::default(),
                selection: SnapshotTruncationEntry::default(),
            },
        };
        let tab = BrowserTab {
            id: 77,
            window_id: 5,
            index: Some(0),
            active: Some(true),
            audible: Some(false),
            pinned: Some(false),
            status: Some("complete".to_string()),
            title: "Browser Article".to_string(),
            url: snapshot.url.clone(),
            fav_icon_url: Some("https://example.com/favicon.ico".to_string()),
            last_accessed: Some(1.0),
        };

        let manifest_path =
            persist_browser_tab_source(&snapshot, Some(&tab)).expect("persist manifest");
        let manifest = load_browser_tab_manifest(&manifest_path).expect("load manifest");
        assert_eq!(manifest.tab_id, 77);
        assert_eq!(manifest.window_id, Some(5));
        assert_eq!(manifest.title, "Browser Article");
        assert!(
            manifest
                .raw_html_path
                .as_ref()
                .is_some_and(|path| path.exists())
        );
        assert!(manifest.html_path.exists());
        assert!(manifest.text_path.exists());
        assert_eq!(infer_recent_title(&manifest_path), "Browser Article");
        assert!(
            infer_recent_snippet(&manifest_path, "Browser Article").contains("Hello browser tab")
        );

        delete_recent_source_and_cache(&manifest_path).expect("delete browser tab recent");
        assert!(!manifest_path.exists());
    }

    #[test]
    fn list_recent_books_prunes_stale_recent_entry_dirs() {
        let source = unique_source_path("txt");
        write_source_file(&source);
        remember_source_path(&source);
        let cache_path = hash_dir(&source);
        assert!(cache_path.exists());

        fs::remove_file(&source).expect("remove source file");

        let recents = list_recent_books(20);
        assert!(!recents.iter().any(|r| r.source_path == source));
        assert!(
            !cache_path.exists(),
            "Cache directory should have been pruned for missing source"
        );
    }

    #[test]
    fn browser_tab_asset_rehydrate_decodes_html_entities_and_fetches_assets() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let addr = listener.local_addr().expect("local addr");
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut buffer = [0_u8; 4096];
                let read = stream.read(&mut buffer).expect("read");
                let request = String::from_utf8_lossy(&buffer[..read]);
                let first_line = request.lines().next().unwrap_or_default().to_string();
                let path = first_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_string();
                let (status, content_type, body) =
                    if path.starts_with("/site.css?lang=en&modules=site.styles") {
                        (
                            "200 OK",
                            "text/css; charset=utf-8",
                            ".hero{background-image:url('/img.png')}"
                                .as_bytes()
                                .to_vec(),
                        )
                    } else if path == "/img.png" {
                        ("200 OK", "image/png", vec![137, 80, 78, 71, 13, 10, 26, 10])
                    } else {
                        ("404 Not Found", "text/plain", b"missing".to_vec())
                    };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("write headers");
                stream.write_all(&body).expect("write body");
            }
        });

        let dir = cache_root().join("test-sources").join(format!(
            "browser-tab-rehydrate-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).expect("create dir");
        let html_path = dir.join("snapshot.html");
        let text_path = dir.join("snapshot.txt");
        let manifest_path = dir.join("browser-tab.lltab");
        fs::write(
            &html_path,
            format!(
                r#"<html><head><link rel="stylesheet" href="http://{addr}/site.css?lang=en&amp;modules=site.styles"></head><body><article class="hero"><img src="http://{addr}/img.png"></article></body></html>"#
            ),
        )
        .expect("write html");
        fs::write(&text_path, "hello").expect("write text");

        let manifest = BrowserTabSourceManifest {
            manifest_version: BROWSER_TAB_MANIFEST_VERSION,
            tab_id: 1,
            window_id: Some(1),
            title: "Example".to_string(),
            url: format!("http://{addr}/article"),
            lang: Some("en".to_string()),
            ready_state: Some("complete".to_string()),
            captured_at: None,
            favicon_url: None,
            active: Some(true),
            audible: Some(false),
            pinned: Some(false),
            raw_html_path: Some(html_path.clone()),
            html_path: html_path.clone(),
            text_path: text_path.clone(),
            asset_dir: None,
            assets: Vec::new(),
            html_truncated: false,
            text_truncated: false,
        };
        fs::write(
            &manifest_path,
            toml::to_string(&manifest).expect("manifest toml"),
        )
        .expect("write manifest");

        rehydrate_browser_tab_manifest_assets(&manifest_path).expect("rehydrate");
        let hydrated = load_browser_tab_manifest(&manifest_path).expect("reload manifest");
        let hydrated_html = fs::read_to_string(&html_path).expect("hydrated html");
        assert!(!hydrated.assets.is_empty());
        assert!(hydrated_html.contains("<style data-ll-origin-href="));

        let _ = fs::remove_dir_all(&dir);
        server.join().expect("join server");
    }

    #[test]
    fn browser_tab_rehydrate_upgrades_legacy_manifest_text_even_with_existing_assets() {
        let dir = cache_root().join("test-sources").join(format!(
            "browser-tab-upgrade-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).expect("create dir");
        let html_path = dir.join("snapshot.html");
        let text_path = dir.join("snapshot.txt");
        let manifest_path = dir.join("browser-tab.lltab");
        let asset_dir = dir.join("assets");
        fs::create_dir_all(&asset_dir).expect("create asset dir");
        let asset_path = asset_dir.join("placeholder.png");
        fs::write(&asset_path, [137_u8, 80, 78, 71, 13, 10, 26, 10]).expect("write asset");
        fs::write(
            &html_path,
            r#"<html><head><title>Example</title></head><body><nav>Site menu</nav><article><p>First article sentence.</p><p>Second article sentence.</p></article></body></html>"#,
        )
        .expect("write html");
        fs::write(&text_path, "Site menu\nLegacy text").expect("write text");

        let manifest = BrowserTabSourceManifest {
            manifest_version: 0,
            tab_id: 2,
            window_id: Some(1),
            title: "Example".to_string(),
            url: "https://example.com/article".to_string(),
            lang: Some("en".to_string()),
            ready_state: Some("complete".to_string()),
            captured_at: None,
            favicon_url: None,
            active: Some(true),
            audible: Some(false),
            pinned: Some(false),
            raw_html_path: Some(html_path.clone()),
            html_path: html_path.clone(),
            text_path: text_path.clone(),
            asset_dir: Some(asset_dir.clone()),
            assets: vec![BrowserTabAsset {
                raw_path: "https://example.com/placeholder.png".to_string(),
                local_path: asset_path,
                kind: "image".to_string(),
            }],
            html_truncated: false,
            text_truncated: false,
        };
        fs::write(
            &manifest_path,
            toml::to_string(&manifest).expect("manifest toml"),
        )
        .expect("write manifest");

        rehydrate_browser_tab_manifest_assets(&manifest_path).expect("rehydrate");
        let hydrated = load_browser_tab_manifest(&manifest_path).expect("reload manifest");
        let hydrated_text = fs::read_to_string(&text_path).expect("hydrated text");
        let hydrated_html = fs::read_to_string(&html_path).expect("hydrated html");

        assert_eq!(hydrated.manifest_version, BROWSER_TAB_MANIFEST_VERSION);
        assert!(!hydrated_text.contains("Site menu"));
        assert!(hydrated_text.contains("First article sentence."));
        assert!(hydrated_html.contains("data-ll-browser-tab-focused=\"1\""));

        let _ = fs::remove_dir_all(&dir);
    }
}
