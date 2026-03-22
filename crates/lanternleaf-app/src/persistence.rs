use crate::contracts::BridgeError;
use crate::contracts::ReaderSnapshot;
use crate::pipeline::PersistenceTrigger;
use lanternleaf_core::{cache, config};
use std::path::Path;
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

pub trait PersistenceService: Send + Sync {
    fn persist_reader_housekeeping(&self, snapshot: &ReaderSnapshot) -> Result<(), BridgeError>;

    fn load_bookmark(&self, source_path: &Path) -> Option<cache::Bookmark>;

    fn load_epub_config(&self, source_path: &Path) -> Option<config::AppConfig>;
}

pub struct FilesystemPersistenceService;

impl PersistenceService for FilesystemPersistenceService {
    fn persist_reader_housekeeping(&self, snapshot: &ReaderSnapshot) -> Result<(), BridgeError> {
        let source_path = Path::new(&snapshot.source_path);
        let sentence_text = snapshot
            .highlighted_sentence_idx
            .and_then(|idx| snapshot.sentences.get(idx).cloned());
        let bookmark = cache::Bookmark {
            page: snapshot.current_page,
            sentence_idx: snapshot.highlighted_sentence_idx,
            sentence_text,
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
        };
        cache::save_bookmark(source_path, &bookmark);
        cache::save_epub_config(source_path, &config::AppConfig::default());
        Ok(())
    }

    fn load_bookmark(&self, source_path: &Path) -> Option<cache::Bookmark> {
        cache::load_bookmark(source_path)
    }

    fn load_epub_config(&self, source_path: &Path) -> Option<config::AppConfig> {
        cache::load_epub_config(source_path)
    }
}

pub struct PersistenceLifecycle<S> {
    service: std::sync::Arc<S>,
    inventory: PersistenceInventory,
    policy: PersistencePolicy,
}

impl<S: PersistenceService> PersistenceLifecycle<S> {
    pub fn new(service: S) -> Self {
        Self {
            service: std::sync::Arc::new(service),
            inventory: PersistenceInventory::default_inventory(),
            policy: PersistencePolicy::default_policy(),
        }
    }

    pub fn service(&self) -> std::sync::Arc<S> {
        std::sync::Arc::clone(&self.service)
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

    pub fn on_live_update(&self, snapshot: &ReaderSnapshot, trigger: PersistenceTrigger) {
        if let Err(error) = self.service.persist_reader_housekeeping(snapshot) {
            warn!(
                trigger = ?trigger,
                error = %error.message,
                "Live persistence failed"
            );
        } else {
            debug!(trigger = ?trigger, "Reader housekeeping persisted");
        }
    }

    pub fn on_source_open(&self, snapshot: &ReaderSnapshot) {
        info!(path = %snapshot.source_path, "Persistence lifecycle: source open");
        if let Err(error) = self.service.persist_reader_housekeeping(snapshot) {
            warn!(error = %error.message, "Source-open persistence failed");
        } else {
            debug!("Reader housekeeping persisted on source open");
        }
    }

    pub fn on_session_close(&self, snapshot: &ReaderSnapshot) {
        info!(path = %snapshot.source_path, "Persistence lifecycle: session close");
        if let Err(error) = self.service.persist_reader_housekeeping(snapshot) {
            warn!(error = %error.message, "Session close persistence failed");
        } else {
            debug!("Reader housekeeping persisted on session close");
        }
    }

    pub fn on_safe_quit(&self, snapshot: &ReaderSnapshot) {
        info!(path = %snapshot.source_path, "Persistence lifecycle: safe quit");
        if let Err(error) = self.service.persist_reader_housekeeping(snapshot) {
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

    pub fn flush_trigger(&self, snapshot: Option<&ReaderSnapshot>, trigger: PersistenceTrigger) {
        match (snapshot, trigger) {
            (Some(snapshot), PersistenceTrigger::ReaderCommand)
            | (Some(snapshot), PersistenceTrigger::RuntimeConfigChange) => {
                self.on_live_update(snapshot, trigger);
            }
            (Some(snapshot), PersistenceTrigger::SourceOpen) => self.on_source_open(snapshot),
            (Some(snapshot), PersistenceTrigger::SessionClose) => self.on_session_close(snapshot),
            (Some(snapshot), PersistenceTrigger::SafeQuit) => self.on_safe_quit(snapshot),
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
    use lanternleaf_core::{cache, config, session};
    use std::path::Path;
    use std::sync::{
        Arc,
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
        fn persist_reader_housekeeping(&self, _: &ReaderSnapshot) -> Result<(), BridgeError> {
            self.persisted.store(true, Ordering::SeqCst);
            Ok(())
        }

        fn load_bookmark(&self, _: &Path) -> Option<cache::Bookmark> {
            self.bookmark_value.clone()
        }

        fn load_epub_config(&self, _: &Path) -> Option<config::AppConfig> {
            None
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

    #[test]
    fn lifecycle_flush_calls_service() {
        let service = StubService::new(None);
        let lifecycle = PersistenceLifecycle::new(service);
        let snapshot = make_reader_snapshot();
        lifecycle.flush_trigger(Some(&snapshot), PersistenceTrigger::SourceOpen);
        assert!(lifecycle.service().persisted.load(Ordering::SeqCst));
    }

    #[test]
    fn lifecycle_flush_calls_service_on_session_close() {
        let service = StubService::new(None);
        let lifecycle = PersistenceLifecycle::new(service);
        let snapshot = make_reader_snapshot();
        lifecycle.flush_trigger(Some(&snapshot), PersistenceTrigger::SessionClose);
        assert!(lifecycle.service().persisted.load(Ordering::SeqCst));
    }

    #[test]
    fn lifecycle_flush_calls_service_on_safe_quit() {
        let service = StubService::new(None);
        let lifecycle = PersistenceLifecycle::new(service);
        let snapshot = make_reader_snapshot();
        lifecycle.flush_trigger(Some(&snapshot), PersistenceTrigger::SafeQuit);
        assert!(lifecycle.service().persisted.load(Ordering::SeqCst));
    }

    #[test]
    fn lifecycle_load_returns_bookmark() {
        let bookmark = sample_bookmark();
        let service = StubService::new(Some(bookmark.clone()));
        let lifecycle = PersistenceLifecycle::new(service);
        let (loaded_bookmark, loaded_config) =
            lifecycle.load_bookmark_and_config(Path::new("/tmp/book.epub"));
        let loaded = loaded_bookmark.expect("bookmark should be loaded");
        assert_eq!(loaded.page, bookmark.page);
        assert!(loaded_config.is_none());
    }
}
