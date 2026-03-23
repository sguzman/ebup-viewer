use eframe::egui::{
    Align, Color32, FontId, Grid, Label, RichText, ScrollArea, Sense, Slider, TextFormat,
    TextStyle, Ui, text::LayoutJob,
};
use lanternleaf_app::contracts::{PrettyKind, ReaderSnapshot};
use lanternleaf_app::pipeline::{AppCommand, ReaderCommand};
use lanternleaf_app::state::AppState;
use lanternleaf_core::session::{ReaderSettingsPatch, SessionCommand, TtsPlaybackState};
use lanternleaf_core::text_utils;
use tracing::trace;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HighlightSource {
    Canonical,
    TtsCursor,
}

#[derive(Debug, Clone, Copy)]
struct HighlightContext {
    idx: Option<usize>,
    source: HighlightSource,
}

impl HighlightContext {
    fn label(self) -> &'static str {
        match self.source {
            HighlightSource::Canonical => "canonical",
            HighlightSource::TtsCursor => "tts_cursor",
        }
    }
}

use crate::app::ui::format::format_duration_secs;
use crate::app::{
    AnchorFallback, LanternLeafApp, PrettyBlock, PrettyBlockKind, PrettyPageCacheKey,
};

impl LanternLeafApp {
    pub(crate) fn render_reader_content(&mut self, ui: &mut Ui, state: &AppState) {
        if let Some(snapshot) = state.reader_document.snapshot.as_ref() {
            let effective_text_only = self.resolved_text_only_mode(snapshot);
            let highlight_ctx = self.highlight_context(snapshot);
            trace!(
                page = snapshot.current_page,
                highlight = ?highlight_ctx.idx,
                highlight_source = highlight_ctx.label(),
                sentences = snapshot.sentences.len(),
                text_only = effective_text_only,
                "rendering reader shell content"
            );
            ui.heading("Reader shell");
            ui.horizontal(|ui| {
                if ui.button("Back to starter").clicked() {
                    self.execute_command(AppCommand::ReturnToStarter);
                }
                if ui.button("Close reader session").clicked() {
                    self.execute_command(AppCommand::CloseReaderSession);
                    self.show_reader_confirm_modal = true;
                }
            });
            self.render_quick_actions_dock(ui, snapshot);
            ui.separator();
            self.render_reader_summary(ui, snapshot);
            ui.add_space(6.0);
            if self.should_render_pretty(snapshot) {
                self.render_pretty_page(ui, snapshot);
            } else {
                trace!(
                    text_only = effective_text_only,
                    pretty_kind = ?snapshot.pretty_kind,
                    "Skipping pretty view in favor of sentence list"
                );
                self.webview_renderer.hide();
                self.render_sentence_list(ui, snapshot);
                ui.add_space(6.0);
                self.render_canonical_preview(ui, snapshot);
            }
            self.render_spoken_sentence_banner(ui, snapshot);
            ui.add_space(6.0);
            self.render_pdf_diagnostics(ui, snapshot);
        } else {
            self.webview_renderer.clear();
            ui.heading("Reader shell");
            ui.label("No reader session currently active.");
        }
    }

    fn highlight_context(&self, snapshot: &ReaderSnapshot) -> HighlightContext {
        if snapshot.tts.state == TtsPlaybackState::Playing {
            if let Some(tts_idx) = snapshot.tts.current_sentence_idx {
                return HighlightContext {
                    idx: Some(tts_idx),
                    source: HighlightSource::TtsCursor,
                };
            }
        }
        HighlightContext {
            idx: snapshot.highlighted_sentence_idx,
            source: HighlightSource::Canonical,
        }
    }

    fn should_render_pretty(&self, snapshot: &ReaderSnapshot) -> bool {
        !self.resolved_text_only_mode(snapshot)
            && snapshot.pretty_kind != PrettyKind::Pdf
            && snapshot.pretty_kind != PrettyKind::None
    }

    fn resolved_text_only_mode(&self, snapshot: &ReaderSnapshot) -> bool {
        self.text_only_override.unwrap_or(snapshot.text_only_mode)
    }

