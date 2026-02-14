use super::messages::{Component, Message, NumericSetting};
use super::state::{
    App, IMAGE_BLOCK_SPACING_PX, IMAGE_FOOTER_FONT_SIZE_PX, IMAGE_FOOTER_LINE_HEIGHT,
    IMAGE_LABEL_FONT_SIZE_PX, IMAGE_LABEL_LINE_HEIGHT, IMAGE_PREVIEW_HEIGHT_PX,
    MAX_HORIZONTAL_MARGIN, MAX_LETTER_SPACING, MAX_TTS_VOLUME, MAX_VERTICAL_MARGIN,
    MAX_WORD_SPACING, MIN_TTS_SPEED, MIN_TTS_VOLUME, PAGE_FLOW_SPACING_PX,
};
use super::topbar_layout::{TopBarLabels, estimate_button_width_px, topbar_plan};
use crate::calibre::CalibreColumn;
use crate::config::HighlightColor;
use crate::pagination::{MAX_FONT_SIZE, MAX_LINES_PER_PAGE, MIN_FONT_SIZE, MIN_LINES_PER_PAGE};
use iced::alignment::Horizontal;
use iced::alignment::Vertical;
use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{
    Column, Row, button, checkbox, column, container, horizontal_space, image, pick_list, row,
    scrollable, slider, text, text_input,
};
use iced::{Border, Color, ContentFit, Element, Length};
use std::time::Duration;

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        if self.starter_mode {
            return self.starter_view();
        }

        let total_pages = self.reader.pages.len().max(1);

        let theme_label = if matches!(self.config.theme, crate::config::ThemeMode::Night) {
            "Day Mode"
        } else {
            "Night Mode"
        };
        let close_session_button =
            Self::control_button("Close Book").on_press(Message::CloseReadingSession);
        let theme_toggle = Self::control_button(theme_label).on_press(Message::ToggleTheme);
        let settings_toggle = Self::control_button(if self.config.show_settings {
            "Hide Settings"
        } else {
            "Show Settings"
        })
        .on_press(Message::ToggleSettings);
        let stats_toggle = Self::control_button(if self.show_stats {
            "Hide Stats"
        } else {
            "Show Stats"
        })
        .on_press(Message::ToggleStats);
        let search_toggle = Self::control_button(if self.search.visible {
            "Hide Search"
        } else {
            "Search"
        })
        .on_press(Message::ToggleSearch);
        let tts_toggle = Self::control_button(if self.config.show_tts {
            "Hide TTS"
        } else {
            "Show TTS"
        })
        .on_press(Message::ToggleTtsControls);
        let text_only_toggle = Self::control_button(if self.text_only_mode {
            "Pretty Text"
        } else {
            "Text Only"
        })
        .on_press(Message::ToggleTextOnly);

        let prev_button = if self.reader.current_page > 0 {
            Self::control_button("Previous").on_press(Message::PreviousPage)
        } else {
            Self::control_button("Previous")
        };

        let next_button = if self.reader.current_page + 1 < total_pages {
            Self::control_button("Next").on_press(Message::NextPage)
        } else {
            Self::control_button("Next")
        };

        let visibility = topbar_plan(
            self.controls_layout_width(),
            TopBarLabels {
                theme: theme_label,
                settings: if self.config.show_settings {
                    "Hide Settings"
                } else {
                    "Show Settings"
                },
                stats: if self.show_stats {
                    "Hide Stats"
                } else {
                    "Show Stats"
                },
                text_mode: if self.text_only_mode {
                    "Pretty Text"
                } else {
                    "Text Only"
                },
                tts: if self.config.show_tts {
                    "Hide TTS"
                } else {
                    "Show TTS"
                },
                search: if self.search.visible {
                    "Hide Search"
                } else {
                    "Search"
                },
            },
        );

        let mut controls_row = row![
            prev_button,
            next_button,
            theme_toggle,
            close_session_button,
            settings_toggle,
            stats_toggle
        ]
        .spacing(10)
        .align_y(Vertical::Center)
        .width(Length::Fill);
        if visibility.show_text_mode {
            controls_row = controls_row.push(text_only_toggle);
        }
        if visibility.show_tts {
            controls_row = controls_row.push(tts_toggle);
        }
        if visibility.show_search {
            controls_row = controls_row.push(search_toggle);
        }
        controls_row = controls_row.push(horizontal_space());
        let controls = container(controls_row)
            .height(Length::Fixed(42.0))
            .align_y(Vertical::Center)
            .width(Length::Fill);

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
                    let display_idx = preview.audio_to_display.get(idx).copied().unwrap_or(idx);
                    let mut span: iced::widget::text::Span<'_, Message> =
                        iced::widget::text::Span::new(sentence.as_str())
                            .font(self.current_font())
                            .size(self.config.font_size as f32)
                            .line_height(LineHeight::Relative(self.config.line_spacing))
                            .link(Message::SentenceClicked(display_idx));

                    if Some(idx) == highlight_idx {
                        span = span.background(iced::Background::Color(highlight));
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
                                .line_height(LineHeight::Relative(self.config.line_spacing))
                                .link(Message::SentenceClicked(idx));

                        if Some(idx) == highlight_idx {
                            span = span.background(iced::Background::Color(highlight));
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

        let mut content: Column<'_, Message> = column![controls, font_controls].spacing(12);

        if self.search.visible {
            content = content.push(self.search_bar());
        }

        content = content.push(text_view).padding(16).height(Length::Fill);

        if self.config.show_tts {
            content = content.push(self.tts_controls());
        }

        let mut layout: Row<'_, Message> = row![container(content).width(Length::Fill)].spacing(16);

        if self.config.show_settings {
            layout = layout.push(self.settings_panel());
        } else if self.show_stats {
            layout = layout.push(self.stats_panel());
        }

        layout.into()
    }
}

