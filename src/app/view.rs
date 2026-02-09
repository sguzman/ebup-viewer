use super::messages::{Component, Message};
use super::state::{
    App, IMAGE_BLOCK_SPACING_PX, IMAGE_FOOTER_FONT_SIZE_PX, IMAGE_FOOTER_LINE_HEIGHT,
    IMAGE_LABEL_FONT_SIZE_PX, IMAGE_LABEL_LINE_HEIGHT, IMAGE_PREVIEW_HEIGHT_PX, MAX_LETTER_SPACING,
    MAX_MARGIN, MAX_TTS_VOLUME, MAX_WORD_SPACING, MIN_TTS_SPEED, MIN_TTS_VOLUME,
    PAGE_FLOW_SPACING_PX,
};
use crate::config::HighlightColor;
use crate::pagination::{MAX_FONT_SIZE, MAX_LINES_PER_PAGE, MIN_FONT_SIZE, MIN_LINES_PER_PAGE};
use iced::alignment::Horizontal;
use iced::alignment::Vertical;
use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{
    Column, Row, button, checkbox, column, container, horizontal_space, image, pick_list, row,
    scrollable, slider, text,
};
use iced::{ContentFit, Element, Length};
use std::time::Duration;

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        let total_pages = self.reader.pages.len().max(1);
        let page_label = format!("Page {} of {}", self.reader.current_page + 1, total_pages);
        let tts_progress_label = self.audio_progress_label();

        let theme_label = if matches!(self.config.theme, crate::config::ThemeMode::Night) {
            "Day Mode"
        } else {
            "Night Mode"
        };
        let theme_toggle = button(theme_label).on_press(Message::ToggleTheme);
        let settings_toggle = button(if self.config.show_settings {
            "Hide Settings"
        } else {
            "Show Settings"
        })
        .on_press(Message::ToggleSettings);
        let tts_toggle = button(if self.config.show_tts {
            "Hide TTS"
        } else {
            "Show TTS"
        })
        .on_press(Message::ToggleTtsControls);
        let text_only_toggle = button(if self.text_only_mode {
            "Pretty Text"
        } else {
            "Text-Only"
        })
        .on_press(Message::ToggleTextOnly);

        let prev_button = if self.reader.current_page > 0 {
            button("Previous").on_press(Message::PreviousPage)
        } else {
            button("Previous")
        };

        let next_button = if self.reader.current_page + 1 < total_pages {
            button("Next").on_press(Message::NextPage)
        } else {
            button("Next")
        };

        let available_width = self.estimated_controls_width();
        let show_compact_status = !self.config.show_settings;
        let show_page_label = show_compact_status && available_width >= 760.0;
        let show_tts_progress_label = show_compact_status && available_width >= 920.0;

        let mut controls = row![
            prev_button,
            next_button,
            theme_toggle,
            settings_toggle,
            tts_toggle,
            text_only_toggle,
            horizontal_space(),
        ]
        .spacing(10)
        .align_y(Vertical::Center)
        .width(Length::Fill);

        if show_page_label {
            controls = controls.push(text(page_label).width(Length::Shrink));
        }
        if show_tts_progress_label {
            controls = controls.push(text(tts_progress_label).width(Length::Shrink));
        }

        let font_controls = row![
            column![
                text(format!("Font: {}", self.config.font_size)),
                slider(
                    MIN_FONT_SIZE as f32..=MAX_FONT_SIZE as f32,
                    self.config.font_size as f32,
                    |value| Message::FontSizeChanged(value.round() as u32),
                )
            ]
            .spacing(4)
            .width(Length::FillPortion(1)),
            column![
                text(format!("Speed: {:.2}x", self.config.tts_speed)),
                slider(
                    MIN_TTS_SPEED..=super::state::MAX_TTS_SPEED,
                    self.config.tts_speed,
                    Message::SetTtsSpeed,
                )
                .step(0.05)
            ]
            .spacing(4)
            .width(Length::FillPortion(1)),
            column![
                text(format!("Volume: {:.0}%", self.config.tts_volume * 100.0)),
                slider(
                    MIN_TTS_VOLUME..=MAX_TTS_VOLUME,
                    self.config.tts_volume,
                    Message::SetTtsVolume,
                )
                .step(0.01)
            ]
            .spacing(4)
            .width(Length::FillPortion(1)),
        ]
        .spacing(12)
        .align_y(Vertical::Center)
        .width(Length::Fill);

        let raw_sentences = self.raw_sentences_for_page(self.reader.current_page);
        let text_view_content: Element<'_, Message> = if self.text_only_mode {
            if let Some(preview) = self.text_only_preview_for_current_page() {
                let highlight_idx = self.text_only_highlight_audio_idx_for_current_page();
                let highlight = self.highlight_color();
                let mut spans: Vec<iced::widget::text::Span<'_, Message>> =
                    Vec::with_capacity(preview.audio_sentences.len().saturating_mul(2));

                for (idx, sentence) in preview.audio_sentences.iter().enumerate() {
                    let mut span: iced::widget::text::Span<'_, Message> =
                        iced::widget::text::Span::new(sentence.as_str())
                            .font(self.current_font())
                            .size(self.config.font_size as f32)
                            .line_height(LineHeight::Relative(self.config.line_spacing));

                    if Some(idx) == highlight_idx {
                        span = span
                            .background(iced::Background::Color(highlight))
                            .padding(iced::Padding::from(2u16));
                    }
                    spans.push(span);

                    if idx + 1 < preview.audio_sentences.len() {
                        spans.push(
                            iced::widget::text::Span::new("\n\n")
                                .font(self.current_font())
                                .size(self.config.font_size as f32)
                                .line_height(LineHeight::Relative(self.config.line_spacing)),
                        );
                    }
                }

                let rich: iced::widget::text::Rich<'_, Message> =
                    iced::widget::text::Rich::with_spans(spans);
                rich.width(Length::Fill)
                    .wrapping(Wrapping::WordOrGlyph)
                    .align_x(Horizontal::Left)
                    .into()
            } else {
                text("Preparing normalized text preview...")
                    .size(self.config.font_size as f32)
                    .line_height(LineHeight::Relative(self.config.line_spacing))
                    .width(Length::Fill)
                    .wrapping(Wrapping::WordOrGlyph)
                    .align_x(Horizontal::Left)
                    .font(self.current_font())
                    .into()
            }
        } else {
            let fallback_page_content = self.formatted_page_content();
            let display_sentences =
                if self.config.word_spacing == 0 && self.config.letter_spacing == 0 {
                    raw_sentences.clone()
                } else {
                    self.display_sentences_for_current_page()
                };

            if display_sentences.is_empty() {
                text(fallback_page_content)
                    .size(self.config.font_size as f32)
                    .line_height(LineHeight::Relative(self.config.line_spacing))
                    .width(Length::Fill)
                    .wrapping(Wrapping::WordOrGlyph)
                    .align_x(Horizontal::Left)
                    .font(self.current_font())
                    .into()
            } else {
                let highlight_idx = self
                    .tts
                    .current_sentence_idx
                    .filter(|idx| *idx < display_sentences.len());
                let highlight = self.highlight_color();

                let spans: Vec<iced::widget::text::Span<'_, Message>> = display_sentences
                    .into_iter()
                    .enumerate()
                    .map(|(idx, sentence)| {
                        let mut span: iced::widget::text::Span<'_, Message> =
                            iced::widget::text::Span::new(sentence)
                                .font(self.current_font())
                                .size(self.config.font_size as f32)
                                .line_height(LineHeight::Relative(self.config.line_spacing));

                        if idx < raw_sentences.len() {
                            span = span.link(Message::SentenceClicked(idx));
                        }

                        if Some(idx) == highlight_idx {
                            span = span
                                .background(iced::Background::Color(highlight))
                                .padding(iced::Padding::from(2u16));
                        }

                        span
                    })
                    .collect();

                let rich: iced::widget::text::Rich<'_, Message> =
                    iced::widget::text::Rich::with_spans(spans);

                rich.width(Length::Fill)
                    .wrapping(Wrapping::WordOrGlyph)
                    .align_x(Horizontal::Left)
                    .into()
            }
        };

        let mut pane_content: Column<'_, Message> = column![text_view_content]
            .spacing(PAGE_FLOW_SPACING_PX)
            .width(Length::Fill);

        if !self.text_only_mode {
            let mut image_count = 0usize;
            for (idx, img) in self.reader.images.iter().enumerate() {
                if self.image_assigned_page(idx) != self.reader.current_page {
                    continue;
                }
                image_count += 1;
                let image_block = column![
                    text(format!("Image: {}", img.label))
                        .size(IMAGE_LABEL_FONT_SIZE_PX)
                        .line_height(LineHeight::Relative(IMAGE_LABEL_LINE_HEIGHT)),
                    image(img.path.clone())
                        .width(Length::Fill)
                        .height(Length::Fixed(IMAGE_PREVIEW_HEIGHT_PX))
                        .content_fit(ContentFit::Contain)
                ]
                .spacing(IMAGE_BLOCK_SPACING_PX)
                .width(Length::Fill);
                pane_content = pane_content.push(container(image_block).width(Length::Fill));
            }
            if image_count > 0 {
                pane_content = pane_content.push(
                    text(format!("Rendered {image_count} image(s) on this page."))
                        .size(IMAGE_FOOTER_FONT_SIZE_PX)
                        .line_height(LineHeight::Relative(IMAGE_FOOTER_LINE_HEIGHT)),
                );
            }
        }

        let text_view = scrollable(
            container(pane_content)
                .width(Length::Fill)
                .padding([self.config.margin_vertical, self.config.margin_horizontal]),
        )
        .on_scroll(|viewport| Message::Scrolled {
            offset: viewport.relative_offset(),
            viewport_width: viewport.bounds().width,
            viewport_height: viewport.bounds().height,
            content_width: viewport.content_bounds().width,
            content_height: viewport.content_bounds().height,
        })
        .id(super::state::TEXT_SCROLL_ID.clone())
        .height(Length::FillPortion(1));

        let mut content: Column<'_, Message> = column![controls, font_controls, text_view]
            .padding(16)
            .spacing(12)
            .height(Length::Fill);

        if self.config.show_tts {
            content = content.push(self.tts_controls());
        }

        let mut layout: Row<'_, Message> = row![container(content).width(Length::Fill)].spacing(16);

        if self.config.show_settings {
            layout = layout.push(self.settings_panel());
        }

        layout.into()
    }
}

