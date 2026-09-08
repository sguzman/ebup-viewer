use lanternleaf_app::contracts::{BridgeError, ReaderPlaybackStateEvent, TtsStateEvent};
use lanternleaf_app::pipeline::{AppEvent, OperationScope};
use tracing::{info, trace, warn};

use super::LanternLeafApp;

fn normalize_for_tts_compare(input: &str) -> String {
    let mut out = String::new();
    let mut last_space = false;
    for ch in input.chars() {
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(ch.to_ascii_lowercase());
            last_space = false;
        }
    }
    out.trim().to_string()
}

impl LanternLeafApp {
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

            if let Some(playback) = event.playback.clone() {
                trace!(
                    tts_request_id,
                    app_request_id,
                    source_path = %playback.source_path,
                    page = playback.current_page + 1,
                    highlighted_sentence_idx = ?playback.highlighted_sentence_idx,
                    tts_state = ?playback.tts.state,
                    "TTS playback delta received"
                );
                self.runtime.apply_event(AppEvent::ReaderPlaybackUpdated(
                    ReaderPlaybackStateEvent {
                        request_id: app_request_id,
                        action: event.action.clone(),
                        playback,
                    },
                ));
                self.runtime.apply_event(AppEvent::OperationChanged {
                    scope: OperationScope::ReaderTts,
                    active: false,
                });
                self.runtime.apply_event(AppEvent::OperationChanged {
                    scope: OperationScope::ReaderCommand,
                    active: false,
                });
            }
            if let Some(tts) = event.tts.clone() {
                if tts.current_sentence_idx.is_some() {
                    self.auto_scroll_state.note_auto_scroll();
                }
                self.runtime
                    .apply_event(AppEvent::TtsStateUpdated(TtsStateEvent {
                        request_id: app_request_id,
                        action: event.action.clone(),
                        tts,
                    }));
            }
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