impl App {
    fn starter_view(&self) -> Element<'_, Message> {
        let starter_width = self.config.window_width.max(320.0);
        let show_recent_panel = self.recent.visible && starter_width >= 1_260.0;
        let show_calibre_panel = self.calibre.visible && starter_width >= 980.0;
        let show_refresh_button = starter_width >= 1_120.0;

        let open_button = if self.book_loading {
            button("Opening...")
        } else {
            button("Open Path").on_press(Message::OpenPathRequested)
        };
        let mut top = column![
            text("Welcome").size(28.0),
            text("Open a local file or choose a book from Calibre / Recent.").size(14.0),
            row![
                text_input("Path to .epub/.txt/.md", &self.open_path_input)
                    .on_input(Message::OpenPathInputChanged)
                    .on_submit(Message::OpenPathRequested)
                    .padding(10)
                    .width(Length::Fill),
                open_button
            ]
            .spacing(8)
            .align_y(Vertical::Center),
            row![
                button(if self.recent.visible {
                    "Hide Recent"
                } else {
                    "Show Recent"
                })
                .on_press(Message::ToggleRecentBooks),
                button(if self.calibre.visible {
                    "Hide Calibre"
                } else {
                    "Show Calibre"
                })
                .on_press(Message::ToggleCalibreBrowser),
                if show_refresh_button {
                    button("Refresh Calibre").on_press(Message::RefreshCalibreBooks)
                } else {
                    button("Refresh")
                },
            ]
            .spacing(8),
        ]
        .spacing(12);

        if self.book_loading {
            top = top.push(text("Loading selected book...").size(13.0));
        }
        if let Some(err) = &self.book_loading_error {
            top = top.push(text(err).size(13.0));
        }
        if self.recent.visible && !show_recent_panel {
            top = top.push(text("Recent panel hidden: window too narrow.").size(12.0));
        }
        if self.calibre.visible && !show_calibre_panel {
            top = top.push(text("Calibre panel hidden: window too narrow.").size(12.0));
        }

