use super::super::state::{
    App, MAX_TTS_SPEED, MAX_TTS_VOLUME, MIN_TTS_SPEED, MIN_TTS_VOLUME, TtsLifecycle,
};
use super::Effect;
use iced::Task;
use iced::widget::scrollable::RelativeOffset;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

mod effects;
mod transitions;

impl App {
    pub(super) fn handle_toggle_tts_controls(&mut self, effects: &mut Vec<Effect>) {
        debug!("Toggled TTS controls");
        self.config.show_tts = !self.config.show_tts;
        effects.push(Effect::SaveConfig);
    }

    pub(super) fn handle_pause_after_sentence_changed(
        &mut self,
        pause: f32,
        effects: &mut Vec<Effect>,
    ) {
        let clamped = if pause.is_finite() {
            pause.clamp(0.0, 2.0)
        } else {
            self.config.pause_after_sentence
        };
        if (clamped - self.config.pause_after_sentence).abs() > f32::EPSILON {
            self.config.pause_after_sentence = clamped;
            info!(pause_secs = clamped, "Updated pause after sentence");
            effects.push(Effect::SaveConfig);
            if let Some(playback) = &self.tts.playback {
                self.tts.resume_after_prepare = !playback.is_paused();
                let idx = self
                    .tts
                    .current_sentence_idx
                    .or_else(|| self.display_index_for_audio_sentence(self.tts.sentence_offset))
                    .unwrap_or(0);
                effects.push(Effect::StartTts {
                    page: self.reader.current_page,
                    sentence_idx: idx,
                });
                effects.push(Effect::AutoScrollToCurrent);
                effects.push(Effect::SaveBookmark);
            }
        }
    }

    pub(super) fn handle_auto_scroll_tts_changed(
        &mut self,
        enabled: bool,
        effects: &mut Vec<Effect>,
    ) {
        if self.config.auto_scroll_tts != enabled {
            self.config.auto_scroll_tts = enabled;
            info!(enabled, "Updated auto-scroll to spoken sentence");
            effects.push(Effect::SaveConfig);
            if enabled {
                effects.push(Effect::AutoScrollToCurrent);
                effects.push(Effect::SaveBookmark);
            }
        }
    }

    pub(super) fn handle_center_spoken_sentence_changed(
        &mut self,
        centered: bool,
        effects: &mut Vec<Effect>,
    ) {
        if self.config.center_spoken_sentence != centered {
            self.config.center_spoken_sentence = centered;
            info!(centered, "Updated centered tracking preference");
            effects.push(Effect::SaveConfig);
            if self.config.auto_scroll_tts {
                effects.push(Effect::AutoScrollToCurrent);
                effects.push(Effect::SaveBookmark);
            }
        }
    }

    pub(super) fn handle_set_tts_speed(&mut self, speed: f32, effects: &mut Vec<Effect>) {
        let clamped = speed.clamp(MIN_TTS_SPEED, MAX_TTS_SPEED);
        self.config.tts_speed = clamped;
        info!(speed = self.config.tts_speed, "Adjusted TTS speed");
        if let Some(playback) = &self.tts.playback {
            self.tts.resume_after_prepare = !playback.is_paused();
            let idx = self.tts.current_sentence_idx.unwrap_or(0);
            effects.push(Effect::StartTts {
                page: self.reader.current_page,
                sentence_idx: idx,
            });
            effects.push(Effect::AutoScrollToCurrent);
            effects.push(Effect::SaveBookmark);
        }
        effects.push(Effect::SaveConfig);
    }

    pub(super) fn handle_set_tts_volume(&mut self, volume: f32, effects: &mut Vec<Effect>) {
        let clamped = volume.clamp(MIN_TTS_VOLUME, MAX_TTS_VOLUME);
        self.config.tts_volume = clamped;
        if let Some(playback) = &self.tts.playback {
            playback.set_volume(clamped);
        }
        info!(volume = self.config.tts_volume, "Adjusted TTS volume");
        effects.push(Effect::SaveConfig);
    }

    pub(super) fn handle_play(&mut self, effects: &mut Vec<Effect>) {
        if let Some((page, sentence_idx, _)) = self.tts.preparing_context() {
            info!(
                page = page + 1,
                sentence_idx,
                "TTS batch preparation already in progress; ignoring duplicate play request"
            );
            return;
        }
        if let Some(playback) = &self.tts.playback {
            info!("Resuming TTS playback");
            playback.play();
            self.tts.lifecycle = TtsLifecycle::Playing;
            self.tts.started_at = Some(Instant::now());
        } else {
            let start_idx = self.tts.current_sentence_idx.unwrap_or(0);
            self.tts.resume_after_prepare = true;
            info!(start_idx, "Starting TTS playback from cursor");
            effects.push(Effect::StartTts {
                page: self.reader.current_page,
                sentence_idx: start_idx,
            });
            effects.push(Effect::AutoScrollToCurrent);
            effects.push(Effect::SaveBookmark);
        }
    }