    fn resolved_highlight_idx(&self, snapshot: &ReaderSnapshot) -> Option<usize> {
        if snapshot.tts.state == TtsPlaybackState::Playing {
            snapshot
                .tts
                .current_sentence_idx
                .or(snapshot.highlighted_sentence_idx)
        } else {
            snapshot.highlighted_sentence_idx
        }
    }

    fn render_pretty_page(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        let highlight_ctx = self.highlight_context(snapshot);
        let highlight_idx = highlight_ctx.idx;
        let highlight_sentence = highlight_idx
            .and_then(|idx| snapshot.sentences.get(idx))
            .map(String::as_str);
        let (highlight_anchor, highlight_fallback) = match highlight_idx {
            Some(idx) => LanternLeafApp::resolve_sentence_anchor(snapshot, idx),
            None => (None, AnchorFallback::Missing),
        };
        trace!(
            page = snapshot.current_page,
            pretty_kind = ?snapshot.pretty_kind,
            highlight_source = highlight_ctx.label(),
            highlight_sentence = ?highlight_idx,
            html_payload = snapshot.reading_html_page.is_some(),
            frame_handles_ready = self.frame_handles.is_some(),
            "render_pretty_page configuration"
        );
        if let Some(idx) = highlight_idx {
            trace!(
                sentence_idx = idx,
                pretty_highlight_anchor = highlight_anchor,
                pretty_highlight_fallback = highlight_fallback.label(),
                "pretty highlight resolved"
            );
        }
        let highlight_color = self.resolve_highlight_color(snapshot);
        let mut scroll_anchor = None;
        let auto_scroll_requested = self.auto_scroll_state.consume_auto_scroll();
        let html_available = snapshot.reading_html_page.is_some();
        let frame_ready = self.frame_handles.is_some();
        if snapshot.pretty_kind == PrettyKind::Html && html_available && frame_ready {
            ui.group(|ui| {
                ui.label("Pretty view");
                let available = ui.available_size();
                let (rect, _) = ui.allocate_exact_size(available, Sense::hover());
                trace!(rect = ?rect, available = ?available, "Allocated HTML webview region");
                if auto_scroll_requested && highlight_anchor.is_some() && highlight_idx.is_some() {
                    let idx = highlight_idx.unwrap_or_default();
                    let decision = self
                        .auto_scroll_state
                        .decide_scroll(idx, highlight_fallback);
                    let decision_label = match decision {
                        crate::app::ScrollDecision::Scroll => "scroll",
                        crate::app::ScrollDecision::Blocked(
                            crate::app::ScrollBlockReason::Duplicate,
                        ) => "blocked_duplicate",
                        crate::app::ScrollDecision::Blocked(
                            crate::app::ScrollBlockReason::Throttled(_),
                        ) => "blocked_throttled",
                    };
                    trace!(
                        pretty_scroll_action = decision_label,
                        pretty_scroll_anchor = highlight_anchor,
                        pretty_scroll_fallback = highlight_fallback.label(),
                        "pretty scroll decision"
                    );
                    if matches!(decision, crate::app::ScrollDecision::Scroll) {
                        scroll_anchor = highlight_anchor;
                        self.auto_scroll_state.record(idx, highlight_fallback);
                    }
                }
                let highlight_css = color32_to_css(highlight_color);
                self.webview_renderer.render_html(
                    ui.ctx(),
                    self.frame_handles.as_ref(),
                    rect,
                    snapshot,
                    highlight_anchor,
                    highlight_sentence,
                    &highlight_css,
                    scroll_anchor,
                );
                trace!("Dispatched HTML payload to webview renderer");
            });
            return;
        }

        self.webview_renderer.hide();
        trace!(
            html_available = html_available,
            frame_ready = frame_ready,
            "HTML branch skipped, rendering cached text blocks"
        );
        self.refresh_pretty_cache(snapshot);
        ui.group(|ui| {
            ui.label("Pretty view");
            ScrollArea::vertical()
                .id_source("pretty_page")
                .show(ui, |ui| {
                    for block in &self.pretty_page_cache_blocks {
                        let mut response = None;
                        let mut highlight_matched = false;
                        let display_text = if block.text.is_empty() {
                            " "
                        } else {
                            block.text.as_str()
                        };
                        let highlight_job = if highlight_anchor == Some(block.anchor_idx) {
                            if let Some(sentence) = highlight_sentence {
                                let (job, matched) = build_highlight_job(
                                    ui,
                                    display_text,
                                    sentence,
                                    block.kind,
                                    highlight_color,
                                );
                                highlight_matched = matched;
                                Some(job)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        match block.kind {
                            PrettyBlockKind::Heading => {
                                if let Some(job) = highlight_job {
                                    response = Some(ui.add(Label::new(job).wrap(true)));
                                } else {
                                    response = Some(
                                        ui.add(
                                            Label::new(
                                                RichText::new(display_text).strong().size(18.0),
                                            )
                                            .wrap(true),
                                        ),
                                    );
                                }
                            }
                            PrettyBlockKind::Paragraph => {
                                if let Some(job) = highlight_job {
                                    response = Some(ui.add(Label::new(job).wrap(true)));
                                } else {
                                    response = Some(ui.add(Label::new(display_text).wrap(true)));
                                }
                            }
                            PrettyBlockKind::ListItem => {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("•");
                                    let label = if let Some(job) = highlight_job {
                                        Label::new(job).wrap(true)
                                    } else {
                                        Label::new(display_text).wrap(true)
                                    };
                                    response = Some(ui.add(label));
                                });
                            }
                        }

                        if highlight_anchor == Some(block.anchor_idx) {
                            trace!(
                                pretty_highlight_anchor = block.anchor_idx,
                                pretty_highlight_fallback = highlight_fallback.label(),
                                pretty_highlight_sentence_match = highlight_matched,
                                "pretty highlight applied"
                            );
                        }

                        if auto_scroll_requested
                            && highlight_anchor == Some(block.anchor_idx)
                            && highlight_idx.is_some()
                        {
                            if let Some(response) = response.as_ref() {
                                let idx = highlight_idx.unwrap_or_default();
                                let decision = self
                                    .auto_scroll_state
                                    .decide_scroll(idx, highlight_fallback);
                                let decision_label = match decision {
                                    crate::app::ScrollDecision::Scroll => "scroll",
                                    crate::app::ScrollDecision::Blocked(
                                        crate::app::ScrollBlockReason::Duplicate,
                                    ) => "blocked_duplicate",
                                    crate::app::ScrollDecision::Blocked(
                                        crate::app::ScrollBlockReason::Throttled(_),
                                    ) => "blocked_throttled",
                                };
                                trace!(
                                    pretty_scroll_action = decision_label,
                                    pretty_scroll_anchor = block.anchor_idx,
                                    pretty_scroll_fallback = highlight_fallback.label(),
                                    "pretty scroll decision"
                                );
                                if matches!(decision, crate::app::ScrollDecision::Scroll) {
                                    response.scroll_to_me(Some(Align::Center));
                                    self.auto_scroll_state.record(idx, highlight_fallback);
                                }
                            }
                        }

                        ui.add_space(6.0);
                    }
                });
        });
    }

