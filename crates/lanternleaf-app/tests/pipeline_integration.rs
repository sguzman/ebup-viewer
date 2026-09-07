use lanternleaf_app::AppRuntime;
use lanternleaf_app::contracts::{OpenSourceResult, SessionState, UiMode};
use lanternleaf_app::pipeline::{AppCommand, AppEvent, RuntimeEffect};
use lanternleaf_core::{config, session};

fn make_reader_snapshot(source_path: &str) -> session::ReaderSnapshot {
    session::ReaderSnapshot {
        source_path: source_path.to_string(),
        source_name: source_path
            .rsplit('/')
            .next()
            .unwrap_or("book.epub")
            .to_string(),
        current_page: 1,
        total_pages: 10,
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
        tts_current_sentence_text: None,
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
            tts_backend: config::TtsBackend::Piper,
            windows_voice_id: None,
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
            page_index: 1,
            total_pages: 10,
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

fn make_open_result(source_path: &str) -> OpenSourceResult {
    OpenSourceResult {
        session: SessionState {
            mode: UiMode::Reader,
            active_source_path: Some(source_path.to_string()),
            open_in_flight: false,
            panels: session::PanelState::default(),
        },
        reader: make_reader_snapshot(source_path),
    }
}

#[test]
fn open_source_command_flows_into_state() {
    let runtime = AppRuntime::default();
    let plan = runtime.plan_command(AppCommand::OpenSourcePath {
        path: "/tmp/book.epub".to_string(),
    });

    assert!(
        plan.effects
            .iter()
            .any(|effect| { matches!(effect.effect, RuntimeEffect::OpenSourcePath { .. }) })
    );

    for event in plan.local_events {
        runtime.apply_event(event);
    }

    let after_plan = runtime.state_snapshot();
    assert!(after_plan.app_shell.operations.source_open);

    runtime.apply_event(AppEvent::SourceOpened {
        request_id: plan.request_id,
        result: make_open_result("/tmp/book.epub"),
    });

    let after_open = runtime.state_snapshot();
    assert!(!after_open.app_shell.operations.source_open);
    assert!(!after_open.app_shell.busy);
    assert_eq!(
        after_open
            .session
            .session
            .as_ref()
            .and_then(|session| session.active_source_path.as_deref()),
        Some("/tmp/book.epub")
    );
    assert_eq!(
        after_open
            .reader_document
            .source
            .as_ref()
            .map(|source| source.source_name.as_str()),
        Some("book.epub")
    );
    assert!(after_open.reader_playback.playback.is_some());
}
