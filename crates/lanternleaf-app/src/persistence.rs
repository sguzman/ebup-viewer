use crate::contracts::BridgeError;
use crate::contracts::ReaderPlaybackState;
use crate::pipeline::PersistenceTrigger;
use lanternleaf_core::{cache, cache_service, config};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub struct PersistenceItem {
    pub name: &'static str,
    pub kind: &'static str,
    pub owner: &'static str,
    pub path_hint: &'static str,
    pub version_hint: Option<&'static str>,
    pub notes: &'static str,
}

#[derive(Debug, Clone)]
pub struct PersistenceInventory {
    pub items: Vec<PersistenceItem>,
}

impl PersistenceInventory {
    pub fn default_inventory() -> Self {
        Self {
            items: vec![
                PersistenceItem {
                    name: "base config",
                    kind: "config",
                    owner: "config::io",
                    path_hint: "conf/config.toml",
                    version_hint: None,
                    notes: "Loaded on startup; defaults applied on parse errors.",
                },
                PersistenceItem {
                    name: "per-book config overrides",
                    kind: "config",
                    owner: "cache::bookmarks_config",
                    path_hint: ".cache/lantern-leaf/<source_hash>/epub-config.toml",
                    version_hint: None,
                    notes: "Overrides applied when opening a source.",
                },
                PersistenceItem {
                    name: "bookmarks",
                    kind: "bookmark",
                    owner: "cache::bookmarks_config",
                    path_hint: ".cache/lantern-leaf/<source_hash>/bookmark.toml",
                    version_hint: None,
                    notes: "Saved on open/close/safe quit.",
                },
                PersistenceItem {
                    name: "recent books",
                    kind: "recent",
                    owner: "cache::list_recent_books",
                    path_hint: ".cache/lantern-leaf/<source_hash>/source.txt",
                    version_hint: None,
                    notes: "Derived from cache directories and source hints.",
                },
                PersistenceItem {
                    name: "content artifacts",
                    kind: "content",
                    owner: "cache::content_artifacts",
                    path_hint: ".cache/lantern-leaf/<source_hash>/content/*",
                    version_hint: Some("content layout: dual-view-v3"),
                    notes: "Includes tts-text, markdown, html, and PDF artifacts.",
                },
                PersistenceItem {
                    name: "thumbnails",
                    kind: "image",
                    owner: "cache::infer_recent_thumbnail",
                    path_hint: ".cache/lantern-leaf/<source_hash>/thumbnail*",
                    version_hint: None,
                    notes: "Stored alongside recents for fast preview.",
                },
                PersistenceItem {
                    name: "PDF sync/OCR artifacts",
                    kind: "pdf",
                    owner: "cache::content_artifacts",
                    path_hint: ".cache/lantern-leaf/<source_hash>/content/pdf-*",
                    version_hint: Some("pdf sync meta v3, ocr alignment v2, render precompute v1"),
                    notes: "Rebuilt when signatures or versions change.",
                },
                PersistenceItem {
                    name: "browser-tab snapshots",
                    kind: "browser_tab",
                    owner: "cache::browser_tab_cache",
                    path_hint: ".cache/lantern-leaf/browser-tabs/<digest>/",
                    version_hint: None,
                    notes: "Optional artifacts for browser-tab ingestion.",
                },
                PersistenceItem {
                    name: "calibre cache + thumbnails",
                    kind: "calibre",
                    owner: "calibre::cache_store",
                    path_hint: ".cache/lantern-leaf/calibre/*",
                    version_hint: None,
                    notes: "Catalog cache + downloaded thumbnails.",
                },
            ],
        }
    }

    pub fn log(&self) {
        for item in &self.items {
            info!(
                name = item.name,
                kind = item.kind,
                owner = item.owner,
                path = item.path_hint,
                version = item.version_hint.unwrap_or("none"),
                notes = item.notes,
                "Persistence inventory item"
            );
        }
    }
}

#[derive(Debug, Clone)]
pub struct PersistencePolicy {
    pub cache_layout_version: &'static str,
    pub compatibility_rules: Vec<&'static str>,
    pub invalidation_rules: Vec<&'static str>,
}