    fn render_reader_summary(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        ui.group(|ui| {
            ui.label(format!("Source: {}", snapshot.source_name));
            ui.label(format!("Path: {}", snapshot.source_path));
            if snapshot.pretty_kind == PrettyKind::Html {
                ui.label("HTML view: all sections rendered as a single stream.");
            } else {
                ui.label(format!(
                    "Page {} / {}",
                    snapshot.current_page + 1,
                    snapshot.total_pages
                ));
            }
            ui.label(format!(
                "Pretty mode: {:?}{}",
                snapshot.pretty_kind,
                if snapshot.text_only_mode {
                    " (text-only)"
                } else {
                    ""
                }
            ));
            if snapshot.pretty_kind == PrettyKind::Pdf {
                let tier = self
                    .pdf_render_state
                    .confidence_tier
                    .map(|tier| tier.label())
                    .unwrap_or("unknown");
                ui.label(format!("PDF confidence: {}", tier));
            }
        });
    }

    fn render_sentence_list(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        ui.group(|ui| {
            ui.label("Sentence list");
            if snapshot.sentences.is_empty() {
                ui.label("No sentence data available.");
                return;
            }
            let auto_scroll_requested = self.auto_scroll_state.consume_auto_scroll();
            let highlight_ctx = self.highlight_context(snapshot);
            let target_idx = highlight_ctx.idx;
            ScrollArea::vertical()
                .id_source("sentence_list")
                .max_height(240.0)
                .show(ui, |ui| {
                    for (idx, sentence) in snapshot.sentences.iter().enumerate() {
                        let selected = target_idx == Some(idx);
                        let label = format!("{:03} {}", idx + 1, sentence);
                        let response = ui.selectable_label(selected, label);
                        if response.clicked() {
                            self.execute_reader_command(ReaderCommand::Session(
                                SessionCommand::SentenceClick { sentence_idx: idx },
                            ));
                            self.execute_reader_command(ReaderCommand::Session(
                                SessionCommand::TtsPlayFromHighlight,
                            ));
                            self.auto_scroll_state.request_jump();
                        }
                        if auto_scroll_requested && target_idx == Some(idx) {
                            let (_anchor, fallback) =
                                LanternLeafApp::resolve_sentence_anchor(snapshot, idx);
                            if matches!(
                                self.auto_scroll_state.decide_scroll(idx, fallback),
                                crate::app::ScrollDecision::Scroll
                            ) {
                                response.scroll_to_me(Some(Align::Center));
                                self.auto_scroll_state.record(idx, fallback);
                            }
                        }
                    }
                });
        });
    }

