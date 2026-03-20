mod helpers;
mod pdf;

use std::time::{Duration, Instant};

use eframe::{
    NativeOptions,
    egui::{
        self, Align, Button, CentralPanel, CollapsingHeader, Color32, Context, RichText,
        ScrollArea, SidePanel, TopBottomPanel, Ui, Vec2, Visuals,
    },
};
use helpers::{app_config_path, bootstrap_config_from_app_config, format_combo};

use crate::pdf::{
    PdfPageRegistryEntry, PdfViewportBudgetInput, PdfViewportPlanInput,
    build_pdf_viewport_render_plan, choose_pdf_viewport_evictions,
};
use lanternleaf_app::{
    AppRuntime,
    contracts::{PrettyKind, ReaderSnapshot, UiMode},
    pipeline::{AppCommand, DispatchPlan, ReaderCommand},
    shortcuts::{ShortcutAction, ShortcutScope, UiShortcutAction},
    state::AppState,
    tracing::init_tracing,
};
use lanternleaf_core::{
    config,
    session::{SessionCommand, TtsPlaybackState},
};
use tracing::{Level, info, trace};

fn main() {
    let config_path = app_config_path();
    let app_config = config::load_config(&config_path);
    let bootstrap_config = bootstrap_config_from_app_config(&app_config);
    let tracing_guard = init_tracing(&bootstrap_config.log_level);

    let runtime = AppRuntime::with_bootstrap_config(&bootstrap_config);
    let mut options = NativeOptions::default();
    options.viewport.inner_size = Some(egui::vec2(
        app_config.window_width as f32,
        app_config.window_height as f32,
    ));

    info!("Starting LanternLeaf egui shell");

    eframe::run_native(
        "LanternLeaf",
        options,
        Box::new(move |cc| {
            Box::new(LanternLeafApp::new(cc, runtime.clone(), tracing_guard))
                as Box<dyn eframe::App>
        }),
    );
}

struct LanternLeafApp {
    runtime: AppRuntime,
    tracing_guard: tracing_appender::non_blocking::WorkerGuard,
    status_log: Vec<String>,
    show_safe_quit_modal: bool,
    show_reader_confirm_modal: bool,
    pending_search_focus: bool,
    last_plan: Option<DispatchPlan>,
    auto_scroll_state: AutoScrollState,
    anchor_diagnostics: AnchorDiagnostics,
    sentence_scroll_offset: Option<Vec2>,
}

impl LanternLeafApp {
    fn new(
        _cc: &eframe::CreationContext<'_>,
        runtime: AppRuntime,
        tracing_guard: tracing_appender::non_blocking::WorkerGuard,
    ) -> Self {
        Self {
            runtime,
            tracing_guard,
            status_log: Vec::new(),
            show_safe_quit_modal: false,
            show_reader_confirm_modal: false,
            pending_search_focus: false,
            last_plan: None,
            auto_scroll_state: AutoScrollState::default(),
            anchor_diagnostics: AnchorDiagnostics::default(),
            sentence_scroll_offset: None,
        }
    }

    fn execute_command(&mut self, command: AppCommand) {
        let plan = self.runtime.plan_command(command);
        self.log_plan(&plan);
        self.last_plan = Some(plan);
    }

    fn execute_reader_command(&mut self, command: ReaderCommand) {
        self.execute_command(AppCommand::Reader(command));
    }

    fn log_plan(&mut self, plan: &DispatchPlan) {
        let entry = format!("Planned {} ({})", plan.action, plan.effects.len());
        self.status_log.push(entry);
        if self.status_log.len() > 8 {
            self.status_log.remove(0);
        }
    }

    fn handle_shortcuts(&mut self, ctx: &Context, state: &AppState) {
        let mode_scope = match state.session.session.as_ref().map(|session| session.mode) {
            Some(UiMode::Reader) => ShortcutScope::Reader,
            _ => ShortcutScope::Global,
        };
        ctx.input(|input| {
            for event in &input.events {
                if let egui::Event::Key {
                    key,
                    pressed,
                    modifiers,
                    ..
                } = event
                {
                    if !*pressed {
                        continue;
                    }
                    if let Some(combo) = format_combo(*key, *modifiers) {
                        let matches = self.runtime.shortcut_registry().matches(&combo, mode_scope);
                        for binding in matches {
                            self.execute_shortcut_action(&binding.action);
                        }
                    }
                }
            }
        });
    }

    fn execute_shortcut_action(&mut self, action: &ShortcutAction) {
        match action {
            ShortcutAction::Command(command) => self.execute_command(command.clone()),
            ShortcutAction::Ui(UiShortcutAction::FocusSearch) => {
                self.pending_search_focus = true;
                self.status_log.push("Shortcut: focus search".to_string());
            }
        }
        if self.status_log.len() > 8 {
            self.status_log.remove(0);
        }
    }