impl App {
    fn audio_progress_label(&self) -> String {
        let total_sentences = self.reader.page_sentence_counts.iter().sum::<usize>();
        if total_sentences == 0 {
            return "TTS 0.0%".to_string();
        }

        let current_idx = self.tts.current_sentence_idx.unwrap_or(0);
        let before: usize = self
            .reader
            .page_sentence_counts
            .iter()
            .take(self.reader.current_page)
            .sum();
        let global_idx = before
            .saturating_add(current_idx)
            .min(total_sentences.saturating_sub(1));
        let percent = (global_idx as f32 + 1.0) / total_sentences as f32 * 100.0;
        format!("TTS {:.1}%", percent)
    }

    fn color_row<'a>(
        &self,
        label: &'a str,
        color: HighlightColor,
        msg: impl Fn(Component, f32) -> Message + Copy + 'a,
    ) -> Row<'a, Message> {
        row![
            text(label),
            slider(0.0..=1.0, color.r, move |v| msg(Component::R, v)).step(0.01),
            slider(0.0..=1.0, color.g, move |v| msg(Component::G, v)).step(0.01),
            slider(0.0..=1.0, color.b, move |v| msg(Component::B, v)).step(0.01),
            slider(0.0..=1.0, color.a, move |v| msg(Component::A, v)).step(0.01),
        ]
        .spacing(6)
        .align_y(Vertical::Center)
    }

    pub(super) fn settings_panel(&self) -> Element<'_, Message> {
        let family_picker = pick_list(
            super::state::FONT_FAMILIES,
            Some(self.config.font_family),
            Message::FontFamilyChanged,
        );
        let weight_picker = pick_list(
            super::state::FONT_WEIGHTS,
            Some(self.config.font_weight),
            Message::FontWeightChanged,
        );

        let line_spacing_slider = slider(
            0.8..=2.5,
            self.config.line_spacing,
            Message::LineSpacingChanged,
        )
        .step(0.05);
        let lines_per_page_slider = slider(
            MIN_LINES_PER_PAGE as f32..=MAX_LINES_PER_PAGE as f32,
            self.config.lines_per_page as f32,
            |value| Message::LinesPerPageChanged(value.round() as u32),
        )
        .step(1.0);

        let margin_slider = slider(
            0.0..=MAX_MARGIN as f32,
            self.config.margin_horizontal as f32,
            |value| Message::MarginHorizontalChanged(value.round() as u16),
        );

        let margin_vertical_slider = slider(
            0.0..=MAX_MARGIN as f32,
            self.config.margin_vertical as f32,
            |value| Message::MarginVerticalChanged(value.round() as u16),
        );

        let word_spacing_slider = slider(
            0.0..=MAX_WORD_SPACING as f32,
            self.config.word_spacing as f32,
            |value| Message::WordSpacingChanged(value.round() as u32),
        );

        let letter_spacing_slider = slider(
            0.0..=MAX_LETTER_SPACING as f32,
            self.config.letter_spacing as f32,
            |value| Message::LetterSpacingChanged(value.round() as u32),
        );
        let total_pages = self.reader.pages.len().max(1);
        let page_label = format!("Page {} of {}", self.reader.current_page + 1, total_pages);
        let status_panel = column![
            text("Reading Status").size(18.0),
            text(page_label),
            text(self.audio_progress_label()),
            text(format!("Page remaining: {}", self.page_eta_label())),
            text(format!("Book remaining: {}", self.book_eta_label())),
        ]
        .spacing(4);

        let panel = column![
            text("Reader Settings").size(20.0),
            row![text("Font family"), family_picker]
                .spacing(8)
                .align_y(Vertical::Center),
            row![text("Font weight"), weight_picker]
                .spacing(8)
                .align_y(Vertical::Center),
            row![text("Line spacing"), line_spacing_slider]
                .spacing(8)
                .align_y(Vertical::Center),
            row![
                text(format!(
                    "Pause after sentence: {:.1} s",
                    self.config.pause_after_sentence
                )),
                slider(
                    0.0..=2.0,
                    self.config.pause_after_sentence,
                    Message::PauseAfterSentenceChanged
                )
                .step(0.1)
            ]
            .spacing(8)
            .align_y(Vertical::Center),
            checkbox(
                "Auto-scroll to spoken sentence",
                self.config.auto_scroll_tts
            )
            .on_toggle(Message::AutoScrollTtsChanged),
            checkbox(
                "Center tracked sentence while auto-scrolling",
                self.config.center_spoken_sentence
            )
            .on_toggle(Message::CenterSpokenSentenceChanged),
            row![
                text(format!("Lines per page: {}", self.config.lines_per_page)),
                lines_per_page_slider
            ]
            .spacing(8)
            .align_y(Vertical::Center),
            row![
                text(format!(
                    "Horizontal margin: {} px",
                    self.config.margin_horizontal
                )),
                margin_slider
            ]
            .spacing(8)
            .align_y(Vertical::Center),
            row![
                text(format!(
                    "Vertical margin: {} px",
                    self.config.margin_vertical
                )),
                margin_vertical_slider
            ]
            .spacing(8)
            .align_y(Vertical::Center),
            row![
                text(format!("Word spacing: {}", self.config.word_spacing)),
                word_spacing_slider
            ]
            .spacing(8)
            .align_y(Vertical::Center),
            row![
                text(format!("Letter spacing: {}", self.config.letter_spacing)),
                letter_spacing_slider
            ]
            .spacing(8)
            .align_y(Vertical::Center),
            text("Highlight Colors").size(18.0),
            self.color_row("Day highlight", self.config.day_highlight, |c, v| {
                Message::DayHighlightChanged(c, v)
            }),
            self.color_row("Night highlight", self.config.night_highlight, |c, v| {
                Message::NightHighlightChanged(c, v)
            }),
            status_panel,
        ]
        .spacing(12)
        .width(Length::Fixed(280.0));

        container(panel).padding(12).into()
    }

    pub(super) fn tts_controls(&self) -> Element<'_, Message> {
        let play_label = if self
            .tts
            .playback
            .as_ref()
            .map(|p| p.is_paused())
            .unwrap_or(true)
        {
            "Play"
        } else {
            "Pause"
        };

        let play_button = if play_label == "Play" {
            button(play_label).on_press(Message::Play)
        } else {
            button(play_label).on_press(Message::Pause)
        };
        let play_from_start = button("Play Page").on_press(Message::PlayFromPageStart);
        let jump_disabled = self.tts.current_sentence_idx.is_none();
        let jump_button = if jump_disabled {
            button("Jump to Audio")
        } else {
            button("Jump to Audio").on_press(Message::JumpToCurrentAudio)
        };
        let play_from_cursor = if let Some(idx) = self.tts.current_sentence_idx {
            button("Play From Highlight").on_press(Message::PlayFromCursor(idx))
        } else {
            button("Play From Highlight")
        };
        let available_width = self.estimated_controls_width();
        let mut controls = row![
            button("⏮").on_press(Message::SeekBackward),
            play_button,
            button("⏭").on_press(Message::SeekForward),
            play_from_start,
            play_from_cursor,
            jump_button,
            horizontal_space(),
        ]
        .spacing(10)
        .align_y(Vertical::Center)
        .width(Length::Fill);
        if !self.config.show_settings && available_width >= 920.0 {
            controls = controls.push(self.eta_trackers_view(available_width));
        }

        container(
            column![text("TTS Controls"), controls]
                .spacing(8)
                .padding(8),
        )
        .into()
    }

    fn page_eta_label(&self) -> String {
        Self::format_duration_dhms(self.estimate_remaining_page_duration())
    }

    fn book_eta_label(&self) -> String {
        let page_remaining = self.estimate_remaining_page_duration();
        let average_sentence = self.estimated_avg_sentence_duration();
        let mut remaining_after_page = 0usize;
        for page_idx in (self.reader.current_page + 1)..self.reader.pages.len() {
            remaining_after_page += self.sentence_count_for_page(page_idx);
        }
        let book_remaining = page_remaining
            + Duration::from_secs_f64(average_sentence.as_secs_f64() * remaining_after_page as f64);
        Self::format_duration_dhms(book_remaining)
    }

    fn estimate_remaining_page_duration(&self) -> Duration {
        let sentence_count = self.sentence_count_for_page(self.reader.current_page);
        if sentence_count == 0 {
            return Duration::ZERO;
        }
        let current_display_idx = self
            .tts
            .current_sentence_idx
            .unwrap_or(0)
            .min(sentence_count.saturating_sub(1));
        let current_audio_idx = self
            .find_audio_start_for_display_sentence(current_display_idx)
            .unwrap_or(self.tts.sentence_offset);

        if !self.tts.track.is_empty() && current_audio_idx >= self.tts.sentence_offset {
            let start = current_audio_idx - self.tts.sentence_offset;
            if start < self.tts.track.len() {
                let speech_remaining = self.tts.track[start..]
                    .iter()
                    .fold(Duration::ZERO, |acc, (_, d)| acc + *d);
                let pause = Duration::from_secs_f32(self.config.pause_after_sentence.max(0.0));
                let pause_remaining = Duration::from_secs_f64(
                    pause.as_secs_f64() * (self.tts.track.len() - start) as f64,
                );
                return speech_remaining + pause_remaining;
            }
        }

        let avg_sentence = self.estimated_avg_sentence_duration();
        let remaining = if !self.tts.audio_to_display.is_empty() {
            self.tts
                .audio_to_display
                .len()
                .saturating_sub(current_audio_idx)
        } else {
            sentence_count.saturating_sub(current_display_idx)
        };
        Duration::from_secs_f64(avg_sentence.as_secs_f64() * remaining as f64)
    }

    fn estimated_avg_sentence_duration(&self) -> Duration {
        let pause = Duration::from_secs_f32(self.config.pause_after_sentence.max(0.0));
        if !self.tts.track.is_empty() {
            let speech_total = self
                .tts
                .track
                .iter()
                .fold(Duration::ZERO, |acc, (_, d)| acc + *d);
            let avg_speech = speech_total.as_secs_f64() / self.tts.track.len() as f64;
            return Duration::from_secs_f64(avg_speech + pause.as_secs_f64());
        }

        let sentences = self
            .reader
            .page_sentences
            .get(self.reader.current_page)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if !sentences.is_empty() {
            let total_chars: usize = sentences.iter().map(|s| s.chars().count()).sum();
            let avg_chars = total_chars as f64 / sentences.len() as f64;
            let speech_secs = (avg_chars / 14.0) / self.config.tts_speed.max(0.1) as f64;
            return Duration::from_secs_f64((speech_secs + pause.as_secs_f64()).max(0.1));
        }

        Duration::from_secs_f64((2.5 / self.config.tts_speed.max(0.1)) as f64 + pause.as_secs_f64())
    }

    fn eta_trackers_view(&self, available_width: f32) -> Element<'_, Message> {
        let page_eta = self.page_eta_label();
        let book_eta = self.book_eta_label();

        if available_width < 1_120.0 {
            return row![
                text(format!("Page {page_eta}")).size(13.0),
                text(format!("Book {book_eta}")).size(13.0)
            ]
            .spacing(10)
            .align_y(Vertical::Center)
            .into();
        }

        row![
            column![text("Page Remaining").size(13.0), text(page_eta).size(13.0)].spacing(2),
            column![text("Book Remaining").size(13.0), text(book_eta).size(13.0)].spacing(2),
        ]
        .spacing(16)
        .align_y(Vertical::Center)
        .into()
    }

    fn estimated_controls_width(&self) -> f32 {
        let mut width = if self.bookmark.viewport_width > 0.0 {
            self.bookmark.viewport_width
        } else {
            self.config.window_width
        };
        if self.config.show_settings {
            // Settings panel is fixed width plus layout padding/spacers.
            width = (width - 320.0).max(0.0);
        }
        width.max(0.0)
    }

    fn format_duration_dhms(duration: Duration) -> String {
        let total_secs = duration.as_secs();
        let days = total_secs / 86_400;
        let hours = (total_secs % 86_400) / 3_600;
        let minutes = (total_secs % 3_600) / 60;
        let seconds = total_secs % 60;
        format!("{days}d {hours:02}h {minutes:02}m {seconds:02}s")
    }
}
