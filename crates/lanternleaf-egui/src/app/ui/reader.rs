use eframe::egui::{
    Align, Color32, FontFamily, Frame, Grid, Image, Label, RichText, ScrollArea, Slider, Stroke,
    TextFormat, Ui, text::LayoutJob,
};
use lanternleaf_app::contracts::{PrettyKind, ReaderSnapshot};
use lanternleaf_app::pipeline::{AppCommand, ReaderCommand};
use lanternleaf_app::state::AppState;
use lanternleaf_core::session::{ReaderSettingsPatch, SessionCommand};
use lanternleaf_core::text_utils;
use tracing::trace;

use crate::app::ui::format::format_duration_secs;
use crate::app::{AnchorFallback, LanternLeafApp, PrettySentenceTarget};
use crate::pretty::{
    PrettyBlock, PrettyBlockKind, PrettyPageCacheKey, PrettySourceKind, PrettySpan, PrettyStyle,
    clamp_image_size, font_id_for, html_to_blocks, markdown_to_blocks,
};

impl LanternLeafApp {
    pub(crate) fn render_reader_content(&mut self, ui: &mut Ui, state: &AppState) {
        if let Some(snapshot) = state.reader_document.snapshot.as_ref() {
            let highlighted_sentence_idx = state
                .reader_playback
                .highlighted_sentence_idx
                .or(snapshot.highlighted_sentence_idx);
            let effective_text_only = self.resolved_text_only_mode(snapshot);
            trace!(
                page = snapshot.current_page,
                highlight = ?snapshot.highlighted_sentence_idx,
                tts_audio_idx = ?snapshot.tts.current_sentence_idx,
                sentences = snapshot.sentences.len(),
                text_only = effective_text_only,
                text_only_show_original_text = snapshot.settings.text_only_show_original_text,
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
                self.render_pretty_page(ui, snapshot, highlighted_sentence_idx);
            } else {
                trace!(
                    text_only = effective_text_only,
                    pretty_kind = ?snapshot.pretty_kind,
                    "Skipping pretty view in favor of sentence list"
                );
                self.render_sentence_list(ui, snapshot, highlighted_sentence_idx);
                ui.add_space(6.0);
                self.render_canonical_preview(ui, snapshot);
            }
            self.render_spoken_sentence_banner(ui, snapshot, highlighted_sentence_idx);
            ui.add_space(6.0);
            self.render_pdf_diagnostics(ui, snapshot);
        } else {
            ui.heading("Reader shell");
            ui.label("No reader session currently active.");
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

    fn render_pretty_page(
        &mut self,
        ui: &mut Ui,
        snapshot: &ReaderSnapshot,
        effective_highlighted_sentence_idx: Option<usize>,
    ) {
        let render_started = std::time::Instant::now();
        self.refresh_pretty_cache(snapshot);
        let highlight_idx = effective_highlighted_sentence_idx;
        let canonical_highlight_idx =
            highlight_idx.map(|idx| canonical_display_index(snapshot, idx));
        let highlight_target = canonical_highlight_idx
            .and_then(|idx| self.pretty_sentence_targets.get(idx))
            .cloned()
            .flatten();
        let highlight_block_idx = highlight_target.as_ref().map(|target| target.block_index);
        let follow_requested = canonical_highlight_idx.is_some_and(|idx| {
            self.auto_scroll_state
                .pending_for(&snapshot.source_path, idx)
        });
        trace!(
            canonical_display_idx = ?canonical_highlight_idx,
            highlight_block_idx,
            mapping = ?highlight_target,
            follow_requested,
            "Resolved pretty highlight target"
        );
        trace!(
            page = snapshot.current_page,
            pretty_kind = ?snapshot.pretty_kind,
            tts_audio_idx = ?snapshot.tts.current_sentence_idx,
            highlight_sentence = ?highlight_idx,
            html_payload = snapshot.reading_html_page.is_some(),
            "render_pretty_page configuration"
        );
        let highlight_color = self.resolve_highlight_color(snapshot);
        trace!(
            renderer = "pure_egui",
            "Rendering pretty view via pretty blocks"
        );
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
                .show_viewport(ui, |ui, viewport| {
                    let max_width = 720.0;
                    let available_width = ui.available_width();
                    let margin = ((available_width - max_width) / 2.0).max(0.0);

                    ui.horizontal(|ui| {
                        ui.add_space(margin);
                        ui.vertical(|ui| {
                            ui.set_max_width(max_width);
                            let pretty_cfg = snapshot.settings.pretty;
                            let base_px = (snapshot.settings.font_size as f32
                                * pretty_cfg.base_font_scale)
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

                            let total_blocks = self.pretty_page_cache_blocks.len();
                            let overscan = 8usize;
                            let estimates = self.pretty_block_heights_for(base_px, pretty_cfg);
                            let prefix = prefix_sums(&estimates);
                            let total_height = prefix.last().copied().unwrap_or(0.0);
                            let render_window = pretty_render_window(
                                total_blocks,
                                viewport.min.y,
                                viewport.max.y,
                                &prefix,
                                overscan,
                                follow_requested.then_some(highlight_block_idx).flatten(),
                            );
                            let render_start = render_window.start;
                            let render_end = render_window.end;
                            ui.set_min_height(total_height);
                            ui.add_space(prefix.get(render_start).copied().unwrap_or(0.0));
                            let mut measured_heights = estimates.clone();
                            trace!(
                                total_blocks,
                                active_blocks = render_end.saturating_sub(render_start),
                                overscan,
                                canonical_display_idx = ?canonical_highlight_idx,
                                target_block = ?highlight_block_idx,
                                target_in_window = highlight_block_idx
                                    .is_some_and(|idx| (render_start..render_end).contains(&idx)),
                                follow_requested,
                                "Rendering bounded pretty block window"
                            );

                            for (block_i, block) in self
                                .pretty_page_cache_blocks
                                .iter()
                                .enumerate()
                                .skip(render_start)
                                .take(render_end.saturating_sub(render_start))
                            {
                                if block_i > 0 {
                                    if let PrettyBlockKind::Heading { level } = &block.kind {
                                        let extra_space = match *level {
                                            1 => pretty_cfg.block_spacing * 2.5,
                                            2 => pretty_cfg.block_spacing * 2.0,
                                            _ => pretty_cfg.block_spacing * 1.5,
                                        };
                                        ui.add_space(extra_space);
                                    }
                                }

                                let mut response = None;
                                let highlight_matched = highlight_block_idx == Some(block_i);
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
                                            snapshot.settings.line_spacing,
                                        );
                                        response = Some(ui.add(Label::new(job).wrap(true)));
                                    }
                                    PrettyBlockKind::Paragraph | PrettyBlockKind::BlockQuote => {
                                        let text_color =
                                            if matches!(block.kind, PrettyBlockKind::BlockQuote) {
                                                ui.visuals().weak_text_color()
                                            } else {
                                                ui.visuals().text_color()
                                            };
                                        let job = if highlight_matched {
                                            if let Some(target) = highlight_target.as_ref() {
                                                spans_to_job_with_sentence_target(
                                                    ui,
                                                    &block.spans,
                                                    base_px,
                                                    text_color,
                                                    target,
                                                    highlight_color,
                                                    regular_family.clone(),
                                                    bold_family.clone(),
                                                    mono_regular.clone(),
                                                    mono_bold.clone(),
                                                    pretty_cfg,
                                                    snapshot.settings.line_spacing,
                                                )
                                            } else {
                                                spans_to_job_with_base(
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
                                                    snapshot.settings.line_spacing,
                                                )
                                            }
                                        } else {
                                            spans_to_job_with_base(
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
                                                snapshot.settings.line_spacing,
                                            )
                                        };
                                        if matches!(block.kind, PrettyBlockKind::BlockQuote) {
                                            let border_color = ui.visuals().widgets.active.bg_fill;
                                            let bg_fill =
                                                ui.visuals().widgets.noninteractive.bg_fill;
                                            Frame::none()
                                                .fill(bg_fill)
                                                .inner_margin(eframe::egui::Margin {
                                                    left: 16.0,
                                                    right: 8.0,
                                                    top: 8.0,
                                                    bottom: 8.0,
                                                })
                                                .show(ui, |ui| {
                                                    let rect = ui.max_rect();
                                                    ui.painter().line_segment(
                                                        [rect.left_top(), rect.left_bottom()],
                                                        Stroke::new(3.0, border_color),
                                                    );
                                                    response =
                                                        Some(ui.add(Label::new(job).wrap(true)));
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
                                        let indent =
                                            pretty_cfg.list_indent * (*depth as f32).max(1.0);
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
                                                snapshot.settings.line_spacing,
                                            );
                                            response = Some(ui.add(Label::new(job).wrap(true)));
                                        });
                                    }
                                    PrettyBlockKind::HorizontalRule => {
                                        ui.add_space(pretty_cfg.hr_margin);
                                        let (rect, _) = ui.allocate_exact_size(
                                            eframe::egui::vec2(
                                                ui.available_width(),
                                                pretty_cfg.hr_thickness,
                                            ),
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
                                        let bg = ui.visuals().extreme_bg_color.linear_multiply(
                                            pretty_cfg.code_bg_alpha.clamp(0.0, 1.0),
                                        );
                                        Frame::none()
                                            .fill(bg)
                                            .rounding(4.0)
                                            .inner_margin(eframe::egui::Margin::symmetric(
                                                12.0, 10.0,
                                            ))
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
                                                    snapshot.settings.line_spacing,
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
                                            (ui.available_width()
                                                * (pretty_cfg.image_max_width_pct / 100.0))
                                                as u32,
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
                                                Image::new(&texture).fit_to_exact_size(
                                                    eframe::egui::vec2(size[0], size[1]),
                                                ),
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
                                        let stripe = ui.visuals().faint_bg_color.linear_multiply(
                                            pretty_cfg.table_stripe_alpha.clamp(0.0, 1.0),
                                        );
                                        let border = ui
                                            .visuals()
                                            .widgets
                                            .noninteractive
                                            .bg_stroke
                                            .color
                                            .linear_multiply(
                                                pretty_cfg.table_border_alpha.clamp(0.0, 1.0),
                                            );
                                        Frame::none()
                                            .stroke(Stroke::new(1.0, border))
                                            .inner_margin(eframe::egui::Margin::symmetric(
                                                pretty_cfg.table_cell_padding,
                                                pretty_cfg.table_cell_padding,
                                            ))
                                            .show(ui, |ui| {
                                                Grid::new(format!("pretty_table_{}", block_i))
                                                    .spacing([
                                                        pretty_cfg.table_cell_padding,
                                                        pretty_cfg.table_cell_padding,
                                                    ])
                                                    .striped(true)
                                                    .show(ui, |ui| {
                                                        for (row_i, row) in rows.iter().enumerate()
                                                        {
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
                                                                    snapshot.settings.line_spacing,
                                                                );
                                                                let cell_frame = if row_i % 2 == 1 {
                                                                    Frame::none().fill(stripe)
                                                                } else {
                                                                    Frame::none()
                                                                };
                                                                cell_frame.show(ui, |ui| {
                                                                    ui.add(
                                                                        Label::new(job).wrap(true),
                                                                    );
                                                                });
                                                            }
                                                            ui.end_row();
                                                        }
                                                    });
                                            });
                                    }
                                }

                                if follow_requested
                                    && highlight_matched
                                    && canonical_highlight_idx.is_some()
                                {
                                    if let Some(response) = response.as_ref() {
                                        let idx = canonical_highlight_idx.unwrap_or_default();
                                        let decision = self.auto_scroll_state.decide_scroll(
                                            &snapshot.source_path,
                                            idx,
                                            AnchorFallback::Exact,
                                        );
                                        if matches!(decision, crate::app::ScrollDecision::Scroll) {
                                            let align = if snapshot.settings.center_spoken_sentence
                                            {
                                                Align::Center
                                            } else {
                                                Align::Min
                                            };
                                            response.scroll_to_me(Some(align));
                                            self.auto_scroll_state.record(
                                                &snapshot.source_path,
                                                idx,
                                                AnchorFallback::Exact,
                                            );
                                            trace!(
                                                source_path = %snapshot.source_path,
                                                canonical_display_idx = idx,
                                                target_block = block_i,
                                                follow_state = "scrolled",
                                                "Committed canonical pretty follow scroll"
                                            );
                                        }
                                    }
                                }

                                let spacing = match block.kind {
                                    PrettyBlockKind::Paragraph | PrettyBlockKind::BlockQuote => {
                                        pretty_cfg.paragraph_spacing
                                    }
                                    PrettyBlockKind::ListItem { .. } => {
                                        pretty_cfg.list_item_spacing
                                    }
                                    _ => pretty_cfg.block_spacing,
                                };
                                if let Some(response) = response.as_ref() {
                                    measured_heights[block_i] =
                                        (response.rect.height() + spacing.max(0.0)).max(1.0);
                                }
                                ui.add_space(spacing.max(0.0));
                            }
                            ui.add_space(
                                total_height
                                    - prefix.get(render_end).copied().unwrap_or(total_height),
                            );
                            self.pretty_block_heights = measured_heights;
                        });
                    });
                });
        });
        trace!(
            elapsed_ms = render_started.elapsed().as_millis(),
            total_blocks = self.pretty_page_cache_blocks.len(),
            "Finished bounded pretty render"
        );
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

            if let Some(config) = self.shell_state.bootstrap.as_ref().map(|b| &b.config) {
                if let Some(url) = config.remote_url.as_ref() {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(format!("Remote sync: {}", url));
                        if self.shell_state.last_playback_update_at > 0 {
                            ui.label(format!(
                                "(Last: {} ms)",
                                self.shell_state.last_playback_update_at
                            ));
                        }
                    });
                }
            }
        });
    }

    fn render_sentence_list(
        &mut self,
        ui: &mut Ui,
        snapshot: &ReaderSnapshot,
        effective_highlighted_sentence_idx: Option<usize>,
    ) {
        ui.group(|ui| {
            ui.label("Sentence list");
            if snapshot.sentences.is_empty() {
                ui.label("No sentence data available.");
                return;
            }
            let target_idx = effective_highlighted_sentence_idx;
            let target_canonical_idx = target_idx.map(|idx| canonical_display_index(snapshot, idx));
            let auto_scroll_requested = target_canonical_idx.is_some_and(|idx| {
                self.auto_scroll_state
                    .pending_for(&snapshot.source_path, idx)
            });
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
                            self.auto_scroll_state.request_cursor(
                                snapshot.source_path.clone(),
                                canonical_display_index(snapshot, idx),
                            );
                        }
                        if auto_scroll_requested && target_idx == Some(idx) {
                            let (_anchor, fallback) =
                                LanternLeafApp::resolve_sentence_anchor(snapshot, idx);
                            if matches!(
                                self.auto_scroll_state.decide_scroll(
                                    &snapshot.source_path,
                                    target_canonical_idx.unwrap_or(idx),
                                    fallback,
                                ),
                                crate::app::ScrollDecision::Scroll
                            ) {
                                let align = if snapshot.settings.center_spoken_sentence {
                                    Align::Center
                                } else {
                                    Align::Min
                                };
                                response.scroll_to_me(Some(align));
                                self.auto_scroll_state.record(
                                    &snapshot.source_path,
                                    target_canonical_idx.unwrap_or(idx),
                                    fallback,
                                );
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
        self.pretty_sentence_targets =
            aligned_targets_for_snapshot(snapshot, &self.pretty_page_cache_blocks);
        self.pretty_block_heights.clear();
        self.pretty_page_cache_key = Some(key);
    }

    fn pretty_block_heights_for(
        &self,
        base_px: f32,
        pretty_cfg: lanternleaf_core::config::PrettyUiConfig,
    ) -> Vec<f32> {
        self.pretty_page_cache_blocks
            .iter()
            .enumerate()
            .map(|(idx, block)| {
                self.pretty_block_heights
                    .get(idx)
                    .copied()
                    .filter(|height| *height > 0.0)
                    .unwrap_or_else(|| estimated_block_height(block, base_px, pretty_cfg))
            })
            .collect()
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
                    let state = self.runtime.state_snapshot();
                    let local_idx = state
                        .reader_playback
                        .highlighted_sentence_idx
                        .or(snapshot.highlighted_sentence_idx);
                    let page = state
                        .reader_playback
                        .playback
                        .as_ref()
                        .map(|playback| playback.current_page)
                        .unwrap_or(snapshot.current_page);
                    let canonical_idx = local_idx.map(|idx| {
                        snapshot
                            .page_sentence_counts
                            .iter()
                            .take(page)
                            .sum::<usize>()
                            .saturating_add(idx)
                    });
                    self.auto_scroll_state
                        .request_jump(snapshot.source_path.clone(), canonical_idx);
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

    fn render_spoken_sentence_banner(
        &mut self,
        ui: &mut Ui,
        snapshot: &ReaderSnapshot,
        effective_highlighted_sentence_idx: Option<usize>,
    ) {
        let background = self.resolve_highlight_color(snapshot);
        ui.group(|ui| {
            ui.label("TTS vs highlight");
            let highlighted = effective_highlighted_sentence_idx
                .and_then(|idx| snapshot.sentences.get(idx))
                .map(|text| text.as_str())
                .unwrap_or("None");
            let spoken = snapshot
                .tts_current_sentence_text
                .as_deref()
                .unwrap_or("None");
            ui.label(
                RichText::new(format!("Highlighted: {highlighted}")).background_color(background),
            );
            ui.label(format!("Spoken: {spoken}"));
        });
    }

    fn resolve_highlight_color(&self, snapshot: &ReaderSnapshot) -> Color32 {
        let theme = self.theme_override.unwrap_or(snapshot.settings.theme);
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
    let text = block_text(block);
    let sentences = text_utils::split_sentences(&text);
    match_sentence_index(&sentences, target_sentence).is_some()
}

fn block_text(block: &PrettyBlock) -> String {
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
    text
}

fn pretty_render_window(
    total_blocks: usize,
    viewport_min_y: f32,
    viewport_max_y: f32,
    prefix: &[f32],
    overscan: usize,
    target_block: Option<usize>,
) -> std::ops::Range<usize> {
    if total_blocks == 0 {
        return 0..0;
    }
    if let Some(target) = target_block {
        let start = target.saturating_sub(overscan);
        return start..(target + overscan + 1).min(total_blocks);
    }
    let visible_start = prefix
        .partition_point(|offset| *offset <= viewport_min_y)
        .saturating_sub(1)
        .min(total_blocks);
    let visible_end = prefix
        .partition_point(|offset| *offset < viewport_max_y)
        .min(total_blocks);
    visible_start.saturating_sub(overscan)..(visible_end + overscan).min(total_blocks)
}

fn estimated_block_height(
    block: &PrettyBlock,
    base_px: f32,
    pretty_cfg: lanternleaf_core::config::PrettyUiConfig,
) -> f32 {
    let content = match block.kind {
        PrettyBlockKind::Heading { level } => {
            base_px * heading_size(base_px, level, pretty_cfg) / base_px.max(1.0) * 1.8
        }
        PrettyBlockKind::CodeBlock => base_px * pretty_cfg.code_font_scale * 3.0,
        PrettyBlockKind::Image => pretty_cfg.image_max_height_px.min(320.0),
        PrettyBlockKind::Table => base_px * 4.0,
        PrettyBlockKind::ListItem { .. } => base_px * 2.1,
        PrettyBlockKind::BlockQuote => base_px * 2.6,
        PrettyBlockKind::Paragraph => base_px * 1.8,
        PrettyBlockKind::HorizontalRule => pretty_cfg.hr_margin * 2.0 + pretty_cfg.hr_thickness,
    };
    (content + pretty_cfg.paragraph_spacing.max(0.0) + 12.0).max(28.0)
}

fn prefix_sums(heights: &[f32]) -> Vec<f32> {
    let mut prefix = Vec::with_capacity(heights.len() + 1);
    prefix.push(0.0);
    for height in heights {
        prefix.push(prefix.last().copied().unwrap_or(0.0) + height.max(1.0));
    }
    prefix
}

fn canonical_display_index(snapshot: &ReaderSnapshot, local_idx: usize) -> usize {
    snapshot
        .page_sentence_counts
        .iter()
        .take(snapshot.current_page)
        .sum::<usize>()
        .saturating_add(local_idx)
}

fn aligned_targets_for_snapshot(
    snapshot: &ReaderSnapshot,
    blocks: &[PrettyBlock],
) -> Vec<Option<PrettySentenceTarget>> {
    if snapshot.pretty_kind == PrettyKind::Html {
        return align_canonical_sentences(&snapshot.canonical_sentences, blocks);
    }
    let local = align_canonical_sentences(&snapshot.sentences, blocks);
    let total = snapshot
        .canonical_sentences
        .len()
        .max(snapshot.page_sentence_counts.iter().sum::<usize>());
    let page_base = snapshot
        .page_sentence_counts
        .iter()
        .take(snapshot.current_page)
        .sum::<usize>();
    let mut targets = vec![None; total];
    for (local_idx, target) in local.into_iter().enumerate() {
        if let Some(slot) = targets.get_mut(page_base + local_idx) {
            *slot = target;
        }
    }
    targets
}

fn align_canonical_sentences(
    canonical: &[String],
    blocks: &[PrettyBlock],
) -> Vec<Option<PrettySentenceTarget>> {
    let pretty: Vec<(usize, usize, String, Option<usize>, Option<usize>)> = blocks
        .iter()
        .enumerate()
        .flat_map(|(block_index, block)| {
            let text = block_text(block);
            sentence_ranges(&text).into_iter().enumerate().map(
                move |(local_sentence_index, (sentence, start, end))| {
                    (
                        block_index,
                        local_sentence_index,
                        normalize_for_match(&sentence),
                        start,
                        end,
                    )
                },
            )
        })
        .collect();
    let mut targets = vec![None; canonical.len()];
    let mut pretty_cursor = 0usize;
    for (canonical_idx, sentence) in canonical.iter().enumerate() {
        let normalized = normalize_for_match(sentence);
        if normalized.is_empty() {
            continue;
        }
        let lookahead_end = (pretty_cursor + 12).min(pretty.len());
        let Some(found) = pretty[pretty_cursor..lookahead_end]
            .iter()
            .position(|(_, _, candidate, _, _)| *candidate == normalized)
            .map(|offset| pretty_cursor + offset)
        else {
            continue;
        };
        let (block_index, local_sentence_index, _, text_start, text_end) = &pretty[found];
        targets[canonical_idx] = Some(PrettySentenceTarget {
            block_index: *block_index,
            local_sentence_index: *local_sentence_index,
            text_start: *text_start,
            text_end: *text_end,
            source: if found == pretty_cursor {
                "exact-sequential"
            } else {
                "exact-lookahead"
            },
        });
        pretty_cursor = found + 1;
    }
    targets
}

fn sentence_ranges(text: &str) -> Vec<(String, Option<usize>, Option<usize>)> {
    let sentences = text_utils::split_sentences(text);
    let mut cursor = 0usize;
    sentences
        .into_iter()
        .map(|sentence| {
            let trimmed = sentence.trim();
            let start = text[cursor..].find(trimmed).map(|offset| cursor + offset);
            let end = start.map(|value| value + trimmed.len());
            if let Some(end) = end {
                cursor = end;
            }
            (sentence, start, end)
        })
        .collect()
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
    line_spacing_scale: f32,
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
        line_spacing_scale,
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
    line_spacing_scale: f32,
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
        format.line_height = Some(base_px * line_spacing_scale);
        job.append(&span.text, 0.0, format);
    }
    job
}

fn spans_to_job_with_sentence_target(
    ui: &Ui,
    spans: &[PrettySpan],
    base_px: f32,
    base_color: Color32,
    target: &PrettySentenceTarget,
    highlight: Color32,
    regular_family: FontFamily,
    bold_family: FontFamily,
    mono_regular: FontFamily,
    mono_bold: FontFamily,
    pretty_cfg: lanternleaf_core::config::PrettyUiConfig,
    line_spacing_scale: f32,
) -> LayoutJob {
    let Some(start) = target.text_start else {
        return spans_to_job_with_base(
            ui,
            spans,
            base_px,
            base_color,
            Some(highlight),
            regular_family,
            bold_family,
            mono_regular,
            mono_bold,
            pretty_cfg,
            line_spacing_scale,
        );
    };
    let end = target.text_end.unwrap_or(start);
    let mut job = LayoutJob::default();
    let mut offset = 0usize;
    for span in spans {
        let span_start = offset;
        let span_end = offset + span.text.len();
        let mut cuts = vec![0usize, span.text.len()];
        for cut in [start, end] {
            if cut > span_start && cut < span_end {
                cuts.push(cut - span_start);
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        for pair in cuts.windows(2) {
            let segment = &span.text[pair[0]..pair[1]];
            if segment.is_empty() {
                continue;
            }
            let segment_start = span_start + pair[0];
            let background =
                (segment_start < end && segment_start + segment.len() > start).then_some(highlight);
            let segment_span = PrettySpan {
                text: segment.to_string(),
                style: span.style,
            };
            append_span_to_job(
                ui,
                &mut job,
                &segment_span,
                base_px,
                base_color,
                background,
                regular_family.clone(),
                bold_family.clone(),
                mono_regular.clone(),
                mono_bold.clone(),
                pretty_cfg,
                line_spacing_scale,
            );
        }
        offset = span_end;
    }
    job
}

fn append_span_to_job(
    ui: &Ui,
    job: &mut LayoutJob,
    span: &PrettySpan,
    base_px: f32,
    base_color: Color32,
    background: Option<Color32>,
    regular_family: FontFamily,
    bold_family: FontFamily,
    mono_regular: FontFamily,
    mono_bold: FontFamily,
    pretty_cfg: lanternleaf_core::config::PrettyUiConfig,
    line_spacing_scale: f32,
) {
    let style = span.style;
    let font_id = font_id_for(
        base_px,
        style,
        regular_family,
        bold_family,
        mono_regular,
        mono_bold,
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
    format.line_height = Some(base_px * line_spacing_scale);
    job.append(&span.text, 0.0, format);
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

    #[test]
    fn large_pretty_document_uses_a_bounded_active_window() {
        let heights = vec![32.0; 1_644];
        let prefix = prefix_sums(&heights);
        let window = pretty_render_window(1_644, 12_000.0, 12_600.0, &prefix, 8, None);
        assert!(window.len() <= 40);
        assert!(window.start > 0);

        let jumped = pretty_render_window(1_644, 0.0, 640.0, &prefix, 8, Some(1_500));
        assert!(jumped.contains(&1_500));
        assert!(jumped.len() <= 17);
    }

    fn test_paragraph(block_index: usize, text: String) -> PrettyBlock {
        PrettyBlock {
            kind: PrettyBlockKind::Paragraph,
            spans: vec![PrettySpan {
                text,
                style: PrettyStyle::default(),
            }],
            code: None,
            image: None,
            table: None,
            anchor_idx: block_index,
            source_kind: PrettySourceKind::Html,
        }
    }

    #[test]
    fn canonical_alignment_preserves_duplicate_occurrence_identity_at_scale() {
        let mut blocks = Vec::with_capacity(1_500);
        let mut canonical = Vec::with_capacity(10_500);
        for block_index in 0..1_500 {
            let mut sentences = Vec::with_capacity(7);
            for local_index in 0..7 {
                let sentence = if local_index == 0 && (block_index == 10 || block_index == 1_200) {
                    "Repeated chapter sentence.".to_string()
                } else {
                    format!("Block {block_index} sentence {local_index}.")
                };
                canonical.push(sentence.clone());
                sentences.push(sentence);
            }
            blocks.push(test_paragraph(block_index, sentences.join(" ")));
        }

        let targets = align_canonical_sentences(&canonical, &blocks);
        assert_eq!(targets.len(), 10_500);
        let first_duplicate = 10 * 7;
        let second_duplicate = 1_200 * 7;
        assert_eq!(targets[first_duplicate].as_ref().unwrap().block_index, 10);
        assert_eq!(
            targets[second_duplicate].as_ref().unwrap().block_index,
            1_200
        );
        assert!(targets.windows(2).all(|pair| {
            pair[0]
                .as_ref()
                .zip(pair[1].as_ref())
                .is_none_or(|(left, right)| left.block_index <= right.block_index)
        }));
    }

    #[test]
    fn canonical_alignment_is_monotonic_and_leaves_unmapped_sentences_unmapped() {
        let blocks = vec![
            test_paragraph(0, "First   sentence. Entity & value.".to_string()),
            test_paragraph(1, "Second sentence. Repeated sentence.".to_string()),
            test_paragraph(2, "Far repeated sentence.".to_string()),
        ];
        let canonical = vec![
            "First sentence.".to_string(),
            "Entity & value.".to_string(),
            "Deliberately missing sentence.".to_string(),
            "Second sentence.".to_string(),
            "Repeated sentence.".to_string(),
            "Far repeated sentence.".to_string(),
        ];
        let targets = align_canonical_sentences(&canonical, &blocks);
        assert_eq!(targets[0].as_ref().unwrap().block_index, 0);
        assert_eq!(targets[1].as_ref().unwrap().block_index, 0);
        assert!(targets[2].is_none());
        assert_eq!(targets[3].as_ref().unwrap().block_index, 1);
        assert_eq!(targets[5].as_ref().unwrap().block_index, 2);
        assert!(
            targets[2].is_none(),
            "unmapped canonical identity must not use an anchor fallback"
        );
    }

    #[test]
    fn variable_height_prefix_keeps_target_window_bounded() {
        let heights = vec![28.0, 240.0, 42.0, 640.0, 36.0];
        let prefix = prefix_sums(&heights);
        let window = pretty_render_window(heights.len(), 240.0, 500.0, &prefix, 1, None);
        assert!(window.contains(&1));
        assert!(window.len() <= 5);
        assert_eq!(
            pretty_render_window(heights.len(), 0.0, 40.0, &prefix, 1, Some(3)),
            2..5
        );
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
