#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeCommandGroup {
    AppShell,
    RecentBooks,
    SourceOpen,
    BrowserTabs,
    ReaderSession,
    PdfArtifacts,
    Logging,
    Calibre,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BridgeCommandSpec {
    pub name: &'static str,
    pub group: BridgeCommandGroup,
}

pub const BRIDGE_COMMAND_SPECS: &[BridgeCommandSpec] = &[
    BridgeCommandSpec {
        name: "session_get_bootstrap",
        group: BridgeCommandGroup::AppShell,
    },
    BridgeCommandSpec {
        name: "session_get_state",
        group: BridgeCommandGroup::AppShell,
    },
    BridgeCommandSpec {
        name: "session_return_to_starter",
        group: BridgeCommandGroup::AppShell,
    },
    BridgeCommandSpec {
        name: "session_toggle_theme",
        group: BridgeCommandGroup::AppShell,
    },
    BridgeCommandSpec {
        name: "panel_toggle_settings",
        group: BridgeCommandGroup::AppShell,
    },
    BridgeCommandSpec {
        name: "panel_toggle_stats",
        group: BridgeCommandGroup::AppShell,
    },
    BridgeCommandSpec {
        name: "panel_toggle_tts",
        group: BridgeCommandGroup::AppShell,
    },
    BridgeCommandSpec {
        name: "recent_list",
        group: BridgeCommandGroup::RecentBooks,
    },
    BridgeCommandSpec {
        name: "recent_delete",
        group: BridgeCommandGroup::RecentBooks,
    },
    BridgeCommandSpec {
        name: "source_open_path",
        group: BridgeCommandGroup::SourceOpen,
    },
    BridgeCommandSpec {
        name: "source_open_clipboard",
        group: BridgeCommandGroup::SourceOpen,
    },
    BridgeCommandSpec {
        name: "source_open_clipboard_text",
        group: BridgeCommandGroup::SourceOpen,
    },
    BridgeCommandSpec {
        name: "browser_tabs_health",
        group: BridgeCommandGroup::BrowserTabs,
    },
    BridgeCommandSpec {
        name: "browser_tabs_list_windows",
        group: BridgeCommandGroup::BrowserTabs,
    },
    BridgeCommandSpec {
        name: "browser_tabs_list_tabs",
        group: BridgeCommandGroup::BrowserTabs,
    },
    BridgeCommandSpec {
        name: "recent_close_browser_tab",
        group: BridgeCommandGroup::BrowserTabs,
    },
    BridgeCommandSpec {
        name: "source_open_browser_tab",
        group: BridgeCommandGroup::BrowserTabs,
    },
    BridgeCommandSpec {
        name: "source_open_browser_tab_bundle",
        group: BridgeCommandGroup::BrowserTabs,
    },
    BridgeCommandSpec {
        name: "source_refresh_browser_tab",
        group: BridgeCommandGroup::BrowserTabs,
    },
    BridgeCommandSpec {
        name: "reader_get_snapshot",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "reader_next_page",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "reader_prev_page",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "reader_set_page",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "reader_sentence_click",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "reader_next_sentence",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "reader_prev_sentence",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "reader_toggle_text_only",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "reader_apply_settings",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "reader_search_set_query",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "reader_search_next",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "reader_search_prev",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "reader_tts_play",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "reader_tts_pause",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "reader_tts_toggle_play_pause",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "reader_tts_play_from_page_start",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "reader_tts_play_from_highlight",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "reader_tts_seek_next",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "reader_tts_seek_prev",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "reader_tts_repeat_sentence",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "reader_tts_precompute_page",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "reader_load_pdf_bytes",
        group: BridgeCommandGroup::PdfArtifacts,
    },
    BridgeCommandSpec {
        name: "reader_load_pdf_render_precomputed",
        group: BridgeCommandGroup::PdfArtifacts,
    },
    BridgeCommandSpec {
        name: "reader_load_pdf_sync_map",
        group: BridgeCommandGroup::PdfArtifacts,
    },
    BridgeCommandSpec {
        name: "reader_persist_pdf_sync_map",
        group: BridgeCommandGroup::PdfArtifacts,
    },
    BridgeCommandSpec {
        name: "reader_close_session",
        group: BridgeCommandGroup::ReaderSession,
    },
    BridgeCommandSpec {
        name: "app_safe_quit",
        group: BridgeCommandGroup::AppShell,
    },
    BridgeCommandSpec {
        name: "logging_set_level",
        group: BridgeCommandGroup::Logging,
    },
    BridgeCommandSpec {
        name: "calibre_load_cached_books",
        group: BridgeCommandGroup::Calibre,
    },
    BridgeCommandSpec {
        name: "calibre_load_books",
        group: BridgeCommandGroup::Calibre,
    },
    BridgeCommandSpec {
        name: "calibre_open_book",
        group: BridgeCommandGroup::Calibre,
    },
    BridgeCommandSpec {
        name: "calibre_ensure_thumbnail",
        group: BridgeCommandGroup::Calibre,
    },
];

pub const BRIDGE_COMMAND_NAMES: &[&str] = &[
    "session_get_bootstrap",
    "session_get_state",
    "session_return_to_starter",
    "session_toggle_theme",
    "panel_toggle_settings",
    "panel_toggle_stats",
    "panel_toggle_tts",
    "recent_list",
    "recent_delete",
    "source_open_path",
    "source_open_clipboard",
    "source_open_clipboard_text",
    "browser_tabs_health",
    "browser_tabs_list_windows",
    "browser_tabs_list_tabs",
    "recent_close_browser_tab",
    "source_open_browser_tab",
    "source_open_browser_tab_bundle",
    "source_refresh_browser_tab",
    "reader_get_snapshot",
    "reader_next_page",
    "reader_prev_page",
    "reader_set_page",
    "reader_sentence_click",
    "reader_next_sentence",
    "reader_prev_sentence",
    "reader_toggle_text_only",
    "reader_apply_settings",
    "reader_search_set_query",
    "reader_search_next",
    "reader_search_prev",
    "reader_tts_play",
    "reader_tts_pause",
    "reader_tts_toggle_play_pause",
    "reader_tts_play_from_page_start",
    "reader_tts_play_from_highlight",
    "reader_tts_seek_next",
    "reader_tts_seek_prev",
    "reader_tts_repeat_sentence",
    "reader_tts_precompute_page",
    "reader_load_pdf_bytes",
    "reader_load_pdf_render_precomputed",
    "reader_load_pdf_sync_map",
    "reader_persist_pdf_sync_map",
    "reader_close_session",
    "app_safe_quit",
    "logging_set_level",
    "calibre_load_cached_books",
    "calibre_load_books",
    "calibre_open_book",
    "calibre_ensure_thumbnail",
];

pub const BRIDGE_EVENT_NAMES: &[&str] = &[
    "source-open",
    "calibre-load",
    "session-state",
    "reader-state",
    "reader-playback-state",
    "tts-state",
    "pdf-transcription",
    "log-level",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_specs_and_names_stay_in_sync() {
        let spec_names: Vec<&str> = BRIDGE_COMMAND_SPECS.iter().map(|spec| spec.name).collect();
        assert_eq!(spec_names.as_slice(), BRIDGE_COMMAND_NAMES);
        assert_eq!(BRIDGE_COMMAND_NAMES.len(), 51);
    }
}