    fn render_top_bar(&mut self, ctx: &Context, state: &AppState) {
        TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("LanternLeaf (egui)");
                ui.separator();
                if ui
                    .button("Refresh recents (AppCommand::RefreshRecents)")
                    .clicked()
                {
                    self.execute_command(AppCommand::RefreshRecents { limit: Some(10) });
                }
                if ui.button("Safe quit (AppCommand::SafeQuit)").clicked() {
                    self.show_safe_quit_modal = true;
                }
                let session_mode = state.session.session.as_ref().map(|session| session.mode);
                ui.label(format!(
                    "Mode: {:?}",
                    session_mode.unwrap_or(UiMode::Starter)
                ));
                ui.label(format!("Busy: {}", state.app_shell.busy));
            });
        });
    }

    fn render_panels(
        &mut self,
        ctx: &Context,
        state: &AppState,
        reader_snapshot: Option<&ReaderSnapshot>,
    ) {
        SidePanel::left("panel_toggle").show(ctx, |ui| {
            ui.heading("Panels");
            if ui
                .button("Toggle settings (AppCommand::ToggleSettingsPanel)")
                .clicked()
            {
                self.execute_command(AppCommand::ToggleSettingsPanel);
            }
            if ui
                .button("Toggle stats (AppCommand::ToggleStatsPanel)")
                .clicked()
            {
                self.execute_command(AppCommand::ToggleStatsPanel);
            }
            if ui
                .button("Toggle TTS (AppCommand::ToggleTtsPanel)")
                .clicked()
            {
                self.execute_command(AppCommand::ToggleTtsPanel);
            }
            if let Some(panels) = state.reader_ui.panels.as_ref() {
                ui.label(format!("Settings: {}", panels.show_settings));
                ui.label(format!("Stats: {}", panels.show_stats));
                ui.label(format!("TTS: {}", panels.show_tts));
            }
            self.render_anchor_diagnostics(ui, reader_snapshot);
        });
        SidePanel::right("shortcuts").show(ctx, |ui| {
            ui.heading("Shortcut registry");
            for binding in self.runtime.shortcut_registry().bindings() {
                ui.label(format!("{} → {:?}", binding.combo, binding.action));
            }
        });
    }

    fn refresh_anchor_diagnostics(&mut self, snapshot: Option<&ReaderSnapshot>) {
        if let Some(snapshot) = snapshot {
            self.anchor_diagnostics.refresh(snapshot);
        } else {
            self.anchor_diagnostics.clear();
        }
    }

    fn render_anchor_diagnostics(&self, ui: &mut Ui, snapshot: Option<&ReaderSnapshot>) {
        CollapsingHeader::new("Anchor diagnostics")
            .id_source("anchor-diagnostics")
            .default_open(false)
            .show(ui, |ui| {
                let snapshot = match snapshot {
                    Some(snapshot) => snapshot,
                    None => {
                        ui.label("Activate a reader session to collect anchor diagnostics.");
                        return;
                    }
                };
                if self.anchor_diagnostics.is_empty() {
                    ui.label("Gathering anchor fallback data...");
                    return;
                }
                let total = self.anchor_diagnostics.total();
                ui.label(format!("Sentences scanned: {}", total));
                for (fallback, count) in self.anchor_diagnostics.fallback_counts() {
                    let pct = if total > 0 {
                        (count as f32 / total as f32) * 100.0
                    } else {
                        0.0
                    };
                    ui.horizontal(|ui| {
                        ui.label(format!("{}:", fallback.label()));
                        ui.label(format!("{} ({:.1}%)", count, pct));
                    });
                }
                if let Some(age) = self.anchor_diagnostics.last_refresh_age() {
                    ui.label(format!(
                        "Diagnostics refreshed {:.2}s ago.",
                        age.as_secs_f32()
                    ));
                }
                if let Some(elapsed) = self.auto_scroll_state.last_jump_elapsed() {
                    ui.label(format!(
                        "Last JumpToSentence {:.2}s ago (throttle window {}ms).",
                        elapsed.as_secs_f32(),
                        AutoScrollState::JUMP_THROTTLE.as_millis()
                    ));
                } else {
                    ui.label("JumpToSentence has not run yet.");
                }
                ui.label(format!(
                    "Throttled JumpToSentence attempts: {}",
                    self.auto_scroll_state.throttle_blocked()
                ));
                if snapshot.pretty_kind == PrettyKind::Pdf {
                    ui.separator();
                    ui.label("PDF anchor / OCR diagnostics:");
                    if let Some(alignment) = snapshot.pdf_ocr_alignment.as_ref() {
                        ui.label(format!("OCR quality: {:?}", alignment.quality_class));
                        ui.label(format!(
                            "Exact sentence rate: {:.1}%",
                            alignment.exact_sentence_rate * 100.0
                        ));
                        if !alignment.degraded_reasons.is_empty() {
                            ui.label(format!(
                                "OCR degraded reasons: {}",
                                alignment.degraded_reasons.join(", ")
                            ));
                        }
                    }
                    if let Some(policy) = snapshot.pdf_runtime_policy.as_ref() {
                        ui.label(format!(
                            "Highlight policy: {:?}",
                            policy.sentence_highlight_policy
                        ));
                        if !policy.degraded_reasons.is_empty() {
                            ui.label(format!(
                                "Policy degraded reasons: {}",
                                policy.degraded_reasons.join(", ")
                            ));
                        }
                    }
                }
            });
    }

    fn render_center(&mut self, ctx: &Context, state: &AppState) {
        CentralPanel::default().show(ctx, |ui| {
            match state.session.session.as_ref().map(|session| session.mode) {
                Some(UiMode::Reader) => self.render_reader_content(ui, state),
                _ => self.render_starter_content(ui),
            }
            if self.pending_search_focus {
                ui.label("Search field would be focused (shortcut handled).");
                self.pending_search_focus = false;
            }
            if let Some(plan) = self.last_plan.as_ref() {
                ui.separator();
                ui.label(format!(
                    "Last command: {} ({} effects)",
                    plan.action,
                    plan.effects.len()
                ));
            }
        });
    }

    fn render_starter_content(&mut self, ui: &mut Ui) {
        ui.heading("Starter shell");
        if ui
            .button("Return to starter (AppCommand::ReturnToStarter)")
            .clicked()
        {
            self.execute_command(AppCommand::ReturnToStarter);
        }
    }

    fn render_reader_content(&mut self, ui: &mut Ui, state: &AppState) {
        if let Some(snapshot) = state.reader_document.snapshot.as_ref() {
            trace!(
                page = snapshot.current_page,
                highlight = ?snapshot.highlighted_sentence_idx,
                sentences = snapshot.sentences.len(),
                "rendering reader shell content"
            );
            ui.heading("Reader shell");
            ui.horizontal(|ui| {
                if ui
                    .button(
                        "Play/Pause (ReaderCommand::Session(SessionCommand::TtsTogglePlayPause))",
                    )
                    .clicked()
                {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::TtsTogglePlayPause,
                    ));
                }
                if ui
                    .button("Next sentence (ReaderCommand::Session(SessionCommand::TtsSeekNext))")
                    .clicked()
                {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::TtsSeekNext,
                    ));
                }
                if ui
                    .button("Prev sentence (ReaderCommand::Session(SessionCommand::TtsSeekPrev))")
                    .clicked()
                {
                    self.execute_reader_command(ReaderCommand::Session(
                        SessionCommand::TtsSeekPrev,
                    ));
                }
            });
            ui.separator();
            self.render_reader_summary(ui, snapshot);
            ui.add_space(6.0);
            self.render_sentence_list(ui, snapshot);
            ui.add_space(6.0);
            self.render_canonical_preview(ui, snapshot);
            ui.add_space(6.0);
            self.render_pdf_diagnostics(ui, snapshot);
            ui.add_space(6.0);
            if ui
                .button("Close reader session (AppCommand::CloseReaderSession)")
                .clicked()
            {
                self.execute_command(AppCommand::CloseReaderSession);
                self.show_reader_confirm_modal = true;
            }
        } else {
            ui.heading("Reader shell");
            ui.label("No reader session currently active.");
        }
    }

    fn render_reader_summary(&self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        let anchor_hits = snapshot
            .sentence_anchor_map
            .iter()
            .filter(|value| value.is_some())
            .count();
        let progress_pct = (snapshot.tts.progress_pct * 100.0).max(0.0);
        ui.horizontal(|ui| {
            ui.label(format!(
                "Page {}/{}",
                snapshot.current_page + 1,
                snapshot.total_pages
            ));
            ui.separator();
            ui.label(format!(
                "Mode: {}",
                if snapshot.text_only_mode {
                    "text-only".to_string()
                } else {
                    format!("pretty ({:?})", snapshot.pretty_kind)
                }
            ));
            ui.separator();
            ui.label(format!(
                "TTS: {:?} ({:.0}% progress)",
                snapshot.tts.state, progress_pct
            ));
        });
        ui.horizontal(|ui| {
            let highlighted = snapshot
                .highlighted_sentence_idx
                .map(|idx| format!("{}", idx + 1))
                .unwrap_or_else(|| "none".to_string());
            ui.label(format!("Highlighted sentence: {}", highlighted));
            ui.separator();
            ui.label(format!("Search matches: {}", snapshot.search_matches.len()));
            ui.separator();
            ui.label(format!(
                "Anchors mapped: {}/{}",
                anchor_hits,
                snapshot.sentence_anchor_map.len()
            ));
        });
    }

    fn render_sentence_list(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        if snapshot.sentences.is_empty() {
            ui.label("No sentences available for this page.");
            return;
        }
        let highlight_color = Self::sentence_highlight_color(snapshot);
        let anchor_hits = snapshot
            .sentence_anchor_map
            .iter()
            .filter(|value| value.is_some())
            .count();
        trace!(anchor_hits = anchor_hits, "rendering sentence list");
        let anchor_info = self.anchor_diagnostics.entries().to_vec();
        let auto_scroll_enabled = self.should_auto_scroll(snapshot);
        if !auto_scroll_enabled {
            self.auto_scroll_state.reset();
        }
        let auto_scroll_align = if snapshot.settings.center_spoken_sentence {
            Align::Center
        } else {
            Align::Min
        };
        let scroll_response = ScrollArea::vertical()
            .auto_shrink([false, true])
            .id_source("reader-sentence-scroll")
            .show(ui, |ui| {
                for (idx, sentence) in snapshot.sentences.iter().enumerate() {
                    let is_highlighted = snapshot.highlighted_sentence_idx == Some(idx);
                    let is_search_match = snapshot.search_matches.contains(&idx);
                    let anchor_idx = snapshot
                        .sentence_anchor_map
                        .get(idx)
                        .and_then(|value| *value);
                    let canonical_preview = anchor_idx.and_then(|anchor| {
                        snapshot
                            .canonical_sentences
                            .get(anchor)
                            .map(|text| (anchor, text))
                    });
                    let anchor_meta = anchor_info
                        .get(idx)
                        .copied()
                        .unwrap_or_else(AnchorInfo::missing);
                    let mut label_text = format!("{}: {}", idx + 1, sentence);
                    if is_search_match {
                        label_text.push_str(" (search match)");
                    }
                    let mut text = RichText::new(label_text).size(14.0);
                    if is_highlighted {
                        text = text.text_style(egui::TextStyle::Body);
                    }
                    let button = Button::new(text)
                        .fill(if is_highlighted {
                            highlight_color
                        } else {
                            ui.visuals().widgets.inactive.bg_fill
                        })
                        .wrap(true);
                    let response = ui.add(button);
                    if is_highlighted && auto_scroll_enabled {
                        match self
                            .auto_scroll_state
                            .decide_scroll(idx, anchor_meta.fallback)
                        {
                            ScrollDecision::Scroll => {
                                let scroll_alignment_label =
                                    if snapshot.settings.center_spoken_sentence {
                                        "center"
                                    } else {
                                        "top"
                                    };
                                let jump_span = tracing::span!(
                                    Level::TRACE,
                                    "JumpToSentence",
                                    budget_plan = "shell.performance_budget",
                                    anchor_path = anchor_meta.fallback.label(),
                                    target_sentence = idx,
                                    command = "reader.highlight",
                                    auto_scroll = true,
                                    scroll_alignment = scroll_alignment_label,
                                    canonical_anchor = ?anchor_meta.anchor,
                                );
                                let _enter = jump_span.enter();
                                trace!(
                                    jump_to_sentence = idx,
                                    highlight_anchor = anchor_meta.fallback.label(),
                                    canonical_anchor = ?anchor_meta.anchor,
                                    "JumpToSentence: auto-scrolling highlighted sentence"
                                );
                                self.auto_scroll_state.note_auto_scroll();
                                response.scroll_to_me(Some(auto_scroll_align));
                                self.auto_scroll_state.record(idx, anchor_meta.fallback);
                            }
                            ScrollDecision::Blocked(reason) => {
                                trace!(
                                    jump_to_sentence = idx,
                                    reason = ?reason,
                                    "JumpToSentence suppressed"
                                );
                            }
                        }
                    }
                    if response.clicked() {
                        trace!(sentence_idx = idx, anchor = ?anchor_idx, "reader sentence clicked");
                        let manual_span = tracing::span!(
                            Level::TRACE,
                            "JumpToSentence",
                            budget_plan = "shell.performance_budget",
                            anchor_path = anchor_meta.fallback.label(),
                            target_sentence = idx,
                            command = "reader.sentence_click",
                            auto_scroll = false,
                            scroll_alignment = "manual",
                            canonical_anchor = ?anchor_meta.anchor,
                        );
                        let _enter = manual_span.enter();
                        trace!(
                            jump_to_sentence = idx,
                            highlight_anchor = anchor_meta.fallback.label(),
                            canonical_anchor = ?anchor_meta.anchor,
                            "JumpToSentence: manual sentence click"
                        );
                        self.execute_reader_command(ReaderCommand::Session(
                            SessionCommand::SentenceClick { sentence_idx: idx },
                        ));
                    }
                    let fallback_label = anchor_meta.fallback.label();
                    if let Some((anchor, canonical)) = canonical_preview {
                        ui.label(
                            RichText::new(format!(
                                "anchor {} → {} ({})",
                                anchor, canonical, fallback_label
                            ))
                            .small()
                            .italics()
                            .weak(),
                        );
                    } else if let Some(anchor) = anchor_idx {
                        ui.label(
                            RichText::new(format!("anchor {} ({})", anchor, fallback_label))
                                .small()
                                .italics()
                                .weak(),
                        );
                    } else {
                        ui.label(
                            RichText::new(format!("anchor missing ({})", fallback_label))
                                .small()
                                .italics()
                                .weak(),
                        );
                    }
                    ui.separator();
                }
            });
        let offset = scroll_response.state.offset;
        let manual_scroll_delta = self
            .sentence_scroll_offset
            .map(|last| offset - last)
            .unwrap_or(Vec2::ZERO);
        let offset_changed = self
            .sentence_scroll_offset
            .map(|last| offset != last)
            .unwrap_or(false);
        self.sentence_scroll_offset = Some(offset);
        let auto_scroll_this_frame = self.auto_scroll_state.consume_auto_scroll();
        if offset_changed
            && !auto_scroll_this_frame
            && manual_scroll_delta != Vec2::ZERO
            && snapshot.highlighted_sentence_idx.is_some()
        {
            let highlighted_idx = snapshot.highlighted_sentence_idx;
            let anchor_meta = highlighted_idx
                .and_then(|idx| anchor_info.get(idx).copied())
                .unwrap_or_else(AnchorInfo::missing);
            let manual_span = tracing::span!(
                Level::TRACE,
                "JumpToSentence",
                budget_plan = "shell.performance_budget",
                anchor_path = anchor_meta.fallback.label(),
                target_sentence = ?highlighted_idx,
                command = "reader.scroll",
                auto_scroll = false,
                scroll_alignment = "manual",
                scroll_delta_y = manual_scroll_delta.y,
                canonical_anchor = ?anchor_meta.anchor,
            );
            let _enter = manual_span.enter();
            trace!(
                scroll_delta = ?manual_scroll_delta,
                highlight_anchor = anchor_meta.fallback.label(),
                highlight_idx = ?highlighted_idx,
                "JumpToSentence: manual scroll request"
            );
        }
    }

    fn should_auto_scroll(&self, snapshot: &ReaderSnapshot) -> bool {
        snapshot.settings.auto_scroll_tts && snapshot.tts.state == TtsPlaybackState::Playing
    }

    fn resolve_sentence_anchor(
        snapshot: &ReaderSnapshot,
        sentence_idx: usize,
    ) -> (Option<usize>, AnchorFallback) {
        if sentence_idx >= snapshot.sentence_anchor_map.len() {
            return (None, AnchorFallback::Missing);
        }
        if let Some(anchor_idx) = snapshot.sentence_anchor_map[sentence_idx] {
            return (Some(anchor_idx), AnchorFallback::Exact);
        }
        let mut best_distance = usize::MAX;
        let mut candidate = None;
        for (candidate_idx, entry) in snapshot.sentence_anchor_map.iter().enumerate() {
            if let Some(anchor_idx) = entry {
                let distance = sentence_idx.abs_diff(candidate_idx);
                if distance < best_distance {
                    best_distance = distance;
                    candidate = Some(*anchor_idx);
                }
            }
        }
        if let Some(anchor_idx) = candidate {
            (Some(anchor_idx), AnchorFallback::Nearest)
        } else {
            (None, AnchorFallback::Missing)
        }
    }

    fn render_canonical_preview(&self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        CollapsingHeader::new("Canonical sentences preview")
            .id_source("canonical-preview")
            .default_open(false)
            .show(ui, |ui| {
                let total = snapshot.canonical_sentences.len();
                ui.label(format!("{} canonical sentences (showing first 5)", total));
                for (idx, canonical) in snapshot.canonical_sentences.iter().enumerate() {
                    if idx >= 5 {
                        ui.label("…");
                        break;
                    }
                    ui.label(RichText::new(format!("{}: {}", idx + 1, canonical)).small());
                }
            });
    }

    fn render_pdf_diagnostics(&self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        if snapshot.pretty_kind != PrettyKind::Pdf {
            return;
        }
        CollapsingHeader::new("PDF diagnostics")
            .id_source("pdf-diagnostics")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(format!(
                    "Page {}/{}",
                    snapshot.current_page + 1,
                    snapshot.total_pages
                ));
                if let Some(classification) = snapshot.pdf_classification.as_ref() {
                    ui.label(format!(
                        "Document class: {:?} ({:.2})",
                        classification.document_class, classification.confidence
                    ));
                    ui.label(format!(
                        "OCR recommendation: {:?}",
                        classification.ocr_recommendation
                    ));
                }
                if let Some(policy) = snapshot.pdf_runtime_policy.as_ref() {
                    ui.label(format!("Text policy: {:?}", policy.text_only_policy));
                    ui.label(format!(
                        "Highlight policy: {:?}",
                        policy.sentence_highlight_policy
                    ));
                    ui.label(format!("Search policy: {:?}", policy.search_policy));
                    ui.label(format!("Policy explanation: {}", policy.explanation));
                }
                if let Some(alignment) = snapshot.pdf_ocr_alignment.as_ref() {
                    ui.label(format!("OCR source: {:?}", alignment.source_kind));
                    ui.label(format!(
                        "Mapped sentences: {}/{}",
                        alignment.mapped_sentence_count, alignment.sentence_count
                    ));
                    ui.label(format!(
                        "Exact sentence rate: {:.1}%",
                        alignment.exact_sentence_rate * 100.0
                    ));
                    if !alignment.degraded_reasons.is_empty() {
                        ui.label(format!(
                            "OCR degraded reasons: {}",
                            alignment.degraded_reasons.join(", ")
                        ));
                    }
                }
                if let Some(pipeline) = snapshot.pdf_ocr_pipeline.as_ref() {
                    ui.label(format!("OCR engine: {:?}", pipeline.engine_policy));
                    if !pipeline.fallback_decisions.is_empty() {
                        ui.label(format!(
                            "Fallbacks: {}",
                            pipeline
                                .fallback_decisions
                                .iter()
                                .map(|decision| format!("{decision:?}"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                    if !pipeline.fallback_strategy_labels.is_empty() {
                        ui.label(format!(
                            "Fallback labels: {}",
                            pipeline.fallback_strategy_labels.join(", ")
                        ));
                    }
                }
                if snapshot.total_pages == 0 {
                    ui.label("PDF viewport plan unavailable (no pages).");
                    return;
                }
                let visible_page_indexes = vec![snapshot.current_page];
                let active_tts_page_index = Self::page_index_for_global_sentence(
                    &snapshot.page_sentence_counts,
                    snapshot.tts.current_sentence_idx,
                )
                .or(Some(snapshot.current_page));
                let jump_target_page_index = snapshot
                    .highlighted_sentence_idx
                    .map(|_| snapshot.current_page);
                let plan_input = PdfViewportPlanInput {
                    total_pages: snapshot.total_pages,
                    visible_page_indexes,
                    overscan: 1,
                    active_tts_page_index,
                    jump_target_page_index,
                };
                let plan = build_pdf_viewport_render_plan(&plan_input);
                ui.label(format!(
                    "Priority pages: {}",
                    Self::format_pdf_page_list(&plan.priority_page_indexes)
                ));
                ui.label(format!(
                    "Medium-priority pages: {}",
                    Self::format_pdf_page_list(&plan.medium_priority_page_indexes)
                ));
                ui.label(format!(
                    "Low-priority pages: {}",
                    Self::format_pdf_page_list(&plan.low_priority_page_indexes)
                ));
                let entries = (0..snapshot.total_pages)
                    .map(|page_index| PdfPageRegistryEntry {
                        page_index,
                        last_touched_at: snapshot.current_page as u64 + page_index as u64 + 1,
                        rendered_zoom: Some(1.0),
                        text_layer_zoom: Some(1.0),
                    })
                    .collect::<Vec<_>>();
                let decision = choose_pdf_viewport_evictions(&PdfViewportBudgetInput {
                    entries,
                    keep_canvas_page_indexes: plan.canvas_page_indexes.clone(),
                    keep_text_layer_page_indexes: plan.text_layer_page_indexes.clone(),
                    max_canvas_pages: plan.canvas_page_indexes.len().max(1),
                    max_text_layer_pages: plan.text_layer_page_indexes.len().max(1),
                });
                if !decision.evict_canvas_page_indexes.is_empty() {
                    ui.label(format!(
                        "Canvas evictions: {}",
                        Self::format_pdf_page_list(&decision.evict_canvas_page_indexes)
                    ));
                }
                if !decision.evict_text_layer_page_indexes.is_empty() {
                    ui.label(format!(
                        "Text layer evictions: {}",
                        Self::format_pdf_page_list(&decision.evict_text_layer_page_indexes)
                    ));
                }
                let plan_span = tracing::span!(
                    Level::TRACE,
                    "PdfViewportScheduling",
                    budget_plan = "shell.performance_budget",
                    total_pages = snapshot.total_pages,
                    visible_pages = ?plan_input.visible_page_indexes,
                    active_tts_page = ?plan_input.active_tts_page_index,
                    jump_target_page = ?plan_input.jump_target_page_index,
                    ocr_quality = ?snapshot
                        .pdf_ocr_alignment
                        .as_ref()
                        .map(|alignment| alignment.quality_class),
                );
                let _enter = plan_span.enter();
                trace!(
                    render_plan = ?plan,
                    eviction = ?decision,
                    "PDF viewport scheduling recorded"
                );
            });
    }

    fn page_index_for_global_sentence(
        page_sentence_counts: &[usize],
        sentence_idx: Option<usize>,
    ) -> Option<usize> {
        let mut remaining = sentence_idx?;
        for (page_idx, &count) in page_sentence_counts.iter().enumerate() {
            if remaining < count {
                return Some(page_idx);
            }
            remaining = remaining.saturating_sub(count);
        }
        page_sentence_counts.len().checked_sub(1)
    }

    fn format_pdf_page_list(pages: &[usize]) -> String {
        if pages.is_empty() {
            "none".to_string()
        } else {
            pages
                .iter()
                .map(|idx| (idx + 1).to_string())
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    fn sentence_highlight_color(snapshot: &ReaderSnapshot) -> Color32 {
        let highlight = if snapshot.settings.theme == config::ThemeMode::Day {
            snapshot.settings.day_highlight
        } else {
            snapshot.settings.night_highlight
        };
        Self::color32_from_highlight(highlight)
    }

    fn color32_from_highlight(color: config::HighlightColor) -> Color32 {
        fn to_byte(value: f32) -> u8 {
            let clamped = value.clamp(0.0, 1.0);
            (clamped * 255.0).round() as u8
        }
        Color32::from_rgba_unmultiplied(
            to_byte(color.r),
            to_byte(color.g),
            to_byte(color.b),
            to_byte(color.a),
        )
    }

    fn render_modals(&mut self, ctx: &Context) {
        let mut show_safe_quit_modal = self.show_safe_quit_modal;
        let mut show_reader_confirm_modal = self.show_reader_confirm_modal;
        let mut safe_quit_confirmed = false;
        let mut return_confirmed = false;
        let mut close_safe_quit_modal = false;
        let mut close_reader_confirm_modal = false;

        egui::Window::new("Safe quit confirmation")
            .open(&mut show_safe_quit_modal)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Are you sure you want to quit?");
                ui.horizontal(|ui| {
                    if ui.button("Yes").clicked() {
                        safe_quit_confirmed = true;
                        close_safe_quit_modal = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close_safe_quit_modal = true;
                    }
                });
            });
        egui::Window::new("Reader close confirmation")
            .open(&mut show_reader_confirm_modal)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Return to starter after closing reader?");
                ui.horizontal(|ui| {
                    if ui.button("Confirm").clicked() {
                        return_confirmed = true;
                        close_reader_confirm_modal = true;
                    }
                    if ui.button("Dismiss").clicked() {
                        close_reader_confirm_modal = true;
                    }
                });
            });

        if close_safe_quit_modal {
            show_safe_quit_modal = false;
        }
        self.show_safe_quit_modal = show_safe_quit_modal;
        if safe_quit_confirmed {
            self.execute_command(AppCommand::SafeQuit);
        }
        if close_reader_confirm_modal {
            show_reader_confirm_modal = false;
        }
        self.show_reader_confirm_modal = show_reader_confirm_modal;
        if return_confirmed {
            self.execute_command(AppCommand::ReturnToStarter);
        }
    }

    fn render_status(&mut self, ctx: &Context) {
        TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Status log:");
                for entry in &self.status_log {
                    ui.label(entry);
                }
            });
        });
    }
}

