use std::path::{Path, PathBuf};

use crate::{browser_tabs, cache, config};

pub trait CacheService: Send + Sync {
    fn save_bookmark(&self, source_path: &Path, bookmark: &cache::Bookmark);
    fn save_epub_config(&self, source_path: &Path, config: &config::AppConfig);
    fn delete_recent_source_and_cache(&self, source_path: &Path) -> Result<(), String>;
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
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FilesystemCacheService;

impl CacheService for FilesystemCacheService {
    fn save_bookmark(&self, source_path: &Path, bookmark: &cache::Bookmark) {
        cache::save_bookmark(source_path, bookmark);
    }

    fn save_epub_config(&self, source_path: &Path, config: &config::AppConfig) {
        cache::save_epub_config(source_path, config);
    }

    fn delete_recent_source_and_cache(&self, source_path: &Path) -> Result<(), String> {
        cache::delete_recent_source_and_cache(source_path).map_err(|err| err.to_string())
    }

    fn persist_clipboard_text_source(&self, text: &str) -> Result<PathBuf, String> {
        cache::persist_clipboard_text_source(text)
    }

    fn persist_browser_tab_source(
        &self,
        snapshot: &browser_tabs::BrowserTabSnapshot,
        tab_meta: Option<&browser_tabs::BrowserTab>,
    ) -> Result<PathBuf, String> {
        cache::persist_browser_tab_source(snapshot, tab_meta)
    }

    fn persist_browser_tab_bundle_source(
        &self,
        capture: &browser_tabs::BrowserTabBundleCapture,
        tab_meta: Option<&browser_tabs::BrowserTab>,
    ) -> Result<PathBuf, String> {
        cache::persist_browser_tab_bundle_source(capture, tab_meta)
    }

    fn load_browser_tab_manifest(
        &self,
        source_path: &Path,
    ) -> Result<cache::BrowserTabSourceManifest, String> {
        cache::load_browser_tab_manifest(source_path)
            .ok_or_else(|| "browser_tab_manifest_missing".to_string())
    }
}
