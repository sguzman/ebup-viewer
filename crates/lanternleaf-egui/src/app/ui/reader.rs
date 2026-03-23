use eframe::egui::{
    text::LayoutJob, Align, Color32, FontId, Label, RichText, ScrollArea, Slider, TextFormat,
    TextStyle, Ui,
};
use html5ever::{parse_document, tendril::TendrilSink};
use lanternleaf_app::contracts::{PrettyKind, ReaderSnapshot};
use lanternleaf_app::pipeline::{AppCommand, ReaderCommand};
use lanternleaf_app::state::AppState;
use lanternleaf_core::session::{ReaderSettingsPatch, SessionCommand};
use lanternleaf_core::text_utils;
use markup5ever::Attribute;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use std::time::Instant;
use tracing::trace;

use crate::app::ui::format::format_duration_secs;
use crate::app::{
    AnchorFallback, LanternLeafApp, PrettyBlock, PrettyBlockKind, PrettyPageCacheKey,
};

impl LanternLeafApp {
    pub(crate) fn render_reader_content(&mut self, ui: &mut Ui, state: &AppState) {
        if let Some(snapshot) = state.reader_document.snapshot.as_ref() {
            trace!(
                page = snapshot.current_page,
                highlight = ?snapshot.highlighted_sentence_idx,
                sentences = snapshot.sentences.len(),
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

    fn should_render_pretty(&self, snapshot: &ReaderSnapshot) -> bool {
        !snapshot.text_only_mode
            && snapshot.pretty_kind != PrettyKind::Pdf
            && snapshot.pretty_kind != PrettyKind::None
    }

    fn render_pretty_page(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        self.refresh_pretty_cache(snapshot);
        let highlight_idx = snapshot.highlighted_sentence_idx;
        let highlight_sentence = highlight_idx
            .and_then(|idx| snapshot.sentences.get(idx))
            .map(String::as_str);
        let (highlight_anchor, highlight_fallback) = match highlight_idx {
            Some(idx) => LanternLeafApp::resolve_sentence_anchor(snapshot, idx),
            None => (None, AnchorFallback::Missing),
        };
        if let Some(idx) = highlight_idx {
            trace!(
                sentence_idx = idx,
                pretty_highlight_anchor = highlight_anchor,
                pretty_highlight_fallback = highlight_fallback.label(),
                "pretty highlight resolved"
            );
        }
        let highlight_color = self.resolve_highlight_color(snapshot);
        let auto_scroll_requested = self.auto_scroll_state.consume_auto_scroll();
        ui.group(|ui| {
            ui.label("Pretty page");
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
                                            RichText::new(display_text)
                                                .strong()
                                                .size(18.0),
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
                            let decision =
                                self.auto_scroll_state.decide_scroll(idx, highlight_fallback);
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
                            if matches!(
                                decision,
                                crate::app::ScrollDecision::Scroll
                            ) {
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
            ui.label(format!(
                "Page {} / {}",
                snapshot.current_page + 1,
                snapshot.total_pages
            ));
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
            let target_idx = snapshot.highlighted_sentence_idx;
            ScrollArea::vertical()
                .id_source("sentence_list")
                .max_height(240.0)
                .show(ui, |ui| {
                for (idx, sentence) in snapshot.sentences.iter().enumerate() {
                    let selected = snapshot.highlighted_sentence_idx == Some(idx);
                    let label = format!("{:03} {}", idx + 1, sentence);
                    let response = ui.selectable_label(selected, label);
                    if response.clicked() {
                        self.execute_reader_command(ReaderCommand::Session(
                            SessionCommand::SentenceClick { sentence_idx: idx },
                        ));
                    }
                    if auto_scroll_requested && target_idx == Some(idx) {
                        let (_anchor, fallback) =
                            LanternLeafApp::resolve_sentence_anchor(snapshot, idx);
                        if matches!(self.auto_scroll_state.decide_scroll(idx, fallback), crate::app::ScrollDecision::Scroll) {
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
        if let Some(html) = snapshot.reading_html_page.as_deref() {
            let blocks = self.html_to_blocks(html);
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
            if let Some(item) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
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

    fn html_to_blocks(&self, html: &str) -> Vec<PrettyBlock> {
        let (blocks, stats) = html5_to_blocks(html);
        trace!(
            pretty_html_parse_ms = stats.parse_ms,
            pretty_html_block_count = blocks.len(),
            pretty_html_skipped_nodes = stats.skipped_nodes,
            "pretty html parse"
        );
        blocks
    }

    fn decode_html_entities(input: &str) -> String {
        input
            .replace("&nbsp;", " ")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
    }

    fn html_to_plain(&self, html: &str) -> String {
        let mut out = String::new();
        let mut in_tag = false;
        let mut tag = String::new();

        for ch in html.chars() {
            if in_tag {
                if ch == '>' {
                    in_tag = false;
                    let name = tag
                        .trim()
                        .trim_start_matches('/')
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if matches!(
                        name.as_str(),
                        "br" | "p" | "div" | "section" | "article" | "h1" | "h2" | "h3"
                            | "h4" | "h5" | "h6" | "blockquote"
                    ) {
                        out.push('\n');
                    }
                    if name == "li" {
                        out.push('\n');
                        out.push_str("• ");
                    }
                    tag.clear();
                } else {
                    tag.push(ch);
                }
                continue;
            }

            if ch == '<' {
                in_tag = true;
                continue;
            }
            out.push(ch);
        }

        let decoded = Self::decode_html_entities(&out);
        let mut normalized = String::new();
        let mut last_was_blank = false;
        for line in decoded.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                if !last_was_blank {
                    normalized.push('\n');
                    normalized.push('\n');
                    last_was_blank = true;
                }
            } else {
                normalized.push_str(trimmed);
                normalized.push('\n');
                last_was_blank = false;
            }
        }
        normalized
    }

    fn render_quick_actions_dock(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        ui.group(|ui| {
            ui.label("Quick actions");
            ui.horizontal(|ui| {
                let text_only_label = if snapshot.text_only_mode {
                    "Switch to pretty"
                } else {
                    "Switch to text-only"
                };
                if ui.button(text_only_label).clicked() {
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
                    self.auto_scroll_state.note_auto_scroll();
                }
            });
        });
    }

    pub(crate) fn render_tts_widget(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        ui.group(|ui| {
            ui.label("TTS controls");
            ui.horizontal(|ui| {
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
            ui.horizontal(|ui| {
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
            ui.horizontal(|ui| {
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
            ui.horizontal(|ui| {
                ui.label(format!(
                    "TTS progress: {:.1}%",
                    snapshot.tts.progress_pct
                ));
                ui.separator();
                ui.label(format!(
                    "Page ETA: {}",
                    format_duration_secs(snapshot.stats.page_time_remaining_secs)
                ));
                ui.separator();
                ui.label(format!(
                    "Book ETA: {}",
                    format_duration_secs(snapshot.stats.book_time_remaining_secs)
                ));
            });
            if let Some(event) = self.last_tts_runtime_event.as_ref() {
                ui.horizontal(|ui| {
                    ui.label(format!("Last TTS event: {:?}", event.kind));
                    ui.separator();
                    ui.label(event.action.as_str());
                    if let Some(message) = event.message.as_ref() {
                        ui.separator();
                        ui.label(message);
                    }
                });
            } else {
                ui.label("Last TTS event: none");
            }
        });
    }

    fn render_spoken_sentence_banner(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        let Some(idx) = snapshot.highlighted_sentence_idx else {
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
        ui.label(format!(
            "Matches: {}",
            state.reader_ui.search_matches.len()
        ));
        if ui.button("Focus search").clicked() {
            self.pending_search_focus = true;
            self.push_status("Search focus requested".to_string());
        }
    }

}

struct HtmlParseStats {
    parse_ms: u128,
    skipped_nodes: usize,
}

fn html5_to_blocks(html: &str) -> (Vec<PrettyBlock>, HtmlParseStats) {
    let start = Instant::now();
    let dom = parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut html.as_bytes());
    let parse_ms = start.elapsed().as_millis();
    let mut skipped_nodes = 0usize;
    let mut blocks = Vec::new();
    let mut anchor_idx = 0usize;

    let dom = match dom {
        Ok(dom) => dom,
        Err(err) => {
            trace!(error = ?err, "pretty html parse failed");
            return (
                blocks,
                HtmlParseStats {
                    parse_ms,
                    skipped_nodes,
                },
            );
        }
    };

    collect_html_blocks(
        &dom.document,
        &mut blocks,
        &mut anchor_idx,
        &mut skipped_nodes,
    );

    (
        blocks,
        HtmlParseStats {
            parse_ms,
            skipped_nodes,
        },
    )
}

fn collect_html_blocks(
    handle: &Handle,
    blocks: &mut Vec<PrettyBlock>,
    anchor_idx: &mut usize,
    skipped_nodes: &mut usize,
) {
    match &handle.data {
        NodeData::Element { name, attrs, .. } => {
            let tag = name.local.as_ref();
            if is_skip_container(tag) {
                *skipped_nodes = skipped_nodes.saturating_add(1);
                return;
            }
            if let Some(kind) = block_kind_for_tag(tag) {
                if tag == "img" {
                    if let Some(label) = img_alt_label(attrs) {
                        blocks.push(PrettyBlock {
                            kind: PrettyBlockKind::Paragraph,
                            text: label,
                            anchor_idx: *anchor_idx,
                        });
                        *anchor_idx = anchor_idx.saturating_add(1);
                    }
                } else {
                    let text = collect_block_text(handle);
                    let normalized = normalize_whitespace(&text);
                    blocks.push(PrettyBlock {
                        kind,
                        text: normalized,
                        anchor_idx: *anchor_idx,
                    });
                    *anchor_idx = anchor_idx.saturating_add(1);
                }
            }
            for child in handle.children.borrow().iter() {
                collect_html_blocks(child, blocks, anchor_idx, skipped_nodes);
            }
        }
        NodeData::Document => {
            for child in handle.children.borrow().iter() {
                collect_html_blocks(child, blocks, anchor_idx, skipped_nodes);
            }
        }
        _ => {
            for child in handle.children.borrow().iter() {
                collect_html_blocks(child, blocks, anchor_idx, skipped_nodes);
            }
        }
    }
}

fn collect_block_text(handle: &Handle) -> String {
    let mut out = String::new();
    for child in handle.children.borrow().iter() {
        collect_text_inner(child, &mut out, true);
    }
    out
}

fn collect_text_inner(handle: &Handle, out: &mut String, skip_blocks: bool) {
    match &handle.data {
        NodeData::Text { contents } => {
            out.push_str(&contents.borrow());
        }
        NodeData::Element { name, .. } => {
            let tag = name.local.as_ref();
            if is_skip_container(tag) {
                return;
            }
            if skip_blocks && block_kind_for_tag(tag).is_some() {
                return;
            }
            if tag == "br" {
                out.push('\n');
            }
            for child in handle.children.borrow().iter() {
                collect_text_inner(child, out, skip_blocks);
            }
        }
        _ => {
            for child in handle.children.borrow().iter() {
                collect_text_inner(child, out, skip_blocks);
            }
        }
    }
}

fn img_alt_label(attrs: &std::cell::RefCell<Vec<Attribute>>) -> Option<String> {
    let alt = attrs
        .borrow()
        .iter()
        .find(|attr| attr.name.local.as_ref() == "alt")
        .map(|attr| attr.value.to_string());
    let label = alt
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("[Image: {value}]"))
        .unwrap_or_else(|| "[Image]".to_string());
    Some(label)
}

fn is_skip_container(tag: &str) -> bool {
    matches!(tag, "head" | "style" | "script")
}

fn block_kind_for_tag(tag: &str) -> Option<PrettyBlockKind> {
    match tag {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => Some(PrettyBlockKind::Heading),
        "p" | "blockquote" | "pre" => Some(PrettyBlockKind::Paragraph),
        "li" => Some(PrettyBlockKind::ListItem),
        "img" => Some(PrettyBlockKind::Paragraph),
        _ => None,
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

fn text_format_for_kind(
    ui: &Ui,
    kind: PrettyBlockKind,
    background: Option<Color32>,
) -> TextFormat {
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
    fn html5_parser_skips_style_script_head() {
        let html = "<head><style>.a{}</style></head><body><p>One</p><script>console.log(1)</script><p>Two</p></body>";
        let (blocks, _stats) = html5_to_blocks(html);
        let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(texts, vec!["One", "Two"]);
    }

    #[test]
    fn html5_parser_preserves_document_order() {
        let html = "<h1>Title</h1><p>First</p><p>Second</p><li>Item</li>";
        let (blocks, _stats) = html5_to_blocks(html);
        let kinds: Vec<PrettyBlockKind> = blocks.iter().map(|b| b.kind).collect();
        assert_eq!(
            kinds,
            vec![
                PrettyBlockKind::Heading,
                PrettyBlockKind::Paragraph,
                PrettyBlockKind::Paragraph,
                PrettyBlockKind::ListItem
            ]
        );
        let anchors: Vec<usize> = blocks.iter().map(|b| b.anchor_idx).collect();
        assert_eq!(anchors, vec![0, 1, 2, 3]);
    }

    #[test]
    fn highlight_matching_finds_sentence() {
        let text = "First sentence. Second sentence!";
        let sentences = text_utils::split_sentences(text);
        let target = "Second sentence!";
        let matched = match_sentence_index(&sentences, target);
        assert_eq!(matched, Some(1));
    }
}
