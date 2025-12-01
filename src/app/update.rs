use super::messages::Message;
use super::state::{
    App, MAX_LETTER_SPACING, MAX_MARGIN, MAX_TTS_SPEED, MAX_WORD_SPACING, MIN_TTS_SPEED,
    TEXT_SCROLL_ID, apply_component,
};
use crate::cache::save_last_page;
use crate::pagination::{MAX_FONT_SIZE, MAX_LINES_PER_PAGE, MIN_FONT_SIZE, MIN_LINES_PER_PAGE};
use crate::text_utils::split_sentences;
use iced::time;
use iced::{Subscription, Task};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

impl App {
    pub fn subscription(app: &App) -> Subscription<Message> {
        if app.tts_running {
            time::every(Duration::from_millis(50)).map(Message::Tick)
        } else {
            Subscription::none()
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let mut page_changed = false;
        let mut tasks: Vec<Task<Message>> = Vec::new();

        match message {
            Message::NextPage => {
                if self.current_page + 1 < self.pages.len() {
                    self.current_page += 1;
                    page_changed = true;
                    info!(page = self.current_page + 1, "Navigated to next page");
                    tasks.push(self.start_playback_from(self.current_page, 0));
                    self.queue_auto_scroll(&mut tasks);
                }
            }
            Message::PreviousPage => {
                if self.current_page > 0 {
                    self.current_page -= 1;
                    page_changed = true;
                    info!(page = self.current_page + 1, "Navigated to previous page");
                    tasks.push(self.start_playback_from(self.current_page, 0));
                    self.queue_auto_scroll(&mut tasks);
                }
            }
            Message::FontSizeChanged(size) => {
                let clamped = size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
                if clamped != self.font_size {
                    debug!(old = self.font_size, new = clamped, "Font size changed");
                    self.font_size = clamped;
                    self.repaginate();
                }
            }
            Message::ToggleTheme => {
                info!(night_mode = !self.night_mode, "Toggled theme");
                self.night_mode = !self.night_mode;
                self.save_epub_config();
            }
            Message::ToggleSettings => {
                debug!("Toggled settings panel");
                self.settings_open = !self.settings_open;
                self.save_epub_config();
            }
            Message::FontFamilyChanged(family) => {
                debug!(?family, "Font family changed");
                self.font_family = family;
                self.save_epub_config();
            }
            Message::FontWeightChanged(weight) => {
                debug!(?weight, "Font weight changed");
                self.font_weight = weight;
                self.save_epub_config();
            }
            Message::LineSpacingChanged(spacing) => {
                self.line_spacing = spacing.clamp(0.8, 2.5);
                debug!(line_spacing = self.line_spacing, "Line spacing changed");
                self.save_epub_config();
            }
            Message::MarginHorizontalChanged(margin) => {
                self.margin_horizontal = margin.min(MAX_MARGIN);
                debug!(
                    margin_horizontal = self.margin_horizontal,
                    "Horizontal margin changed"
                );
                self.save_epub_config();
            }
            Message::MarginVerticalChanged(margin) => {
                self.margin_vertical = margin.min(MAX_MARGIN);
                debug!(
                    margin_vertical = self.margin_vertical,
                    "Vertical margin changed"
                );
                self.save_epub_config();
            }
            Message::JustificationChanged(justification) => {
                debug!(?justification, "Text justification changed");
                self.justification = justification;
                self.save_epub_config();
            }
            Message::WordSpacingChanged(spacing) => {
                self.word_spacing = spacing.min(MAX_WORD_SPACING);
                debug!(word_spacing = self.word_spacing, "Word spacing changed");
                self.save_epub_config();
            }
            Message::LetterSpacingChanged(spacing) => {
                self.letter_spacing = spacing.min(MAX_LETTER_SPACING);
                debug!(
                    letter_spacing = self.letter_spacing,
                    "Letter spacing changed"
                );
                self.save_epub_config();
            }
            Message::LinesPerPageChanged(lines) => {
                let clamped =
                    lines.clamp(MIN_LINES_PER_PAGE as u32, MAX_LINES_PER_PAGE as u32) as usize;
                if clamped != self.lines_per_page {
                    let anchor = self
                        .pages
                        .get(self.current_page)
                        .and_then(|p| split_sentences(p.clone()).into_iter().next());
                    let before = self.current_page;
                    self.lines_per_page = clamped;
                    self.repaginate();
                    if let Some(sentence) = anchor {
                        if let Some(idx) =
                            self.pages.iter().position(|page| page.contains(&sentence))
                        {
                            self.current_page = idx;
                        }
                    }
                    if self.current_page != before {
                        page_changed = true;
                    }
                    debug!(
                        lines_per_page = self.lines_per_page,
                        "Lines per page changed"
                    );
                    self.save_epub_config();
                }
            }
            Message::DayHighlightChanged(component, value) => {
                self.day_highlight = apply_component(self.day_highlight, component, value);
                debug!(?component, value, "Day highlight updated");
                self.save_epub_config();
            }
            Message::PauseAfterSentenceChanged(pause) => {
                let clamped = pause.clamp(0.0, 2.0);
                if (clamped - self.pause_after_sentence).abs() > f32::EPSILON {
                    self.pause_after_sentence = clamped;
                    info!(pause_secs = clamped, "Updated pause after sentence");
                    self.save_epub_config();
                    if self.tts_playback.is_some() {
                        let idx = self.current_sentence_idx.unwrap_or(0);
                        tasks.push(self.start_playback_from(self.current_page, idx));
                        self.queue_auto_scroll(&mut tasks);
                    }
                }
            }
            Message::NightHighlightChanged(component, value) => {
                self.night_highlight = apply_component(self.night_highlight, component, value);
                debug!(?component, value, "Night highlight updated");
                self.save_epub_config();
            }
            Message::AutoScrollTtsChanged(enabled) => {
                if self.auto_scroll_tts != enabled {
                    self.auto_scroll_tts = enabled;
                    info!(enabled, "Updated auto-scroll to spoken sentence");
                    self.save_epub_config();
                    if enabled {
                        self.queue_auto_scroll(&mut tasks);
                    }
                }
            }
            Message::CenterSpokenSentenceChanged(centered) => {
                if self.center_spoken_sentence != centered {
                    self.center_spoken_sentence = centered;
                    info!(centered, "Updated centered tracking preference");
                    self.save_epub_config();
                    if self.auto_scroll_tts {
                        self.queue_auto_scroll(&mut tasks);
                    }
                }
            }
            Message::ToggleTtsControls => {
                debug!("Toggled TTS controls");
                self.tts_open = !self.tts_open;
                self.save_epub_config();
            }
            Message::SetTtsSpeed(speed) => {
                let clamped = speed.clamp(MIN_TTS_SPEED, MAX_TTS_SPEED);
                self.tts_speed = clamped;
                info!(speed = self.tts_speed, "Adjusted TTS speed");
                if self.tts_playback.is_some() {
                    let idx = self.current_sentence_idx.unwrap_or(0);
                    tasks.push(self.start_playback_from(self.current_page, idx));
                    self.queue_auto_scroll(&mut tasks);
                }
                self.save_epub_config();
            }
            Message::Play => {
                if let Some(playback) = &self.tts_playback {
                    info!("Resuming TTS playback");
                    playback.play();
                    self.tts_running = true;
                    self.tts_started_at = Some(Instant::now());
                } else {
                    info!("Starting TTS playback from current page");
                    tasks.push(self.start_playback_from(self.current_page, 0));
                    self.queue_auto_scroll(&mut tasks);
                }
            }
            Message::PlayFromPageStart => {
                info!("Playing page from start");
                tasks.push(self.start_playback_from(self.current_page, 0));
                self.queue_auto_scroll(&mut tasks);
            }
            Message::PlayFromCursor(idx) => {
                info!(idx, "Playing from cursor");
                tasks.push(self.start_playback_from(self.current_page, idx));
                self.queue_auto_scroll(&mut tasks);
            }
            Message::JumpToCurrentAudio => {
                if let Some(idx) = self.current_sentence_idx {
                    let total = self.last_sentences.len();
                    if let Some(offset) = self.scroll_offset_for_sentence(idx, total) {
                        info!(
                            idx,
                            fraction = offset.y,
                            "Jumping to current audio sentence (scroll only)"
                        );
                        tasks.push(iced::widget::scrollable::snap_to(
                            TEXT_SCROLL_ID.clone(),
                            offset,
                        ));
                    }
                }
            }
            Message::Pause => {
                if let Some(playback) = &self.tts_playback {
                    info!("Pausing TTS playback");
                    playback.pause();
                }
                self.tts_running = false;
                if let Some(started) = self.tts_started_at.take() {
                    self.tts_elapsed += Instant::now().saturating_duration_since(started);
                }
            }
            Message::SeekForward => {
                let next_idx = self.current_sentence_idx.unwrap_or(0) + 1;
                if next_idx < self.last_sentences.len() {
                    info!(next_idx, "Seeking forward within page");
                    tasks.push(self.start_playback_from(self.current_page, next_idx));
                    self.queue_auto_scroll(&mut tasks);
                } else if self.current_page + 1 < self.pages.len() {
                    self.current_page += 1;
                    info!("Seeking forward into next page");
                    tasks.push(self.start_playback_from(self.current_page, 0));
                    page_changed = true;
                    self.save_epub_config();
                    self.queue_auto_scroll(&mut tasks);
                }
            }
            Message::SeekBackward => {
                let current_idx = self.current_sentence_idx.unwrap_or(0);
                if current_idx > 0 {
                    info!(
                        previous_idx = current_idx.saturating_sub(1),
                        "Seeking backward within page"
                    );
                    tasks.push(self.start_playback_from(self.current_page, current_idx - 1));
                    self.queue_auto_scroll(&mut tasks);
                } else if self.current_page > 0 {
                    self.current_page -= 1;
                    let last_idx = split_sentences(
                        self.pages
                            .get(self.current_page)
                            .map(String::as_str)
                            .unwrap_or("")
                            .to_string(),
                    )
                    .len()
                    .saturating_sub(1);
                    info!("Seeking backward into previous page");
                    tasks.push(self.start_playback_from(self.current_page, last_idx));
                    page_changed = true;
                    self.save_epub_config();
                    self.queue_auto_scroll(&mut tasks);
                }
            }
            Message::Tick(now) => {
                if self.tts_running {
                    if self
                        .tts_playback
                        .as_ref()
                        .map(|p| p.is_paused())
                        .unwrap_or(false)
                    {
                        return Task::none();
                    }

                    let Some(started) = self.tts_started_at else {
                        return Task::none();
                    };
                    let elapsed = self.tts_elapsed + now.saturating_duration_since(started);

                    let mut acc = Duration::ZERO;
                    let mut target_idx = None;
                    let offset = self.tts_sentence_offset;
                    let pause = Duration::from_secs_f32(self.pause_after_sentence);
                    for (i, (_, dur)) in self.tts_track.iter().enumerate() {
                        acc += *dur + pause;
                        if elapsed <= acc {
                            target_idx = Some(offset + i);
                            break;
                        }
                    }

                    if let Some(idx) = target_idx {
                        let clamped = idx.min(self.last_sentences.len().saturating_sub(1));
                        if Some(clamped) != self.current_sentence_idx {
                            self.current_sentence_idx = Some(clamped);
                            self.queue_auto_scroll(&mut tasks);
                        }
                    } else {
                        self.stop_playback();
                        if self.current_page + 1 < self.pages.len() {
                            self.current_page += 1;
                            info!("Playback finished page, advancing");
                            tasks.push(self.start_playback_from(self.current_page, 0));
                            self.queue_auto_scroll(&mut tasks);
                        } else {
                            info!("Playback finished at end of book");
                        }
                    }
                }
            }
            Message::TtsPrepared {
                page,
                start_idx,
                request_id,
                files,
            } => {
                if request_id != self.tts_request_id {
                    debug!(
                        request_id,
                        current = self.tts_request_id,
                        "Ignoring stale TTS request"
                    );
                    return Task::none();
                }
                info!(
                    page,
                    start_idx,
                    file_count = files.len(),
                    "Received prepared TTS batch"
                );
                if page != self.current_page {
                    debug!(
                        page,
                        current = self.current_page,
                        "Ignoring stale TTS batch"
                    );
                    return Task::none();
                }
                if files.is_empty() {
                    warn!("TTS batch was empty; stopping playback");
                    self.stop_playback();
                    self.current_sentence_idx = None;
                    return Task::none();
                }
                self.stop_playback();
                if let Some(engine) = &self.tts_engine {
                    if let Ok(playback) = engine.play_files(
                        &files.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>(),
                        Duration::from_secs_f32(self.pause_after_sentence),
                    ) {
                        self.tts_playback = Some(playback);
                        self.tts_track = files.clone();
                        self.tts_sentence_offset =
                            start_idx.min(self.last_sentences.len().saturating_sub(1));
                        self.current_sentence_idx = Some(self.tts_sentence_offset);
                        self.tts_elapsed = Duration::ZERO;
                        self.tts_started_at = Some(Instant::now());
                        self.tts_running = true;
                        self.queue_auto_scroll(&mut tasks);
                        debug!(
                            offset = self.tts_sentence_offset,
                            "Started TTS playback and highlighting"
                        );
                    } else {
                        warn!("Failed to start playback from prepared files");
                    }
                }
            }
        }

        if page_changed {
            save_last_page(&self.epub_path, self.current_page);
        }

        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    pub(super) fn start_playback_from(
        &mut self,
        page: usize,
        sentence_idx: usize,
    ) -> Task<Message> {
        let Some(engine) = self.tts_engine.clone() else {
            return Task::none();
        };

        self.stop_playback();
        self.tts_track.clear();
        self.tts_elapsed = Duration::ZERO;
        self.tts_started_at = None;

        let sentences = split_sentences(
            self.pages
                .get(page)
                .map(String::as_str)
                .unwrap_or("")
                .to_string(),
        );
        self.last_sentences = sentences.clone();
        if sentences.is_empty() {
            self.current_sentence_idx = None;
            self.tts_sentence_offset = 0;
            return Task::none();
        }

        let sentence_idx = sentence_idx.min(sentences.len().saturating_sub(1));
        self.tts_sentence_offset = sentence_idx;
        self.current_sentence_idx = Some(sentence_idx);

        let cache_root = crate::cache::tts_dir(&self.epub_path);
        let speed = self.tts_speed;
        let threads = self.tts_threads.max(1);
        let page_id = page;
        self.tts_started_at = None;
        self.tts_elapsed = Duration::ZERO;
        self.tts_request_id = self.tts_request_id.wrapping_add(1);
        let request_id = self.tts_request_id;
        self.save_epub_config();
        info!(
            page = page + 1,
            sentence_idx, speed, threads, "Preparing playback task"
        );

        Task::perform(
            async move {
                engine
                    .prepare_batch(cache_root, sentences, sentence_idx, speed, threads)
                    .map(|files| Message::TtsPrepared {
                        page: page_id,
                        start_idx: sentence_idx,
                        request_id,
                        files,
                    })
                    .unwrap_or_else(|_| Message::TtsPrepared {
                        page: page_id,
                        start_idx: sentence_idx,
                        request_id,
                        files: Vec::new(),
                    })
            },
            |msg| msg,
        )
    }

    fn scroll_offset_for_sentence(
        &self,
        sentence_idx: usize,
        total_sentences: usize,
    ) -> Option<iced::widget::scrollable::RelativeOffset> {
        if total_sentences == 0 {
            return None;
        }

        let base = if total_sentences > 1 {
            sentence_idx.min(total_sentences.saturating_sub(1)) as f32
                / total_sentences.saturating_sub(1) as f32
        } else {
            0.0
        };

        let y = if self.center_spoken_sentence {
            let half_step = 0.5 / total_sentences as f32;
            (base - half_step).clamp(0.0, 1.0)
        } else {
            base
        };

        Some(iced::widget::scrollable::RelativeOffset { x: 0.0, y })
    }

    fn queue_auto_scroll(&self, tasks: &mut Vec<Task<Message>>) {
        if !self.auto_scroll_tts {
            return;
        }

        let Some(idx) = self.current_sentence_idx else {
            return;
        };

        if let Some(offset) = self.scroll_offset_for_sentence(idx, self.last_sentences.len()) {
            tasks.push(iced::widget::scrollable::snap_to(
                TEXT_SCROLL_ID.clone(),
                offset,
            ));
        }
    }
}