impl PersistencePolicy {
    pub fn default_policy() -> Self {
        Self {
            cache_layout_version: "dual-view-v3",
            compatibility_rules: vec![
                "Load existing bookmark/config formats when parseable; fall back to defaults.",
                "Respect PDF sync metadata when signature matches source contents.",
                "Honor cache-root override from config or environment.",
            ],
            invalidation_rules: vec![
                "Invalidate content artifacts when layout version changes.",
                "Invalidate PDF sync/OCR artifacts when version/signature mismatches.",
                "Rebuild normalized page caches when config hash changes.",
            ],
        }
    }

    pub fn log(&self) {
        info!(
            cache_layout_version = self.cache_layout_version,
            "Persistence policy: cache layout"
        );
        for rule in &self.compatibility_rules {
            info!(rule = *rule, "Persistence policy: compatibility");
        }
        for rule in &self.invalidation_rules {
            info!(rule = *rule, "Persistence policy: invalidation");
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReaderHousekeeping {
    pub source_path: String,
    pub bookmark: cache::Bookmark,
    pub config: config::AppConfig,
    pub playback: Option<ReaderPlaybackState>,
}

impl ReaderHousekeeping {
    pub fn from_parts(
        source_path: impl Into<String>,
        bookmark: cache::Bookmark,
        config: config::AppConfig,
        playback: Option<ReaderPlaybackState>,
    ) -> Self {
        Self { source_path: source_path.into(), bookmark, config, playback }
    }
}

pub trait PersistenceService: Send + Sync {
    fn persist_reader_housekeeping(
        &self,
        housekeeping: ReaderHousekeeping,
    ) -> Result<(), BridgeError>;

    fn load_bookmark(&self, source_path: &Path) -> Option<cache::Bookmark>;

    fn load_epub_config(&self, source_path: &Path) -> Option<config::AppConfig>;

    fn list_recent_books(&self, limit: usize) -> Vec<cache::RecentBook>;

    fn delete_recent_book(&self, source_path: &Path) -> Result<(), String>;
    fn start_sync_thread(&self, _event_tx: std::sync::mpsc::Sender<crate::pipeline::AppEvent>) {}
}

pub struct FilesystemPersistenceService {
    cache_service: Arc<dyn cache_service::CacheService>,
}

impl FilesystemPersistenceService {
    pub fn new(cache_service: Arc<dyn cache_service::CacheService>) -> Self {
        Self { cache_service }
    }
}

impl Default for FilesystemPersistenceService {
    fn default() -> Self {
        Self::new(Arc::new(cache_service::FilesystemCacheService))
    }
}

impl PersistenceService for FilesystemPersistenceService {
    fn persist_reader_housekeeping(
        &self,
        housekeeping: ReaderHousekeeping,
    ) -> Result<(), BridgeError> {
        let source_path = Path::new(&housekeeping.source_path);
        self.cache_service.remember_source_path(source_path);
        self.cache_service
            .save_bookmark(source_path, &housekeeping.bookmark);
        self.cache_service
            .save_epub_config(source_path, &housekeeping.config);
        Ok(())
    }

    fn load_bookmark(&self, source_path: &Path) -> Option<cache::Bookmark> {
        cache::load_bookmark(source_path)
    }

    fn load_epub_config(&self, source_path: &Path) -> Option<config::AppConfig> {
        cache::load_epub_config(source_path)
    }

    fn list_recent_books(&self, limit: usize) -> Vec<cache::RecentBook> {
        self.cache_service.list_recent_books(limit)
    }

    fn delete_recent_book(&self, source_path: &Path) -> Result<(), String> {
        self.cache_service
            .delete_recent_source_and_cache(source_path)
    }
}

pub struct RemotePersistenceService {
    server_url: String,
    client: reqwest::blocking::Client,
}

impl RemotePersistenceService {
    pub fn new(server_url: String) -> Self {
        Self {
            server_url,
            client: reqwest::blocking::Client::new(),
        }
    }

    fn book_url(&self, source_path: &Path) -> String {
        let hash = cache::source_hash(source_path);
        format!("{}/api/v1/book/{}", self.server_url, hash)
    }

    pub fn start_sync_thread(&self, event_tx: std::sync::mpsc::Sender<crate::pipeline::AppEvent>) {
        use tungstenite::connect;
        use url::Url;

        let ws_url = self
            .server_url
            .replace("http://", "ws://")
            .replace("https://", "wss://")
            + "/api/v1/ws";
        let url = Url::parse(&ws_url).expect("Invalid server URL for WebSocket");

        std::thread::spawn(move || {
            loop {
                info!("Connecting to remote sync server at {}...", ws_url);
                match connect(url.to_string()) {
                    Ok((mut socket, _)) => {
                        info!("Connected to remote sync server.");
                        loop {
                            match socket.read() {
                                Ok(msg) => {
                                    if let tungstenite::Message::Text(text) = msg {
                                        if let Ok(state) =
                                            serde_json::from_str::<
                                                crate::contracts::ReaderPlaybackState,
                                            >(&text)
                                        {
                                            let _ = event_tx.send(crate::pipeline::AppEvent::RemotePlaybackStateUpdated(state));
                                        }
                                    }
                                }
                                Err(err) => {
                                    warn!("WebSocket read error: {}. Reconnecting...", err);
                                    break;
                                }
                            }
                        }
                    }
                    Err(err) => {
                        warn!(
                            "Failed to connect to remote sync server: {}. Retrying in 5s...",
                            err
                        );
                        std::thread::sleep(std::time::Duration::from_secs(5));
                    }
                }
            }
        });
    }
}

impl PersistenceService for RemotePersistenceService {
    fn persist_reader_housekeeping(
        &self,
        housekeeping: ReaderHousekeeping,
    ) -> Result<(), BridgeError> {
        let source_path = Path::new(&housekeeping.source_path);

        let updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let playback = crate::contracts::ReaderPlaybackState {
            source_path: housekeeping.source_path.clone(),
            current_page: housekeeping
                .playback
                .as_ref()
                .map(|p| p.current_page)
                .unwrap_or(housekeeping.bookmark.page),
            highlighted_sentence_idx: housekeeping.bookmark.sentence_idx,
            tts: housekeeping
                .playback
                .as_ref()
                .map(|p| p.tts.clone())
                .unwrap_or_else(|| crate::contracts::ReaderTtsView {
                    state: lanternleaf_core::session::TtsPlaybackState::Idle,
                    current_sentence_idx: None,
                    sentence_count: 0,
                    can_seek_prev: false,
                    can_seek_next: false,
                    progress_pct: 0.0,
                }),
            stats: housekeeping
                .playback
                .as_ref()
                .map(|p| p.stats.clone())
                .unwrap_or_else(|| crate::contracts::ReaderStats {
                    page_index: housekeeping.bookmark.page + 1,
                    total_pages: 0,
                    tts_progress_pct: 0.0,
                    global_progress_pct: 0.0,
                    page_time_remaining_secs: 0.0,
                    book_time_remaining_secs: 0.0,
                    page_word_count: 0,
                    page_sentence_count: 0,
                    page_start_percent: 0.0,
                    page_end_percent: 0.0,
                    words_read_up_to_page_start: 0,
                    sentences_read_up_to_page_start: 0,
                    words_read_up_to_page_end: 0,
                    sentences_read_up_to_page_end: 0,
                    words_read_up_to_current_position: 0,
                    sentences_read_up_to_current_position: 0,
                }),
            updated_at,
        };

        let update = serde_json::json!({
            "bookmark": housekeeping.bookmark,
            "config": housekeeping.config,
            "playback": playback,
        });

        self.client
            .post(self.book_url(source_path))
            .json(&update)
            .send()
            .map_err(|err| BridgeError {
                code: "network_error".to_string(),
                message: err.to_string(),
            })?;

        Ok(())
    }

    fn load_bookmark(&self, source_path: &Path) -> Option<cache::Bookmark> {
        let resp = self.client.get(self.book_url(source_path)).send().ok()?;
        if resp.status().is_success() {
            let data: serde_json::Value = resp.json().ok()?;
            serde_json::from_value(data.get("bookmark")?.clone()).ok()
        } else {
            None
        }
    }

    fn load_epub_config(&self, source_path: &Path) -> Option<config::AppConfig> {
        let resp = self.client.get(self.book_url(source_path)).send().ok()?;
        if resp.status().is_success() {
            let data: serde_json::Value = resp.json().ok()?;
            serde_json::from_value(data.get("config")?.clone()).ok()
        } else {
            None
        }
    }

    fn list_recent_books(&self, limit: usize) -> Vec<cache::RecentBook> {
        let url = format!("{}/api/v1/recent?limit={}", self.server_url, limit);
        match self.client.get(&url).send() {
            Ok(resp) if resp.status().is_success() => {
                let remote_books: Vec<crate::contracts::RecentBook> =
                    resp.json().unwrap_or_default();
                remote_books
                    .into_iter()
                    .map(|b| cache::RecentBook {
                        source_path: PathBuf::from(b.source_path),
                        display_title: b.display_title,
                        snippet: b.snippet,
                        thumbnail_path: b.thumbnail_path.map(PathBuf::from),
                        last_opened_unix_secs: b.last_opened_unix_secs,
                        browser_tab_id: b.browser_tab_id,
                        browser_window_id: b.browser_window_id,
                    })
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    fn delete_recent_book(&self, source_path: &Path) -> Result<(), String> {
        let hash = cache::source_hash(source_path);
        let url = format!("{}/api/v1/book/{}", self.server_url, hash);
        match self.client.delete(&url).send() {
            Ok(resp) if resp.status().is_success() => Ok(()),
            _ => Err(format!("Failed to delete remote book: {}", hash)),
        }
    }

    fn start_sync_thread(&self, event_tx: std::sync::mpsc::Sender<crate::pipeline::AppEvent>) {
        self.start_sync_thread(event_tx);
    }
}

pub struct PersistenceLifecycle {
    service: std::sync::Arc<dyn PersistenceService>,
    inventory: PersistenceInventory,
    policy: PersistencePolicy,
}

impl PersistenceLifecycle {
    pub fn new(service: Arc<dyn PersistenceService>) -> Self {
        Self {
            service,
            inventory: PersistenceInventory::default_inventory(),
            policy: PersistencePolicy::default_policy(),
        }
    }

    pub fn service(&self) -> std::sync::Arc<dyn PersistenceService> {
        std::sync::Arc::clone(&self.service)
    }

    pub fn start_sync_thread(&self, event_tx: std::sync::mpsc::Sender<crate::pipeline::AppEvent>) {
        self.service.start_sync_thread(event_tx);
    }

    pub fn inventory(&self) -> &PersistenceInventory {
        &self.inventory
    }

    pub fn policy(&self) -> &PersistencePolicy {
        &self.policy
    }

    pub fn on_startup(&self) {
        info!("Persistence lifecycle: startup");
        self.inventory.log();
        self.policy.log();
    }

    pub fn on_live_update(&self, housekeeping: ReaderHousekeeping, trigger: PersistenceTrigger) {
        let source_path = housekeeping.source_path.clone();
        if let Err(error) = self.service.persist_reader_housekeeping(housekeeping) {
            warn!(
                trigger = ?trigger,
                error = %error.message,
                "Live persistence failed"
            );
        } else {
            debug!(
                trigger = ?trigger,
                path = %source_path,
                "Reader housekeeping persisted"
            );
        }
    }

    pub fn on_source_open(&self, housekeeping: ReaderHousekeeping) {
        info!(
            path = %housekeeping.source_path,
            "Persistence lifecycle: source open"
        );
        if let Err(error) = self.service.persist_reader_housekeeping(housekeeping) {
            warn!(error = %error.message, "Source-open persistence failed");
        } else {
            debug!("Reader housekeeping persisted on source open");
        }
    }

    pub fn on_session_close(&self, housekeeping: ReaderHousekeeping) {
        info!(
            path = %housekeeping.source_path,
            "Persistence lifecycle: session close"
        );
        if let Err(error) = self.service.persist_reader_housekeeping(housekeeping) {
            warn!(error = %error.message, "Session close persistence failed");
        } else {
            debug!("Reader housekeeping persisted on session close");
        }
    }

    pub fn on_safe_quit(&self, housekeeping: ReaderHousekeeping) {
        info!(
            path = %housekeeping.source_path,
            "Persistence lifecycle: safe quit"
        );
        if let Err(error) = self.service.persist_reader_housekeeping(housekeeping) {
            warn!(error = %error.message, "Safe quit persistence failed");
        } else {
            debug!("Reader housekeeping persisted on safe quit");
        }
    }

    pub fn load_bookmark_and_config(
        &self,
        source_path: &Path,
    ) -> (Option<cache::Bookmark>, Option<config::AppConfig>) {
        (
            self.service.load_bookmark(source_path),
            self.service.load_epub_config(source_path),
        )
    }

    pub fn flush_trigger(
        &self,
        housekeeping: Option<ReaderHousekeeping>,
        trigger: PersistenceTrigger,
    ) {
        let has_housekeeping = housekeeping.is_some();
        debug!(
            trigger = ?trigger,
            has_housekeeping,
            "Persistence flush requested"
        );
        match (housekeeping, trigger) {
            (Some(housekeeping), PersistenceTrigger::ReaderCommand)
            | (Some(housekeeping), PersistenceTrigger::RuntimeConfigChange) => {
                self.on_live_update(housekeeping, trigger);
            }
            (Some(housekeeping), PersistenceTrigger::SourceOpen) => {
                self.on_source_open(housekeeping)
            }
            (Some(housekeeping), PersistenceTrigger::SessionClose) => {
                self.on_session_close(housekeeping)
            }
            (Some(housekeeping), PersistenceTrigger::SafeQuit) => self.on_safe_quit(housekeeping),
            _ => {
                warn!(trigger = ?trigger, "No reader snapshot available for persistence trigger");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::ReaderSnapshot;
    use crate::pipeline::PersistenceTrigger;
    use lanternleaf_core::{cache, cache_service::CacheService, config, session};
    use std::path::Path;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };

    fn make_reader_snapshot() -> ReaderSnapshot {
        session::ReaderSnapshot {
            source_path: "/tmp/book.epub".to_string(),
            source_name: "book.epub".to_string(),
            current_page: 3,
            total_pages: 12,
            text_only_mode: false,
            has_structured_markdown: true,
            pretty_kind: session::PrettyKind::Html,
            pdf_geometry_mode: None,
            pdf_sync_strategy: None,
            pdf_classification: None,
            pdf_runtime_policy: None,
            pdf_ocr_alignment: None,
            pdf_ocr_pipeline: None,
            images: Vec::new(),
            tts_text_page: "tts".to_string(),
            reading_markdown_page: None,
            reading_html_page: Some("<p>hi</p>".to_string()),
            tts_current_sentence_text: Some("one".to_string()),
            page_text: "page".to_string(),
            sentences: vec!["one".to_string()],
            canonical_sentences: vec!["one".to_string()],
            page_sentence_counts: vec![1],
            sentence_anchor_map: vec![Some(0)],
            highlighted_sentence_idx: Some(0),
            search_query: "query".to_string(),
            search_matches: vec![0],
            selected_search_match: Some(0),
            settings: session::ReaderSettingsView {
                theme: config::ThemeMode::Day,
                font_family: config::FontFamily::Lexend,
                font_weight: config::FontWeight::Bold,
                day_highlight: config::HighlightColor {
                    r: 0.1,
                    g: 0.2,
                    b: 0.3,
                    a: 0.4,
                },
                night_highlight: config::HighlightColor {
                    r: 0.5,
                    g: 0.6,
                    b: 0.7,
                    a: 0.8,
                },
                font_size: 18,
                line_spacing: 1.2,
                word_spacing: 0,
                letter_spacing: 0,
                margin_horizontal: 24,
                margin_vertical: 12,
                lines_per_page: 400,
                pause_after_sentence: 0.0,
                auto_scroll_tts: true,
                center_spoken_sentence: true,
                text_only_show_original_text: false,
                time_remaining_display: config::TimeRemainingDisplay::Adaptive,
                tts_speed: 1.0,
                tts_volume: 1.0,
                tts_backend: config::TtsBackend::Piper,
                windows_voice_id: None,
                pretty: config::PrettyUiConfig::default(),
            },
            tts: session::ReaderTtsView {
                state: session::TtsPlaybackState::Playing,
                current_sentence_idx: Some(0),
                sentence_count: 1,
                can_seek_prev: false,
                can_seek_next: false,
                progress_pct: 0.5,
            },
            stats: session::ReaderStats {
                page_index: 3,
                total_pages: 12,
                tts_progress_pct: 0.5,
                global_progress_pct: 0.25,
                page_time_remaining_secs: 10.0,
                book_time_remaining_secs: 100.0,
                page_word_count: 100,
                page_sentence_count: 1,
                page_start_percent: 0.2,
                page_end_percent: 0.3,
                words_read_up_to_page_start: 50,
                sentences_read_up_to_page_start: 2,
                words_read_up_to_page_end: 150,
                sentences_read_up_to_page_end: 3,
                words_read_up_to_current_position: 55,
                sentences_read_up_to_current_position: 2,
            },
            panels: session::PanelState {
                show_settings: true,
                show_stats: false,
                show_tts: true,
            },
        }
    }

    struct StubService {
        persisted: Arc<AtomicBool>,
        bookmark_value: Option<cache::Bookmark>,
    }

    impl StubService {
        fn new(bookmark_value: Option<cache::Bookmark>) -> Self {
            Self {
                persisted: Arc::new(AtomicBool::new(false)),
                bookmark_value,
            }
        }
    }

    impl PersistenceService for StubService {
        fn persist_reader_housekeeping(&self, _: ReaderHousekeeping) -> Result<(), BridgeError> {
            self.persisted.store(true, Ordering::SeqCst);
            Ok(())
        }

        fn load_bookmark(&self, _: &Path) -> Option<cache::Bookmark> {
            self.bookmark_value.clone()
        }

        fn load_epub_config(&self, _: &Path) -> Option<config::AppConfig> {
            None
        }

        fn list_recent_books(&self, _: usize) -> Vec<cache::RecentBook> {
            Vec::new()
        }

        fn delete_recent_book(&self, _: &Path) -> Result<(), String> {
            Ok(())
        }
    }

    fn sample_bookmark() -> cache::Bookmark {
        cache::Bookmark {
            page: 1,
            sentence_idx: None,
            sentence_text: None,
            scroll_y: 0.0,
            pdf_page_idx: None,
            pdf_rects: Vec::new(),
            pdf_line_rects: Vec::new(),
            pdf_block_rects: Vec::new(),
            pdf_confidence: None,
            pdf_reason: None,
            pdf_quality_class: None,
            pdf_sentence_text_hash: None,
            pdf_token_lineage: Vec::new(),
        }
    }

    struct TestCacheService {
        saved_bookmark: Arc<AtomicBool>,
        saved_config: Arc<Mutex<Option<config::AppConfig>>>,
    }

    impl TestCacheService {
        fn new() -> (Self, Arc<AtomicBool>, Arc<Mutex<Option<config::AppConfig>>>) {
            let saved_bookmark = Arc::new(AtomicBool::new(false));
            let saved_config = Arc::new(Mutex::new(None));
            (
                Self {
                    saved_bookmark: Arc::clone(&saved_bookmark),
                    saved_config: Arc::clone(&saved_config),
                },
                saved_bookmark,
                saved_config,
            )
        }
    }

    impl CacheService for TestCacheService {
        fn save_bookmark(&self, _source_path: &Path, _bookmark: &cache::Bookmark) {
            self.saved_bookmark.store(true, Ordering::SeqCst);
        }

        fn save_epub_config(&self, _source_path: &Path, config: &config::AppConfig) {
            let mut guard = self.saved_config.lock().expect("config lock");
            *guard = Some(config.clone());
        }

        fn delete_recent_source_and_cache(&self, _source_path: &Path) -> Result<(), String> {
            Ok(())
        }

        fn remember_source_path(&self, _source_path: &Path) {}

        fn persist_clipboard_text_source(&self, _text: &str) -> Result<std::path::PathBuf, String> {
            Err("not_used".to_string())
        }

        fn persist_browser_tab_source(
            &self,
            _snapshot: &lanternleaf_core::browser_tabs::BrowserTabSnapshot,
            _tab_meta: Option<&lanternleaf_core::browser_tabs::BrowserTab>,
        ) -> Result<std::path::PathBuf, String> {
            Err("not_used".to_string())
        }

        fn persist_browser_tab_bundle_source(
            &self,
            _capture: &lanternleaf_core::browser_tabs::BrowserTabBundleCapture,
            _tab_meta: Option<&lanternleaf_core::browser_tabs::BrowserTab>,
        ) -> Result<std::path::PathBuf, String> {
            Err("not_used".to_string())
        }

        fn list_recent_books(&self, _limit: usize) -> Vec<cache::RecentBook> {
            Vec::new()
        }

        fn load_browser_tab_manifest(
            &self,
            _source_path: &Path,
        ) -> Result<cache::BrowserTabSourceManifest, String> {
            Err("not_used".to_string())
        }

        fn load_pdf_ocr_alignment_artifact(
            &self,
            _source_path: &Path,
        ) -> Option<cache::PdfOcrAlignmentArtifact> {
            None
        }

        fn load_pdf_sentence_map(
            &self,
            _source_path: &Path,
        ) -> Option<Vec<cache::PdfSentenceLocation>> {
            None
        }

        fn load_pdf_render_precomputed_state(
            &self,
            _source_path: &Path,
        ) -> Option<cache::PdfRenderPrecomputedState> {
            None
        }

        fn persist_pdf_sentence_map(
            &self,
            _source_path: &Path,
            _locations: &[cache::PdfSentenceLocation],
        ) {
        }

        fn persist_pdf_render_precomputed_state(
            &self,
            _source_path: &Path,
            _artifact: &cache::PdfRenderPrecomputedState,
        ) {
        }
    }

    #[test]
    fn lifecycle_flush_calls_service() {
        let service = Arc::new(StubService::new(None));
        let lifecycle = PersistenceLifecycle::new(service.clone());
        let config = config::AppConfig::default();
        lifecycle.flush_trigger(
            Some(ReaderHousekeeping::from_parts(
                "/tmp/test.epub",
                sample_bookmark(),
                config,
                None,
            )),
            PersistenceTrigger::SourceOpen,
        );
        assert!(service.persisted.load(Ordering::SeqCst));
    }

    #[test]
    fn lifecycle_flush_calls_service_on_session_close() {
        let service = Arc::new(StubService::new(None));
        let lifecycle = PersistenceLifecycle::new(service.clone());
        let config = config::AppConfig::default();
        lifecycle.flush_trigger(
            Some(ReaderHousekeeping::from_parts(
                "/tmp/test.epub",
                sample_bookmark(),
                config,
                None,
            )),
            PersistenceTrigger::SessionClose,
        );
        assert!(service.persisted.load(Ordering::SeqCst));
    }

    #[test]
    fn lifecycle_flush_calls_service_on_safe_quit() {
        let service = Arc::new(StubService::new(None));
        let lifecycle = PersistenceLifecycle::new(service.clone());
        let config = config::AppConfig::default();
        lifecycle.flush_trigger(
            Some(ReaderHousekeeping::from_parts(
                "/tmp/test.epub",
                sample_bookmark(),
                config,
                None,
            )),
            PersistenceTrigger::SafeQuit,
        );
        assert!(service.persisted.load(Ordering::SeqCst));
    }

    #[test]
    fn lifecycle_load_returns_bookmark() {
        let bookmark = sample_bookmark();
        let service = Arc::new(StubService::new(Some(bookmark.clone())));
        let lifecycle = PersistenceLifecycle::new(service);
        let (loaded_bookmark, loaded_config) =
            lifecycle.load_bookmark_and_config(Path::new("/tmp/book.epub"));
        let loaded = loaded_bookmark.expect("bookmark should be loaded");
        assert_eq!(loaded.page, bookmark.page);
        assert!(loaded_config.is_none());
    }

    #[test]
    fn filesystem_persistence_uses_cache_service_config() {
        let (cache_service, saved_bookmark, saved_config) = TestCacheService::new();
        let service = FilesystemPersistenceService::new(Arc::new(cache_service));
        let mut cfg = config::AppConfig::default();
        cfg.tts_speed = 3.5;
        service
            .persist_reader_housekeeping(ReaderHousekeeping::from_parts(
                "/tmp/test.epub",
                sample_bookmark(),
                cfg,
                None,
            ))
            .expect("persist should succeed");
        assert!(saved_bookmark.load(Ordering::SeqCst));
        let guard = saved_config.lock().expect("config lock");
        let saved = guard.as_ref().expect("config saved");
        assert!((saved.tts_speed - 3.5).abs() < f32::EPSILON);
    }

    struct RecordingCacheService {
        calls: Arc<Mutex<Vec<&'static str>>>,
    }

    impl RecordingCacheService {
        fn new() -> (Self, Arc<Mutex<Vec<&'static str>>>) {
            let calls = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    calls: Arc::clone(&calls),
                },
                calls,
            )
        }

        fn record(&self, name: &'static str) {
            self.calls.lock().expect("calls lock").push(name);
        }
    }

    impl CacheService for RecordingCacheService {
        fn save_bookmark(&self, _source_path: &Path, _bookmark: &cache::Bookmark) {
            self.record("save_bookmark");
        }

        fn save_epub_config(&self, _source_path: &Path, _config: &config::AppConfig) {
            self.record("save_epub_config");
        }

        fn delete_recent_source_and_cache(&self, _source_path: &Path) -> Result<(), String> {
            Ok(())
        }

        fn remember_source_path(&self, _source_path: &Path) {
            self.record("remember_source_path");
        }

        fn persist_clipboard_text_source(&self, _text: &str) -> Result<std::path::PathBuf, String> {
            Err("not_used".to_string())
        }

        fn persist_browser_tab_source(
            &self,
            _snapshot: &lanternleaf_core::browser_tabs::BrowserTabSnapshot,
            _tab_meta: Option<&lanternleaf_core::browser_tabs::BrowserTab>,
        ) -> Result<std::path::PathBuf, String> {
            Err("not_used".to_string())
        }

        fn persist_browser_tab_bundle_source(
            &self,
            _capture: &lanternleaf_core::browser_tabs::BrowserTabBundleCapture,
            _tab_meta: Option<&lanternleaf_core::browser_tabs::BrowserTab>,
        ) -> Result<std::path::PathBuf, String> {
            Err("not_used".to_string())
        }

        fn list_recent_books(&self, _limit: usize) -> Vec<cache::RecentBook> {
            Vec::new()
        }

        fn load_browser_tab_manifest(
            &self,
            _source_path: &Path,
        ) -> Result<cache::BrowserTabSourceManifest, String> {
            Err("not_used".to_string())
        }

        fn load_pdf_ocr_alignment_artifact(
            &self,
            _source_path: &Path,
        ) -> Option<cache::PdfOcrAlignmentArtifact> {
            None
        }

        fn load_pdf_sentence_map(
            &self,
            _source_path: &Path,
        ) -> Option<Vec<cache::PdfSentenceLocation>> {
            None
        }

        fn load_pdf_render_precomputed_state(
            &self,
            _source_path: &Path,
        ) -> Option<cache::PdfRenderPrecomputedState> {
            None
        }

        fn persist_pdf_sentence_map(
            &self,
            _source_path: &Path,
            _locations: &[cache::PdfSentenceLocation],
        ) {
        }

        fn persist_pdf_render_precomputed_state(
            &self,
            _source_path: &Path,
            _artifact: &cache::PdfRenderPrecomputedState,
        ) {
        }
    }

    #[test]
    fn filesystem_persistence_remembers_source_before_saving() {
        let (cache_service, calls) = RecordingCacheService::new();
        let service = FilesystemPersistenceService::new(Arc::new(cache_service));
        let config = config::AppConfig::default();

        service
            .persist_reader_housekeeping(ReaderHousekeeping::from_parts(
                "/tmp/test.epub",
                sample_bookmark(),
                config,
                None,
            ))
            .expect("persist should succeed");

        let recorded = calls.lock().expect("calls lock").clone();
        assert_eq!(
            recorded,
            vec!["remember_source_path", "save_bookmark", "save_epub_config"]
        );
    }
}
