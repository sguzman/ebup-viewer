use super::messages::Message;
use super::state::{
    App, MAX_LETTER_SPACING, MAX_MARGIN, MAX_TTS_SPEED, MAX_WORD_SPACING, MIN_TTS_SPEED,
    TEXT_SCROLL_ID, apply_component,
};
use crate::cache::{Bookmark, save_bookmark};
use crate::pagination::{MAX_FONT_SIZE, MAX_LINES_PER_PAGE, MIN_FONT_SIZE, MIN_LINES_PER_PAGE};
use crate::text_utils::split_sentences;
use iced::time;
use iced::widget::scrollable::RelativeOffset;
use iced::{Subscription, Task};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Describes work that must be performed outside the pure reducer.
pub enum Effect {
    SaveConfig,
    SaveBookmark,
    StartTts { page: usize, sentence_idx: usize },
    StopTts,
    ScrollTo(RelativeOffset),
    AutoScrollToCurrent,
}

impl App {
    pub fn subscription(app: &App) -> Subscription<Message> {
        if app.tts.running {
            time::every(Duration::from_millis(50)).map(Message::Tick)
        } else {
            Subscription::none()
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let effects = self.reduce(message);
        if effects.is_empty() {
            Task::none()
        } else {
            Task::batch(effects.into_iter().map(|effect| self.run_effect(effect)))
        }
    }

    fn reduce(&mut self, message: Message) -> Vec<Effect> {
        let mut effects = Vec::new();

        match message {
            Message::NextPage => {
                effects.extend(self.go_to_page(self.reader.current_page + 1));
            }
            Message::PreviousPage => {
                if self.reader.current_page > 0 {
                    effects.extend(self.go_to_page(self.reader.current_page - 1));
                }
            }
            Message::FontSizeChanged(size) => {
                let clamped = size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
                if clamped != self.config.font_size {
                    debug!(old = self.config.font_size, new = clamped, "Font size changed");
                    self.config.font_size = clamped;
                    self.repaginate();
                }
            }
            Message::ToggleTheme => {
                let next = match self.config.theme {
                    crate::config::ThemeMode::Night => crate::config::ThemeMode::Day,
                    crate::config::ThemeMode::Day => crate::config::ThemeMode::Night,
                };
                info!(night_mode = matches!(next, crate::config::ThemeMode::Night), "Toggled theme");
                self.config.theme = next;
                effects.push(Effect::SaveConfig);
            }
            Message::ToggleSettings => {
                debug!("Toggled settings panel");
                self.config.show_settings = !self.config.show_settings;
                effects.push(Effect::SaveConfig);
            }
            Message::FontFamilyChanged(family) => {
                debug!(?family, "Font family changed");
                self.config.font_family = family;
                effects.push(Effect::SaveConfig);
            }
            Message::FontWeightChanged(weight) => {
                debug!(?weight, "Font weight changed");
                self.config.font_weight = weight;
                effects.push(Effect::SaveConfig);
            }
            Message::LineSpacingChanged(spacing) => {
                self.config.line_spacing = spacing.clamp(0.8, 2.5);
                debug!(line_spacing = self.config.line_spacing, "Line spacing changed");
                effects.push(Effect::SaveConfig);
            }
            Message::MarginHorizontalChanged(margin) => {
                self.config.margin_horizontal = margin.min(MAX_MARGIN);
                debug!(
                    margin_horizontal = self.config.margin_horizontal,
                    "Horizontal margin changed"
                );
                effects.push(Effect::SaveConfig);
            }
            Message::MarginVerticalChanged(margin) => {
                self.config.margin_vertical = margin.min(MAX_MARGIN);
                debug!(
                    margin_vertical = self.config.margin_vertical,
                    "Vertical margin changed"
                );
                effects.push(Effect::SaveConfig);
            }
            Message::WordSpacingChanged(spacing) => {
                self.config.word_spacing = spacing.min(MAX_WORD_SPACING);
                debug!(word_spacing = self.config.word_spacing, "Word spacing changed");
                effects.push(Effect::SaveConfig);
            }
            Message::LetterSpacingChanged(spacing) => {
                self.config.letter_spacing = spacing.min(MAX_LETTER_SPACING);
                debug!(
                    letter_spacing = self.config.letter_spacing,
                    "Letter spacing changed"
                );
                effects.push(Effect::SaveConfig);
            }
            Message::LinesPerPageChanged(lines) => {
                let clamped =
                    lines.clamp(MIN_LINES_PER_PAGE as u32, MAX_LINES_PER_PAGE as u32) as usize;
                if clamped != self.config.lines_per_page {
                    let anchor = self
                        .reader
                        .pages
                        .get(self.reader.current_page)
                        .and_then(|p| split_sentences(p.clone()).into_iter().next());
                    let before = self.reader.current_page;
                    self.config.lines_per_page = clamped;
                    self.repaginate();
                    if let Some(sentence) = anchor {
                        if let Some(idx) =
                            self.reader.pages.iter().position(|page| page.contains(&sentence))
                        {
                            self.reader.current_page = idx;
                        }
                    }
                    if self.reader.current_page != before {
                        self.bookmark.last_scroll_offset = RelativeOffset::START;
                        effects.push(Effect::SaveBookmark);
                    }
                    debug!(
                        lines_per_page = self.config.lines_per_page,
                        "Lines per page changed"
                    );
                    effects.push(Effect::SaveConfig);
                }
            }
            Message::DayHighlightChanged(component, value) => {
                self.config.day_highlight = apply_component(self.config.day_highlight, component, value);
                debug!(?component, value, "Day highlight updated");
                effects.push(Effect::SaveConfig);
            }
            Message::PauseAfterSentenceChanged(pause) => {
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
            Message::NightHighlightChanged(component, value) => {
                self.config.night_highlight = apply_component(self.config.night_highlight, component, value);
                debug!(?component, value, "Night highlight updated");
                effects.push(Effect::SaveConfig);
            }
            Message::AutoScrollTtsChanged(enabled) => {
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
            Message::CenterSpokenSentenceChanged(centered) => {
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
            Message::ToggleTtsControls => {
                debug!("Toggled TTS controls");
                self.config.show_tts = !self.config.show_tts;
                effects.push(Effect::SaveConfig);
            }
            Message::SetTtsSpeed(speed) => {
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
            Message::Play => {
                if let Some(playback) = &self.tts.playback {
                    info!("Resuming TTS playback");
                    playback.play();
                    self.tts.running = true;
                    self.tts.started_at = Some(Instant::now());
                } else {
                    info!("Starting TTS playback from current page");
                    effects.push(Effect::StartTts {
                        page: self.reader.current_page,
                        sentence_idx: 0,
                    });
                    effects.push(Effect::AutoScrollToCurrent);
                    effects.push(Effect::SaveBookmark);
                }
            }
            Message::PlayFromPageStart => {
                info!("Playing page from start");
                effects.push(Effect::StartTts {
                    page: self.reader.current_page,
                    sentence_idx: 0,
                });
                effects.push(Effect::AutoScrollToCurrent);
                effects.push(Effect::SaveBookmark);
            }
            Message::PlayFromCursor(idx) => {
                info!(idx, "Playing from cursor");
                effects.push(Effect::StartTts {
                    page: self.reader.current_page,
                    sentence_idx: idx,
                });
                effects.push(Effect::AutoScrollToCurrent);
                effects.push(Effect::SaveBookmark);
            }
            Message::JumpToCurrentAudio => {
                if let Some(idx) = self.tts.current_sentence_idx {
                    let total = self.tts.last_sentences.len();
                    if let Some(offset) = self.scroll_offset_for_sentence(idx, total) {
                        info!(
                            idx,
                            fraction = offset.y,
                            "Jumping to current audio sentence (scroll only)"
                        );
                        effects.push(Effect::ScrollTo(offset));
                        effects.push(Effect::SaveBookmark);
                    }
                }
            }
            Message::Pause => {
                if let Some(playback) = &self.tts.playback {
                    info!("Pausing TTS playback");
                    playback.pause();
                }
                self.tts.running = false;
                if let Some(started) = self.tts.started_at.take() {
                    self.tts.elapsed += Instant::now().saturating_duration_since(started);
                }
            }
            Message::SeekForward => {
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
            Message::SeekBackward => {
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
                    let last_idx = split_sentences(
                        self.reader
                            .pages
                            .get(self.reader.current_page)
                            .map(String::as_str)
                            .unwrap_or("")
                            .to_string(),
                    )
                    .len()
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
            Message::Scrolled(offset) => {
                let sanitized = Self::sanitize_offset(offset);
                if sanitized != self.bookmark.last_scroll_offset {
                    self.bookmark.last_scroll_offset = sanitized;
                    effects.push(Effect::SaveBookmark);
                }
            }
            Message::Tick(now) => {
                if self.tts.running {
                    if self
                        .tts
                        .playback
                        .as_ref()
                        .map(|p| p.is_paused())
                        .unwrap_or(false)
                    {
                        return Vec::new();
                    }

                    let Some(started) = self.tts.started_at else {
                        return Vec::new();
                    };
                    let elapsed = self.tts.elapsed + now.saturating_duration_since(started);

                    let mut acc = Duration::ZERO;
                    let mut target_idx = None;
                    let offset = self.tts.sentence_offset;
                    let pause = Duration::from_secs_f32(self.config.pause_after_sentence);
                    for (i, (_, dur)) in self.tts.track.iter().enumerate() {
                        acc += *dur + pause;
                        if elapsed <= acc {
                            target_idx = Some(offset + i);
                            break;
                        }
                    }

                    if let Some(idx) = target_idx {
                        let clamped = idx.min(self.tts.last_sentences.len().saturating_sub(1));
                        if Some(clamped) != self.tts.current_sentence_idx {
                            self.tts.current_sentence_idx = Some(clamped);
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
            }
            Message::TtsPrepared {
                page,
                start_idx,
                request_id,
                files,
            } => {
                if request_id != self.tts.request_id {
                    debug!(
                        request_id,
                        current = self.tts.request_id,
                        "Ignoring stale TTS request"
                    );
                    return Vec::new();
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
                    return Vec::new();
                }
                if files.is_empty() {
                    warn!("TTS batch was empty; stopping playback");
                    self.stop_playback();
                    self.tts.current_sentence_idx = None;
                    return Vec::new();
                }
                self.stop_playback();
                if let Some(engine) = &self.tts.engine {
                    if let Ok(playback) = engine.play_files(
                        &files.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>(),
                        Duration::from_secs_f32(self.config.pause_after_sentence),
                    ) {
                        self.tts.playback = Some(playback);
                        self.tts.track = files.clone();
                        self.tts.sentence_offset =
                            start_idx.min(self.tts.last_sentences.len().saturating_sub(1));
                        self.tts.current_sentence_idx = Some(self.tts.sentence_offset);
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
        }

        effects
    }

    fn run_effect(&mut self, effect: Effect) -> Task<Message> {
        match effect {
            Effect::SaveConfig => {
                self.save_epub_config();
                Task::none()
            }
            Effect::SaveBookmark => {
                self.persist_bookmark();
                Task::none()
            }
            Effect::StartTts { page, sentence_idx } => self.start_playback_from(page, sentence_idx),
            Effect::StopTts => {
                self.stop_playback();
                Task::none()
            }
            Effect::ScrollTo(offset) => {
                self.bookmark.last_scroll_offset = offset;
                iced::widget::scrollable::snap_to(TEXT_SCROLL_ID.clone(), offset)
            }
            Effect::AutoScrollToCurrent => {
                if !self.config.auto_scroll_tts {
                    return Task::none();
                }
                if let Some(idx) = self.tts.current_sentence_idx {
                    if let Some(offset) =
                        self.scroll_offset_for_sentence(idx, self.tts.last_sentences.len())
                    {
                        self.bookmark.last_scroll_offset = offset;
                        return iced::widget::scrollable::snap_to(
                            TEXT_SCROLL_ID.clone(),
                            offset,
                        );
                    }
                }
                Task::none()
            }
        }
    }

    fn go_to_page(&mut self, new_page: usize) -> Vec<Effect> {
        let mut effects = Vec::new();
        if new_page < self.reader.pages.len() {
            self.reader.current_page = new_page;
            self.bookmark.last_scroll_offset = RelativeOffset::START;
            info!(page = self.reader.current_page + 1, "Navigated to page");
            effects.push(Effect::StartTts {
                page: self.reader.current_page,
                sentence_idx: 0,
            });
            effects.push(Effect::AutoScrollToCurrent);
            effects.push(Effect::SaveBookmark);
        }
        effects
    }

    pub(super) fn start_playback_from(
        &mut self,
        page: usize,
        sentence_idx: usize,
    ) -> Task<Message> {
        let Some(engine) = self.tts.engine.clone() else {
            return Task::none();
        };

        self.stop_playback();
        self.tts.track.clear();
        self.tts.elapsed = Duration::ZERO;
        self.tts.started_at = None;

        let sentences = split_sentences(
            self.reader
                .pages
                .get(page)
                .map(String::as_str)
                .unwrap_or("")
                .to_string(),
        );
        self.tts.last_sentences = sentences.clone();
        if sentences.is_empty() {
            self.tts.current_sentence_idx = None;
            self.tts.sentence_offset = 0;
            return Task::none();
        }

        let sentence_idx = sentence_idx.min(sentences.len().saturating_sub(1));
        self.tts.sentence_offset = sentence_idx;
        self.tts.current_sentence_idx = Some(sentence_idx);

        let cache_root = crate::cache::tts_dir(&self.epub_path);
        let speed = self.config.tts_speed;
        let threads = self.config.tts_threads.max(1);
        let page_id = page;
        self.tts.started_at = None;
        self.tts.elapsed = Duration::ZERO;
        self.tts.request_id = self.tts.request_id.wrapping_add(1);
        let request_id = self.tts.request_id;
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

    fn persist_bookmark(&self) {
        let sentences = self.current_sentences();

        let sentence_idx = self
            .tts
            .current_sentence_idx
            .filter(|idx| *idx < sentences.len())
            .or_else(|| {
                if sentences.is_empty() {
                    None
                } else {
                    let frac = Self::sanitize_offset(self.bookmark.last_scroll_offset).y;
                    let idx = (frac * (sentences.len().saturating_sub(1) as f32)).round() as usize;
                    Some(idx.min(sentences.len().saturating_sub(1)))
                }
            });
        let sentence_text = sentence_idx.and_then(|idx| sentences.get(idx).cloned());
        let scroll_y = Self::sanitize_offset(self.bookmark.last_scroll_offset).y;

        let bookmark = Bookmark {
            page: self.reader.current_page,
            sentence_idx,
            sentence_text,
            scroll_y,
        };

        save_bookmark(&self.epub_path, &bookmark);
    }

    fn sanitize_offset(offset: RelativeOffset) -> RelativeOffset {
        let clamp = |v: f32| {
            if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        RelativeOffset {
            x: clamp(offset.x),
            y: clamp(offset.y),
        }
    }

    fn current_sentences(&self) -> Vec<String> {
        split_sentences(
            self.reader
                .pages
                .get(self.reader.current_page)
                .map(String::as_str)
                .unwrap_or("")
                .to_string(),
        )
    }

    pub(super) fn scroll_offset_for_sentence(
        &self,
        sentence_idx: usize,
        total_sentences: usize,
    ) -> Option<iced::widget::scrollable::RelativeOffset> {
        if total_sentences == 0 {
            return None;
        }

        let clamped_idx = sentence_idx.min(total_sentences.saturating_sub(1)) as f32;
        let denom = total_sentences.saturating_sub(1).max(1) as f32;
        let step = 1.0 / denom;
        let base = (clamped_idx / denom).clamp(0.0, 1.0);
        let y = if self.config.center_spoken_sentence {
            (base - 0.5 * step).clamp(0.0, 1.0)
        } else {
            base
        };

        Some(iced::widget::scrollable::RelativeOffset { x: 0.0, y })
    }
}
