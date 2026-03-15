use crate::contracts::{
    BootstrapState, BridgeError, BrowserTabsHealth, BrowserTabsTab, BrowserTabsWindow,
    CalibreBookDto, OpenSourceResult, ReaderSnapshot, RecentBook, SessionState,
};
use async_trait::async_trait;
use lanternleaf_core::{cache, session};

#[async_trait]
pub trait AppShellService: Send + Sync {
    async fn bootstrap(&self) -> Result<BootstrapState, BridgeError>;
    async fn session_state(&self) -> Result<SessionState, BridgeError>;
    async fn return_to_starter(&self) -> Result<SessionState, BridgeError>;
    async fn toggle_theme(&self) -> Result<session::ReaderSnapshot, BridgeError>;
    async fn toggle_settings_panel(&self) -> Result<SessionState, BridgeError>;
    async fn toggle_stats_panel(&self) -> Result<SessionState, BridgeError>;
    async fn toggle_tts_panel(&self) -> Result<SessionState, BridgeError>;
    async fn safe_quit(&self) -> Result<(), BridgeError>;
}

#[async_trait]
pub trait RecentBooksService: Send + Sync {
    async fn list_recent(&self, limit: Option<usize>) -> Result<Vec<RecentBook>, BridgeError>;
    async fn delete_recent(&self, source_path: String) -> Result<Vec<RecentBook>, BridgeError>;
    async fn close_browser_tab_for_recent(&self, source_path: String) -> Result<(), BridgeError>;
}

#[async_trait]
pub trait SourceOpenService: Send + Sync {
    async fn open_path(&self, path: String) -> Result<OpenSourceResult, BridgeError>;
    async fn open_clipboard(&self) -> Result<OpenSourceResult, BridgeError>;
    async fn open_clipboard_text(&self, text: String) -> Result<OpenSourceResult, BridgeError>;
}

#[async_trait]
pub trait BrowserTabsService: Send + Sync {
    async fn health(&self) -> Result<BrowserTabsHealth, BridgeError>;
    async fn list_windows(&self) -> Result<Vec<BrowserTabsWindow>, BridgeError>;
    async fn list_tabs(
        &self,
        window_id: Option<u64>,
        query: Option<String>,
        refresh: Option<bool>,
    ) -> Result<Vec<BrowserTabsTab>, BridgeError>;
    async fn open_browser_tab(
        &self,
        tab_id: u64,
        window_id: Option<u64>,
    ) -> Result<OpenSourceResult, BridgeError>;
    async fn open_browser_tab_bundle(
        &self,
        tab_id: u64,
        window_id: Option<u64>,
    ) -> Result<OpenSourceResult, BridgeError>;
    async fn refresh_browser_tab(
        &self,
        tab_id: u64,
        window_id: Option<u64>,
    ) -> Result<OpenSourceResult, BridgeError>;
}

#[async_trait]
pub trait ReaderSessionService: Send + Sync {
    async fn apply_command(
        &self,
        command: session::SessionCommand,
    ) -> Result<ReaderSnapshot, BridgeError>;
    async fn close_session(&self) -> Result<SessionState, BridgeError>;
    async fn precompute_tts_page(&self) -> Result<(), BridgeError>;
}

#[async_trait]
pub trait PdfArtifactsService: Send + Sync {
    async fn load_pdf_bytes(&self, path: String) -> Result<Vec<u8>, BridgeError>;
    async fn load_pdf_sync_map(
        &self,
        path: String,
    ) -> Result<Vec<cache::PdfSentenceLocation>, BridgeError>;
    async fn persist_pdf_sync_map(
        &self,
        path: String,
        locations: Vec<cache::PdfSentenceLocation>,
    ) -> Result<(), BridgeError>;
    async fn load_pdf_render_precomputed(
        &self,
        path: String,
    ) -> Result<cache::PdfRenderPrecomputedState, BridgeError>;
}

#[async_trait]
pub trait LoggingService: Send + Sync {
    async fn set_level(&self, level: String) -> Result<(), BridgeError>;
}

#[async_trait]
pub trait CalibreService: Send + Sync {
    async fn load_cached_books(&self) -> Result<Vec<CalibreBookDto>, BridgeError>;
    async fn load_books(&self) -> Result<Vec<CalibreBookDto>, BridgeError>;
    async fn open_book(&self, id: u64) -> Result<OpenSourceResult, BridgeError>;
    async fn ensure_thumbnail(&self, id: u64) -> Result<Option<String>, BridgeError>;
}
