use lanternleaf_app::contracts::{BridgeError, ReaderPlaybackStateEvent, TtsStateEvent};
use lanternleaf_app::pipeline::{AppEvent, OperationScope};
use lanternleaf_core::config::TtsBackend;
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

fn actionable_tts_failure_message(backend: TtsBackend, message: &str) -> String {
    let lower = message.to_ascii_lowercase();
    let cause = if lower.contains("voice") {
        "Windows voice availability"
    } else if lower.contains("piper")
        || lower.contains("model")
        || lower.contains("onnx")
        || lower.contains("config")
    {
        "Piper model/configuration"
    } else if lower.contains("audio")
        || lower.contains("output")
        || lower.contains("rodio")
        || lower.contains("sink")
    {
        "audio output"
    } else {
        "TTS synthesis/runtime"
    };
    format!("TTS failed (backend={backend:?}; cause={cause}): {message}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlaybackCursorProjection {
    pub(crate) source_path: String,
    pub(crate) page: usize,
    pub(crate) display_idx: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CanonicalFollowTarget {
    pub(crate) source_path: String,
    pub(crate) canonical_display_idx: usize,
}

pub(crate) fn follow_target_for_transition(
    previous: Option<&PlaybackCursorProjection>,
    incoming: &PlaybackCursorProjection,
    page_sentence_counts: &[usize],
    auto_scroll_enabled: bool,
) -> Option<CanonicalFollowTarget> {
    if !auto_scroll_enabled {
        return None;
    }
    let changed = previous
        .map(|previous| {
            previous.source_path != incoming.source_path
                || previous.page != incoming.page
                || previous.display_idx != incoming.display_idx
        })
        .unwrap_or(true);
    let display_idx = incoming.display_idx?;
    if !changed {
        return None;
    }
    let page_base = page_sentence_counts
        .iter()
        .take(incoming.page)
        .sum::<usize>();
    Some(CanonicalFollowTarget {
        source_path: incoming.source_path.clone(),
        canonical_display_idx: page_base.saturating_add(display_idx),
    })
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
                let previous = self.runtime.state_snapshot();
                let auto_scroll_enabled = previous
                    .reader_ui
                    .settings
                    .as_ref()
                    .map(|settings| settings.auto_scroll_tts)
                    .unwrap_or(true);
                let previous_cursor = previous.reader_playback.playback.as_ref().map(|value| {
                    PlaybackCursorProjection {
                        source_path: value.source_path.clone(),
                        page: value.current_page,
                        display_idx: value.highlighted_sentence_idx,
                    }
                });
                let incoming_cursor = PlaybackCursorProjection {
                    source_path: playback.source_path.clone(),
                    page: playback.current_page,
                    display_idx: playback.highlighted_sentence_idx,
                };
                let page_sentence_counts = previous
                    .reader_document
                    .snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.page_sentence_counts.as_slice())
                    .unwrap_or(&[]);
                if let Some(target) = follow_target_for_transition(
                    previous_cursor.as_ref(),
                    &incoming_cursor,
                    page_sentence_counts,
                    auto_scroll_enabled,
                ) {
                    self.auto_scroll_state
                        .request_cursor(target.source_path.clone(), target.canonical_display_idx);
                    trace!(
                        source_path = %target.source_path,
                        canonical_display_idx = target.canonical_display_idx,
                        "Requested pretty follow for canonical display cursor transition"
                    );
                }
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
                let backend = self
                    .runtime
                    .state_snapshot()
                    .reader_ui
                    .settings
                    .as_ref()
                    .map(|settings| settings.tts_backend)
                    .unwrap_or_default();
                let error = BridgeError {
                    code: "tts_runtime_failed".to_string(),
                    message: actionable_tts_failure_message(backend, &error_message),
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

#[cfg(test)]
mod tests {
    use super::{
        PlaybackCursorProjection, actionable_tts_failure_message, follow_target_for_transition,
    };
    use lanternleaf_core::config::TtsBackend;

    #[test]
    fn tts_failure_message_explains_backend_and_configuration_cause() {
        let message = actionable_tts_failure_message(
            TtsBackend::Piper,
            "Piper config not found at models/en_US-amy-medium.onnx.json",
        );
        assert!(message.contains("backend=Piper"));
        assert!(message.contains("Piper model/configuration"));
        assert!(message.contains("models/en_US-amy-medium.onnx.json"));
    }

    #[test]
    fn tts_failure_message_explains_voice_and_audio_causes() {
        assert!(
            actionable_tts_failure_message(TtsBackend::Windows, "configured voice not found")
                .contains("Windows voice availability")
        );
        assert!(
            actionable_tts_failure_message(TtsBackend::Windows, "Opening audio output failed")
                .contains("audio output")
        );
    }

    fn cursor(page: usize, display_idx: Option<usize>) -> PlaybackCursorProjection {
        PlaybackCursorProjection {
            source_path: "book.epub".to_string(),
            page,
            display_idx,
        }
    }

    #[test]
    fn production_follow_rule_tracks_100_plus_monotonic_canonical_advances() {
        let counts = [40, 40, 40, 40];
        let mut previous = None;
        for global_idx in 0..128 {
            let incoming = cursor(global_idx / 40, Some(global_idx % 40));
            let target = follow_target_for_transition(previous.as_ref(), &incoming, &counts, true)
                .expect("each changed canonical cursor should request one follow target");
            assert_eq!(target.canonical_display_idx, global_idx);
            previous = Some(incoming);
        }
    }

    #[test]
    fn production_follow_rule_handles_next_prev_and_page_transition_once() {
        let counts = [10, 10];
        let current = cursor(0, Some(5));
        let next = cursor(0, Some(6));
        let prev = cursor(0, Some(4));
        assert_eq!(
            follow_target_for_transition(Some(&current), &next, &counts, true)
                .unwrap()
                .canonical_display_idx,
            6
        );
        assert_eq!(
            follow_target_for_transition(Some(&current), &prev, &counts, true)
                .unwrap()
                .canonical_display_idx,
            4
        );
        let page_two = cursor(1, Some(0));
        assert_eq!(
            follow_target_for_transition(Some(&current), &page_two, &counts, true)
                .unwrap()
                .canonical_display_idx,
            10
        );
    }

    #[test]
    fn unchanged_pause_resume_repeat_voice_and_settings_do_not_follow() {
        let counts = [10];
        let current = cursor(0, Some(5));
        for unchanged_event in [
            cursor(0, Some(5)), // pause
            cursor(0, Some(5)), // resume
            cursor(0, Some(5)), // repeat
            cursor(0, Some(5)), // voice/backend/settings update
        ] {
            assert!(
                follow_target_for_transition(Some(&current), &unchanged_event, &counts, true,)
                    .is_none()
            );
        }
        assert!(follow_target_for_transition(Some(&current), &current, &counts, false).is_none());
        assert!(
            follow_target_for_transition(Some(&current), &cursor(0, Some(6)), &counts, false,)
                .is_none()
        );
    }
}
