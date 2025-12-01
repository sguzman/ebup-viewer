use super::messages::{Component, Message};
use super::state::{App, MAX_LETTER_SPACING, MAX_MARGIN, MAX_WORD_SPACING, MIN_TTS_SPEED};
use crate::config::HighlightColor;
use crate::pagination::{MAX_FONT_SIZE, MAX_LINES_PER_PAGE, MIN_FONT_SIZE, MIN_LINES_PER_PAGE};
use crate::text_utils::split_sentences;
use iced::alignment::Horizontal;
use iced::alignment::Vertical;
use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{
    Column, Row, button, checkbox, column, container, pick_list, row, scrollable, slider, text,
};
use iced::{Element, Length};

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        let total_pages = self.reader.pages.len().max(1);
        let page_label = format!("Page {} of {}", self.reader.current_page + 1, total_pages);

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

        let controls = row![
            prev_button,
            next_button,
            theme_toggle,
            settings_toggle,
            tts_toggle,
            text(page_label)
        ]
        .spacing(10)
        .align_y(Vertical::Center)
        .width(Length::Fill);

        let font_label = text(format!("Font size: {}", self.config.font_size));
        let font_slider = slider(
            MIN_FONT_SIZE as f32..=MAX_FONT_SIZE as f32,
            self.config.font_size as f32,
            |value| Message::FontSizeChanged(value.round() as u32),
        );

        let font_controls = row![font_label, font_slider]
            .spacing(10)
            .align_y(Vertical::Center);

        let page_content = self.formatted_page_content();

        let text_view_content: Element<'_, Message> =
            if self.tts.playback.is_some() && !self.tts.last_sentences.is_empty() {
                let sentences = split_sentences(page_content.clone());
                if sentences.is_empty() {
                    return text(page_content)
                        .size(self.config.font_size as f32)
                        .line_height(LineHeight::Relative(self.config.line_spacing))
                        .width(Length::Fill)
                        .wrapping(Wrapping::WordOrGlyph)
                        .align_x(Horizontal::Left)
                        .font(self.current_font())
                        .into();
                }
                let highlight_idx = self
                    .tts
                    .current_sentence_idx
                    .unwrap_or(0)
                    .min(sentences.len().saturating_sub(1));
                let highlight = self.highlight_color();

                let spans: Vec<iced::widget::text::Span<'_, Message>> = sentences
                    .into_iter()
                    .enumerate()
                    .map(|(idx, sentence)| {
                        let mut span: iced::widget::text::Span<'_, Message> =
                            iced::widget::text::Span::new(sentence)
                                .font(self.current_font())
                                .size(self.config.font_size as f32)
                                .line_height(LineHeight::Relative(self.config.line_spacing));

                        if idx == highlight_idx {
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
            } else {
                text(page_content)
                    .size(self.config.font_size as f32)
                    .line_height(LineHeight::Relative(self.config.line_spacing))
                    .width(Length::Fill)
                    .wrapping(Wrapping::WordOrGlyph)
                    .align_x(Horizontal::Left)
                    .font(self.current_font())
                    .into()
            };

        let text_view = scrollable(
            container(text_view_content)
                .width(Length::Fill)
                .padding([
                    self.config.margin_vertical,
                    self.config.margin_horizontal,
                ]),
        )
        .on_scroll(|viewport| Message::Scrolled(viewport.relative_offset()))
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
                text(format!("Horizontal margin: {} px", self.config.margin_horizontal)),
                margin_slider
            ]
            .spacing(8)
            .align_y(Vertical::Center),
            row![
                text(format!("Vertical margin: {} px", self.config.margin_vertical)),
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
            self.color_row(
                "Night highlight",
                self.config.night_highlight,
                |c, v| { Message::NightHighlightChanged(c, v) }
            ),
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

        let speed_slider = slider(
            MIN_TTS_SPEED..=super::state::MAX_TTS_SPEED,
            self.config.tts_speed,
            Message::SetTtsSpeed,
        )
        .step(0.05);

        let controls = row![
            button("⏮").on_press(Message::SeekBackward),
            play_button,
            button("⏭").on_press(Message::SeekForward),
            play_from_start,
            play_from_cursor,
            jump_button,
            text(format!("Speed: {:.2}x", self.config.tts_speed)),
            speed_slider,
        ]
        .spacing(10)
        .align_y(Vertical::Center);

        container(
            column![text("TTS Controls"), controls]
                .spacing(8)
                .padding(8),
        )
        .into()
    }
}
