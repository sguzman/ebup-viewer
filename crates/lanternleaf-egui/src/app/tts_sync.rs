use lanternleaf_app::contracts::{BridgeError, ReaderPlaybackStateEvent, ReaderStateEvent, TtsStateEvent};
use lanternleaf_app::pipeline::{AppEvent, OperationScope};
use tracing::{info, trace, warn};

use super::LanternLeafApp;

impl LanternLeafApp {
    pub(crate) fn sync_tts_runtime_session(&mut self) {
        let session_guard = match self.effect_session.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        let current_source = session_guard.as_ref().map(|session| session.source_path.clone());
        if current_source == self.tts_session_source {
            return;
        }
        self.tts_session_source = current_source.clone();
        self.tts_runtime.set_session(session_guard.clone());
        trace!(source = ?current_source, "Synced TTS runtime session");
    }

    pub(crate) fn handle_tts_runtime_events(&mut self) {
        for event in self.tts_runtime.collect_events() {
            let tts_request_id = event.request_id;
            let app_request_id = self.runtime.next_request_id();
            match event.kind {
                lanternleaf_app::tts_runtime::TtsRuntimeEventKind::Progress
                | lanternleaf_app::tts_runtime::TtsRuntimeEventKind::StateChanged => {
                    trace!(
                        tts_request_id,
                        app_request_id,
                        action = %event.action,
                        kind = ?event.kind,
                        "Applying TTS runtime state event"
                    );
                }
                lanternleaf_app::tts_runtime::TtsRuntimeEventKind::Queued => {
                    info!(
                        tts_request_id,
                        app_request_id,
                        action = %event.action,
                        message = event.message.as_deref().unwrap_or("queued"),
                        "Queued TTS runtime batch"
                    );
                }
                lanternleaf_app::tts_runtime::TtsRuntimeEventKind::Completed => {
                    info!(
                        tts_request_id,
                        app_request_id,
                        action = %event.action,
                        "TTS runtime completed"
                    );
                }
                lanternleaf_app::tts_runtime::TtsRuntimeEventKind::Cancelled => {
                    warn!(
                        tts_request_id,
                        app_request_id,
                        action = %event.action,
                        "TTS runtime cancelled"
                    );
                }
                lanternleaf_app::tts_runtime::TtsRuntimeEventKind::Failed => {
                    warn!(
                        tts_request_id,
                        app_request_id,
                        action = %event.action,
                        message = event.message.as_deref().unwrap_or("unknown"),
                        "TTS runtime failed"
                    );
                }
            }

            if let Some(snapshot) = event.snapshot.clone() {
                if let Some(cursor) = event.cursor {
                    trace!(
                        tts_request_id,
                        app_request_id,
                        page = cursor.page + 1,
                        audio_idx = ?cursor.audio_idx,
                        display_idx = ?cursor.display_idx,
                        "TTS cursor updated"
                    );
                }
                if snapshot.settings.auto_scroll_tts
                    && matches!(
                        event.kind,
                        lanternleaf_app::tts_runtime::TtsRuntimeEventKind::Progress
                            | lanternleaf_app::tts_runtime::TtsRuntimeEventKind::StateChanged
                    )
                    && snapshot.highlighted_sentence_idx.is_some()
                {
                    self.auto_scroll_state.note_auto_scroll();
                }
                self.runtime.apply_event(AppEvent::ReaderUpdated(ReaderStateEvent {
                    request_id: app_request_id,
                    action: event.action.clone(),
                    reader: snapshot.clone(),
                }));
                self.runtime.apply_event(AppEvent::TtsStateUpdated(TtsStateEvent {
                    request_id: app_request_id,
                    action: event.action.clone(),
                    tts: snapshot.tts.clone(),
                }));
            } else if let Some(playback) = event.playback.clone() {
                self.runtime
                    .apply_event(AppEvent::ReaderPlaybackUpdated(ReaderPlaybackStateEvent {
                        request_id: app_request_id,
                        action: event.action.clone(),
                        playback,
                    }));
            }

            if event.kind == lanternleaf_app::tts_runtime::TtsRuntimeEventKind::Failed {
                let error_message = event
                    .message
                    .clone()
                    .unwrap_or_else(|| "TTS runtime failed".to_string());
                let error = BridgeError {
                    code: "tts_runtime_failed".to_string(),
                    message: error_message,
                };
                self.runtime.apply_event(AppEvent::CommandFailed {
                    request_id: app_request_id,
                    scope: Some(OperationScope::ReaderTts),
                    error,
                });
            }

            self.last_tts_runtime_event = Some(event);
        }
    }
}
