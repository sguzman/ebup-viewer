use super::Effect;
use super::super::messages::Message;
use super::super::state::{App, TEXT_SCROLL_ID};
use iced::time;
use iced::{Subscription, Task};
use std::time::Duration;

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
            Message::NextPage => self.handle_next_page(&mut effects),
            Message::PreviousPage => self.handle_previous_page(&mut effects),
            Message::FontSizeChanged(size) => self.handle_font_size_changed(size, &mut effects),
            Message::ToggleTheme => self.handle_toggle_theme(&mut effects),
            Message::ToggleSettings => self.handle_toggle_settings(&mut effects),
            Message::FontFamilyChanged(family) => {
                self.handle_font_family_changed(family, &mut effects);
            }
            Message::FontWeightChanged(weight) => {
                self.handle_font_weight_changed(weight, &mut effects);
            }
            Message::LineSpacingChanged(spacing) => {
                self.handle_line_spacing_changed(spacing, &mut effects);
            }
            Message::MarginHorizontalChanged(margin) => {
                self.handle_margin_horizontal_changed(margin, &mut effects);
            }
            Message::MarginVerticalChanged(margin) => {
                self.handle_margin_vertical_changed(margin, &mut effects);
            }
            Message::WordSpacingChanged(spacing) => {
                self.handle_word_spacing_changed(spacing, &mut effects);
            }
            Message::LetterSpacingChanged(spacing) => {
                self.handle_letter_spacing_changed(spacing, &mut effects);
            }
            Message::LinesPerPageChanged(lines) => {
                self.handle_lines_per_page_changed(lines, &mut effects);
            }
            Message::DayHighlightChanged(component, value) => {
                self.handle_day_highlight_changed(component, value, &mut effects);
            }
            Message::PauseAfterSentenceChanged(pause) => {
                self.handle_pause_after_sentence_changed(pause, &mut effects);
            }
            Message::NightHighlightChanged(component, value) => {
                self.handle_night_highlight_changed(component, value, &mut effects);
            }
            Message::AutoScrollTtsChanged(enabled) => {
                self.handle_auto_scroll_tts_changed(enabled, &mut effects);
            }
            Message::CenterSpokenSentenceChanged(centered) => {
                self.handle_center_spoken_sentence_changed(centered, &mut effects);
            }
            Message::ToggleTtsControls => self.handle_toggle_tts_controls(&mut effects),
            Message::JumpToCurrentAudio => self.handle_jump_to_current_audio(&mut effects),
            Message::Play => self.handle_play(&mut effects),
            Message::PlayFromPageStart => self.handle_play_from_page_start(&mut effects),
            Message::PlayFromCursor(idx) => self.handle_play_from_cursor(idx, &mut effects),
            Message::Pause => self.handle_pause(&mut effects),
            Message::SetTtsSpeed(speed) => self.handle_set_tts_speed(speed, &mut effects),
            Message::SeekForward => self.handle_seek_forward(&mut effects),
            Message::SeekBackward => self.handle_seek_backward(&mut effects),
            Message::Scrolled(offset) => self.handle_scrolled(offset, &mut effects),
            Message::TtsPrepared {
                page,
                start_idx,
                request_id,
                files,
            } => self.handle_tts_prepared(page, start_idx, request_id, files, &mut effects),
            Message::Tick(now) => self.handle_tick(now, &mut effects),
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
}
