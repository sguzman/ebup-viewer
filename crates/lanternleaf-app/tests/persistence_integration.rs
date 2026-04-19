use lanternleaf_app::persistence::{
    FilesystemPersistenceService, PersistenceLifecycle, ReaderHousekeeping,
};
use lanternleaf_app::pipeline::PersistenceTrigger;
use lanternleaf_core::{cache, config, session};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static CACHE_LOCK: Mutex<()> = Mutex::new(());

fn unique_source_path(ext: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("lanternleaf_persist_{nanos}.{ext}"))
}

fn unique_cache_root() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("lanternleaf_cache_{nanos}"))
}

fn with_cache_root<F: FnOnce()>(test_body: F) {
    let _guard = CACHE_LOCK.lock().expect("cache lock");
    let cache_root = unique_cache_root();
    unsafe { std::env::set_var(cache::CACHE_DIR_ENV, &cache_root) };
    let _ = fs::create_dir_all(&cache_root);
    test_body();
    unsafe { std::env::remove_var(cache::CACHE_DIR_ENV) };
    let _ = fs::remove_dir_all(cache_root);
}
fn write_source(path: &Path) {
    fs::write(path, "test content").expect("write source");
}

fn cleanup_source(path: &Path) {
    let _ = cache::delete_recent_source_and_cache(path);
    let _ = fs::remove_file(path);
}

fn sample_snapshot(path: &Path) -> session::ReaderSnapshot {
    session::ReaderSnapshot {
        source_path: path.to_string_lossy().to_string(),
        source_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("test.epub")
            .to_string(),
        current_page: 0,
        total_pages: 1,
        text_only_mode: false,
        has_structured_markdown: false,
        pretty_kind: session::PrettyKind::None,
        pdf_geometry_mode: None,
        pdf_sync_strategy: None,
        pdf_classification: None,
        pdf_runtime_policy: None,
        pdf_ocr_alignment: None,
        pdf_ocr_pipeline: None,
        images: Vec::new(),
        tts_text_page: "tts".to_string(),
        reading_markdown_page: None,
        reading_html_page: None,
        page_text: "page".to_string(),
        sentences: vec!["sentence".to_string()],
        canonical_sentences: vec!["sentence".to_string()],
        page_sentence_counts: vec![1],
        sentence_anchor_map: vec![Some(0)],
        highlighted_sentence_idx: Some(0),
        search_query: String::new(),
        search_matches: vec![0],
        selected_search_match: Some(0),
        settings: session::ReaderSettingsView {
            theme: config::ThemeMode::Day,
            font_family: config::FontFamily::Lexend,
            font_weight: config::FontWeight::Normal,
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
            margin_horizontal: 20,
            margin_vertical: 12,
            lines_per_page: 40,
            pause_after_sentence: 0.0,
            auto_scroll_tts: true,
            center_spoken_sentence: true,
            text_only_show_original_text: false,
            time_remaining_display: config::TimeRemainingDisplay::Adaptive,
            tts_speed: 1.0,
            tts_volume: 1.0,
            pretty: config::PrettyUiConfig::default(),
        },
        tts: session::ReaderTtsView {
            state: session::TtsPlaybackState::Paused,
            current_sentence_idx: Some(0),
            sentence_count: 1,
            can_seek_prev: false,
            can_seek_next: false,
            progress_pct: 0.0,
        },
        stats: session::ReaderStats {
            page_index: 0,
            total_pages: 1,
            tts_progress_pct: 0.0,
            global_progress_pct: 0.0,
            page_time_remaining_secs: 0.0,
            book_time_remaining_secs: 0.0,
            page_word_count: 1,
            page_sentence_count: 1,
            page_start_percent: 0.0,
            page_end_percent: 1.0,
            words_read_up_to_page_start: 0,
            sentences_read_up_to_page_start: 0,
            words_read_up_to_page_end: 1,
            sentences_read_up_to_page_end: 1,
            words_read_up_to_current_position: 1,
            sentences_read_up_to_current_position: 1,
        },
        panels: session::PanelState {
            show_settings: false,
            show_stats: false,
            show_tts: true,
        },
    }
}

#[test]
fn persistence_roundtrip_and_delete() {
    with_cache_root(|| {
        let source = unique_source_path("epub");
        write_source(&source);
        cache::remember_source_path(&source);
        let lifecycle = PersistenceLifecycle::new(FilesystemPersistenceService::default());
        let snapshot = sample_snapshot(&source);
        let config = config::AppConfig::default();

        lifecycle.flush_trigger(
            Some(ReaderHousekeeping {
                snapshot: &snapshot,
                config: &config,
            }),
            PersistenceTrigger::SourceOpen,
        );
        let loaded = cache::load_bookmark(&source);
        assert!(loaded.is_some(), "bookmark should be persisted");

        cache::delete_recent_source_and_cache(&source).expect("delete source and cache");
        let loaded_after_delete = cache::load_bookmark(&source);
        assert!(loaded_after_delete.is_none(), "bookmark should be deleted");

        cleanup_source(&source);
    });
}

#[test]
fn persistence_rebuilds_after_corruption() {
    with_cache_root(|| {
        let source = unique_source_path("epub");
        write_source(&source);
        cache::remember_source_path(&source);

        let bookmark_path = cache::hash_dir(&source).join("bookmark.toml");
        if let Some(parent) = bookmark_path.parent() {
            fs::create_dir_all(parent).expect("create cache dir");
        }
        fs::write(&bookmark_path, "not = valid = toml").expect("write corrupt bookmark");
        let loaded = cache::load_bookmark(&source);
        assert!(loaded.is_none(), "corrupt bookmark should be ignored");

        let lifecycle = PersistenceLifecycle::new(FilesystemPersistenceService::default());
        let snapshot = sample_snapshot(&source);
        let config = config::AppConfig::default();
        lifecycle.flush_trigger(
            Some(ReaderHousekeeping {
                snapshot: &snapshot,
                config: &config,
            }),
            PersistenceTrigger::SourceOpen,
        );
        let rebuilt = fs::read_to_string(&bookmark_path).unwrap_or_default();
        assert!(
            rebuilt.contains("page"),
            "bookmark file should be rebuilt with content"
        );

        cleanup_source(&source);
    });
}
