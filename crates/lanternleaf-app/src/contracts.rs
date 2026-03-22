use lanternleaf_core::{browser_tabs, calibre, config, session};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum UiMode {
    Starter,
    Reader,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BootstrapConfig {
    pub theme: config::ThemeMode,
    pub font_family: config::FontFamily,
    pub font_weight: config::FontWeight,
    pub day_highlight: config::HighlightColor,
    pub night_highlight: config::HighlightColor,
    pub log_level: String,
    pub default_font_size: u32,
    pub default_lines_per_page: usize,
    pub default_tts_speed: f32,
    pub default_pause_after_sentence: f32,
    pub key_toggle_play_pause: String,
    pub key_next_sentence: String,
    pub key_prev_sentence: String,
    pub key_repeat_sentence: String,
    pub key_toggle_search: String,
    pub key_safe_quit: String,
    pub key_toggle_settings: String,
    pub key_toggle_stats: String,
    pub key_toggle_tts: String,
    pub browser_tabs_enabled: bool,
    pub close_browser_tab_on_recent_delete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BootstrapState {
    pub app_name: String,
    pub mode: String,
    pub config: BootstrapConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SessionState {
    pub mode: UiMode,
    pub active_source_path: Option<String>,
    pub open_in_flight: bool,
    pub panels: session::PanelState,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct OpenSourceResult {
    pub session: SessionState,
    pub reader: session::ReaderSnapshot,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RecentBook {
    pub source_path: String,
    pub display_title: String,
    pub snippet: String,
    pub thumbnail_path: Option<String>,
    #[ts(type = "number")]
    pub last_opened_unix_secs: u64,
    #[ts(type = "number | null")]
    pub browser_tab_id: Option<u64>,
    #[ts(type = "number | null")]
    pub browser_window_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CalibreBookDto {
    #[ts(type = "number")]
    pub id: u64,
    pub title: String,
    pub extension: String,
    pub authors: String,
    pub year: Option<i32>,
    #[ts(type = "number | null")]
    pub file_size_bytes: Option<u64>,
    pub source_path: Option<String>,
    pub cover_thumbnail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SourceOpenEvent {
    #[ts(type = "number")]
    pub request_id: u64,
    pub phase: String,
    pub source_path: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CalibreLoadEvent {
    #[ts(type = "number")]
    pub request_id: u64,
    pub phase: String,
    pub count: Option<usize>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TtsStateEvent {
    #[ts(type = "number")]
    pub request_id: u64,
    pub action: String,
    pub tts: session::ReaderTtsView,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PdfTranscriptionEvent {
    #[ts(type = "number")]
    pub request_id: u64,
    pub phase: String,
    pub source_path: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LogLevelEvent {
    #[ts(type = "number")]
    pub request_id: u64,
    pub level: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SessionStateEvent {
    #[ts(type = "number")]
    pub request_id: u64,
    pub action: String,
    pub session: SessionState,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ReaderStateEvent {
    #[ts(type = "number")]
    pub request_id: u64,
    pub action: String,
    pub reader: session::ReaderSnapshot,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ReaderPlaybackState {
    pub source_path: String,
    pub current_page: usize,
    pub highlighted_sentence_idx: Option<usize>,
    pub tts: session::ReaderTtsView,
    pub stats: session::ReaderStats,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ReaderPlaybackStateEvent {
    #[ts(type = "number")]
    pub request_id: u64,
    pub action: String,
    pub playback: ReaderPlaybackState,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BridgeError {
    pub code: String,
    pub message: String,
}

pub type BrowserTabsHealth = browser_tabs::BrowsrHealth;
pub type BrowserTabsWindow = browser_tabs::BrowserWindow;
pub type BrowserTabsTab = browser_tabs::BrowserTab;
pub type ReaderSnapshot = session::ReaderSnapshot;
pub type ReaderSettingsPatch = session::ReaderSettingsPatch;
pub type ReaderSettingsView = session::ReaderSettingsView;
pub type ReaderTtsView = session::ReaderTtsView;
pub type ReaderStats = session::ReaderStats;
pub type PrettyKind = session::PrettyKind;
pub type TtsPlaybackState = session::TtsPlaybackState;
pub type PanelState = session::PanelState;
pub type CalibreBook = calibre::CalibreBook;