impl eframe::App for LanternLeafApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let snapshot = self.runtime.state_snapshot();
        let reader_snapshot = snapshot.reader_document.snapshot.as_ref();
        self.refresh_anchor_diagnostics(reader_snapshot);
        ctx.set_visuals(Visuals::dark());
        self.handle_shortcuts(ctx, &snapshot);
        self.render_top_bar(ctx, &snapshot);
        self.render_panels(ctx, &snapshot, reader_snapshot);
        self.render_center(ctx, &snapshot);
        self.render_modals(ctx);
        self.render_status(ctx);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnchorFallback {
    Exact,
    Nearest,
    Missing,
}

impl AnchorFallback {
    const VARIANT_COUNT: usize = 3;
    const VARIANTS: [AnchorFallback; AnchorFallback::VARIANT_COUNT] = [
        AnchorFallback::Exact,
        AnchorFallback::Nearest,
        AnchorFallback::Missing,
    ];

    fn label(self) -> &'static str {
        match self {
            AnchorFallback::Exact => "exact",
            AnchorFallback::Nearest => "nearest",
            AnchorFallback::Missing => "missing",
        }
    }

    fn index(self) -> usize {
        match self {
            AnchorFallback::Exact => 0,
            AnchorFallback::Nearest => 1,
            AnchorFallback::Missing => 2,
        }
    }
}

