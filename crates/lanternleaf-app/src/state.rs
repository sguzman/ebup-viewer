use crate::contracts::{
    BootstrapState, BrowserTabsHealth, BrowserTabsTab, BrowserTabsWindow, CalibreBookDto,
    CalibreLoadEvent, LogLevelEvent, PdfTranscriptionEvent, ReaderPlaybackState,
    ReaderPlaybackStateEvent, ReaderSnapshot, RecentBook, SessionState, SourceOpenEvent,
    TtsStateEvent,
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OperationState {
    pub source_open: bool,
    pub starter_command: bool,
    pub reader_command: bool,
    pub reader_tts: bool,
    pub reader_settings: bool,
    pub browser_tab_refresh: bool,
    pub calibre_load: bool,
    pub runtime_config: bool,
}

impl OperationState {
    pub fn is_busy(&self) -> bool {
        self.source_open
            || self.starter_command
            || self.reader_command
            || self.reader_tts
            || self.reader_settings
            || self.browser_tab_refresh
            || self.calibre_load
            || self.runtime_config
    }
}

#[derive(Debug, Clone, Default)]
pub struct AppShellState {
    pub bootstrap: Option<BootstrapState>,
    pub runtime_log_level: String,
    pub operations: OperationState,
    pub loading_bootstrap: bool,
    pub busy: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SessionDomainState {
    pub session: Option<SessionState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SourceMetadata {
    pub source_path: String,
    pub source_name: String,
}

#[derive(Debug, Clone, Default)]
pub struct ReaderDocumentState {
    pub source: Option<SourceMetadata>,
    pub snapshot: Option<ReaderSnapshot>,
}

#[derive(Debug, Clone, Default)]
pub struct ReaderPlaybackDomainState {
    pub playback: Option<ReaderPlaybackState>,
    pub tts_state_event: Option<TtsStateEvent>,
    pub playback_event: Option<ReaderPlaybackStateEvent>,
}

#[derive(Debug, Clone, Default)]
pub struct ReaderUiState {
    pub source_path: Option<String>,
    pub current_page: Option<usize>,
    pub total_pages: Option<usize>,
    pub text_only_mode: bool,
    pub pretty_kind: Option<crate::contracts::PrettyKind>,
    pub search_query: String,
    pub search_matches: Vec<usize>,
    pub selected_search_match: Option<usize>,
    pub panels: Option<crate::contracts::PanelState>,
    pub settings: Option<crate::contracts::ReaderSettingsView>,
}

#[derive(Debug, Clone, Default)]
pub struct StarterState {
    pub recents: Vec<RecentBook>,
    pub calibre_books: Vec<CalibreBookDto>,
    pub browser_tabs_health: Option<BrowserTabsHealth>,
    pub browser_tabs_windows: Vec<BrowserTabsWindow>,
    pub browser_tabs_tabs: Vec<BrowserTabsTab>,
    pub loading_recents: bool,
    pub loading_calibre: bool,
    pub loading_browser_tabs: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeJobState {
    pub source_open_event: Option<SourceOpenEvent>,
    pub calibre_load_event: Option<CalibreLoadEvent>,
    pub pdf_transcription_event: Option<PdfTranscriptionEvent>,
    pub log_level_event: Option<LogLevelEvent>,
    pub tts_state_subscribed: bool,
    pub source_open_subscribed: bool,
    pub calibre_subscribed: bool,
    pub pdf_transcription_subscribed: bool,
    pub log_level_subscribed: bool,
    pub session_state_subscribed: bool,
    pub reader_state_subscribed: bool,
    pub reader_playback_state_subscribed: bool,
    pub last_session_event_request_id: u64,
    pub last_reader_event_request_id: u64,
    pub last_reader_playback_event_request_id: u64,
    pub last_source_open_event_request_id: u64,
    pub last_calibre_event_request_id: u64,
    pub last_tts_event_request_id: u64,
    pub last_pdf_event_request_id: u64,
    pub last_log_level_event_request_id: u64,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub app_shell: AppShellState,
    pub session: SessionDomainState,
    pub reader_document: ReaderDocumentState,
    pub reader_playback: ReaderPlaybackDomainState,
    pub reader_ui: ReaderUiState,
    pub starter: StarterState,
    pub runtime_jobs: RuntimeJobState,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            app_shell: AppShellState {
                runtime_log_level: "info".to_string(),
                ..AppShellState::default()
            },
            session: SessionDomainState::default(),
            reader_document: ReaderDocumentState::default(),
            reader_playback: ReaderPlaybackDomainState::default(),
            reader_ui: ReaderUiState::default(),
            starter: StarterState::default(),
            runtime_jobs: RuntimeJobState::default(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeJobPatch {
    pub source_open_event: Option<SourceOpenEvent>,
    pub calibre_load_event: Option<CalibreLoadEvent>,
    pub pdf_transcription_event: Option<PdfTranscriptionEvent>,
    pub log_level_event: Option<LogLevelEvent>,
    pub source_open_subscribed: Option<bool>,
    pub calibre_subscribed: Option<bool>,
    pub tts_state_subscribed: Option<bool>,
    pub pdf_transcription_subscribed: Option<bool>,
    pub log_level_subscribed: Option<bool>,
    pub session_state_subscribed: Option<bool>,
    pub reader_state_subscribed: Option<bool>,
    pub reader_playback_state_subscribed: Option<bool>,
    pub last_session_event_request_id: Option<u64>,
    pub last_reader_event_request_id: Option<u64>,
    pub last_reader_playback_event_request_id: Option<u64>,
    pub last_source_open_event_request_id: Option<u64>,
    pub last_calibre_event_request_id: Option<u64>,
    pub last_tts_event_request_id: Option<u64>,
    pub last_pdf_event_request_id: Option<u64>,
    pub last_log_level_event_request_id: Option<u64>,
}

impl AppState {
    pub fn update_runtime_log_level(&mut self, level: impl Into<String>) {
        self.app_shell.runtime_log_level = level.into();
    }

    pub fn set_operations(&mut self, operations: OperationState) {
        self.app_shell.busy = operations.is_busy();
        self.app_shell.operations = operations;
    }

    pub fn set_loading_bootstrap(&mut self, loading: bool) {
        self.app_shell.loading_bootstrap = loading;
    }

    pub fn set_bootstrap(&mut self, bootstrap: Option<BootstrapState>) {
        self.app_shell.bootstrap = bootstrap;
    }

    pub fn set_session(&mut self, session: Option<SessionState>) {
        self.session.session = session;
    }

    pub fn set_reader_document(&mut self, snapshot: Option<ReaderSnapshot>) {
        self.reader_document.source = snapshot.as_ref().map(|reader| SourceMetadata {
            source_path: reader.source_path.clone(),
            source_name: reader.source_name.clone(),
        });
        self.reader_document.snapshot = snapshot.clone();
        self.reader_ui = derive_reader_ui(snapshot.as_ref());
    }

    pub fn set_reader_playback(&mut self, playback: Option<ReaderPlaybackState>) {
        self.reader_playback.playback = playback;
    }

    pub fn set_tts_state_event(&mut self, event: Option<TtsStateEvent>) {
        self.reader_playback.tts_state_event = event;
    }

    pub fn set_reader_playback_event(&mut self, event: Option<ReaderPlaybackStateEvent>) {
        self.reader_playback.playback_event = event;
    }

    pub fn set_starter_recents(&mut self, recents: Vec<RecentBook>) {
        self.starter.recents = recents;
    }

    pub fn set_starter_calibre_books(&mut self, calibre_books: Vec<CalibreBookDto>) {
        self.starter.calibre_books = calibre_books;
    }

    pub fn set_starter_browser_tabs_health(&mut self, health: Option<BrowserTabsHealth>) {
        self.starter.browser_tabs_health = health;
    }

    pub fn set_starter_browser_tabs_windows(&mut self, windows: Vec<BrowserTabsWindow>) {
        self.starter.browser_tabs_windows = windows;
    }

    pub fn set_starter_browser_tabs_tabs(&mut self, tabs: Vec<BrowserTabsTab>) {
        self.starter.browser_tabs_tabs = tabs;
    }

    pub fn set_loading_recents(&mut self, loading: bool) {
        self.starter.loading_recents = loading;
    }

    pub fn set_loading_calibre(&mut self, loading: bool) {
        self.starter.loading_calibre = loading;
    }

    pub fn set_loading_browser_tabs(&mut self, loading: bool) {
        self.starter.loading_browser_tabs = loading;
    }

    pub fn apply_runtime_job_patch(&mut self, patch: RuntimeJobPatch) {
        if let Some(value) = patch.source_open_event {
            self.runtime_jobs.source_open_event = Some(value);
        }
        if let Some(value) = patch.calibre_load_event {
            self.runtime_jobs.calibre_load_event = Some(value);
        }
        if let Some(value) = patch.pdf_transcription_event {
            self.runtime_jobs.pdf_transcription_event = Some(value);
        }
        if let Some(value) = patch.log_level_event {
            self.runtime_jobs.log_level_event = Some(value);
        }
        if let Some(value) = patch.source_open_subscribed {
            self.runtime_jobs.source_open_subscribed = value;
        }
        if let Some(value) = patch.calibre_subscribed {
            self.runtime_jobs.calibre_subscribed = value;
        }
        if let Some(value) = patch.tts_state_subscribed {
            self.runtime_jobs.tts_state_subscribed = value;
        }
        if let Some(value) = patch.pdf_transcription_subscribed {
            self.runtime_jobs.pdf_transcription_subscribed = value;
        }
        if let Some(value) = patch.log_level_subscribed {
            self.runtime_jobs.log_level_subscribed = value;
        }
        if let Some(value) = patch.session_state_subscribed {
            self.runtime_jobs.session_state_subscribed = value;
        }
        if let Some(value) = patch.reader_state_subscribed {
            self.runtime_jobs.reader_state_subscribed = value;
        }
        if let Some(value) = patch.reader_playback_state_subscribed {
            self.runtime_jobs.reader_playback_state_subscribed = value;
        }
        if let Some(value) = patch.last_session_event_request_id {
            self.runtime_jobs.last_session_event_request_id = value;
        }
        if let Some(value) = patch.last_reader_event_request_id {
            self.runtime_jobs.last_reader_event_request_id = value;
        }
        if let Some(value) = patch.last_reader_playback_event_request_id {
            self.runtime_jobs.last_reader_playback_event_request_id = value;
        }
        if let Some(value) = patch.last_source_open_event_request_id {
            self.runtime_jobs.last_source_open_event_request_id = value;
        }
        if let Some(value) = patch.last_calibre_event_request_id {
            self.runtime_jobs.last_calibre_event_request_id = value;
        }
        if let Some(value) = patch.last_tts_event_request_id {
            self.runtime_jobs.last_tts_event_request_id = value;
        }
        if let Some(value) = patch.last_pdf_event_request_id {
            self.runtime_jobs.last_pdf_event_request_id = value;
        }
        if let Some(value) = patch.last_log_level_event_request_id {
            self.runtime_jobs.last_log_level_event_request_id = value;
        }
    }
}

pub fn derive_reader_ui(reader: Option<&ReaderSnapshot>) -> ReaderUiState {
    ReaderUiState {
        source_path: reader.map(|value| value.source_path.clone()),
        current_page: reader.map(|value| value.current_page),
        total_pages: reader.map(|value| value.total_pages),
        text_only_mode: reader.map(|value| value.text_only_mode).unwrap_or(false),
        pretty_kind: reader.map(|value| value.pretty_kind),
        search_query: reader
            .map(|value| value.search_query.clone())
            .unwrap_or_default(),
        search_matches: reader
            .map(|value| value.search_matches.clone())
            .unwrap_or_default(),
        selected_search_match: reader.and_then(|value| value.selected_search_match),
        panels: reader.map(|value| value.panels),
        settings: reader.map(|value| value.settings.clone()),
    }
}

pub fn derive_reader_playback(reader: Option<&ReaderSnapshot>) -> Option<ReaderPlaybackState> {
    reader.map(|value| ReaderPlaybackState {
        source_path: value.source_path.clone(),
        current_page: value.current_page,
        highlighted_sentence_idx: value.highlighted_sentence_idx,
        tts: value.tts.clone(),
        stats: value.stats.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{BootstrapConfig, PrettyKind, UiMode};
    use lanternleaf_core::{config, session};

    fn make_reader_snapshot() -> ReaderSnapshot {
        ReaderSnapshot {
            source_path: "/tmp/book.epub".to_string(),
            source_name: "book.epub".to_string(),
            current_page: 3,
            total_pages: 12,
            text_only_mode: false,
            has_structured_markdown: true,
            pretty_kind: PrettyKind::Html,
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

    #[test]
    fn derive_reader_ui_tracks_document_and_ui_fields() {
        let snapshot = make_reader_snapshot();
        let reader_ui = derive_reader_ui(Some(&snapshot));

        assert_eq!(reader_ui.source_path.as_deref(), Some("/tmp/book.epub"));
        assert_eq!(reader_ui.current_page, Some(3));
        assert_eq!(reader_ui.total_pages, Some(12));
        assert_eq!(reader_ui.pretty_kind, Some(PrettyKind::Html));
        assert_eq!(reader_ui.search_query, "query");
        assert_eq!(reader_ui.search_matches, vec![0]);
        assert_eq!(reader_ui.selected_search_match, Some(0));
        assert_eq!(reader_ui.panels, Some(snapshot.panels));
    }

    #[test]
    fn derive_reader_playback_tracks_playback_only_fields() {
        let snapshot = make_reader_snapshot();
        let playback = derive_reader_playback(Some(&snapshot)).expect("playback should exist");

        assert_eq!(playback.source_path, snapshot.source_path);
        assert_eq!(playback.current_page, snapshot.current_page);
        assert_eq!(
            playback.highlighted_sentence_idx,
            snapshot.highlighted_sentence_idx
        );
        assert_eq!(playback.tts.state, snapshot.tts.state);
        assert_eq!(playback.stats.page_index, snapshot.stats.page_index);
    }

    #[test]
    fn set_reader_document_updates_document_and_ui_only() {
        let mut state = AppState::default();
        state.reader_playback.playback = Some(ReaderPlaybackState {
            source_path: "/tmp/other.epub".to_string(),
            current_page: 9,
            highlighted_sentence_idx: Some(4),
            tts: session::ReaderTtsView {
                state: session::TtsPlaybackState::Paused,
                current_sentence_idx: Some(4),
                sentence_count: 9,
                can_seek_prev: true,
                can_seek_next: true,
                progress_pct: 0.9,
            },
            stats: make_reader_snapshot().stats,
        });

        state.set_reader_document(Some(make_reader_snapshot()));

        assert_eq!(
            state
                .reader_document
                .source
                .as_ref()
                .map(|source| source.source_name.as_str()),
            Some("book.epub")
        );
        assert_eq!(state.reader_ui.current_page, Some(3));
        assert_eq!(
            state
                .reader_playback
                .playback
                .as_ref()
                .map(|playback| playback.current_page),
            Some(9)
        );
    }

    #[test]
    fn set_operations_updates_busy_without_touching_document_state() {
        let mut state = AppState::default();
        state.set_reader_document(Some(make_reader_snapshot()));

        state.set_operations(OperationState {
            source_open: false,
            starter_command: false,
            reader_command: false,
            reader_tts: true,
            reader_settings: false,
            browser_tab_refresh: false,
            calibre_load: false,
            runtime_config: false,
        });

        assert!(state.app_shell.busy);
        assert_eq!(
            state
                .reader_document
                .snapshot
                .as_ref()
                .map(|reader| reader.current_page),
            Some(3)
        );
    }

    #[test]
    fn bootstrap_and_session_domains_stay_separate() {
        let mut state = AppState::default();
        state.set_bootstrap(Some(BootstrapState {
            app_name: "LanternLeaf".to_string(),
            mode: "dev".to_string(),
            config: BootstrapConfig {
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
                log_level: "info".to_string(),
                default_font_size: 18,
                default_lines_per_page: 400,
                default_tts_speed: 1.0,
                default_pause_after_sentence: 0.0,
                key_toggle_play_pause: "Space".to_string(),
                key_next_sentence: "J".to_string(),
                key_prev_sentence: "K".to_string(),
                key_repeat_sentence: "R".to_string(),
                key_toggle_search: "/".to_string(),
                key_safe_quit: "Ctrl+Q".to_string(),
                key_toggle_settings: "S".to_string(),
                key_toggle_stats: "D".to_string(),
                key_toggle_tts: "T".to_string(),
                browser_tabs_enabled: true,
                close_browser_tab_on_recent_delete: false,
            },
        }));
        state.set_session(Some(SessionState {
            mode: UiMode::Reader,
            active_source_path: Some("/tmp/book.epub".to_string()),
            open_in_flight: false,
            panels: session::PanelState::default(),
        }));

        assert_eq!(
            state
                .app_shell
                .bootstrap
                .as_ref()
                .map(|state| state.app_name.as_str()),
            Some("LanternLeaf")
        );
        assert_eq!(
            state
                .session
                .session
                .as_ref()
                .and_then(|session| session.active_source_path.as_deref()),
            Some("/tmp/book.epub")
        );
    }
}
