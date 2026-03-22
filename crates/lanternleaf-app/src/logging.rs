use crate::pipeline::{AppCommand, AppEvent, PersistenceOutcome, PersistenceTrigger, RuntimeEffect};
use tracing::{Span, field, info_span};

pub fn command_span(request_id: u64, command: &AppCommand) -> Span {
    let span = info_span!(
        "app_command",
        request_id,
        action = command.action(),
        command = %command_name(command),
        source_path = field::Empty,
        tab_id = field::Empty,
        window_id = field::Empty,
        calibre_id = field::Empty,
        trigger = field::Empty,
        log_level = field::Empty,
        text_len = field::Empty,
        refresh = field::Empty,
        query_present = field::Empty
    );
    let fields = command_span_fields(command);
    record_span_fields(&span, &fields);
    span
}

pub fn effect_span(request_id: u64, effect: &RuntimeEffect) -> Span {
    let span = info_span!(
        "runtime_effect",
        request_id,
        effect = %effect_name(effect),
        owner = ?effect.owner(),
        source_path = field::Empty,
        tab_id = field::Empty,
        window_id = field::Empty,
        calibre_id = field::Empty,
        trigger = field::Empty,
        text_len = field::Empty,
        refresh = field::Empty,
        query_present = field::Empty
    );
    let fields = effect_span_fields(effect);
    record_span_fields(&span, &fields);
    span
}

