use eframe::egui::{
    Align, Color32, FontFamily, Frame, Grid, Label, RichText, ScrollArea, Slider, Stroke,
    TextFormat, Ui, text::LayoutJob, Image,
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
use crate::app::{AnchorFallback, LanternLeafApp};
use crate::pretty::{
    PrettyBlock, PrettyBlockKind, PrettyPageCacheKey, PrettySourceKind, PrettySpan, PrettyStyle,
    clamp_image_size, font_id_for, html_to_blocks, markdown_to_blocks,
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
                self.render_sentence_list(ui, snapshot);
                ui.add_space(6.0);
                self.render_canonical_preview(ui, snapshot);
            }
            self.render_spoken_sentence_banner(ui, snapshot);
            ui.add_space(6.0);
            self.render_pdf_diagnostics(ui, snapshot);
        } else {
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
            "render_pretty_page configuration"
        );
        let highlight_color = self.resolve_highlight_color(snapshot);
        let auto_scroll_requested = self.auto_scroll_state.consume_auto_scroll();
        trace!(renderer = "pure_egui", "Rendering pretty view via pretty blocks");
        self.refresh_pretty_cache(snapshot);
        ui.group(|ui| {
            ui.label("Pretty view");
            if !snapshot.settings.pretty.enabled {
                ui.label("Pretty rendering is disabled in config.");
                ui.add_space(6.0);
                ui.label(snapshot.page_text.trim());
                return;
            }
            ScrollArea::vertical()
                .id_source("pretty_page")
                .show(ui, |ui| {
                    let pretty_cfg = snapshot.settings.pretty;
                    let base_px = (snapshot.settings.font_size as f32 * pretty_cfg.base_font_scale)
                        .clamp(8.0, 48.0);
                    let regular_family = if self.fonts_configured {
                        FontFamily::Name("LanternLeafProportionalRegular".into())
                    } else {
                        FontFamily::Proportional
                    };
                    let bold_family = if self.fonts_configured {
                        FontFamily::Name("LanternLeafProportionalBold".into())
                    } else {
                        FontFamily::Proportional
                    };
                    let mono_regular = if self.fonts_configured {
                        FontFamily::Name("LanternLeafMonospaceRegular".into())
                    } else {
                        FontFamily::Monospace
                    };
                    let mono_bold = if self.fonts_configured {
                        FontFamily::Name("LanternLeafMonospaceBold".into())
                    } else {
                        FontFamily::Monospace
                    };

                    for (block_i, block) in self.pretty_page_cache_blocks.iter().enumerate() {
                        let mut response = None;
                        let highlight_matched = match (highlight_anchor, highlight_sentence) {
                            (Some(anchor), _) => anchor == block.anchor_idx,
                            (None, Some(sentence)) => block_contains_sentence(block, sentence),
                            (None, None) => false,
                        };
                        let block_highlight_bg = if highlight_matched {
                            Some(highlight_color)
                        } else {
                            None
                        };

                        match &block.kind {
                            PrettyBlockKind::Heading { level } => {
                                let size = heading_size(base_px, *level, pretty_cfg);
                                let mut spans = block.spans.clone();
                                for span in &mut spans {
                                    if span.style.code {
                                        span.style.code = false;
                                    }
                                    span.style.bold = true;
                                }
                                let job = spans_to_job(
                                    ui,
                                    &spans,
                                    size,
                                    block_highlight_bg,
                                    regular_family.clone(),
                                    bold_family.clone(),
                                    mono_regular.clone(),
                                    mono_bold.clone(),
                                    pretty_cfg,
                                );
                                response = Some(ui.add(Label::new(job).wrap(true)));
                            }
                            PrettyBlockKind::Paragraph | PrettyBlockKind::BlockQuote => {
                                let text_color = if matches!(block.kind, PrettyBlockKind::BlockQuote)
                                {
                                    ui.visuals().weak_text_color()
                                } else {
                                    ui.visuals().text_color()
                                };
                                let job = spans_to_job_with_base(
                                    ui,
                                    &block.spans,
                                    base_px,
                                    text_color,
                                    block_highlight_bg,
                                    regular_family.clone(),
                                    bold_family.clone(),
                                    mono_regular.clone(),
                                    mono_bold.clone(),
                                    pretty_cfg,
                                );
                                if matches!(block.kind, PrettyBlockKind::BlockQuote) {
                                    Frame::none()
                                        .stroke(Stroke::new(
                                            1.0,
                                            ui.visuals().widgets.noninteractive.bg_stroke.color,
                                        ))
                                        .inner_margin(eframe::egui::Margin::symmetric(8.0, 6.0))
                                        .show(ui, |ui| {
                                            response = Some(ui.add(Label::new(job).wrap(true)));
                                        });
                                } else {
                                    response = Some(ui.add(Label::new(job).wrap(true)));
                                }
                            }
                            PrettyBlockKind::ListItem {
                                depth,
                                ordered,
                                index,
                            } => {
                                let indent = pretty_cfg.list_indent * (*depth as f32).max(1.0);
                                ui.horizontal_wrapped(|ui| {
                                    ui.add_space(indent);
                                    let marker = if *ordered {
                                        format!("{}.", index.unwrap_or(1))
                                    } else {
                                        "•".to_string()
                                    };
                                    ui.label(marker);
                                    let job = spans_to_job(
                                        ui,
                                        &block.spans,
                                        base_px,
                                        block_highlight_bg,
                                        regular_family.clone(),
                                        bold_family.clone(),
                                        mono_regular.clone(),
                                        mono_bold.clone(),
                                        pretty_cfg,
                                    );
                                    response = Some(ui.add(Label::new(job).wrap(true)));
                                });
                            }
                            PrettyBlockKind::HorizontalRule => {
                                ui.add_space(pretty_cfg.hr_margin);
                                let (rect, _) = ui.allocate_exact_size(
                                    eframe::egui::vec2(ui.available_width(), pretty_cfg.hr_thickness),
                                    eframe::egui::Sense::hover(),
                                );
                                ui.painter().line_segment(
                                    [rect.left_center(), rect.right_center()],
                                    Stroke::new(
                                        pretty_cfg.hr_thickness,
                                        ui.visuals().widgets.noninteractive.bg_stroke.color,
                                    ),
                                );
                                ui.add_space(pretty_cfg.hr_margin);
                            }
                            PrettyBlockKind::CodeBlock => {
                                let code = block.code.as_deref().unwrap_or_default();
                                let style = PrettyStyle {
                                    code: true,
                                    ..PrettyStyle::default()
                                };
                                let spans = vec![PrettySpan {
                                    text: code.to_string(),
                                    style,
                                }];
                                let bg = ui
                                    .visuals()
                                    .extreme_bg_color
                                    .linear_multiply(pretty_cfg.code_bg_alpha.clamp(0.0, 1.0));
                                Frame::none()
                                    .fill(bg)
                                    .stroke(Stroke::new(
                                        1.0,
                                        ui.visuals()
                                            .widgets
                                            .noninteractive
                                            .bg_stroke
                                            .color
                                            .linear_multiply(
                                                pretty_cfg.code_border_alpha.clamp(0.0, 1.0),
                                            ),
                                    ))
                                    .inner_margin(eframe::egui::Margin::symmetric(8.0, 6.0))
                                    .show(ui, |ui| {
                                        let job = spans_to_job(
                                            ui,
                                            &spans,
                                            base_px * pretty_cfg.code_font_scale,
                                            block_highlight_bg,
                                            regular_family.clone(),
                                            bold_family.clone(),
                                            mono_regular.clone(),
                                            mono_bold.clone(),
                                            pretty_cfg,
                                        );
                                        response = Some(ui.add(Label::new(job).wrap(true)));
                                    });
                            }
                            PrettyBlockKind::Image => {
                                let Some(img) = block.image.as_ref() else {
                                    ui.label("[image]");
                                    continue;
                                };
                                if let Some(texture) = self.pretty_image_cache.texture_for(
                                    ui.ctx(),
                                    &img.local_path,
                                    (ui.available_width() * (pretty_cfg.image_max_width_pct / 100.0)) as u32,
                                    pretty_cfg.image_max_height_px as u32,
                                    pretty_cfg.image_cache_max_entries,
                                ) {
                                    let size = clamp_image_size(
                                        ui.available_width(),
                                        [texture.size()[0], texture.size()[1]],
                                        pretty_cfg.image_max_width_pct,
                                        pretty_cfg.image_max_height_px,
                                    );
                                    response = Some(ui.add(
                                        Image::new(&texture)
                                            .fit_to_exact_size(eframe::egui::vec2(size[0], size[1])),
                                    ));
                                } else {
                                    ui.label(
                                        img.alt
                                            .as_deref()
                                            .unwrap_or(img.src_raw.as_str())
                                            .to_string(),
                                    );
                                }
                            }
                            PrettyBlockKind::Table => {
                                let Some(rows) = block.table.as_ref() else {
                                    ui.label("[table]");
                                    continue;
                                };
                                let stripe = ui
                                    .visuals()
                                    .faint_bg_color
                                    .linear_multiply(pretty_cfg.table_stripe_alpha.clamp(0.0, 1.0));
                                let border = ui
                                    .visuals()
                                    .widgets
                                    .noninteractive
                                    .bg_stroke
                                    .color
                                    .linear_multiply(pretty_cfg.table_border_alpha.clamp(0.0, 1.0));
                                Frame::none()
                                    .stroke(Stroke::new(1.0, border))
                                    .inner_margin(eframe::egui::Margin::symmetric(
                                        pretty_cfg.table_cell_padding,
                                        pretty_cfg.table_cell_padding,
                                    ))
                                    .show(ui, |ui| {
                                        Grid::new(format!("pretty_table_{}", block_i))
                                            .spacing([pretty_cfg.table_cell_padding, pretty_cfg.table_cell_padding])
                                            .striped(true)
                                            .show(ui, |ui| {
                                                for (row_i, row) in rows.iter().enumerate() {
                                                    for cell in row {
                                                        let mut spans = cell.spans.clone();
                                                        if cell.header {
                                                            for span in &mut spans {
                                                                span.style.bold = true;
                                                            }
                                                        }
                                                        let job = spans_to_job(
                                                            ui,
                                                            &spans,
                                                            base_px,
                                                            None,
                                                            regular_family.clone(),
                                                            bold_family.clone(),
                                                            mono_regular.clone(),
                                                            mono_bold.clone(),
                                                            pretty_cfg,
                                                        );
                                                        let cell_frame = if row_i % 2 == 1 {
                                                            Frame::none().fill(stripe)
                                                        } else {
                                                            Frame::none()
                                                        };
                                                        cell_frame.show(ui, |ui| {
                                                            ui.add(Label::new(job).wrap(true));
                                                        });
                                                    }
                                                    ui.end_row();
                                                }
                                            });
                                    });
                            }
                        }

                        if auto_scroll_requested && highlight_matched && highlight_idx.is_some() {
                            if let Some(response) = response.as_ref() {
                                let idx = highlight_idx.unwrap_or_default();
                                let decision =
                                    self.auto_scroll_state.decide_scroll(idx, highlight_fallback);
                                if matches!(decision, crate::app::ScrollDecision::Scroll) {
                                    response.scroll_to_me(Some(Align::Center));
                                    self.auto_scroll_state.record(idx, highlight_fallback);
                                }
                            }
                        }

                        let spacing = match block.kind {
                            PrettyBlockKind::Paragraph | PrettyBlockKind::BlockQuote => {
                                pretty_cfg.paragraph_spacing
                            }
                            PrettyBlockKind::ListItem { .. } => pretty_cfg.list_item_spacing,
                            _ => pretty_cfg.block_spacing,
                        };
                        ui.add_space(spacing.max(0.0));
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
            page: if snapshot.pretty_kind == PrettyKind::Html {
                0
            } else {
                snapshot.current_page
            },
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
        let pretty_cfg = snapshot.settings.pretty;
        match snapshot.pretty_kind {
            PrettyKind::Markdown => {
                if let Some(markdown) = snapshot.reading_markdown_page.as_deref() {
                    let blocks = markdown_to_blocks(markdown, &snapshot.images, pretty_cfg);
                    trace_pretty_block_counts(&blocks);
                    return blocks;
                }
            }
            PrettyKind::Html => {
                if let Some(html) = snapshot.reading_html_page.as_deref() {
                    let blocks = html_to_blocks(html, &snapshot.images, pretty_cfg);
                    trace_pretty_block_counts(&blocks);
                    return blocks;
                }
            }
            _ => {}
        }

        let text = snapshot.page_text.trim();
        if text.is_empty() {
            return vec![PrettyBlock {
                kind: PrettyBlockKind::Paragraph,
                spans: vec![PrettySpan {
                    text: "No pretty content available for this page.".to_string(),
                    style: PrettyStyle::default(),
                }],
                code: None,
                image: None,
                table: None,
                anchor_idx: 0,
                source_kind: PrettySourceKind::Markdown,
            }];
        }
        vec![PrettyBlock {
            kind: PrettyBlockKind::Paragraph,
            spans: vec![PrettySpan {
                text: text.to_string(),
                style: PrettyStyle::default(),
            }],
            code: None,
            image: None,
            table: None,
            anchor_idx: 0,
            source_kind: PrettySourceKind::Markdown,
        }]
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

fn block_contains_sentence(block: &PrettyBlock, target_sentence: &str) -> bool {
    let mut text = String::new();
    match block.kind {
        PrettyBlockKind::CodeBlock => {
            if let Some(code) = block.code.as_deref() {
                text.push_str(code);
            }
        }
        _ => {
            for span in &block.spans {
                text.push_str(&span.text);
            }
        }
    }
    let sentences = text_utils::split_sentences(&text);
    match_sentence_index(&sentences, target_sentence).is_some()
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

fn heading_size(
    base_px: f32,
    level: u8,
    pretty_cfg: lanternleaf_core::config::PrettyUiConfig,
) -> f32 {
    let scale = match level {
        1 => pretty_cfg.heading_scale_h1,
        2 => pretty_cfg.heading_scale_h2,
        3 => pretty_cfg.heading_scale_h3,
        4 => pretty_cfg.heading_scale_h4,
        5 => pretty_cfg.heading_scale_h5,
        _ => pretty_cfg.heading_scale_h6,
    };
    (base_px * scale.max(0.5)).max(base_px)
}

fn spans_to_job(
    ui: &Ui,
    spans: &[PrettySpan],
    base_px: f32,
    background: Option<Color32>,
    regular_family: FontFamily,
    bold_family: FontFamily,
    mono_regular: FontFamily,
    mono_bold: FontFamily,
    pretty_cfg: lanternleaf_core::config::PrettyUiConfig,
) -> LayoutJob {
    spans_to_job_with_base(
        ui,
        spans,
        base_px,
        ui.visuals().text_color(),
        background,
        regular_family,
        bold_family,
        mono_regular,
        mono_bold,
        pretty_cfg,
    )
}

fn spans_to_job_with_base(
    ui: &Ui,
    spans: &[PrettySpan],
    base_px: f32,
    base_color: Color32,
    background: Option<Color32>,
    regular_family: FontFamily,
    bold_family: FontFamily,
    mono_regular: FontFamily,
    mono_bold: FontFamily,
    pretty_cfg: lanternleaf_core::config::PrettyUiConfig,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    for span in spans {
        let style = span.style;
        let font_id = font_id_for(
            base_px,
            style,
            regular_family.clone(),
            bold_family.clone(),
            mono_regular.clone(),
            mono_bold.clone(),
        );
        let color = style.color.unwrap_or(base_color);
        let mut format = TextFormat {
            font_id,
            color,
            italics: style.italics,
            ..Default::default()
        };
        if let Some(bg) = background {
            format.background = bg;
        } else if style.code {
            format.background = ui
                .visuals()
                .extreme_bg_color
                .linear_multiply(pretty_cfg.code_bg_alpha.clamp(0.0, 1.0));
        }
        if style.underline {
            format.underline = Stroke::new(1.0, color);
        }
        if style.strikethrough {
            format.strikethrough = Stroke::new(1.0, color);
        }
        if style.sup {
            format.valign = Align::TOP;
        }
        if style.sub {
            format.valign = Align::BOTTOM;
        }
        job.append(&span.text, 0.0, format);
    }
    job
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

fn trace_pretty_block_counts(blocks: &[PrettyBlock]) {
    let mut markdown = 0usize;
    let mut html = 0usize;
    for b in blocks {
        match b.source_kind {
            PrettySourceKind::Markdown => markdown += 1,
            PrettySourceKind::Html => html += 1,
        }
    }
    tracing::debug!(
        total = blocks.len(),
        markdown_blocks = markdown,
        html_blocks = html,
        "Pretty blocks built"
    );
}
