use lanternleaf_app::contracts::{
    BridgeError, ReaderPlaybackStateEvent, ReaderStateEvent, TtsStateEvent,
};
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
    pub(crate) fn sync_tts_runtime_session(&mut self) {
        let session_guard = match self.effect_session.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        let current_source = session_guard
            .as_ref()
            .map(|session| session.source_path.clone());
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
                trace!(
                    tts_request_id,
                    app_request_id,
                    source_path = %snapshot.source_path,
                    page = snapshot.current_page + 1,
                    text_only_mode = snapshot.text_only_mode,
                    text_only_show_original_text = snapshot.settings.text_only_show_original_text,
                    text_only_override = ?self.text_only_override,
                    sentence_count = snapshot.sentences.len(),
                    tts_state = ?snapshot.tts.state,
                    highlighted_sentence_idx = ?snapshot.highlighted_sentence_idx,
                    tts_audio_idx = ?snapshot.tts.current_sentence_idx,
                    "TTS runtime snapshot received"
                );
                if snapshot.tts.state == lanternleaf_core::session::TtsPlaybackState::Playing {
                    let highlighted_text = snapshot
                        .highlighted_sentence_idx
                        .and_then(|idx| snapshot.sentences.get(idx))
                        .map(String::as_str);
                    let spoken_text = snapshot.tts_current_sentence_text.as_deref();
                    match (highlighted_text, spoken_text) {
                        (Some(highlighted), Some(spoken)) => {
                            let highlighted_norm = normalize_for_tts_compare(highlighted);
                            let spoken_norm = normalize_for_tts_compare(spoken);
                            if !highlighted_norm.is_empty()
                                && !spoken_norm.is_empty()
                                && highlighted_norm != spoken_norm
                            {
                                trace!(
                                    tts_request_id,
                                    app_request_id,
                                    highlighted_len = highlighted.len(),
                                    spoken_len = spoken.len(),
                                    highlighted_preview = %highlighted.chars().take(80).collect::<String>(),
                                    spoken_preview = %spoken.chars().take(80).collect::<String>(),
                                    display_idx = snapshot.highlighted_sentence_idx,
                                    audio_idx = snapshot.tts.current_sentence_idx,
                                    "TTS desync detected: spoken sentence text differs from highlighted sentence text"
                                );
                            }
                        }
                        _ => {}
                    }
                }
                self.maybe_reapply_text_only(&snapshot);
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
                self.runtime
                    .apply_event(AppEvent::ReaderUpdated(ReaderStateEvent {
                        request_id: app_request_id,
                        action: event.action.clone(),
                        reader: snapshot.clone(),
                    }));
                self.runtime
                    .apply_event(AppEvent::TtsStateUpdated(TtsStateEvent {
                        request_id: app_request_id,
                        action: event.action.clone(),
                        tts: snapshot.tts.clone(),
                    }));
            } else if let Some(playback) = event.playback.clone() {
                self.runtime.apply_event(AppEvent::ReaderPlaybackUpdated(
                    ReaderPlaybackStateEvent {
                        request_id: app_request_id,
                        action: event.action.clone(),
                        playback,
                    },
                ));
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