    pub(super) fn handle_toggle_play_pause(&mut self, effects: &mut Vec<Effect>) {
        let currently_playing = self
            .tts
            .playback
            .as_ref()
            .map(|p| !p.is_paused())
            .unwrap_or(false);
        if self.tts.is_preparing() || currently_playing {
            self.handle_pause(effects);
        } else {
            self.handle_play(effects);
        }
    }

    pub(super) fn handle_play_from_page_start(&mut self, effects: &mut Vec<Effect>) {
        info!("Playing page from start");
        self.tts.resume_after_prepare = true;
        effects.push(Effect::StartTts {
            page: self.reader.current_page,
            sentence_idx: 0,
        });
        effects.push(Effect::AutoScrollToCurrent);
        effects.push(Effect::SaveBookmark);
    }

    pub(super) fn handle_play_from_cursor(&mut self, idx: usize, effects: &mut Vec<Effect>) {
        self.begin_play_from_sentence(idx, effects, "Playing from cursor");
    }

    pub(super) fn handle_sentence_clicked(&mut self, idx: usize, effects: &mut Vec<Effect>) {
        self.begin_play_from_sentence(idx, effects, "Sentence clicked; playing from sentence");
    }

    pub(super) fn handle_repeat_current_sentence(&mut self, effects: &mut Vec<Effect>) {
        let idx = self.tts.current_sentence_idx.unwrap_or(0);
        self.begin_play_from_sentence(idx, effects, "Repeating current sentence");
    }

    pub(super) fn handle_pause(&mut self, _effects: &mut Vec<Effect>) {
        let mut paused_playback = false;
        if self.tts.is_preparing() {
            self.tts.request_id = self.tts.request_id.wrapping_add(1);
            self.tts.lifecycle = TtsLifecycle::Idle;
            self.tts.pending_append = false;
            self.tts.pending_append_batch = None;
            info!("Cancelled pending TTS batch preparation");
        }
        if let Some(playback) = &self.tts.playback {
            info!("Pausing TTS playback");
            playback.pause();
            paused_playback = true;
        }
        if paused_playback {
            self.tts.lifecycle = TtsLifecycle::Paused;
            if let Some(started) = self.tts.started_at.take() {
                self.tts.elapsed += Instant::now().saturating_duration_since(started);
            }
        }
    }

    pub(super) fn handle_seek_forward(&mut self, effects: &mut Vec<Effect>) {
        let next_idx = self.tts.current_sentence_idx.unwrap_or(0) + 1;
        if next_idx < self.sentence_count_for_page(self.reader.current_page) {
            info!(next_idx, "Seeking forward within page");
            self.tts.resume_after_prepare = true;
            effects.push(Effect::StartTts {
                page: self.reader.current_page,
                sentence_idx: next_idx,
            });
            effects.push(Effect::AutoScrollToCurrent);
            effects.push(Effect::SaveBookmark);
        } else if self.reader.current_page + 1 < self.reader.pages.len() {
            self.reader.current_page += 1;
            info!("Seeking forward into next page");
            self.tts.resume_after_prepare = true;
            effects.push(Effect::StartTts {
                page: self.reader.current_page,
                sentence_idx: 0,
            });
            self.bookmark.last_scroll_offset = RelativeOffset::START;
            effects.push(Effect::SaveConfig);
            effects.push(Effect::AutoScrollToCurrent);
            effects.push(Effect::SaveBookmark);
        }
    }

    pub(super) fn handle_seek_backward(&mut self, effects: &mut Vec<Effect>) {
        let current_idx = self.tts.current_sentence_idx.unwrap_or(0);
        if current_idx > 0 {
            info!(
                previous_idx = current_idx.saturating_sub(1),
                "Seeking backward within page"
            );
            self.tts.resume_after_prepare = true;
            effects.push(Effect::StartTts {
                page: self.reader.current_page,
                sentence_idx: current_idx - 1,
            });
            effects.push(Effect::AutoScrollToCurrent);
            effects.push(Effect::SaveBookmark);
        } else if self.reader.current_page > 0 {
            self.reader.current_page -= 1;
            let last_idx = self
                .sentence_count_for_page(self.reader.current_page)
                .saturating_sub(1);
            info!("Seeking backward into previous page");
            self.tts.resume_after_prepare = true;
            effects.push(Effect::StartTts {
                page: self.reader.current_page,
                sentence_idx: last_idx,
            });
            self.bookmark.last_scroll_offset = RelativeOffset::START;
            effects.push(Effect::SaveConfig);
            effects.push(Effect::AutoScrollToCurrent);
            effects.push(Effect::SaveBookmark);
        }
    }

