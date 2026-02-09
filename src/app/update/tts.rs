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
        let clamped = pause.clamp(0.0, 2.0);
        if (clamped - self.config.pause_after_sentence).abs() > f32::EPSILON {
            self.config.pause_after_sentence = clamped;
            info!(pause_secs = clamped, "Updated pause after sentence");
            effects.push(Effect::SaveConfig);
            if self.tts.playback.is_some() {
                let idx = self.tts.current_sentence_idx.unwrap_or(0);
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
        if self.tts.playback.is_some() {
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
        if let Some(playback) = &self.tts.playback {
            info!("Resuming TTS playback");
            playback.play();
            self.tts.running = true;
            self.tts.started_at = Some(Instant::now());
        } else {
            let start_idx = self.tts.current_sentence_idx.unwrap_or(0);
            info!(start_idx, "Starting TTS playback from cursor");
            effects.push(Effect::StartTts {
                page: self.reader.current_page,
                sentence_idx: start_idx,
            });
            effects.push(Effect::AutoScrollToCurrent);
            effects.push(Effect::SaveBookmark);
        }
    }

    pub(super) fn handle_play_from_page_start(&mut self, effects: &mut Vec<Effect>) {
        info!("Playing page from start");
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

    pub(super) fn handle_pause(&mut self, _effects: &mut Vec<Effect>) {
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
        if next_idx < self.tts.last_sentences.len() {
            info!(next_idx, "Seeking forward within page");
            effects.push(Effect::StartTts {
                page: self.reader.current_page,
                sentence_idx: next_idx,
            });
            effects.push(Effect::AutoScrollToCurrent);
            effects.push(Effect::SaveBookmark);
        } else if self.reader.current_page + 1 < self.reader.pages.len() {
            self.reader.current_page += 1;
            info!("Seeking forward into next page");
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
                    clamped_audio.min(self.tts.last_sentences.len().saturating_sub(1))
                });
            if Some(display_idx) != self.tts.current_sentence_idx {
                self.tts.current_sentence_idx = Some(display_idx);
                effects.push(Effect::AutoScrollToCurrent);
                effects.push(Effect::SaveBookmark);
            }
        } else {
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
        info!(
            page,
            start_idx,
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
        self.stop_playback();
        if let Some(engine) = &self.tts.engine {
            let file_paths: Vec<_> = files.iter().map(|(p, _)| p.clone()).collect();
            if let Ok(playback) = engine.play_files(
                &file_paths,
                Duration::from_secs_f32(self.config.pause_after_sentence),
                self.config.tts_speed,
            ) {
                playback.set_volume(self.config.tts_volume);
                let played = playback.sentence_durations();
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
                        self.tts
                            .sentence_offset
                            .min(self.tts.last_sentences.len().saturating_sub(1))
                    });
                self.tts.current_sentence_idx = Some(display_idx);
                self.tts.sources_per_sentence = if self.config.pause_after_sentence > f32::EPSILON {
                    2
                } else {
                    1
                };
                self.tts.total_sources = self.tts.track.len() * self.tts.sources_per_sentence;
                self.tts.elapsed = Duration::ZERO;
                self.tts.started_at = Some(Instant::now());
                self.tts.running = true;
                effects.push(Effect::AutoScrollToCurrent);
                debug!(
                    offset = self.tts.sentence_offset,
                    "Started TTS playback and highlighting"
                );
            } else {
                warn!("Failed to start playback from prepared files");
            }
        }
    }

    pub(super) fn start_playback_from(
        &mut self,
        page: usize,
        sentence_idx: usize,
    ) -> Task<super::super::messages::Message> {
        let Some(engine) = self.tts.engine.clone() else {
            return Task::none();
        };

        self.stop_playback();
        self.tts.track.clear();
        self.tts.elapsed = Duration::ZERO;
        self.tts.started_at = None;

        let display_sentences = self.raw_sentences_for_page(page);
        self.tts.last_sentences = display_sentences.clone();
        if display_sentences.is_empty() {
            self.tts.current_sentence_idx = None;
            self.tts.sentence_offset = 0;
            self.tts.display_to_audio.clear();
            self.tts.audio_to_display.clear();
            return Task::none();
        }

        let plan = self
            .normalizer
            .plan_page_cached(&self.epub_path, page, &display_sentences);
        self.tts.display_to_audio = plan.display_to_audio;
        self.tts.audio_to_display = plan.audio_to_display;

        let requested_display_idx = sentence_idx.min(display_sentences.len().saturating_sub(1));
        let Some(audio_start_idx) =
            self.find_audio_start_for_display_sentence(requested_display_idx)
        else {
            warn!(
                page = page + 1,
                display_idx = requested_display_idx,
                "No speakable text on page after normalization"
            );
            self.tts.current_sentence_idx = Some(requested_display_idx);
            self.tts.sentence_offset = 0;
            return Task::none();
        };
        let display_start_idx = self
            .display_index_for_audio_sentence(audio_start_idx)
            .unwrap_or(requested_display_idx);
        self.tts.sentence_offset = audio_start_idx;
        self.tts.current_sentence_idx = Some(display_start_idx);

        let cache_root = crate::cache::tts_dir(&self.epub_path);
        let speed = self.config.tts_speed;
        let threads = self.config.tts_threads.max(1);
        let progress_log_interval_secs = self.config.tts_progress_log_interval_secs;
        let page_id = page;
        self.tts.started_at = None;
        self.tts.elapsed = Duration::ZERO;
        self.tts.request_id = self.tts.request_id.wrapping_add(1);
        let request_id = self.tts.request_id;
        self.save_epub_config();
        info!(
            page = page + 1,
            sentence_idx = requested_display_idx,
            audio_start_idx,
            speed,
            threads,
            progress_log_interval_secs,
            "Preparing playback task"
        );

        let audio_sentences = plan.audio_sentences;
        Task::perform(
            async move {
                engine
                    .prepare_batch(
                        cache_root,
                        audio_sentences,
                        audio_start_idx,
                        threads,
                        Duration::from_secs_f32(progress_log_interval_secs),
                    )
                    .map(|files| super::super::messages::Message::TtsPrepared {
                        page: page_id,
                        start_idx: audio_start_idx,
                        request_id,
                        files,
                    })
                    .unwrap_or_else(|_| super::super::messages::Message::TtsPrepared {
                        page: page_id,
                        start_idx: audio_start_idx,
                        request_id,
                        files: Vec::new(),
                    })
            },
            |msg| msg,
        )
    }

    fn begin_play_from_sentence(
        &mut self,
        idx: usize,
        effects: &mut Vec<Effect>,
        log_message: &str,
    ) {
        let sentences = self.raw_sentences_for_page(self.reader.current_page);
        if sentences.is_empty() {
            return;
        }
        let clamped = idx.min(sentences.len().saturating_sub(1));
        self.tts.last_sentences = sentences;
        self.tts.current_sentence_idx = Some(clamped);
        self.tts.sentence_offset = clamped;
        info!(idx = clamped, "{log_message}");
        effects.push(Effect::StartTts {
            page: self.reader.current_page,
            sentence_idx: clamped,
        });
        effects.push(Effect::AutoScrollToCurrent);
        effects.push(Effect::SaveBookmark);
    }
}