pub fn event_span(event: &AppEvent) -> Span {
    let name = event_name(event);
    let id = event_request_id(event).unwrap_or_default();
    let span = info_span!(
        "app_event",
        request_id = id,
        event = name,
        trigger = field::Empty,
        outcome = field::Empty,
        scope = field::Empty,
        error_code = field::Empty
    );
    let fields = event_span_fields(event);
    record_event_span_fields(&span, &fields);
    span
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct SpanFields {
    source_path: Option<String>,
    tab_id: Option<u64>,
    window_id: Option<u64>,
    calibre_id: Option<u64>,
    trigger: Option<PersistenceTrigger>,
    log_level: Option<String>,
    text_len: Option<usize>,
    refresh: Option<bool>,
    query_present: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct EventSpanFields {
    trigger: Option<PersistenceTrigger>,
    outcome: Option<PersistenceOutcome>,
    scope: Option<&'static str>,
    error_code: Option<String>,
}

fn record_span_fields(span: &Span, fields: &SpanFields) {
    if let Some(path) = &fields.source_path {
        span.record("source_path", &field::display(path));
    }
    if let Some(tab_id) = fields.tab_id {
        span.record("tab_id", &field::display(tab_id));
    }
    if let Some(window_id) = fields.window_id {
        span.record("window_id", &field::display(window_id));
    }
    if let Some(calibre_id) = fields.calibre_id {
        span.record("calibre_id", &field::display(calibre_id));
    }
    if let Some(trigger) = fields.trigger {
        span.record("trigger", &field::debug(trigger));
    }
    if let Some(level) = &fields.log_level {
        span.record("log_level", &field::display(level));
    }
    if let Some(text_len) = fields.text_len {
        span.record("text_len", &field::display(text_len));
    }
    if let Some(refresh) = fields.refresh {
        span.record("refresh", &field::display(refresh));
    }
    if let Some(query_present) = fields.query_present {
        span.record("query_present", &field::display(query_present));
    }
}

fn record_event_span_fields(span: &Span, fields: &EventSpanFields) {
    if let Some(trigger) = fields.trigger {
        span.record("trigger", &field::debug(trigger));
    }
    if let Some(outcome) = fields.outcome {
        span.record("outcome", &field::debug(outcome));
    }
    if let Some(scope) = fields.scope {
        span.record("scope", &field::display(scope));
    }
    if let Some(code) = &fields.error_code {
        span.record("error_code", &field::display(code));
    }
}

fn command_span_fields(command: &AppCommand) -> SpanFields {
    match command {
        AppCommand::OpenSourcePath { path } => SpanFields {
            source_path: Some(path.clone()),
            ..SpanFields::default()
        },
        AppCommand::OpenClipboardText { text } => SpanFields {
            text_len: Some(text.len()),
            ..SpanFields::default()
        },
        AppCommand::OpenBrowserTab { tab_id, window_id }
        | AppCommand::OpenBrowserTabBundle { tab_id, window_id }
        | AppCommand::RefreshBrowserTab { tab_id, window_id } => SpanFields {
            tab_id: Some(*tab_id),
            window_id: *window_id,
            ..SpanFields::default()
        },
        AppCommand::ListBrowserTabs { query, refresh, .. } => SpanFields {
            refresh: Some(*refresh),
            query_present: Some(query.as_ref().map(|text| !text.trim().is_empty()).unwrap_or(false)),
            ..SpanFields::default()
        },
        AppCommand::OpenCalibreBook { id } | AppCommand::EnsureCalibreThumbnail { id } => {
            SpanFields {
                calibre_id: Some(*id),
                ..SpanFields::default()
            }
        }
        AppCommand::SetRuntimeLogLevel { level } => SpanFields {
            log_level: Some(level.clone()),
            ..SpanFields::default()
        },
        AppCommand::FlushPersistence { trigger } => SpanFields {
            trigger: Some(*trigger),
            ..SpanFields::default()
        },
        _ => SpanFields::default(),
    }
}

fn effect_span_fields(effect: &RuntimeEffect) -> SpanFields {
    match effect {
        RuntimeEffect::OpenSourcePath { path } => SpanFields {
            source_path: Some(path.clone()),
            ..SpanFields::default()
        },
        RuntimeEffect::OpenClipboardText { text } => SpanFields {
            text_len: Some(text.len()),
            ..SpanFields::default()
        },
        RuntimeEffect::OpenBrowserTab { tab_id, window_id }
        | RuntimeEffect::OpenBrowserTabBundle { tab_id, window_id }
        | RuntimeEffect::RefreshBrowserTab { tab_id, window_id } => SpanFields {
            tab_id: Some(*tab_id),
            window_id: *window_id,
            ..SpanFields::default()
        },
        RuntimeEffect::ListBrowserTabs { query, refresh, .. } => SpanFields {
            refresh: Some(*refresh),
            query_present: Some(query.as_ref().map(|text| !text.trim().is_empty()).unwrap_or(false)),
            ..SpanFields::default()
        },
        RuntimeEffect::OpenCalibreBook { id } | RuntimeEffect::EnsureCalibreThumbnail { id } => {
            SpanFields {
                calibre_id: Some(*id),
                ..SpanFields::default()
            }
        }
        RuntimeEffect::FlushPersistence { trigger } => SpanFields {
            trigger: Some(*trigger),
            ..SpanFields::default()
        },
        _ => SpanFields::default(),
    }
}

fn event_span_fields(event: &AppEvent) -> EventSpanFields {
    match event {
        AppEvent::PersistenceFlushed { trigger, outcome, .. } => EventSpanFields {
            trigger: Some(*trigger),
            outcome: Some(*outcome),
            ..EventSpanFields::default()
        },
        AppEvent::CommandFailed { scope, error, .. } => EventSpanFields {
            scope: scope.map(scope_label),
            error_code: Some(error.code.clone()),
            ..EventSpanFields::default()
        },
        _ => EventSpanFields::default(),
    }
}

fn scope_label(scope: crate::pipeline::OperationScope) -> &'static str {
    match scope {
        crate::pipeline::OperationScope::SourceOpen => "source_open",
        crate::pipeline::OperationScope::StarterCommand => "starter_command",
        crate::pipeline::OperationScope::ReaderCommand => "reader_command",
        crate::pipeline::OperationScope::ReaderTts => "reader_tts",
        crate::pipeline::OperationScope::ReaderSettings => "reader_settings",
        crate::pipeline::OperationScope::BrowserTabRefresh => "browser_tab_refresh",
        crate::pipeline::OperationScope::CalibreLoad => "calibre_load",
        crate::pipeline::OperationScope::RuntimeConfig => "runtime_config",
    }
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
        AppEvent::NotificationRaised { .. } => "NotificationRaised",
        AppEvent::NotificationDismissed { .. } => "NotificationDismissed",
        AppEvent::PersistenceFlushed { .. } => "PersistenceFlushed",
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
        AppEvent::NotificationRaised { request_id, .. } => Some(*request_id),
        AppEvent::NotificationDismissed { request_id, .. } => Some(*request_id),
        AppEvent::PersistenceFlushed { request_id, .. } => Some(*request_id),
        AppEvent::CommandFailed { request_id, .. } => Some(*request_id),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::BridgeError;

    #[test]
    fn command_span_fields_capture_context() {
        let fields = command_span_fields(&AppCommand::OpenSourcePath {
            path: "/tmp/book.epub".to_string(),
        });
        assert_eq!(fields.source_path.as_deref(), Some("/tmp/book.epub"));

        let fields = command_span_fields(&AppCommand::OpenBrowserTab {
            tab_id: 7,
            window_id: Some(2),
        });
        assert_eq!(fields.tab_id, Some(7));
        assert_eq!(fields.window_id, Some(2));

        let fields = command_span_fields(&AppCommand::FlushPersistence {
            trigger: PersistenceTrigger::SafeQuit,
        });
        assert_eq!(fields.trigger, Some(PersistenceTrigger::SafeQuit));
    }

    #[test]
    fn effect_span_fields_capture_context() {
        let fields = effect_span_fields(&RuntimeEffect::OpenClipboardText {
            text: "hello".to_string(),
        });
        assert_eq!(fields.text_len, Some(5));

        let fields = effect_span_fields(&RuntimeEffect::RefreshBrowserTab {
            tab_id: 3,
            window_id: None,
        });
        assert_eq!(fields.tab_id, Some(3));
        assert_eq!(fields.window_id, None);
    }

    #[test]
    fn event_span_fields_capture_context() {
        let fields = event_span_fields(&AppEvent::PersistenceFlushed {
            request_id: 1,
            trigger: PersistenceTrigger::SourceOpen,
            outcome: PersistenceOutcome::Completed,
        });
        assert_eq!(fields.trigger, Some(PersistenceTrigger::SourceOpen));
        assert_eq!(fields.outcome, Some(PersistenceOutcome::Completed));

        let fields = event_span_fields(&AppEvent::CommandFailed {
            request_id: 2,
            scope: Some(crate::pipeline::OperationScope::ReaderCommand),
            error: BridgeError {
                code: "boom".to_string(),
                message: "fail".to_string(),
            },
        });
        assert_eq!(fields.scope, Some("reader_command"));
        assert_eq!(fields.error_code.as_deref(), Some("boom"));
    }
}
