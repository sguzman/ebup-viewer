use crate::pipeline::{AppCommand, AppEvent, RuntimeEffect};
use tracing::{Span, info_span};

pub fn command_span(request_id: u64, command: &AppCommand) -> Span {
    info_span!(
        "app_command",
        request_id,
        action = command.action(),
        command = %command_name(command)
    )
}

pub fn effect_span(request_id: u64, effect: &RuntimeEffect) -> Span {
    info_span!(
        "runtime_effect",
        request_id,
        effect = %effect_name(effect),
        owner = ?effect.owner()
    )
}

pub fn event_span(event: &AppEvent) -> Span {
    let name = event_name(event);
    let id = event_request_id(event).unwrap_or_default();
    info_span!("app_event", request_id = id, event = name)
}

fn command_name(command: &AppCommand) -> &'static str {
    match command {
        AppCommand::Bootstrap => "Bootstrap",
        AppCommand::RefreshRecents { .. } => "RefreshRecents",
        AppCommand::OpenSourcePath { .. } => "OpenSourcePath",
        AppCommand::OpenClipboard => "OpenClipboard",
        AppCommand::OpenClipboardText { .. } => "OpenClipboardText",
        AppCommand::LoadBrowserTabsHealth => "LoadBrowserTabsHealth",
        AppCommand::ListBrowserTabWindows => "ListBrowserTabWindows",
        AppCommand::ListBrowserTabs { .. } => "ListBrowserTabs",
        AppCommand::OpenBrowserTab { .. } => "OpenBrowserTab",
        AppCommand::OpenBrowserTabBundle { .. } => "OpenBrowserTabBundle",
        AppCommand::RefreshBrowserTab { .. } => "RefreshBrowserTab",
        AppCommand::DeleteRecent { .. } => "DeleteRecent",
        AppCommand::CloseRecentBrowserTab { .. } => "CloseRecentBrowserTab",
        AppCommand::ReturnToStarter => "ReturnToStarter",
        AppCommand::CloseReaderSession => "CloseReaderSession",
        AppCommand::ToggleTheme => "ToggleTheme",
        AppCommand::ToggleSettingsPanel => "ToggleSettingsPanel",
        AppCommand::ToggleStatsPanel => "ToggleStatsPanel",
        AppCommand::ToggleTtsPanel => "ToggleTtsPanel",
        AppCommand::Reader(_) => "ReaderCommand",
        AppCommand::LoadCalibreBooks { .. } => "LoadCalibreBooks",
        AppCommand::OpenCalibreBook { .. } => "OpenCalibreBook",
        AppCommand::EnsureCalibreThumbnail { .. } => "EnsureCalibreThumbnail",
        AppCommand::SetRuntimeLogLevel { .. } => "SetRuntimeLogLevel",
        AppCommand::FlushPersistence { .. } => "FlushPersistence",
        AppCommand::SafeQuit => "SafeQuit",
    }
}

pub fn effect_name(effect: &RuntimeEffect) -> &'static str {
    match effect {
        RuntimeEffect::LoadBootstrap => "LoadBootstrap",
        RuntimeEffect::ListRecents { .. } => "ListRecents",
        RuntimeEffect::DeleteRecent { .. } => "DeleteRecent",
        RuntimeEffect::CloseRecentBrowserTab { .. } => "CloseRecentBrowserTab",
        RuntimeEffect::OpenSourcePath { .. } => "OpenSourcePath",
        RuntimeEffect::OpenClipboard => "OpenClipboard",
        RuntimeEffect::OpenClipboardText { .. } => "OpenClipboardText",
        RuntimeEffect::LoadBrowserTabsHealth => "LoadBrowserTabsHealth",
        RuntimeEffect::ListBrowserTabWindows => "ListBrowserTabWindows",
        RuntimeEffect::ListBrowserTabs { .. } => "ListBrowserTabs",
        RuntimeEffect::OpenBrowserTab { .. } => "OpenBrowserTab",
        RuntimeEffect::OpenBrowserTabBundle { .. } => "OpenBrowserTabBundle",
        RuntimeEffect::RefreshBrowserTab { .. } => "RefreshBrowserTab",
        RuntimeEffect::ReturnToStarter => "ReturnToStarter",
        RuntimeEffect::CloseReaderSession => "CloseReaderSession",
        RuntimeEffect::ToggleTheme => "ToggleTheme",
        RuntimeEffect::TogglePanel { .. } => "TogglePanel",
        RuntimeEffect::ApplyReaderCommand { .. } => "ApplyReaderCommand",
        RuntimeEffect::PrecomputeTtsPage => "PrecomputeTtsPage",
        RuntimeEffect::LoadPdfBytes { .. } => "LoadPdfBytes",
        RuntimeEffect::LoadPdfSyncMap { .. } => "LoadPdfSyncMap",
        RuntimeEffect::PersistPdfSyncMap { .. } => "PersistPdfSyncMap",
        RuntimeEffect::LoadPdfRenderPrecomputed { .. } => "LoadPdfRenderPrecomputed",
        RuntimeEffect::LoadCalibreCachedBooks => "LoadCalibreCachedBooks",
        RuntimeEffect::LoadCalibreBooks { .. } => "LoadCalibreBooks",
        RuntimeEffect::OpenCalibreBook { .. } => "OpenCalibreBook",
        RuntimeEffect::EnsureCalibreThumbnail { .. } => "EnsureCalibreThumbnail",
        RuntimeEffect::SetRuntimeLogLevel { .. } => "SetRuntimeLogLevel",
        RuntimeEffect::FlushPersistence { .. } => "FlushPersistence",
        RuntimeEffect::SafeQuit => "SafeQuit",
    }
}