#[derive(Clone, Copy)]
struct AnchorInfo {
    anchor: Option<usize>,
    fallback: AnchorFallback,
}

impl AnchorInfo {
    fn missing() -> Self {
        Self {
            anchor: None,
            fallback: AnchorFallback::Missing,
        }
    }
}

#[derive(Default)]
struct AnchorDiagnostics {
    counts: [usize; AnchorFallback::VARIANT_COUNT],
    entries: Vec<AnchorInfo>,
    last_refresh: Option<Instant>,
}

impl AnchorDiagnostics {
    fn refresh(&mut self, snapshot: &ReaderSnapshot) {
        self.entries.clear();
        self.entries.reserve(snapshot.sentences.len());
        self.counts = [0; AnchorFallback::VARIANT_COUNT];
        for idx in 0..snapshot.sentences.len() {
            let (anchor, fallback) = LanternLeafApp::resolve_sentence_anchor(snapshot, idx);
            self.entries.push(AnchorInfo { anchor, fallback });
            self.counts[fallback.index()] += 1;
        }
        self.last_refresh = Some(Instant::now());
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.counts = [0; AnchorFallback::VARIANT_COUNT];
        self.last_refresh = None;
    }

    fn entries(&self) -> &[AnchorInfo] {
        &self.entries
    }