        let mut layout: Row<'_, Message> = row![container(top).padding(16).width(Length::Fill)];
        if show_recent_panel {
            layout = layout.push(self.recent_panel());
        }
        if show_calibre_panel {
            layout = layout.push(self.calibre_panel());
        }
        layout.spacing(16).into()
    }

    fn audio_progress_label(&self) -> String {
        let percent = self.audio_progress_percent();
        format!("TTS {percent:.3}%")
    }

    fn audio_progress_percent(&self) -> f32 {
        let total_sentences = self.reader.page_sentence_counts.iter().sum::<usize>();
        if total_sentences == 0 {
            return 0.0;
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
        (global_idx as f32 + 1.0) / total_sentences as f32 * 100.0
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

    fn search_bar(&self) -> Element<'_, Message> {
        let query_input = text_input("Regex search (current page)", &self.search.query)
            .on_input(Message::SearchQueryChanged)
            .on_submit(Message::SearchSubmit)
            .padding(8)
            .size(14.0)
            .width(Length::Fill);

        let has_matches = !self.search.matches.is_empty();
        let prev_btn = if has_matches {
            button("Prev").on_press(Message::SearchPrev)
        } else {
            button("Prev")
        };
        let next_btn = if has_matches {
            button("Next").on_press(Message::SearchNext)
        } else {
            button("Next")
        };
        let status = if has_matches {
            format!(
                "Match {} of {}",
                self.search.selected_match.saturating_add(1),
                self.search.matches.len()
            )
        } else {
            "No matches".to_string()
        };

        let mut content = column![
            row![
                text("Search"),
                query_input,
                prev_btn,
                next_btn,
                text(status)
            ]
            .spacing(8)
            .align_y(Vertical::Center)
        ]
        .spacing(4);

        if let Some(err) = &self.search.error {
            content = content.push(text(format!("Invalid regex: {err}")).size(12.0));
        }

        container(content).padding(8).width(Length::Fill).into()
    }

    fn recent_panel(&self) -> Element<'_, Message> {
        let mut entries: Column<'_, Message> = column![].spacing(8).width(Length::Fill);
        if self.recent.books.is_empty() {
            entries = entries.push(text("No recent books found in cache.").size(13.0));
        } else {
            for book in self.recent.books.iter().take(80) {
                let thumb_cell: Element<'_, Message> = if let Some(path) = &book.thumbnail_path {
                    image(path.clone())
                        .width(Length::Fixed(34.0))
                        .height(Length::Fixed(48.0))
                        .content_fit(ContentFit::Contain)
                        .into()
                } else {
                    text("x")
                        .width(Length::Fixed(34.0))
                        .align_x(Horizontal::Center)
                        .into()
                };
                let row = row![
                    container(thumb_cell).width(Length::Fixed(42.0)),
                    column![
                        text(Self::truncate_text(&book.display_title, 36)).size(13.0),
                        text(book.source_path.to_string_lossy()).size(11.0),
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                    if self.book_loading {
                        button("Open")
                    } else {
                        button("Open").on_press(Message::OpenRecentBook(book.source_path.clone()))
                    }
                ]
                .spacing(8)
                .align_y(Vertical::Center);
                entries = entries.push(container(row).padding(4).width(Length::Fill));
            }
        }

        let panel = column![
            row![text("Recent Books").size(18.0)]
                .spacing(8)
                .align_y(Vertical::Center),
            scrollable(entries).height(Length::Fill)
        ]
        .spacing(8)
        .width(Length::Fixed(360.0));

        container(panel).padding(12).into()
    }

    fn calibre_panel(&self) -> Element<'_, Message> {
        let mut body: Column<'_, Message> = column![].spacing(6).width(Length::Fill);
        const COVER_COL_WIDTH: f32 = 42.0;
        const ACTION_COL_WIDTH: f32 = 64.0;
        const TABLE_SPACING: f32 = 8.0;

        if !self.calibre.config.enabled {
            body = body.push(
                text("Calibre browser is disabled. Set [calibre].enabled = true in conf/calibre.toml")
                    .size(13.0),
            );
        } else if self.calibre.loading {
            body = body.push(text("Loading Calibre catalog...").size(13.0));
        } else if let Some(err) = &self.calibre.error {
            body = body.push(text(format!("Calibre load failed: {err}")).size(13.0));
        } else if self.calibre.books.is_empty() {
            body = body.push(text("No eligible books found.").size(13.0));
        } else {
            let mut columns = self.calibre.config.sanitized_columns();
            let mut estimated_min_width = 420.0;
            for column in &columns {
                estimated_min_width += match column {
                    CalibreColumn::Title => 240.0,
                    CalibreColumn::Author => 220.0,
                    CalibreColumn::Extension => 72.0,
                    CalibreColumn::Year => 72.0,
                    CalibreColumn::Size => 88.0,
                };
            }
            let panel_budget = if self.recent.visible {
                (self.config.window_width - 420.0).max(420.0)
            } else {
                (self.config.window_width - 120.0).max(420.0)
            };
            while estimated_min_width > panel_budget && columns.len() > 1 {
                if let Some(idx) = columns
                    .iter()
                    .position(|c| matches!(c, CalibreColumn::Size))
                {
                    columns.remove(idx);
                    estimated_min_width -= 88.0;
                    continue;
                }
                if let Some(idx) = columns
                    .iter()
                    .position(|c| matches!(c, CalibreColumn::Year))
                {
                    columns.remove(idx);
                    estimated_min_width -= 72.0;
                    continue;
                }
                if let Some(idx) = columns
                    .iter()
                    .position(|c| matches!(c, CalibreColumn::Extension))
                {
                    columns.remove(idx);
                    estimated_min_width -= 72.0;
                    continue;
                }
                if let Some(idx) = columns
                    .iter()
                    .position(|c| matches!(c, CalibreColumn::Author))
                {
                    columns.remove(idx);
                    estimated_min_width -= 220.0;
                    continue;
                }
                break;
            }
            let header_font_size = (self.config.font_size as f32 - 1.0).max(10.0);
            let row_font_size = (self.config.font_size as f32 - 2.0).max(9.0);
            let needle = self.calibre.search_query.trim().to_ascii_lowercase();
            let filtered_books: Vec<&crate::calibre::CalibreBook> = if needle.is_empty() {
                self.calibre.books.iter().collect()
            } else {
                self.calibre
                    .books
                    .iter()
                    .filter(|book| {
                        let title = book.title.to_ascii_lowercase();
                        let authors = book.authors.to_ascii_lowercase();
                        let extension = book.extension.to_ascii_lowercase();
                        let year = book.year.map(|y| y.to_string()).unwrap_or_default();
                        let id = book.id.to_string();
                        title.contains(&needle)
                            || authors.contains(&needle)
                            || extension.contains(&needle)
                            || year.contains(&needle)
                            || id.contains(&needle)
                    })
                    .collect()
            };
            let filtered_count = filtered_books.len();

            body = body.push(
                row![
                    text_input(
                        "Search calibre books (title, author, ext, year, id)",
                        &self.calibre.search_query
                    )
                    .on_input(Message::CalibreSearchQueryChanged)
                    .padding(8)
                    .size(row_font_size)
                    .width(Length::Fill),
                    text(format!(
                        "{}/{}",
                        filtered_books.len(),
                        self.calibre.books.len()
                    ))
                    .size(row_font_size),
                ]
                .spacing(8)
                .align_y(Vertical::Center),
            );

            let mut header: Row<'_, Message> = row![
                text("Cover")
                    .size(header_font_size)
                    .width(Length::Fixed(COVER_COL_WIDTH))
                    .align_x(Horizontal::Center)
            ]
            .spacing(TABLE_SPACING)
            .align_y(Vertical::Center);
            for column in &columns {
                header = match column {
                    CalibreColumn::Title => header.push(self.calibre_header_button(
                        CalibreColumn::Title,
                        "Title",
                        Length::FillPortion(2),
                        header_font_size,
                    )),
                    CalibreColumn::Extension => header.push(self.calibre_header_button(
                        CalibreColumn::Extension,
                        "Ext",
                        Length::FillPortion(1),
                        header_font_size,
                    )),
                    CalibreColumn::Author => header.push(self.calibre_header_button(
                        CalibreColumn::Author,
                        "Author",
                        Length::FillPortion(2),
                        header_font_size,
                    )),
                    CalibreColumn::Year => header.push(self.calibre_header_button(
                        CalibreColumn::Year,
                        "Year",
                        Length::FillPortion(2),
                        header_font_size,
                    )),
                    CalibreColumn::Size => header.push(self.calibre_header_button(
                        CalibreColumn::Size,
                        "Size",
                        Length::FillPortion(2),
                        header_font_size,
                    )),
                };
            }
            header = header.push(
                text("Open")
                    .size(header_font_size)
                    .width(Length::Fixed(ACTION_COL_WIDTH))
                    .align_x(Horizontal::Center),
            );
            body = body.push(header);

            let mut rows: Column<'_, Message> = column![].spacing(4).width(Length::Fill);
            for book in filtered_books.into_iter().take(400) {
                let year = book
                    .year
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string());
                let size = book
                    .file_size_bytes
                    .map(Self::format_bytes)
                    .unwrap_or_else(|| "-".to_string());
                let mut line: Row<'_, Message> =
                    row![].spacing(TABLE_SPACING).align_y(Vertical::Center);
                let cover_cell: Element<'_, Message> = if let Some(path) = &book.cover_thumbnail {
                    image(path.clone())
                        .width(Length::Fixed(34.0))
                        .height(Length::Fixed(48.0))
                        .content_fit(ContentFit::Contain)
                        .into()
                } else {
                    text("x")
                        .width(Length::Fixed(34.0))
                        .align_x(Horizontal::Center)
                        .into()
                };
                line = line.push(container(cover_cell).width(Length::Fixed(COVER_COL_WIDTH)));
                for column in &columns {
                    line = match column {
                        CalibreColumn::Title => line.push(
                            text(Self::truncate_text(&book.title, 44))
                                .size(row_font_size)
                                .width(Length::FillPortion(2))
                                .wrapping(Wrapping::None),
                        ),
                        CalibreColumn::Extension => line.push(
                            text(book.extension.to_uppercase())
                                .size(row_font_size)
                                .width(Length::FillPortion(1))
                                .wrapping(Wrapping::None),
                        ),
                        CalibreColumn::Author => line.push(
                            text(Self::truncate_text(&book.authors, 26))
                                .size(row_font_size)
                                .width(Length::FillPortion(2))
                                .wrapping(Wrapping::None),
                        ),
                        CalibreColumn::Year => line.push(
                            text(year.clone())
                                .size(row_font_size)
                                .width(Length::FillPortion(2))
                                .wrapping(Wrapping::None),
                        ),
                        CalibreColumn::Size => line.push(
                            text(size.clone())
                                .size(row_font_size)
                                .width(Length::FillPortion(2))
                                .wrapping(Wrapping::None),
                        ),
                    };
                }
                let open_button = if self.book_loading {
                    button(text("Open").size(row_font_size))
                } else {
                    button(text("Open").size(row_font_size))
                        .on_press(Message::OpenCalibreBook(book.id))
                };
                line = line.push(open_button.width(Length::Fixed(ACTION_COL_WIDTH)));
                rows = rows.push(line);
            }

            if filtered_count == 0 {
                body = body.push(text("No calibre books match your search.").size(row_font_size));
            } else {
                body = body.push(scrollable(rows).height(Length::Fill));
            }
        }

        let panel = column![
            row![
                text("Calibre Library").size(18.0),
                horizontal_space(),
                button("Refresh").on_press(Message::RefreshCalibreBooks)
            ]
            .spacing(8)
            .align_y(Vertical::Center),
            body
        ]
        .spacing(8)
        .width(Length::FillPortion(2));

        container(panel).padding(12).into()
    }

    fn truncate_text(value: &str, max_chars: usize) -> String {
        if max_chars == 0 {
            return String::new();
        }
        let mut out = String::new();
        for (idx, ch) in value.chars().enumerate() {
            if idx >= max_chars {
                out.push_str("...");
                return out;
            }
            out.push(ch);
        }
        out
    }

    fn format_bytes(bytes: u64) -> String {
        const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
        let mut value = bytes as f64;
        let mut unit_idx = 0usize;
        while value >= 1024.0 && unit_idx + 1 < UNITS.len() {
            value /= 1024.0;
            unit_idx += 1;
        }
        if unit_idx == 0 {
            format!("{} {}", bytes, UNITS[unit_idx])
        } else {
            format!("{value:.1} {}", UNITS[unit_idx])
        }
    }

    fn calibre_header_button(
        &self,
        column: CalibreColumn,
        label: &str,
        width: Length,
        font_size: f32,
    ) -> Element<'_, Message> {
        let sort_hint = if self.calibre.sort_column == column {
            if self.calibre.sort_desc { " D" } else { " A" }
        } else {
            ""
        };
        button(
            text(format!("{label}{sort_hint}"))
                .size(font_size)
                .align_x(Horizontal::Left)
                .wrapping(Wrapping::None),
        )
        .on_press(Message::SortCalibreBy(column))
        .width(width)
        .into()
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
            0.0..=MAX_HORIZONTAL_MARGIN as f32,
            self.config.margin_horizontal as f32,
            |value| Message::MarginHorizontalChanged(value.round() as u16),
        );

        let margin_vertical_slider = slider(
            0.0..=MAX_VERTICAL_MARGIN as f32,
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
            row![
                self.numeric_setting_editor(NumericSetting::LineSpacing),
                line_spacing_slider
            ]
            .spacing(8)
            .align_y(Vertical::Center),
            row![
                self.numeric_setting_editor(NumericSetting::PauseAfterSentence),
                slider(
                    0.0..=2.0,
                    self.config.pause_after_sentence,
                    Message::PauseAfterSentenceChanged
                )
                .step(0.01)
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
                self.numeric_setting_editor(NumericSetting::LinesPerPage),
                lines_per_page_slider
            ]
            .spacing(8)
            .align_y(Vertical::Center),
            row![
                self.numeric_setting_editor(NumericSetting::MarginHorizontal),
                margin_slider
            ]
            .spacing(8)
            .align_y(Vertical::Center),
            row![
                self.numeric_setting_editor(NumericSetting::MarginVertical),
                margin_vertical_slider
            ]
            .spacing(8)
            .align_y(Vertical::Center),
            row![
                self.numeric_setting_editor(NumericSetting::WordSpacing),
                word_spacing_slider
            ]
            .spacing(8)
            .align_y(Vertical::Center),
            row![
                self.numeric_setting_editor(NumericSetting::LetterSpacing),
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
        ]
        .spacing(12)
        .width(Length::Fixed(280.0));

        container(panel).padding(12).into()
    }

    fn stats_panel(&self) -> Element<'_, Message> {
        let total_pages = self.reader.pages.len().max(1);
        let current_page = self.reader.current_page.min(total_pages.saturating_sub(1));
        let page_words = self.word_count_for_page(current_page);
        let page_sentences = self.sentence_count_for_page(current_page);
        let words_before = self.word_count_before_page(current_page);
        let total_words = self.total_word_count();
        let words_through = words_before + page_words;

        let sentences_before: usize = self
            .reader
            .page_sentence_counts
            .iter()
            .take(current_page)
            .sum();
        let sentences_through = sentences_before + page_sentences;
        let total_sentences: usize = self.reader.page_sentence_counts.iter().sum();

        let percent_start = if total_sentences == 0 {
            0.0
        } else {
            sentences_before as f32 / total_sentences as f32 * 100.0
        };
        let percent_end = if total_sentences == 0 {
            0.0
        } else {
            sentences_through as f32 / total_sentences as f32 * 100.0
        };

        let panel = column![
            text("Reading Stats").size(20.0),
            text(format!(
                "Page index: {} / {}",
                current_page + 1,
                total_pages
            )),
            text(self.audio_progress_label()),
            text(format!("Page time remaining: {}", self.page_eta_label())),
            text(format!("Book time remaining: {}", self.book_eta_label())),
            text(format!("Words on page: {}", page_words)),
            text(format!("Sentences on page: {}", page_sentences)),
            text(format!("Percent at page start: {:.3}%", percent_start)),
            text(format!("Percent at page end: {:.3}%", percent_end)),
            text(format!(
                "Words read through this page: {} / {}",
                words_through, total_words
            )),
            text(format!(
                "Sentences read through this page: {} / {}",
                sentences_through, total_sentences
            )),
        ]
        .spacing(8)
        .width(Length::Fixed(280.0));

        container(panel).padding(12).into()
    }

    fn numeric_setting_editor(&self, setting: NumericSetting) -> Element<'_, Message> {
        if self.active_numeric_setting == Some(setting) {
            let input = text_input("", &self.numeric_setting_input)
                .on_input(Message::NumericSettingInputChanged)
                .on_submit(Message::CommitNumericSettingInput)
                .padding(6)
                .size(14.0)
                .width(Length::Fixed(170.0));
            let input: Element<'_, Message> = if self.numeric_setting_input_valid(setting) {
                input.into()
            } else {
                container(input)
                    .padding(1)
                    .style(|_theme| iced::widget::container::Style {
                        border: Border {
                            color: Color::from_rgb(0.92, 0.25, 0.25),
                            width: 1.0,
                            radius: 4.0.into(),
                        },
                        ..Default::default()
                    })
                    .into()
            };
            return row![
                input,
                button("x")
                    .on_press(Message::CancelNumericSettingInput)
                    .width(Length::Shrink)
            ]
            .spacing(4)
            .align_y(Vertical::Center)
            .into();
        }

        button(
            text(self.numeric_setting_label(setting))
                .wrapping(Wrapping::None)
                .size(14.0),
        )
        .on_press(Message::BeginNumericSettingEdit(setting))
        .into()
    }

    fn numeric_setting_input_valid(&self, setting: NumericSetting) -> bool {
        self.parse_numeric_setting_input(setting, &self.numeric_setting_input)
            .is_some()
    }

    fn parse_numeric_setting_input(&self, setting: NumericSetting, raw: &str) -> Option<f32> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        let parsed = trimmed.parse::<f32>().ok()?;
        if !parsed.is_finite() {
            return None;
        }
        if Self::numeric_setting_requires_integer(setting)
            && (parsed - parsed.round()).abs() > f32::EPSILON
        {
            return None;
        }
        let (min, max) = Self::numeric_setting_bounds(setting);
        if parsed < min || parsed > max {
            return None;
        }
        Some(parsed)
    }

    fn numeric_setting_label(&self, setting: NumericSetting) -> String {
        match setting {
            NumericSetting::LineSpacing => format!("Line spacing: {:.2}", self.config.line_spacing),
            NumericSetting::PauseAfterSentence => {
                format!(
                    "Pause after sentence: {:.2} s",
                    self.config.pause_after_sentence
                )
            }
            NumericSetting::LinesPerPage => {
                format!("Lines per page: {}", self.config.lines_per_page)
            }
            NumericSetting::MarginHorizontal => {
                format!("Horizontal margin: {} px", self.config.margin_horizontal)
            }
            NumericSetting::MarginVertical => {
                format!("Vertical margin: {} px", self.config.margin_vertical)
            }
            NumericSetting::WordSpacing => format!("Word spacing: {}", self.config.word_spacing),
            NumericSetting::LetterSpacing => {
                format!("Letter spacing: {}", self.config.letter_spacing)
            }
        }
    }

    fn numeric_setting_bounds(setting: NumericSetting) -> (f32, f32) {
        match setting {
            NumericSetting::LineSpacing => (0.8, 2.5),
            NumericSetting::PauseAfterSentence => (0.0, 2.0),
            NumericSetting::LinesPerPage => (MIN_LINES_PER_PAGE as f32, MAX_LINES_PER_PAGE as f32),
            NumericSetting::MarginHorizontal => (0.0, MAX_HORIZONTAL_MARGIN as f32),
            NumericSetting::MarginVertical => (0.0, MAX_VERTICAL_MARGIN as f32),
            NumericSetting::WordSpacing => (0.0, MAX_WORD_SPACING as f32),
            NumericSetting::LetterSpacing => (0.0, MAX_LETTER_SPACING as f32),
        }
    }

    fn numeric_setting_requires_integer(setting: NumericSetting) -> bool {
        matches!(
            setting,
            NumericSetting::LinesPerPage
                | NumericSetting::MarginHorizontal
                | NumericSetting::MarginVertical
                | NumericSetting::WordSpacing
                | NumericSetting::LetterSpacing
        )
    }

    pub(super) fn tts_controls(&self) -> Element<'_, Message> {
        let play_label = if self.tts.is_preparing() {
            "Preparing..."
        } else if self
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

        let play_button = if self.tts.is_preparing() {
            Self::control_button(play_label).on_press(Message::Pause)
        } else if play_label == "Play" {
            Self::control_button(play_label).on_press(Message::Play)
        } else {
            Self::control_button(play_label).on_press(Message::Pause)
        };
        let play_from_start =
            Self::control_button("Play Page").on_press(Message::PlayFromPageStart);
        let jump_disabled = self.tts.current_sentence_idx.is_none();
        let jump_button = if jump_disabled {
            Self::control_button("Jump to Audio")
        } else {
            Self::control_button("Jump to Audio").on_press(Message::JumpToCurrentAudio)
        };
        let play_from_cursor = if let Some(idx) = self.tts.current_sentence_idx {
            Self::control_button("Play From Highlight").on_press(Message::PlayFromCursor(idx))
        } else {
            Self::control_button("Play From Highlight")
        };
        let available_width = self.controls_layout_width();
        let controls_spacing = 10.0;
        let controls_budget = (available_width - 12.0).max(0.0);
        let mut used_controls_width = estimate_button_width_px(play_label);
        let mut add_optional = |label: &str| -> bool {
            let extra = controls_spacing + estimate_button_width_px(label);
            if used_controls_width + extra <= controls_budget {
                used_controls_width += extra;
                true
            } else {
                false
            }
        };
        let show_prev_sentence = add_optional("Prev Sent");
        let show_next_sentence = add_optional("Next Sent");
        let show_play_page = add_optional("Play Page");
        let show_play_from_highlight = add_optional("Play From Highlight");
        let show_jump = add_optional("Jump to Audio");

        let mut controls_row = row![]
            .spacing(10)
            .align_y(Vertical::Center)
            .width(Length::Fill);
        if show_prev_sentence {
            controls_row = controls_row
                .push(Self::control_button("Prev Sent").on_press(Message::SeekBackward));
        }
        controls_row = controls_row.push(play_button);
        if show_next_sentence {
            controls_row =
                controls_row.push(Self::control_button("Next Sent").on_press(Message::SeekForward));
        }
        if show_play_page {
            controls_row = controls_row.push(play_from_start);
        }
        if show_play_from_highlight {
            controls_row = controls_row.push(play_from_cursor);
        }
        if show_jump {
            controls_row = controls_row.push(jump_button);
        }
        controls_row = controls_row.push(horizontal_space());
        let controls = container(controls_row)
            .height(Length::Fixed(42.0))
            .align_y(Vertical::Center)
            .width(Length::Fill);

        container(
            column![text("TTS Controls"), controls]
                .spacing(8)
                .padding(8),
        )
        .height(Length::Fixed(86.0))
        .into()
    }

    fn word_count_for_page(&self, page: usize) -> usize {
        self.reader
            .pages
            .get(page)
            .map(|content| content.split_whitespace().count())
            .unwrap_or(0)
    }

    fn word_count_before_page(&self, page: usize) -> usize {
        self.reader
            .pages
            .iter()
            .take(page)
            .map(|content| content.split_whitespace().count())
            .sum()
    }

    fn total_word_count(&self) -> usize {
        self.reader
            .pages
            .iter()
            .map(|content| content.split_whitespace().count())
            .sum()
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

    fn estimated_controls_width(&self) -> f32 {
        let mut width = self.config.window_width.max(320.0);
        if self.config.show_settings || self.show_stats {
            // Settings panel is fixed width (280) plus row spacing (16).
            width = (width - 296.0).max(0.0);
        }
        // Reader content applies 16px horizontal padding on each side.
        (width - 32.0).max(0.0)
    }

    fn controls_layout_width(&self) -> f32 {
        self.estimated_controls_width().max(320.0)
    }

    fn control_button<'a>(label: &'a str) -> iced::widget::Button<'a, Message> {
        button(text(label).wrapping(Wrapping::None))
            .width(Length::Fixed(estimate_button_width_px(label)))
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