fn event_name(event: &AppEvent) -> &'static str {
    match event {
        AppEvent::OperationChanged { .. } => "OperationChanged",
        AppEvent::LoadingBootstrapChanged(_) => "LoadingBootstrapChanged",
        AppEvent::LoadingRecentsChanged(_) => "LoadingRecentsChanged",
        AppEvent::LoadingCalibreChanged(_) => "LoadingCalibreChanged",
        AppEvent::LoadingBrowserTabsChanged(_) => "LoadingBrowserTabsChanged",
        AppEvent::BootstrapLoaded { .. } => "BootstrapLoaded",
        AppEvent::SessionUpdated(_) => "SessionUpdated",
        AppEvent::ReaderUpdated(_) => "ReaderUpdated",
        AppEvent::ReaderPlaybackUpdated(_) => "ReaderPlaybackUpdated",
        AppEvent::SourceOpenProgress(_) => "SourceOpenProgress",
        AppEvent::SourceOpened { .. } => "SourceOpened",
        AppEvent::RecentsLoaded { .. } => "RecentsLoaded",
        AppEvent::CalibreBooksLoaded { .. } => "CalibreBooksLoaded",
        AppEvent::BrowserTabsHealthLoaded { .. } => "BrowserTabsHealthLoaded",
        AppEvent::BrowserTabWindowsLoaded { .. } => "BrowserTabWindowsLoaded",
        AppEvent::BrowserTabsLoaded { .. } => "BrowserTabsLoaded",
        AppEvent::CalibreLoadProgress(_) => "CalibreLoadProgress",
        AppEvent::TtsStateUpdated(_) => "TtsStateUpdated",
        AppEvent::PdfTranscriptionProgress(_) => "PdfTranscriptionProgress",
        AppEvent::LogLevelUpdated(_) => "LogLevelUpdated",
        AppEvent::CommandFailed { .. } => "CommandFailed",
    }
}

fn event_request_id(event: &AppEvent) -> Option<u64> {
    match event {
        AppEvent::BootstrapLoaded { request_id, .. } => Some(*request_id),
        AppEvent::RecentsLoaded { request_id, .. } => Some(*request_id),
        AppEvent::CalibreBooksLoaded { request_id, .. } => Some(*request_id),
        AppEvent::BrowserTabsHealthLoaded { request_id, .. } => Some(*request_id),
        AppEvent::BrowserTabWindowsLoaded { request_id, .. } => Some(*request_id),
        AppEvent::BrowserTabsLoaded { request_id, .. } => Some(*request_id),
        AppEvent::SourceOpened { request_id, .. } => Some(*request_id),
        AppEvent::SessionUpdated(event) => Some(event.request_id),
        AppEvent::ReaderUpdated(event) => Some(event.request_id),
        AppEvent::ReaderPlaybackUpdated(event) => Some(event.request_id),
        AppEvent::SourceOpenProgress(event) => Some(event.request_id),
        AppEvent::CalibreLoadProgress(event) => Some(event.request_id),
        AppEvent::TtsStateUpdated(event) => Some(event.request_id),
        AppEvent::PdfTranscriptionProgress(event) => Some(event.request_id),
        AppEvent::LogLevelUpdated(event) => Some(event.request_id),
        AppEvent::CommandFailed { request_id, .. } => Some(*request_id),
        _ => None,
    }
}
