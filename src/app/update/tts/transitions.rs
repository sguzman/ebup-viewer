use super::super::super::state::{App, TtsLifecycle};
use crate::normalizer::PageNormalization;
use std::time::Duration;
use tracing::{debug, info, warn};

#[derive(Debug)]
pub(super) enum TtsEvent {
    StartRequested {
        page: usize,
        sentence_idx: usize,
    },
    PlanReady {
        page: usize,
        requested_display_idx: usize,
        request_id: u64,
        plan: PageNormalization,
    },
}

#[derive(Debug)]
pub(super) enum TtsAction {
    SchedulePlan {
        page: usize,
        requested_display_idx: usize,
        request_id: u64,
        display_sentences: Vec<String>,
    },
    DispatchPrepareBatches {
        page: usize,
        request_id: u64,
        audio_start_idx: usize,
        audio_sentences: Vec<String>,
    },
}

pub(super) fn transition(app: &mut App, event: TtsEvent) -> Vec<TtsAction> {
    match event {
        TtsEvent::StartRequested { page, sentence_idx } => {
            on_start_requested(app, page, sentence_idx)
        }
        TtsEvent::PlanReady {
            page,
            requested_display_idx,
            request_id,
            plan,
        } => on_plan_ready(app, page, requested_display_idx, request_id, plan),
    }
}

fn on_start_requested(app: &mut App, page: usize, sentence_idx: usize) -> Vec<TtsAction> {
    if app.tts.engine.is_none() {
        return Vec::new();
    }

    app.stop_playback();
    app.tts.track.clear();
    app.tts.elapsed = Duration::ZERO;
    app.tts.started_at = None;

    let display_sentences = app.raw_sentences_for_page(page);
    if display_sentences.is_empty() {
        app.tts.lifecycle = TtsLifecycle::Idle;
        app.tts.pending_append = false;
        app.tts.pending_append_batch = None;
        app.tts.current_sentence_idx = None;
        app.tts.sentence_offset = 0;
        app.tts.display_to_audio.clear();
        app.tts.audio_to_display.clear();
        return Vec::new();
    }

    let requested_display_idx = sentence_idx.min(display_sentences.len().saturating_sub(1));
    if let Some((preparing_page, preparing_sentence_idx, _)) = app.tts.preparing_context() {
        if preparing_page == page && preparing_sentence_idx == requested_display_idx {
            info!(
                page = page + 1,
                sentence_idx = requested_display_idx,
                "Skipping duplicate TTS start request while preparation is in progress"
            );
            return Vec::new();
        }
    }
    app.tts.current_sentence_idx = Some(requested_display_idx);
    app.tts.sentence_offset = requested_display_idx;

    app.tts.started_at = None;
    app.tts.elapsed = Duration::ZERO;
    app.tts.pending_append = false;
    app.tts.pending_append_batch = None;
    app.tts.request_id = app.tts.request_id.wrapping_add(1);
    let request_id = app.tts.request_id;
    app.tts.lifecycle = TtsLifecycle::Preparing {
        page,
        sentence_idx: requested_display_idx,
        request_id,
    };
    info!(
        page = page + 1,
        sentence_idx = requested_display_idx,
        request_id,
        "Scheduling async TTS planning tasks"
    );
    debug!(
        page = page + 1,
        sentence_idx = requested_display_idx,
        request_id,
        "Quick-start batch disabled; waiting for full normalization plan"
    );

    vec![TtsAction::SchedulePlan {
        page,
        requested_display_idx,
        request_id,
        display_sentences,
    }]
}

fn on_plan_ready(
    app: &mut App,
    page: usize,
    requested_display_idx: usize,
    request_id: u64,
    plan: PageNormalization,
) -> Vec<TtsAction> {
    if request_id != app.tts.request_id {
        debug!(
            request_id,
            current = app.tts.request_id,
            "Ignoring stale TTS plan request"
        );
        return Vec::new();
    }
    if page != app.reader.current_page {
        debug!(
            page,
            current = app.reader.current_page,
            "Ignoring stale TTS plan for different page"
        );
        return Vec::new();
    }

    let full_audio_sentences = plan.audio_sentences;
    if full_audio_sentences.is_empty() {
        warn!(
            page = page + 1,
            display_idx = requested_display_idx,
            "No speakable text on page after normalization"
        );
        app.tts.lifecycle = TtsLifecycle::Idle;
        app.tts.pending_append = false;
        app.tts.pending_append_batch = None;
        app.tts.current_sentence_idx = Some(requested_display_idx);
        app.tts.sentence_offset = 0;
        return Vec::new();
    }

    app.tts.display_to_audio = plan
        .display_to_audio
        .into_iter()
        .map(|mapped| mapped.filter(|idx| *idx < full_audio_sentences.len()))
        .collect();
    app.tts.audio_to_display = plan
        .audio_to_display
        .into_iter()
        .take(full_audio_sentences.len())
        .collect();

    let Some(mut audio_start_idx) =
        app.find_audio_start_for_display_sentence(requested_display_idx)
    else {
        warn!(
            page = page + 1,
            display_idx = requested_display_idx,
            "No speakable text on page after normalization"
        );
        app.tts.lifecycle = TtsLifecycle::Idle;
        app.tts.pending_append = false;
        app.tts.pending_append_batch = None;
        app.tts.current_sentence_idx = Some(requested_display_idx);
        app.tts.sentence_offset = 0;
        return Vec::new();
    };

    audio_start_idx = audio_start_idx.min(full_audio_sentences.len().saturating_sub(1));
    let display_start_idx = app
        .display_index_for_audio_sentence(audio_start_idx)
        .unwrap_or(requested_display_idx);
    app.tts.sentence_offset = audio_start_idx;
    app.tts.current_sentence_idx = Some(display_start_idx);

    vec![TtsAction::DispatchPrepareBatches {
        page,
        request_id,
        audio_start_idx,
        audio_sentences: full_audio_sentences,
    }]
}