    fn render_canonical_preview(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        ui.group(|ui| {
            ui.label("Canonical sentences");
            if snapshot.canonical_sentences.is_empty() {
                ui.label("No canonical sentences available.");
                return;
            }
            ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                for (idx, sentence) in snapshot.canonical_sentences.iter().enumerate() {
                    ui.label(format!("{:03} {}", idx + 1, sentence));
                }
            });
        });
    }

    fn render_pdf_diagnostics(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        if snapshot.pretty_kind != PrettyKind::Pdf {
            return;
        }
        ui.group(|ui| {
            ui.label("PDF diagnostics");
            if let Some(strategy) = snapshot.pdf_sync_strategy {
                ui.label(format!("Sync strategy: {:?}", strategy));
            }
            if let Some(mode) = snapshot.pdf_geometry_mode {
                ui.label(format!("Geometry mode: {:?}", mode));
            }
            if let Some(alignment) = snapshot.pdf_ocr_alignment.as_ref() {
                ui.label(format!("OCR quality: {:?}", alignment.quality_class));
                ui.label(format!(
                    "Exact sentence rate: {:.1}%",
                    alignment.exact_sentence_rate * 100.0
                ));
            }
            if let Some(policy) = snapshot.pdf_runtime_policy.as_ref() {
                ui.label(format!(
                    "Highlight policy: {:?}",
                    policy.sentence_highlight_policy
                ));
            }
            ui.label(format!(
                "Overlays drawn: {}",
                self.pdf_render_state.rendered_overlays
            ));
        });
    }

    fn refresh_pretty_cache(&mut self, snapshot: &ReaderSnapshot) {
        let key = PrettyPageCacheKey {
            source_path: snapshot.source_path.clone(),
            page: snapshot.current_page,
            pretty_kind: snapshot.pretty_kind,
            text_only: snapshot.text_only_mode,
        };
        if self.pretty_page_cache_key.as_ref() == Some(&key) {
            return;
        }
        self.pretty_page_cache_blocks = self.build_pretty_blocks(snapshot);
        self.pretty_page_cache_key = Some(key);
    }

    fn build_pretty_blocks(&self, snapshot: &ReaderSnapshot) -> Vec<PrettyBlock> {
        if let Some(markdown) = snapshot.reading_markdown_page.as_deref() {
            let blocks = self.markdown_to_blocks(markdown);
            if !blocks.is_empty() {
                return blocks;
            }
        }
        let text = snapshot.page_text.trim();
        if text.is_empty() {
            return vec![PrettyBlock {
                kind: PrettyBlockKind::Paragraph,
                text: "No pretty content available for this page.".to_string(),
                anchor_idx: 0,
            }];
        }
        vec![PrettyBlock {
            kind: PrettyBlockKind::Paragraph,
            text: text.to_string(),
            anchor_idx: 0,
        }]
    }

    fn markdown_to_blocks(&self, markdown: &str) -> Vec<PrettyBlock> {
        let mut blocks = Vec::new();
        let mut paragraph = Vec::new();
        let mut anchor_idx = 0usize;

        for line in markdown.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                if !paragraph.is_empty() {
                    blocks.push(PrettyBlock {
                        kind: PrettyBlockKind::Paragraph,
                        text: paragraph.join(" "),
                        anchor_idx,
                    });
                    anchor_idx = anchor_idx.saturating_add(1);
                    paragraph.clear();
                }
                continue;
            }
            if let Some(stripped) = trimmed.strip_prefix('#') {
                if !paragraph.is_empty() {
                    blocks.push(PrettyBlock {
                        kind: PrettyBlockKind::Paragraph,
                        text: paragraph.join(" "),
                        anchor_idx,
                    });
                    anchor_idx = anchor_idx.saturating_add(1);
                    paragraph.clear();
                }
                let heading = stripped.trim_start_matches('#').trim();
                if !heading.is_empty() {
                    blocks.push(PrettyBlock {
                        kind: PrettyBlockKind::Heading,
                        text: heading.to_string(),
                        anchor_idx,
                    });
                    anchor_idx = anchor_idx.saturating_add(1);
                }
                continue;
            }
            if let Some(item) = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
            {
                if !paragraph.is_empty() {
                    blocks.push(PrettyBlock {
                        kind: PrettyBlockKind::Paragraph,
                        text: paragraph.join(" "),
                        anchor_idx,
                    });
                    anchor_idx = anchor_idx.saturating_add(1);
                    paragraph.clear();
                }
                blocks.push(PrettyBlock {
                    kind: PrettyBlockKind::ListItem,
                    text: item.trim().to_string(),
                    anchor_idx,
                });
                anchor_idx = anchor_idx.saturating_add(1);
                continue;
            }
            paragraph.push(trimmed.to_string());
        }

        if !paragraph.is_empty() {
            blocks.push(PrettyBlock {
                kind: PrettyBlockKind::Paragraph,
                text: paragraph.join(" "),
                anchor_idx,
            });
        }
        blocks
    }

    fn render_quick_actions_dock(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        ui.group(|ui| {
            ui.label("Quick actions");
            ui.horizontal(|ui| {
                let current_text_only = self.resolved_text_only_mode(snapshot);
                let text_only_label = if current_text_only {
                    "Switch to pretty"
                } else {
                    "Switch to text-only"
                };
                if ui.button(text_only_label).clicked() {
                    if !current_text_only {
                        self.text_only_override = Some(true);
                    } else {
                        self.text_only_override = None;
                    }
                    self.text_only_toggle_pending = false;
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::ToggleTextOnly,
                    ));
                }
                if ui.button("Play/Pause").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::TtsTogglePlayPause,
                    ));
                }
                if ui.button("Prev").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::TtsSeekPrev,
                    ));
                }
                if ui.button("Next").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::TtsSeekNext,
                    ));
                }
                if ui.button("Repeat").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::TtsRepeatSentence,
                    ));
                }
                if ui.button("Jump to highlight").clicked() {
                    self.auto_scroll_state.request_jump();
                }
            });
        });
    }

    pub(crate) fn render_tts_widget(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        ui.group(|ui| {
            ui.label("TTS controls");
            ui.horizontal_wrapped(|ui| {
                if ui.button("Play").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(SessionCommand::TtsPlay));
                }
                if ui.button("Pause").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(SessionCommand::TtsPause));
                }
                if ui.button("Stop").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(SessionCommand::TtsStop));
                }
                if ui.button("Repeat").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::TtsRepeatSentence,
                    ));
                }
            });
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button("Play from page").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::TtsPlayFromPageStart,
                    ));
                }
                if ui.button("Play from highlight").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::TtsPlayFromHighlight,
                    ));
                }
                if ui.button("Prev sentence").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::TtsSeekPrev,
                    ));
                }
                if ui.button("Next sentence").clicked() {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::TtsSeekNext,
                    ));
                }
            });
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                let mut tts_speed = snapshot.settings.tts_speed;
                if ui
                    .add(Slider::new(&mut tts_speed, 0.5..=2.5).text("Speed"))
                    .changed()
                {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::ApplySettings {
                            patch: ReaderSettingsPatch {
                                tts_speed: Some(tts_speed),
                                ..Default::default()
                            },
                        },
                    ));
                }
                let mut tts_volume = snapshot.settings.tts_volume;
                if ui
                    .add(Slider::new(&mut tts_volume, 0.0..=2.0).text("Volume"))
                    .changed()
                {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::ApplySettings {
                            patch: ReaderSettingsPatch {
                                tts_volume: Some(tts_volume),
                                ..Default::default()
                            },
                        },
                    ));
                }
            });
            ui.add_space(6.0);
            Grid::new("tts_stats_grid")
                .spacing([8.0, 4.0])
                .min_col_width(140.0)
                .show(ui, |ui| {
                    ui.label(format!("TTS progress: {:.1}%", snapshot.tts.progress_pct));
                    ui.label(format!(
                        "Page ETA: {}",
                        format_duration_secs(snapshot.stats.page_time_remaining_secs)
                    ));
                    ui.end_row();
                    ui.label(format!(
                        "Book ETA: {}",
                        format_duration_secs(snapshot.stats.book_time_remaining_secs)
                    ));
                    ui.label("");
                    ui.end_row();
                });
            ui.add_space(4.0);
            if let Some(event) = self.last_tts_runtime_event.as_ref() {
                Grid::new("tts_event_grid")
                    .spacing([6.0, 2.0])
                    .min_col_width(120.0)
                    .show(ui, |ui| {
                        ui.label(format!("Last TTS event: {:?}", event.kind));
                        ui.label(event.action.as_str());
                        if let Some(message) = event.message.as_ref() {
                            ui.label(message);
                        }
                        ui.end_row();
                    });
            } else {
                ui.label("Last TTS event: none");
            }
        });
    }

    fn render_spoken_sentence_banner(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        let highlight_ctx = self.highlight_context(snapshot);
        let Some(idx) = highlight_ctx.idx else {
            return;
        };
        let sentence = snapshot
            .sentences
            .get(idx)
            .map(|text| text.as_str())
            .unwrap_or("Unknown sentence");
        let background = self.resolve_highlight_color(snapshot);
        ui.group(|ui| {
            ui.label("Spoken sentence");
            ui.label(RichText::new(sentence).background_color(background));
        });
    }

    fn resolve_highlight_color(&self, snapshot: &ReaderSnapshot) -> Color32 {
        let theme = self.resolve_theme(&self.runtime.state_snapshot(), Some(snapshot));
        let highlight = match theme {
            lanternleaf_core::config::ThemeMode::Day => snapshot.settings.day_highlight,
            lanternleaf_core::config::ThemeMode::Night => snapshot.settings.night_highlight,
        };
        Color32::from_rgba_unmultiplied(
            (highlight.r * 255.0) as u8,
            (highlight.g * 255.0) as u8,
            (highlight.b * 255.0) as u8,
            (highlight.a * 255.0) as u8,
        )
    }

    pub(crate) fn render_stats_panel(&mut self, ui: &mut Ui, snapshot: Option<&ReaderSnapshot>) {
        let Some(snapshot) = snapshot else {
            ui.label("No reader session.");
            return;
        };
        ui.label(format!(
            "Page {} / {}",
            snapshot.stats.page_index + 1,
            snapshot.stats.total_pages
        ));
        ui.label(format!(
            "Page progress: {:.1}%",
            snapshot.stats.page_end_percent * 100.0
        ));
        ui.label(format!(
            "Book progress: {:.1}%",
            snapshot.stats.global_progress_pct * 100.0
        ));
        ui.label(format!(
            "Page ETA: {}",
            format_duration_secs(snapshot.stats.page_time_remaining_secs)
        ));
        ui.label(format!(
            "Book ETA: {}",
            format_duration_secs(snapshot.stats.book_time_remaining_secs)
        ));
    }

    pub(crate) fn render_search_panel(&mut self, ui: &mut Ui, state: &AppState) {
        ui.label(format!(
            "Query: {}",
            if state.reader_ui.search_query.is_empty() {
                "none"
            } else {
                &state.reader_ui.search_query
            }
        ));
        ui.label(format!("Matches: {}", state.reader_ui.search_matches.len()));
        if ui.button("Focus search").clicked() {
            self.pending_search_focus = true;
            self.push_status("Search focus requested".to_string());
        }
    }
}

