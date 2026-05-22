use crate::config::{AppConfig, parse_config, serialize_config};
use crate::epub_loader::PdfOcrGeometryQualityClass;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

use super::hash_dir;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Bookmark {
    pub page: usize,
    #[serde(default)]
    pub sentence_idx: Option<usize>,
    #[serde(default)]
    pub sentence_text: Option<String>,
    #[serde(default = "default_scroll")]
    pub scroll_y: f32,
    #[serde(default)]
    pub pdf_page_idx: Option<usize>,
    #[serde(default)]
    pub pdf_rects: Vec<PdfRect>,
    #[serde(default)]
    pub pdf_line_rects: Vec<PdfRect>,
    #[serde(default)]
    pub pdf_block_rects: Vec<PdfRect>,
    #[serde(default)]
    pub pdf_confidence: Option<String>,
    #[serde(default)]
    pub pdf_reason: Option<String>,
    #[serde(default)]
    pub pdf_quality_class: Option<PdfOcrGeometryQualityClass>,
    #[serde(default)]
    pub pdf_sentence_text_hash: Option<String>,
    #[serde(default)]
    pub pdf_token_lineage: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PdfRect {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
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
    #[serde(default)]
    pdf_page_idx: Option<usize>,
    #[serde(default)]
    pdf_rects: Vec<PdfRect>,
    #[serde(default)]
    pdf_line_rects: Vec<PdfRect>,
    #[serde(default)]
    pdf_block_rects: Vec<PdfRect>,
    #[serde(default)]
    pdf_confidence: Option<String>,
    #[serde(default)]
    pdf_reason: Option<String>,
    #[serde(default)]
    pdf_quality_class: Option<PdfOcrGeometryQualityClass>,
    #[serde(default)]
    pdf_sentence_text_hash: Option<String>,
    #[serde(default)]
    pdf_token_lineage: Vec<String>,
}

fn default_scroll() -> f32 {
    0.0
}

fn bookmark_path(source_path: &Path) -> PathBuf {
    hash_dir(source_path).join("bookmark.toml")
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn load_bookmark(source_path: &Path) -> Option<Bookmark> {
    let path = bookmark_path(source_path);
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
        pdf_page_idx: value.pdf_page_idx,
        pdf_rects: value.pdf_rects,
        pdf_line_rects: value.pdf_line_rects,
        pdf_block_rects: value.pdf_block_rects,
        pdf_confidence: value.pdf_confidence,
        pdf_reason: value.pdf_reason,
        pdf_quality_class: value.pdf_quality_class,
        pdf_sentence_text_hash: value.pdf_sentence_text_hash,
        pdf_token_lineage: value.pdf_token_lineage,
    })
}

#[cfg(target_arch = "wasm32")]
pub(super) fn load_bookmark(_source_path: &Path) -> Option<Bookmark> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn save_bookmark(source_path: &Path, bookmark: &Bookmark) {
    let path = bookmark_path(source_path);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let entry = CacheEntry {
        page: bookmark.page,
        sentence_idx: bookmark.sentence_idx,
        sentence_text: bookmark.sentence_text.clone(),
        scroll_y: Some(bookmark.scroll_y),
        pdf_page_idx: bookmark.pdf_page_idx,
        pdf_rects: bookmark.pdf_rects.clone(),
        pdf_line_rects: bookmark.pdf_line_rects.clone(),
        pdf_block_rects: bookmark.pdf_block_rects.clone(),
        pdf_confidence: bookmark.pdf_confidence.clone(),
        pdf_reason: bookmark.pdf_reason.clone(),
        pdf_quality_class: bookmark.pdf_quality_class,
        pdf_sentence_text_hash: bookmark.pdf_sentence_text_hash.clone(),
        pdf_token_lineage: bookmark.pdf_token_lineage.clone(),
    };
    if let Ok(contents) = toml::to_string(&entry)
        && let Ok(mut file) = fs::File::create(path)
    {
        if let Err(err) = file.write_all(contents.as_bytes()) {
            warn!("Failed to persist last page: {err}");
        } else {
            debug!(page = bookmark.page, "Saved last page bookmark");
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub(super) fn save_bookmark(_source_path: &Path, _bookmark: &Bookmark) {
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn load_epub_config(source_path: &Path) -> Option<AppConfig> {
    let path = hash_dir(source_path).join("config.toml");
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

#[cfg(target_arch = "wasm32")]
pub(super) fn load_epub_config(_source_path: &Path) -> Option<AppConfig> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn save_epub_config(source_path: &Path, config: &AppConfig) {
    let dir = hash_dir(source_path);
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

#[cfg(target_arch = "wasm32")]
pub(super) fn save_epub_config(_source_path: &Path, _config: &AppConfig) {
}
