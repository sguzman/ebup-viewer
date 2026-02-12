use super::super::state::{App, MAX_TTS_SPEED, MAX_TTS_VOLUME, MIN_TTS_SPEED, MIN_TTS_VOLUME};
use super::Effect;
use iced::Task;
use iced::widget::scrollable::RelativeOffset;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

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
        if self.tts.preparing {
            info!(
                page = self.tts.preparing_page.unwrap_or(self.reader.current_page) + 1,
                sentence_idx = self.tts.preparing_sentence_idx.unwrap_or(0),
                "TTS batch preparation already in progress; ignoring duplicate play request"
            );
            return;
        }
        if let Some(playback) = &self.tts.playback {
            info!("Resuming TTS playback");
            playback.play();
            self.tts.running = true;
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
        if self.tts.preparing || currently_playing {
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
        if self.tts.preparing {
            self.tts.request_id = self.tts.request_id.wrapping_add(1);
            self.tts.preparing = false;
            self.tts.prepare_dispatched = false;
            self.tts.preparing_page = None;
            self.tts.preparing_sentence_idx = None;
            self.tts.quick_start_display_idx = None;
            self.tts.pending_append = false;
            self.tts.pending_append_batch = None;
            info!("Cancelled pending TTS batch preparation");
        }
        if let Some(playback) = &self.tts.playback {
            info!("Pausing TTS playback");
            playback.pause();
        }
        self.tts.running = false;
        if let Some(started) = self.tts.started_at.take() {
            self.tts.elapsed += Instant::now().saturating_duration_since(started);
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
        if !self.tts.running {
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
        self.tts.preparing = false;
        self.tts.preparing_page = None;
        self.tts.preparing_sentence_idx = None;
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
            self.tts.prepare_dispatched = false;
            self.tts.quick_start_display_idx = None;
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
                    self.tts.running = false;
                } else {
                    self.tts.started_at = Some(Instant::now());
                    self.tts.running = true;
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
                self.tts.prepare_dispatched = false;
                self.tts.quick_start_display_idx = None;
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
        if request_id != self.tts.request_id {
            debug!(
                request_id,
                current = self.tts.request_id,
                "Ignoring stale TTS plan request"
            );
            return;
        }
        if page != self.reader.current_page {
            debug!(
                page,
                current = self.reader.current_page,
                "Ignoring stale TTS plan for different page"
            );
            return;
        }

        self.tts.display_to_audio = plan.display_to_audio;
        self.tts.audio_to_display = plan.audio_to_display;
        let Some(audio_start_idx) =
            self.find_audio_start_for_display_sentence(requested_display_idx)
        else {
            warn!(
                page = page + 1,
                display_idx = requested_display_idx,
                "No speakable text on page after normalization"
            );
            self.tts.preparing = false;
            self.tts.prepare_dispatched = false;
            self.tts.preparing_page = None;
            self.tts.preparing_sentence_idx = None;
            self.tts.quick_start_display_idx = None;
            self.tts.pending_append = false;
            self.tts.pending_append_batch = None;
            self.tts.current_sentence_idx = Some(requested_display_idx);
            self.tts.sentence_offset = 0;
            return;
        };
        let mut full_audio_sentences = plan.audio_sentences;
        if !self.tts.prepare_dispatched {
            let display_start_idx = self
                .display_index_for_audio_sentence(audio_start_idx)
                .unwrap_or(requested_display_idx);
            self.tts.sentence_offset = audio_start_idx;
            self.tts.current_sentence_idx = Some(display_start_idx);
            self.tts.prepare_dispatched = true;
            effects.push(Effect::PrepareTtsBatches {
                page,
                request_id,
                audio_start_idx,
                audio_sentences: full_audio_sentences,
            });
            return;
        }

        let quick_display_idx = self
            .tts
            .quick_start_display_idx
            .unwrap_or(requested_display_idx);
        let quick_audio_idx = self
            .find_audio_start_for_display_sentence(quick_display_idx)
            .unwrap_or(audio_start_idx);
        let display_start_idx = self
            .display_index_for_audio_sentence(quick_audio_idx)
            .unwrap_or(quick_display_idx);
        self.tts.sentence_offset = quick_audio_idx;
        self.tts.current_sentence_idx = Some(display_start_idx);

        let append_start = quick_audio_idx.saturating_add(1);
        if append_start >= full_audio_sentences.len() {
            self.tts.pending_append = false;
            self.tts.pending_append_batch = None;
            return;
        }

        let append_sentences = full_audio_sentences.split_off(append_start);
        self.tts.pending_append = true;

        if self.tts.playback.is_some() && !self.tts.preparing {
            effects.push(Effect::PrepareTtsAppend {
                page,
                request_id,
                start_idx: append_start,
                audio_sentences: append_sentences,
            });
        } else {
            self.tts.pending_append_batch = Some(super::super::state::PendingAppendBatch {
                page,
                request_id,
                start_idx: append_start,
                audio_sentences: append_sentences,
            });
        }
    }

    pub(super) fn handle_tts_initial_ready(
        &mut self,
        page: usize,
        requested_display_idx: usize,
        request_id: u64,
        sentence_count: usize,
        start_display_idx: Option<usize>,
        start_audio_idx: Option<usize>,
        audio_sentence: Option<String>,
        effects: &mut Vec<Effect>,
    ) {
        if request_id != self.tts.request_id {
            debug!(
                request_id,
                current = self.tts.request_id,
                "Ignoring stale initial TTS request"
            );
            return;
        }
        if page != self.reader.current_page {
            debug!(
                page,
                current = self.reader.current_page,
                "Ignoring stale initial TTS result for different page"
            );
            return;
        }
        if self.tts.prepare_dispatched {
            return;
        }

        let Some(audio_sentence) = audio_sentence else {
            self.tts.quick_start_display_idx = None;
            return;
        };
        let display_idx = start_display_idx.unwrap_or(requested_display_idx);
        let audio_idx = start_audio_idx.unwrap_or(0);
        self.tts.quick_start_display_idx = Some(display_idx);
        self.tts.current_sentence_idx = Some(display_idx);
        self.tts.sentence_offset = audio_idx;
        self.tts.display_to_audio = vec![None; sentence_count];
        if display_idx < self.tts.display_to_audio.len() {
            self.tts.display_to_audio[display_idx] = Some(audio_idx);
        }
        self.tts.audio_to_display = vec![display_idx; audio_idx.saturating_add(1)];
        self.tts.pending_append = true;
        self.tts.prepare_dispatched = true;
        effects.push(Effect::PrepareTtsBatches {
            page,
            request_id,
            audio_start_idx: audio_idx,
            audio_sentences: vec![audio_sentence],
        });
    }

    pub(super) fn start_playback_from(
        &mut self,
        page: usize,
        sentence_idx: usize,
    ) -> Task<super::super::messages::Message> {
        if self.tts.engine.is_none() {
            return Task::none();
        }

        self.stop_playback();
        self.tts.track.clear();
        self.tts.elapsed = Duration::ZERO;
        self.tts.started_at = None;

        let display_sentences = self.raw_sentences_for_page(page);
        if display_sentences.is_empty() {
            self.tts.preparing = false;
            self.tts.prepare_dispatched = false;
            self.tts.preparing_page = None;
            self.tts.preparing_sentence_idx = None;
            self.tts.quick_start_display_idx = None;
            self.tts.pending_append = false;
            self.tts.pending_append_batch = None;
            self.tts.current_sentence_idx = None;
            self.tts.sentence_offset = 0;
            self.tts.display_to_audio.clear();
            self.tts.audio_to_display.clear();
            return Task::none();
        }

        let requested_display_idx = sentence_idx.min(display_sentences.len().saturating_sub(1));
        if self.tts.preparing
            && self.tts.preparing_page == Some(page)
            && self.tts.preparing_sentence_idx == Some(requested_display_idx)
        {
            info!(
                page = page + 1,
                sentence_idx = requested_display_idx,
                "Skipping duplicate TTS start request while preparation is in progress"
            );
            return Task::none();
        }
        self.tts.current_sentence_idx = Some(requested_display_idx);
        self.tts.sentence_offset = requested_display_idx;

        self.tts.started_at = None;
        self.tts.elapsed = Duration::ZERO;
        self.tts.prepare_dispatched = false;
        self.tts.quick_start_display_idx = None;
        self.tts.pending_append = false;
        self.tts.pending_append_batch = None;
        self.tts.preparing = true;
        self.tts.preparing_page = Some(page);
        self.tts.preparing_sentence_idx = Some(requested_display_idx);
        self.tts.request_id = self.tts.request_id.wrapping_add(1);
        let request_id = self.tts.request_id;
        info!(
            page = page + 1,
            sentence_idx = requested_display_idx,
            request_id,
            "Scheduling async TTS planning tasks"
        );
        let normalizer = self.normalizer.clone();
        let initial_normalizer = normalizer.clone();
        let epub_path = self.epub_path.clone();
        let initial_epub_path = epub_path.clone();
        let initial_sentences = display_sentences.clone();
        let initial_task = Task::perform(
            async move {
                let initial = initial_normalizer.first_speakable_sentence_cached(
                    &initial_epub_path,
                    &initial_sentences,
                    requested_display_idx,
                );
                super::super::messages::Message::TtsInitialReady {
                    page,
                    requested_display_idx,
                    request_id,
                    sentence_count: initial_sentences.len(),
                    start_display_idx: initial.as_ref().map(|(display_idx, _, _)| *display_idx),
                    start_audio_idx: initial.as_ref().map(|(_, audio_idx, _)| *audio_idx),
                    audio_sentence: initial.map(|(_, _, sentence)| sentence),
                }
            },
            |msg| msg,
        );
        let full_task = Task::perform(
            async move {
                let plan = normalizer.plan_page_cached(&epub_path, page, &display_sentences);
                super::super::messages::Message::TtsPlanReady {
                    page,
                    requested_display_idx,
                    request_id,
                    plan,
                }
            },
            |msg| msg,
        );
        Task::batch(vec![initial_task, full_task])
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
        let clamped = idx.min(sentence_count.saturating_sub(1));
        self.tts.current_sentence_idx = Some(clamped);
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
