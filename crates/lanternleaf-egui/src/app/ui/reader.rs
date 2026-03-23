use eframe::egui::{Label, RichText, ScrollArea, Slider, Ui};
use lanternleaf_app::contracts::{PrettyKind, ReaderSnapshot};
use lanternleaf_app::pipeline::{AppCommand, ReaderCommand};
use lanternleaf_app::state::AppState;
use lanternleaf_core::session::{ReaderSettingsPatch, SessionCommand};
use tracing::trace;

use crate::app::ui::format::format_duration_secs;
use crate::app::{LanternLeafApp, PrettyBlock, PrettyBlockKind, PrettyPageCacheKey};

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
            self.render_quick_actions_dock(ui);
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
        ui.group(|ui| {
            ui.label("Pretty page");
            ScrollArea::vertical().show(ui, |ui| {
                for block in &self.pretty_page_cache_blocks {
                    match block.kind {
                        PrettyBlockKind::Heading => {
                            ui.add(
                                Label::new(
                                    RichText::new(&block.text)
                                        .strong()
                                        .size(18.0),
                                )
                                .wrap(true),
                            );
                        }
                        PrettyBlockKind::Paragraph => {
                            ui.add(Label::new(&block.text).wrap(true));
                        }
                        PrettyBlockKind::ListItem => {
                            ui.horizontal_wrapped(|ui| {
                                ui.label("•");
                                ui.add(Label::new(&block.text).wrap(true));
                            });
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
            ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                for (idx, sentence) in snapshot.sentences.iter().enumerate() {
                    let selected = snapshot.highlighted_sentence_idx == Some(idx);
                    let label = format!("{:03} {}", idx + 1, sentence);
                    if ui.selectable_label(selected, label).clicked() {
                        self.execute_reader_command(ReaderCommand::Session(
                            SessionCommand::SentenceClick { sentence_idx: idx },
                        ));
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
            }];
        }
        vec![PrettyBlock {
            kind: PrettyBlockKind::Paragraph,
            text: text.to_string(),
        }]
    }

    fn markdown_to_blocks(&self, markdown: &str) -> Vec<PrettyBlock> {
        let mut blocks = Vec::new();
        let mut paragraph = Vec::new();

        for line in markdown.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                if !paragraph.is_empty() {
                    blocks.push(PrettyBlock {
                        kind: PrettyBlockKind::Paragraph,
                        text: paragraph.join(" "),
                    });
                    paragraph.clear();
                }
                continue;
            }
            if let Some(stripped) = trimmed.strip_prefix('#') {
                if !paragraph.is_empty() {
                    blocks.push(PrettyBlock {
                        kind: PrettyBlockKind::Paragraph,
                        text: paragraph.join(" "),
                    });
                    paragraph.clear();
                }
                let heading = stripped.trim_start_matches('#').trim();
                if !heading.is_empty() {
                    blocks.push(PrettyBlock {
                        kind: PrettyBlockKind::Heading,
                        text: heading.to_string(),
                    });
                }
                continue;
            }
            if let Some(item) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
                if !paragraph.is_empty() {
                    blocks.push(PrettyBlock {
                        kind: PrettyBlockKind::Paragraph,
                        text: paragraph.join(" "),
                    });
                    paragraph.clear();
                }
                blocks.push(PrettyBlock {
                    kind: PrettyBlockKind::ListItem,
                    text: item.trim().to_string(),
                });
                continue;
            }
            paragraph.push(trimmed.to_string());
        }

        if !paragraph.is_empty() {
            blocks.push(PrettyBlock {
                kind: PrettyBlockKind::Paragraph,
                text: paragraph.join(" "),
            });
        }
        blocks
    }

    fn html_to_blocks(&self, html: &str) -> Vec<PrettyBlock> {
        let plain = self.html_to_plain(html);
        let mut blocks = Vec::new();
        for chunk in plain.split("\n\n") {
            let trimmed = chunk.trim();
            if trimmed.is_empty() {
                continue;
            }
            for line in trimmed.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Some(item) = line.strip_prefix("• ") {
                    blocks.push(PrettyBlock {
                        kind: PrettyBlockKind::ListItem,
                        text: item.trim().to_string(),
                    });
                } else {
                    blocks.push(PrettyBlock {
                        kind: PrettyBlockKind::Paragraph,
                        text: line.to_string(),
                    });
                }
            }
        }
        blocks
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

        let decoded = out
            .replace("&nbsp;", " ")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'");
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

    fn render_quick_actions_dock(&mut self, ui: &mut Ui) {
        ui.group(|ui| {
            ui.label("Quick actions");
            ui.horizontal(|ui| {
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