    pub(super) fn handle_tick(&mut self, now: Instant, effects: &mut Vec<Effect>) {
        if !self.tts.is_playing() {
            return;
        }
        if self
            .tts
            .playback
            .as_ref()
            .map(|p| p.is_paused())
            .unwrap_or(false)
        {
            return;
        }

        let _ = now;
        let mut target_idx = None;
        let offset = self.tts.sentence_offset;
        if let Some(playback) = &self.tts.playback {
            let total_sources = self.tts.total_sources;
            let remaining = playback.queued_sources();
            let consumed = total_sources.saturating_sub(remaining);
            let per_sentence = self.tts.sources_per_sentence.max(1);
            let sentence_progress = consumed / per_sentence;
            if sentence_progress < self.tts.track.len() {
                target_idx = Some(offset + sentence_progress);
            }
        }

        // Fallback for edge cases where source queue info is unavailable.
        if target_idx.is_none() {
            let Some(started) = self.tts.started_at else {
                return;
            };
            let elapsed = self.tts.elapsed + Instant::now().saturating_duration_since(started);
            let mut acc = Duration::ZERO;
            let pause = Duration::from_secs_f32(self.config.pause_after_sentence);
            for (i, (_, dur)) in self.tts.track.iter().enumerate() {
                acc += *dur + pause;
                if elapsed <= acc {
                    target_idx = Some(offset + i);
                    break;
                }
            }
        }

        if let Some(idx) = target_idx {
            let max_audio_idx = self.tts.audio_to_display.len().saturating_sub(1);
            let clamped_audio = idx.min(max_audio_idx);
            let display_idx = self
                .display_index_for_audio_sentence(clamped_audio)
                .unwrap_or_else(|| {
                    clamped_audio.min(
                        self.sentence_count_for_page(self.reader.current_page)
                            .saturating_sub(1),
                    )
                });
            if Some(display_idx) != self.tts.current_sentence_idx {
                self.tts.current_sentence_idx = Some(display_idx);
                effects.push(Effect::AutoScrollToCurrent);
                effects.push(Effect::SaveBookmark);
            }
        } else {
            if self.tts.pending_append {
                return;
            }
            effects.push(Effect::StopTts);
            if self.reader.current_page + 1 < self.reader.pages.len() {
                self.reader.current_page += 1;
                self.bookmark.last_scroll_offset = RelativeOffset::START;
                info!("Playback finished page, advancing");
                effects.push(Effect::StartTts {
                    page: self.reader.current_page,
                    sentence_idx: 0,
                });
                effects.push(Effect::AutoScrollToCurrent);
                effects.push(Effect::SaveBookmark);
            } else {
                info!("Playback finished at end of book");
            }
        }
    }

    pub(super) fn handle_tts_prepared(
        &mut self,
        page: usize,
        start_idx: usize,
        request_id: u64,
        files: Vec<(std::path::PathBuf, Duration)>,
        effects: &mut Vec<Effect>,
    ) {
        if request_id != self.tts.request_id {
            debug!(
                request_id,
                current = self.tts.request_id,
                "Ignoring stale TTS request"
            );
            return;
        }
        self.tts.lifecycle = TtsLifecycle::Idle;
        info!(
            page,
            start_idx,
            request_id,
            file_count = files.len(),
            "Received prepared TTS batch"
        );
        if page != self.reader.current_page {
            debug!(
                page,
                current = self.reader.current_page,
                "Ignoring stale TTS batch"
            );
            return;
        }
        if files.is_empty() {
            warn!("TTS batch was empty; stopping playback");
            self.stop_playback();
            self.tts.current_sentence_idx = None;
            return;
        }
        let keep_pending_append = self.tts.pending_append;
        let keep_pending_append_batch = self.tts.pending_append_batch.take();
        self.stop_playback();
        self.tts.pending_append = keep_pending_append;
        self.tts.pending_append_batch = keep_pending_append_batch;
        if let Some(engine) = &self.tts.engine {
            let file_paths: Vec<_> = files.iter().map(|(p, _)| p.clone()).collect();
            let start_paused = !self.tts.resume_after_prepare;
            if let Ok(playback) = engine.play_files(
                &file_paths,
                Duration::from_secs_f32(self.config.pause_after_sentence),
                self.config.tts_speed,
                self.config.tts_volume,
                start_paused,
            ) {
                let played = playback.sentence_durations().to_vec();
                self.tts.track = if played.len() == file_paths.len() {
                    file_paths.into_iter().zip(played.iter().copied()).collect()
                } else {
                    files.clone()
                };
                self.tts.playback = Some(playback);
                self.tts.sentence_offset =
                    start_idx.min(self.tts.audio_to_display.len().saturating_sub(1));
                let display_idx = self
                    .display_index_for_audio_sentence(self.tts.sentence_offset)
                    .unwrap_or_else(|| {
                        self.tts.sentence_offset.min(
                            self.sentence_count_for_page(self.reader.current_page)
                                .saturating_sub(1),
                        )
                    });
                self.tts.current_sentence_idx = Some(display_idx);
                self.tts.sources_per_sentence = if self.config.pause_after_sentence > f32::EPSILON {
                    2
                } else {
                    1
                };
                self.tts.total_sources = self.tts.track.len() * self.tts.sources_per_sentence;
                self.tts.elapsed = Duration::ZERO;
                if start_paused {
                    self.tts.started_at = None;
                    self.tts.lifecycle = TtsLifecycle::Paused;
                } else {
                    self.tts.started_at = Some(Instant::now());
                    self.tts.lifecycle = TtsLifecycle::Playing;
                }
                self.tts.resume_after_prepare = true;
                effects.push(Effect::AutoScrollToCurrent);
                if let Some(pending) = self.tts.pending_append_batch.take() {
                    if pending.request_id == request_id && pending.page == page {
                        effects.push(Effect::PrepareTtsAppend {
                            page: pending.page,
                            request_id: pending.request_id,
                            start_idx: pending.start_idx,
                            audio_sentences: pending.audio_sentences,
                        });
                    }
                }
                debug!(
                    offset = self.tts.sentence_offset,
                    "Started TTS playback and highlighting"
                );
            } else {
                warn!("Failed to start playback from prepared files");
                self.tts.lifecycle = TtsLifecycle::Idle;
                self.tts.pending_append = false;
                self.tts.pending_append_batch = None;
            }
        }
    }