fn color32_to_css(color: Color32) -> String {
    let alpha = (color.a() as f32) / 255.0;
    format!(
        "rgba({}, {}, {}, {:.3})",
        color.r(),
        color.g(),
        color.b(),
        alpha
    )
}

fn normalize_whitespace(input: &str) -> String {
    let mut out = String::new();
    let mut last_space = false;
    for ch in input.chars() {
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out.trim().to_string()
}

fn normalize_for_match(input: &str) -> String {
    normalize_whitespace(input).to_ascii_lowercase()
}

fn build_highlight_job(
    ui: &Ui,
    block_text: &str,
    target_sentence: &str,
    kind: PrettyBlockKind,
    highlight_color: Color32,
) -> (LayoutJob, bool) {
    let mut job = LayoutJob::default();
    let sentences = text_utils::split_sentences(block_text);
    if sentences.is_empty() {
        let format = text_format_for_kind(ui, kind, None);
        job.append(block_text, 0.0, format);
        return (job, false);
    }

    let match_idx = match_sentence_index(&sentences, target_sentence);

    for (idx, sentence) in sentences.iter().enumerate() {
        let format = if Some(idx) == match_idx {
            text_format_for_kind(ui, kind, Some(highlight_color))
        } else {
            text_format_for_kind(ui, kind, None)
        };
        job.append(sentence, 0.0, format);
    }

    (job, match_idx.is_some())
}

fn match_sentence_index(sentences: &[String], target_sentence: &str) -> Option<usize> {
    let target_norm = normalize_for_match(target_sentence);
    for (idx, sentence) in sentences.iter().enumerate() {
        if normalize_for_match(sentence) == target_norm {
            return Some(idx);
        }
    }
    None
}

fn text_format_for_kind(ui: &Ui, kind: PrettyBlockKind, background: Option<Color32>) -> TextFormat {
    let text_style = match kind {
        PrettyBlockKind::Heading => TextStyle::Heading,
        _ => TextStyle::Body,
    };
    let font_id = ui
        .style()
        .text_styles
        .get(&text_style)
        .cloned()
        .unwrap_or_else(|| match kind {
            PrettyBlockKind::Heading => FontId::proportional(18.0),
            _ => FontId::proportional(14.0),
        });
    let mut format = TextFormat {
        font_id,
        color: ui.visuals().text_color(),
        ..Default::default()
    };
    if let Some(color) = background {
        format.background = color;
    }
    format
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_matching_finds_sentence() {
        let text = "First sentence. Second sentence!";
        let sentences = text_utils::split_sentences(text);
        let target = "Second sentence!";
        let matched = match_sentence_index(&sentences, target);
        assert_eq!(matched, Some(1));
    }
}
