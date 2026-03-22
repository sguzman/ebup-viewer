use std::path::{Path, PathBuf};

use crate::{browser_tabs, cache, config};
use tracing::{debug, info, warn};

pub trait CacheService: Send + Sync {
    fn save_bookmark(&self, source_path: &Path, bookmark: &cache::Bookmark);
    fn save_epub_config(&self, source_path: &Path, config: &config::AppConfig);
    fn delete_recent_source_and_cache(&self, source_path: &Path) -> Result<(), String>;
    fn remember_source_path(&self, source_path: &Path);
    fn persist_clipboard_text_source(&self, text: &str) -> Result<PathBuf, String>;
    fn persist_browser_tab_source(
        &self,
        snapshot: &browser_tabs::BrowserTabSnapshot,
        tab_meta: Option<&browser_tabs::BrowserTab>,
    ) -> Result<PathBuf, String>;
    fn persist_browser_tab_bundle_source(
        &self,
        capture: &browser_tabs::BrowserTabBundleCapture,
        tab_meta: Option<&browser_tabs::BrowserTab>,
    ) -> Result<PathBuf, String>;
    fn load_browser_tab_manifest(
        &self,
        source_path: &Path,
    ) -> Result<cache::BrowserTabSourceManifest, String>;
    fn persist_pdf_sentence_map(
        &self,
        source_path: &Path,
        locations: &[cache::PdfSentenceLocation],
    );
    fn persist_pdf_render_precomputed_state(
        &self,
        source_path: &Path,
        artifact: &cache::PdfRenderPrecomputedState,
    );
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FilesystemCacheService;

impl CacheService for FilesystemCacheService {
    fn save_bookmark(&self, source_path: &Path, bookmark: &cache::Bookmark) {
        cache::save_bookmark(source_path, bookmark);
        debug!(
            source_path = %source_path.display(),
            "Saved bookmark"
        );
    }

    fn save_epub_config(&self, source_path: &Path, config: &config::AppConfig) {
        cache::save_epub_config(source_path, config);
        debug!(
            source_path = %source_path.display(),
            "Saved epub config"
        );
    }

    fn delete_recent_source_and_cache(&self, source_path: &Path) -> Result<(), String> {
        match cache::delete_recent_source_and_cache(source_path) {
            Ok(()) => {
                info!(
                    source_path = %source_path.display(),
                    "Deleted recent source and cache"
                );
                Ok(())
            }
            Err(err) => {
                warn!(
                    source_path = %source_path.display(),
                    "Failed to delete recent source and cache: {err}"
                );
                Err(err.to_string())
            }
        }
    }

    fn remember_source_path(&self, source_path: &Path) {
        cache::remember_source_path(source_path);
        debug!(
            source_path = %source_path.display(),
            "Remembered source path"
        );
    }

    fn persist_clipboard_text_source(&self, text: &str) -> Result<PathBuf, String> {
        match cache::persist_clipboard_text_source(text) {
            Ok(path) => {
                info!(
                    source_path = %path.display(),
                    "Persisted clipboard text source"
                );
                Ok(path)
            }
            Err(err) => {
                warn!("Failed to persist clipboard text source: {err}");
                Err(err)
            }
        }
    }

    fn persist_browser_tab_source(
        &self,
        snapshot: &browser_tabs::BrowserTabSnapshot,
        tab_meta: Option<&browser_tabs::BrowserTab>,
    ) -> Result<PathBuf, String> {
        match cache::persist_browser_tab_source(snapshot, tab_meta) {
            Ok(path) => {
                info!(
                    source_path = %path.display(),
                    "Persisted browser tab source"
                );
                Ok(path)
            }
            Err(err) => {
                warn!("Failed to persist browser tab source: {err}");
                Err(err)
            }
        }
    }

    fn persist_browser_tab_bundle_source(
        &self,
        capture: &browser_tabs::BrowserTabBundleCapture,
        tab_meta: Option<&browser_tabs::BrowserTab>,
    ) -> Result<PathBuf, String> {
        match cache::persist_browser_tab_bundle_source(capture, tab_meta) {
            Ok(path) => {
                info!(
                    source_path = %path.display(),
                    "Persisted browser tab bundle source"
                );
                Ok(path)
            }
            Err(err) => {
                warn!("Failed to persist browser tab bundle source: {err}");
                Err(err)
            }
        }
    }

    fn load_browser_tab_manifest(
        &self,
        source_path: &Path,
    ) -> Result<cache::BrowserTabSourceManifest, String> {
        match cache::load_browser_tab_manifest(source_path) {
            Some(manifest) => {
                debug!(
                    source_path = %source_path.display(),
                    "Loaded browser tab manifest"
                );
                Ok(manifest)
            }
            None => {
                warn!(
                    source_path = %source_path.display(),
                    "Browser tab manifest missing"
                );
                Err("browser_tab_manifest_missing".to_string())
            }
        }
    }

    fn persist_pdf_sentence_map(
        &self,
        source_path: &Path,
        locations: &[cache::PdfSentenceLocation],
    ) {
        cache::persist_pdf_sentence_map(source_path, locations);
        info!(
            source_path = %source_path.display(),
            sentence_count = locations.len(),
            "Persisted pdf sentence map"
        );
    }

    fn persist_pdf_render_precomputed_state(
        &self,
        source_path: &Path,
        artifact: &cache::PdfRenderPrecomputedState,
    ) {
        cache::persist_pdf_render_precomputed_state(source_path, artifact);
        info!(
            source_path = %source_path.display(),
            "Persisted pdf render precomputed state"
        );
    }
}