    fn fallback_counts(&self) -> impl Iterator<Item = (AnchorFallback, usize)> + '_ {
        AnchorFallback::VARIANTS
            .iter()
            .enumerate()
            .map(|(idx, &fallback)| (fallback, self.counts[idx]))
    }

    fn total(&self) -> usize {
        self.entries.len()
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn last_refresh_age(&self) -> Option<Duration> {
        self.last_refresh.map(|instant| instant.elapsed())
    }
}

#[derive(Debug)]
enum ScrollBlockReason {
    Duplicate,
    Throttled(Duration),
}

enum ScrollDecision {
    Scroll,
    Blocked(ScrollBlockReason),
}

#[derive(Default)]
struct AutoScrollState {
    last_highlighted: Option<(usize, AnchorFallback)>,
    last_jump_at: Option<Instant>,
    throttle_blocked: usize,
    pending_auto_scroll: bool,
}

impl AutoScrollState {
    const JUMP_THROTTLE: Duration = Duration::from_millis(150);

    fn decide_scroll(&mut self, idx: usize, fallback: AnchorFallback) -> ScrollDecision {
        if self.last_highlighted == Some((idx, fallback)) {
            return ScrollDecision::Blocked(ScrollBlockReason::Duplicate);
        }
        if let Some(last) = self.last_jump_at {
            let elapsed = last.elapsed();
            if elapsed < Self::JUMP_THROTTLE {
                self.throttle_blocked = self.throttle_blocked.saturating_add(1);
                let remaining = Self::JUMP_THROTTLE - elapsed;
                return ScrollDecision::Blocked(ScrollBlockReason::Throttled(remaining));
            }
        }
        ScrollDecision::Scroll
    }

    fn note_auto_scroll(&mut self) {
        self.pending_auto_scroll = true;
    }

    fn consume_auto_scroll(&mut self) -> bool {
        let triggered = self.pending_auto_scroll;
        self.pending_auto_scroll = false;
        triggered
    }

    fn record(&mut self, idx: usize, fallback: AnchorFallback) {
        self.last_highlighted = Some((idx, fallback));
        self.last_jump_at = Some(Instant::now());
    }

    fn reset(&mut self) {
        self.last_highlighted = None;
        self.last_jump_at = None;
        self.throttle_blocked = 0;
        self.pending_auto_scroll = false;
    }

    fn throttle_blocked(&self) -> usize {
        self.throttle_blocked
    }

    fn last_jump_elapsed(&self) -> Option<Duration> {
        self.last_jump_at.map(|instant| instant.elapsed())
    }
}