    pub(super) fn handle_tts_append_prepared(
        &mut self,
        page: usize,
        start_idx: usize,
        request_id: u64,
        files: Vec<(std::path::PathBuf, Duration)>,
    ) {
        if request_id != self.tts.request_id {
            debug!(
                request_id,
                current = self.tts.request_id,
                "Ignoring stale append TTS request"
            );
            return;
        }
        if page != self.reader.current_page {
            debug!(
                page,
                current = self.reader.current_page,
                "Ignoring stale append TTS batch"
            );
            return;
        }
        self.tts.pending_append = false;
        self.tts.pending_append_batch = None;
        if files.is_empty() {
            warn!("Append TTS batch was empty");
            return;
        }
        let file_paths: Vec<_> = files.iter().map(|(p, _)| p.clone()).collect();
        let appended = if let Some(playback) = self.tts.playback.as_mut() {
            match playback.append_files(
                &file_paths,
                Duration::from_secs_f32(self.config.pause_after_sentence),
                self.config.tts_speed,
            ) {
                Ok(durations) => durations,
                Err(err) => {
                    warn!("Failed appending prepared TTS files: {err}");
                    return;
                }
            }
        } else {
            return;
        };
        if appended.len() == file_paths.len() {
            self.tts
                .track
                .extend(file_paths.into_iter().zip(appended.iter().copied()));
        } else {
            self.tts.track.extend(files);
        }
        self.tts.total_sources = self.tts.track.len() * self.tts.sources_per_sentence.max(1);
        info!(
            page = page + 1,
            start_idx,
            appended = self.tts.track.len(),
            "Appended prepared TTS files to active playback"
        );
    }

    pub(super) fn handle_tts_plan_ready(
        &mut self,
        page: usize,
        requested_display_idx: usize,
        request_id: u64,
        plan: crate::normalizer::PageNormalization,
        effects: &mut Vec<Effect>,
    ) {
        let actions = transitions::transition(
            self,
            transitions::TtsEvent::PlanReady {
                page,
                requested_display_idx,
                request_id,
                plan,
            },
        );
        effects::append_effects_from_actions(actions, effects);
    }

    pub(super) fn start_playback_from(
        &mut self,
        page: usize,
        sentence_idx: usize,
    ) -> Task<super::super::messages::Message> {
        let actions = transitions::transition(
            self,
            transitions::TtsEvent::StartRequested { page, sentence_idx },
        );
        effects::tasks_from_actions(self, actions)
    }

    fn begin_play_from_sentence(
        &mut self,
        idx: usize,
        effects: &mut Vec<Effect>,
        log_message: &str,
    ) {
        let sentence_count = self.sentence_count_for_page(self.reader.current_page);
        if sentence_count == 0 {
            return;
        }
        self.tts.set_current_sentence_clamped(idx, sentence_count);
        let clamped = self.tts.current_sentence_idx.unwrap_or(0);
        self.tts.sentence_offset = clamped;
        self.tts.resume_after_prepare = true;
        info!(idx = clamped, "{log_message}");
        effects.push(Effect::StartTts {
            page: self.reader.current_page,
            sentence_idx: clamped,
        });
        effects.push(Effect::AutoScrollToCurrent);
    }
}
