use super::super::state::App;
use super::Effect;
use crate::pagination::{MAX_LINES_PER_PAGE, MIN_LINES_PER_PAGE};
use iced::widget::scrollable::RelativeOffset;
use tracing::debug;

impl App {
    pub(super) fn handle_next_page(&mut self, effects: &mut Vec<Effect>) {
        effects.extend(self.go_to_page(self.reader.current_page + 1));
    }

    pub(super) fn handle_previous_page(&mut self, effects: &mut Vec<Effect>) {
        if self.reader.current_page > 0 {
            effects.extend(self.go_to_page(self.reader.current_page - 1));
        }
    }

    pub(super) fn handle_lines_per_page_changed(&mut self, lines: u32, effects: &mut Vec<Effect>) {
        let clamped = lines.clamp(MIN_LINES_PER_PAGE as u32, MAX_LINES_PER_PAGE as u32) as usize;
        if clamped != self.config.lines_per_page {
            let old_page = self.reader.current_page;
            let old_sentence_idx = self.tts.current_sentence_idx.unwrap_or(0);
            let active_sentence = self
                .raw_sentences_for_page(old_page)
                .get(old_sentence_idx)
                .cloned()
                .or_else(|| self.raw_sentences_for_page(old_page).into_iter().next());
            let had_tts = self.tts.playback.is_some() || self.tts.is_preparing();
            let was_playing = self
                .tts
                .playback
                .as_ref()
                .map(|p| !p.is_paused())
                .unwrap_or(self.tts.is_playing());

            let before = self.reader.current_page;
            self.config.lines_per_page = clamped;
            self.repaginate();

            if let Some(sentence) = active_sentence {
                let mut best: Option<(usize, usize, usize)> = None;
                for (page_idx, page_sentences) in self.reader.page_sentences.iter().enumerate() {
                    for (sentence_idx, candidate) in page_sentences.iter().enumerate() {
                        if candidate == &sentence {
                            let distance = page_idx.abs_diff(old_page) * 10_000
                                + sentence_idx.abs_diff(old_sentence_idx);
                            match best {
                                Some((best_distance, _, _)) if best_distance <= distance => {}
                                _ => best = Some((distance, page_idx, sentence_idx)),
                            }
                        }
                    }
                }
                if let Some((_, page_idx, sentence_idx)) = best {
                    self.reader.current_page = page_idx;
                    self.tts.current_sentence_idx = Some(sentence_idx);
                    self.tts.last_sentences = self.raw_sentences_for_page(page_idx);
                    self.bookmark.pending_sentence_snap = Some(sentence_idx);
                    effects.push(Effect::AutoScrollToCurrent);

                    if had_tts {
                        // Invalidate any in-flight work from the old pagination before restart.
                        self.tts.request_id = self.tts.request_id.wrapping_add(1);
                        self.tts.lifecycle = super::super::state::TtsLifecycle::Idle;
                        self.tts.pending_append = false;
                        self.tts.pending_append_batch = None;
                        self.tts.resume_after_prepare = was_playing;
                        effects.push(Effect::StartTts {
                            page: self.reader.current_page,
                            sentence_idx,
                        });
                    }
                }
            }

            if self.reader.current_page != before {
                self.bookmark.last_scroll_offset = RelativeOffset::START;
                effects.push(Effect::SaveBookmark);
            } else if self.tts.current_sentence_idx.is_some() {
                effects.push(Effect::SaveBookmark);
            }
            debug!(
                lines_per_page = self.config.lines_per_page,
                "Lines per page changed"
            );
            effects.push(Effect::SaveConfig);
        }
    }

    fn go_to_page(&mut self, new_page: usize) -> Vec<Effect> {
        let mut effects = Vec::new();
        if new_page < self.reader.pages.len() {
            let was_paused = self
                .tts
                .playback
                .as_ref()
                .map(|p| p.is_paused())
                .unwrap_or(matches!(
                    self.tts.lifecycle,
                    super::super::state::TtsLifecycle::Paused
                ));
            let should_resume_playback = self
                .tts
                .playback
                .as_ref()
                .map(|p| !p.is_paused())
                .unwrap_or_else(|| self.tts.is_playing() || self.tts.is_preparing());
            self.reader.current_page = new_page;
            let sentence_count = self.sentence_count_for_page(new_page);
            self.tts.set_current_sentence_clamped(0, sentence_count);
            self.tts.last_sentences = self.raw_sentences_for_page(new_page);
            self.bookmark.last_scroll_offset = RelativeOffset::START;
            tracing::info!(page = self.reader.current_page + 1, "Navigated to page");
            if should_resume_playback {
                self.tts.resume_after_prepare = true;
                effects.push(Effect::StartTts {
                    page: self.reader.current_page,
                    sentence_idx: 0,
                });
            } else {
                self.stop_playback();
                if was_paused {
                    self.tts.lifecycle = super::super::state::TtsLifecycle::Paused;
                }
            }
            effects.push(Effect::AutoScrollToCurrent);
            effects.push(Effect::SaveBookmark);
        }
        effects
    }
}
