mod helpers;
mod pdf;

use std::{
    collections::HashMap,
    path::Path,
    time::{Duration, Instant},
};

use eframe::{
    NativeOptions,
    egui::{
        self, Align, Align2, Button, CentralPanel, CollapsingHeader, Color32, Context, FontFamily,
        FontId, Pos2, Rect, RichText, ScrollArea, Sense, SidePanel, Stroke, TopBottomPanel, Ui,
        Vec2, Visuals,
    },
};
use helpers::{app_config_path, bootstrap_config_from_app_config, format_combo};

use crate::pdf::{
    PdfPageRegistryEntry, PdfViewportBudgetDecision, PdfViewportBudgetInput, PdfViewportPlanInput,
    PdfViewportRenderPlan, build_pdf_viewport_render_plan, choose_pdf_viewport_evictions,
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
    cache, config,
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

    let _ = eframe::run_native(
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
    overlay_diagnostics: OverlayDiagnostics,
    pdf_render_state: PdfRenderState,
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
            overlay_diagnostics: OverlayDiagnostics::default(),
            pdf_render_state: PdfRenderState::default(),
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
                    let overlay_available = snapshot.pdf_ocr_alignment.is_some();
                    let overlay_highlightable_sentences = snapshot
                        .pdf_ocr_alignment
                        .as_ref()
                        .map(|alignment| alignment.highlightable_sentence_count)
                        .unwrap_or(0);
                    let overlay_budget_pages = self.pdf_render_state.overlay_budget_pages();
                    let overlay_eviction_count = self
                        .pdf_render_state
                        .decision
                        .as_ref()
                        .map(|decision| decision.evict_text_layer_page_indexes.len())
                        .unwrap_or(0);
                    if is_highlighted {
                        let highlight_page = Self::page_index_for_global_sentence(
                            &snapshot.page_sentence_counts,
                            Some(idx),
                        )
                        .unwrap_or(snapshot.current_page);
                        let overlay_rects = Self::global_sentence_index(snapshot, idx)
                            .map(|global_idx| {
                                self.pdf_render_state
                                    .overlay_rects_for_sentence(&snapshot.source_path, global_idx)
                            })
                            .unwrap_or_default();
                        self.pdf_render_state
                            .set_highlighted_page(highlight_page, Some(idx), overlay_rects);
                    }
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
                                let overlay_snapshot = self.capture_overlay_decision();
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
                                    overlay_available = overlay_available,
                                    overlay_highlightable_sentences = overlay_highlightable_sentences,
                                    overlay_budget_pages = overlay_budget_pages,
                                    overlay_eviction_count = overlay_eviction_count,
                                    overlay_budget_allowed = overlay_snapshot.allowed,
                                    overlay_budget_drawn = overlay_snapshot.overlays_drawn,
                                    highlight_page_text_layer = overlay_snapshot.highlight_page_has_text_layer,
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
                                self.overlay_diagnostics
                                    .record_jump("auto-scroll", overlay_snapshot);
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
                        let overlay_snapshot = self.capture_overlay_decision();
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
                            overlay_available = overlay_available,
                            overlay_highlightable_sentences = overlay_highlightable_sentences,
                            overlay_budget_pages = overlay_budget_pages,
                            overlay_eviction_count = overlay_eviction_count,
                            overlay_budget_allowed = overlay_snapshot.allowed,
                            overlay_budget_drawn = overlay_snapshot.overlays_drawn,
                            highlight_page_text_layer = overlay_snapshot.highlight_page_has_text_layer,
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
                        self.overlay_diagnostics
                            .record_jump("sentence-click", overlay_snapshot);
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
        let overlay_available = snapshot.pdf_ocr_alignment.is_some();
        let overlay_highlightable_sentences = snapshot
            .pdf_ocr_alignment
            .as_ref()
            .map(|alignment| alignment.highlightable_sentence_count)
            .unwrap_or(0);
        let overlay_budget_pages = self.pdf_render_state.overlay_budget_pages();
        let overlay_eviction_count = self
            .pdf_render_state
            .decision
            .as_ref()
            .map(|decision| decision.evict_text_layer_page_indexes.len())
            .unwrap_or(0);
        if offset_changed
            && !auto_scroll_this_frame
            && manual_scroll_delta != Vec2::ZERO
            && snapshot.highlighted_sentence_idx.is_some()
        {
            let highlighted_idx = snapshot.highlighted_sentence_idx;
            let anchor_meta = highlighted_idx
                .and_then(|idx| anchor_info.get(idx).copied())
                .unwrap_or_else(AnchorInfo::missing);
            let overlay_snapshot = self.capture_overlay_decision();
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
                overlay_available = overlay_available,
                overlay_highlightable_sentences = overlay_highlightable_sentences,
                overlay_budget_pages = overlay_budget_pages,
                overlay_eviction_count = overlay_eviction_count,
                overlay_budget_allowed = overlay_snapshot.allowed,
                overlay_budget_drawn = overlay_snapshot.overlays_drawn,
                highlight_page_text_layer = overlay_snapshot.highlight_page_has_text_layer,
            );
            let _enter = manual_span.enter();
            trace!(
                scroll_delta = ?manual_scroll_delta,
                highlight_anchor = anchor_meta.fallback.label(),
                highlight_idx = ?highlighted_idx,
                overlay_available = overlay_available,
                overlay_highlightable_sentences = overlay_highlightable_sentences,
                overlay_budget_pages = overlay_budget_pages,
                overlay_eviction_count = overlay_eviction_count,
                "JumpToSentence: manual scroll request"
            );
            self.overlay_diagnostics
                .record_jump("manual-scroll", overlay_snapshot);
        }
    }

    fn should_auto_scroll(&self, snapshot: &ReaderSnapshot) -> bool {
        snapshot.settings.auto_scroll_tts && snapshot.tts.state == TtsPlaybackState::Playing
    }

    fn highlight_page_has_text_layer(&self) -> bool {
        if let Some(page) = self.pdf_render_state.highlighted_page {
            if let Some(plan) = &self.pdf_render_state.plan {
                return plan.text_layer_page_indexes.contains(&page);
            }
        }
        false
    }

    fn capture_overlay_decision(&self) -> OverlayDecisionSnapshot {
        let highlight_has_text_layer = self.highlight_page_has_text_layer();
        let budget_pages = self.pdf_render_state.overlay_budget_pages();
        OverlayDecisionSnapshot {
            allowed: highlight_has_text_layer && budget_pages > 0,
            budget_pages,
            overlays_drawn: self.pdf_render_state.rendered_overlays,
            highlight_page_has_text_layer: highlight_has_text_layer,
        }
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

    fn render_pdf_diagnostics(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
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
                if let Some(plan) = &self.pdf_render_state.plan {
                    ui.separator();
                    ui.label("PDF viewport plan:");
                    ui.label(format!(
                        "Visible pages: {}",
                        Self::format_pdf_page_list(&self.pdf_render_state.visible_page_indexes)
                    ));
                    ui.label(format!(
                        "Canvas pages: {}",
                        Self::format_pdf_page_list(&plan.canvas_page_indexes)
                    ));
                    ui.label(format!(
                        "Text layers: {}",
                        Self::format_pdf_page_list(&plan.text_layer_page_indexes)
                    ));
                    ui.label(format!(
                        "Priority pages: {}",
                        Self::format_pdf_page_list(&plan.priority_page_indexes)
                    ));
                    ui.label(format!(
                        "Medium priority: {}",
                        Self::format_pdf_page_list(&plan.medium_priority_page_indexes)
                    ));
                    ui.label(format!(
                        "Low priority: {}",
                        Self::format_pdf_page_list(&plan.low_priority_page_indexes)
                    ));
                    ui.label(format!(
                        "Active TTS page: {}",
                        self.pdf_render_state
                            .active_tts_page_index
                            .map(|idx| idx + 1)
                            .unwrap_or(0)
                    ));
                    ui.label(format!(
                        "Jump target page: {}",
                        self.pdf_render_state
                            .jump_target_page_index
                            .map(|idx| idx + 1)
                            .unwrap_or(0)
                    ));
                    let canvas_plan_len = plan.canvas_page_indexes.len();
                    let text_plan_len = plan.text_layer_page_indexes.len();
                    self.render_pdf_preview(ui, snapshot);
                    ui.label(format!(
                        "Rendered canvases: {}/{}",
                        self.pdf_render_state.rendered_canvas_pages, canvas_plan_len
                    ));
                    ui.label(format!(
                        "Rendered text layers: {}/{}",
                        self.pdf_render_state.rendered_text_layers, text_plan_len
                    ));
                    ui.label(format!(
                        "Rendered overlays: {}/{}",
                        self.pdf_render_state.rendered_overlays,
                        self.pdf_render_state.overlay_budget_pages()
                    ));
                    if let Some(decision) = self.overlay_diagnostics.preview_decision() {
                        ui.label(format!(
                            "Preview overlay budget: {} pages (text layer: {}, allowed: {})",
                            decision.budget_pages,
                            if decision.highlight_page_has_text_layer {
                                "yes"
                            } else {
                                "no"
                            },
                            if decision.allowed { "yes" } else { "no" }
                        ));
                        ui.label(format!(
                            "Overlays drawn: {}/{}",
                            decision.overlays_drawn,
                            decision.budget_pages.max(1)
                        ));
                    }
                    if let Some((event, decision)) = self.overlay_diagnostics.last_jump_decision() {
                        ui.label(format!(
                            "Last JumpToSentence ({}): budget {} pages (allowed: {})",
                            event,
                            decision.budget_pages,
                            if decision.allowed { "hit" } else { "skipped" }
                        ));
                        ui.label(format!(
                            "Overlay count: {} (text layer present: {})",
                            decision.overlays_drawn,
                            if decision.highlight_page_has_text_layer {
                                "yes"
                            } else {
                                "no"
                            }
                        ));
                    }
                } else {
                    ui.label("PDF viewport scheduler idle.");
                }
                if let Some(decision) = &self.pdf_render_state.decision {
                    ui.label(format!(
                        "Text layer budget: {} pages ({} evicted)",
                        self.pdf_render_state.overlay_budget_pages(),
                        decision.evict_text_layer_page_indexes.len()
                    ));
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
                } else {
                    ui.label("No viewport eviction activity yet.");
                }
                if let Some(age) = self.pdf_render_state.updated_age() {
                    ui.label(format!(
                        "Scheduler refreshed {:.2}s ago.",
                        age.as_secs_f32()
                    ));
                }
                ui.label(format!(
                    "OCR overlay budget pages: {}",
                    self.pdf_render_state.overlay_budget_pages()
                ));
                ui.label(format!(
                    "Highlightable OCR sentences: {}",
                    snapshot
                        .pdf_ocr_alignment
                        .as_ref()
                        .map(|alignment| alignment.highlightable_sentence_count)
                        .unwrap_or(0)
                ));
            });
    }

    fn render_pdf_preview(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        if snapshot.total_pages == 0 {
            ui.label("PDF preview will appear once the document is ready.");
            return;
        }
        let plan = match &self.pdf_render_state.plan {
            Some(plan) => plan,
            None => {
                ui.label("Viewport preview waiting for scheduler updates...");
                return;
            }
        };
        const MAX_PREVIEW_PAGES: usize = 6;
        let preview_size = Vec2::new(ui.available_width(), 180.0);
        let (preview_rect, _) = ui.allocate_exact_size(preview_size, Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(preview_rect, 8.0, Color32::from_gray(18));
        let content_rect = preview_rect.shrink(6.0);
        painter.rect_stroke(content_rect, 6.0, Stroke::new(1.0, Color32::from_gray(60)));

        let mut preview_pages = Vec::new();
        let mut push_page = |page: usize| {
            if page < snapshot.total_pages && !preview_pages.contains(&page) {
                preview_pages.push(page);
            }
        };
        for page in &self.pdf_render_state.visible_page_indexes {
            push_page(*page);
        }
        if let Some(page) = self.pdf_render_state.highlighted_page {
            push_page(page);
        }
        for page in &plan.priority_page_indexes {
            push_page(*page);
        }
        for page in &plan.canvas_page_indexes {
            push_page(*page);
        }
        push_page(snapshot.current_page);
        preview_pages.truncate(MAX_PREVIEW_PAGES);
        if preview_pages.is_empty() {
            preview_pages.push(
                snapshot
                    .current_page
                    .min(snapshot.total_pages.saturating_sub(1)),
            );
        }

        let columns = preview_pages.len();
        let gap = 8.0;
        let total_gap = gap * columns.saturating_sub(1) as f32;
        let raw_width = (content_rect.width() - total_gap).max(0.0);
        let page_width = (raw_width / columns as f32).max(28.0);
        let used_width = page_width * columns as f32 + total_gap;
        let mut current_x =
            content_rect.left() + (content_rect.width() - used_width).max(0.0) / 2.0;
        let font = FontId::new(11.0, FontFamily::Monospace);
        let highlight_page = self.pdf_render_state.highlighted_page;
        let overlay_budget = self.pdf_render_state.overlay_budget_pages();
        let highlight_page_in_text_layers = highlight_page
            .map(|page| plan.text_layer_page_indexes.contains(&page))
            .unwrap_or(false);
        let overlays_allowed = highlight_page_in_text_layers && overlay_budget > 0;

        let mut canvas_drawn = 0;
        let mut text_drawn = 0;
        let mut overlays_drawn = 0;

        for &page in &preview_pages {
            let page_rect = Rect::from_min_max(
                Pos2::new(current_x, content_rect.top()),
                Pos2::new(current_x + page_width, content_rect.bottom()),
            );
            current_x += page_width + gap;
            canvas_drawn += 1;
            let is_priority = plan.priority_page_indexes.contains(&page);
            let has_canvas = plan.canvas_page_indexes.contains(&page);
            let has_text_layer = plan.text_layer_page_indexes.contains(&page);
            let fill_color = if Some(page) == highlight_page {
                Color32::from_rgb(38, 105, 170)
            } else if has_canvas {
                Color32::from_rgb(25, 25, 25)
            } else {
                Color32::from_rgb(15, 15, 15)
            };
            let border_color = if is_priority {
                Color32::from_rgb(220, 190, 120)
            } else if has_canvas {
                Color32::from_rgb(90, 150, 210)
            } else {
                Color32::from_gray(70)
            };
            painter.rect_filled(page_rect, 6.0, fill_color);
            painter.rect_stroke(
                page_rect,
                6.0,
                Stroke::new(if is_priority { 3.0 } else { 1.4 }, border_color),
            );
            let inner = page_rect.shrink(4.0);
            if has_text_layer {
                text_drawn += 1;
                let text_layer_rect = inner.shrink(2.0);
                painter.rect_filled(
                    text_layer_rect,
                    4.0,
                    Color32::from_rgba_unmultiplied(50, 170, 120, 90),
                );
                painter.rect_stroke(
                    text_layer_rect,
                    4.0,
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(140, 220, 180, 200)),
                );
            }
            painter.text(
                Pos2::new(page_rect.center().x, page_rect.bottom() - 12.0),
                Align2::CENTER_BOTTOM,
                format!("Pg {}", page + 1),
                font.clone(),
                Color32::WHITE,
            );
            if Some(page) == highlight_page && overlays_allowed {
                for (idx, rect) in self.pdf_render_state.overlay_rects.iter().enumerate() {
                    if idx >= overlay_budget {
                        break;
                    }
                    overlays_drawn += 1;
                    let overlay = Rect::from_min_max(
                        Pos2::new(
                            inner.left() + rect[0] * inner.width(),
                            inner.top() + rect[1] * inner.height(),
                        ),
                        Pos2::new(
                            inner.left() + rect[2] * inner.width(),
                            inner.top() + rect[3] * inner.height(),
                        ),
                    );
                    painter.rect_filled(
                        overlay,
                        2.0,
                        Color32::from_rgba_unmultiplied(255, 190, 80, 160),
                    );
                }
            }
        }

        self.pdf_render_state
            .record_render_metrics(canvas_drawn, text_drawn, overlays_drawn);
        let overlay_snapshot = self.capture_overlay_decision();
        self.overlay_diagnostics.record_preview(overlay_snapshot);
        let preview_span = tracing::span!(
            Level::TRACE,
            "PdfPreviewRender",
            budget_plan = "shell.performance_budget",
            overlay_budget_pages = overlay_snapshot.budget_pages,
            overlay_budget_allowed = overlay_snapshot.allowed,
            overlay_budget_drawn = overlay_snapshot.overlays_drawn,
            highlight_page = ?highlight_page,
            highlight_page_text_layer = overlay_snapshot.highlight_page_has_text_layer,
        );
        let _enter = preview_span.enter();
        trace!(
            preview_pages = ?preview_pages,
            canvas = canvas_drawn,
            text_layers = text_drawn,
            overlays = overlays_drawn,
            "Rendered simplified PDF preview"
        );
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

    fn global_sentence_index(snapshot: &ReaderSnapshot, sentence_idx: usize) -> Option<usize> {
        let current_page = snapshot.current_page;
        let current_page_size = *snapshot.page_sentence_counts.get(current_page)?;
        if sentence_idx >= current_page_size {
            return None;
        }
        let page_offset = snapshot
            .page_sentence_counts
            .iter()
            .take(current_page)
            .sum::<usize>();
        page_offset.checked_add(sentence_idx)
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

    fn update_pdf_render_state(&mut self, snapshot: Option<&ReaderSnapshot>) {
        if let Some(snapshot) = snapshot {
            if snapshot.pretty_kind == PrettyKind::Pdf && snapshot.total_pages > 0 {
                let visible_page_indexes = vec![snapshot.current_page];
                let highlighted_page = snapshot
                    .highlighted_sentence_idx
                    .and_then(|sentence_idx| {
                        Self::page_index_for_global_sentence(
                            &snapshot.page_sentence_counts,
                            Some(sentence_idx),
                        )
                    })
                    .unwrap_or(snapshot.current_page);
                let plan_input = PdfViewportPlanInput {
                    total_pages: snapshot.total_pages,
                    visible_page_indexes: visible_page_indexes.clone(),
                    overscan: 1,
                    active_tts_page_index: Some(snapshot.current_page),
                    jump_target_page_index: Some(highlighted_page),
                };
                let plan = build_pdf_viewport_render_plan(&plan_input);
                let entries = (0..snapshot.total_pages)
                    .map(|page_index| PdfPageRegistryEntry {
                        page_index,
                        last_touched_at: (snapshot.current_page as u64)
                            .saturating_add(page_index as u64),
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
                trace!(
                    pdf_plan = ?plan,
                    evicted_canvases = ?decision.evict_canvas_page_indexes,
                    evicted_text_layers = ?decision.evict_text_layer_page_indexes,
                    highlighted_page,
                    "PDF scheduler updated"
                );
                self.pdf_render_state.plan = Some(plan);
                self.pdf_render_state.decision = Some(decision);
                self.pdf_render_state.visible_page_indexes = visible_page_indexes;
                self.pdf_render_state.active_tts_page_index = plan_input.active_tts_page_index;
                self.pdf_render_state.jump_target_page_index = plan_input.jump_target_page_index;
                self.pdf_render_state.last_updated = Some(Instant::now());
                return;
            }
        }
        self.pdf_render_state.reset();
    }
}

impl eframe::App for LanternLeafApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let snapshot = self.runtime.state_snapshot();
        let reader_snapshot = snapshot.reader_document.snapshot.as_ref();
        self.refresh_anchor_diagnostics(reader_snapshot);
        self.update_pdf_render_state(reader_snapshot);
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

#[derive(Debug, Clone, Copy)]
struct OverlayDecisionSnapshot {
    allowed: bool,
    budget_pages: usize,
    overlays_drawn: usize,
    highlight_page_has_text_layer: bool,
}

#[derive(Default)]
struct OverlayDiagnostics {
    preview_decision: Option<OverlayDecisionSnapshot>,
    last_jump_decision: Option<(&'static str, OverlayDecisionSnapshot)>,
}

impl OverlayDiagnostics {
    fn record_preview(&mut self, decision: OverlayDecisionSnapshot) {
        self.preview_decision = Some(decision);
    }

    fn record_jump(&mut self, event: &'static str, decision: OverlayDecisionSnapshot) {
        self.last_jump_decision = Some((event, decision));
    }

    fn preview_decision(&self) -> Option<OverlayDecisionSnapshot> {
        self.preview_decision
    }

    fn last_jump_decision(&self) -> Option<(&'static str, OverlayDecisionSnapshot)> {
        self.last_jump_decision
    }
}

#[derive(Debug)]
enum ScrollBlockReason {
    Duplicate,
    #[allow(dead_code)]
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

#[derive(Default)]
struct PdfRenderState {
    plan: Option<PdfViewportRenderPlan>,
    decision: Option<PdfViewportBudgetDecision>,
    visible_page_indexes: Vec<usize>,
    active_tts_page_index: Option<usize>,
    jump_target_page_index: Option<usize>,
    last_updated: Option<Instant>,
    rendered_canvas_pages: usize,
    rendered_text_layers: usize,
    rendered_overlays: usize,
    highlighted_page: Option<usize>,
    highlighted_sentence_idx: Option<usize>,
    overlay_rects: Vec<[f32; 4]>,
    overlay_alignment_source: Option<String>,
    overlay_alignment_rects: HashMap<usize, Vec<[f32; 4]>>,
}

impl PdfRenderState {
    fn reset(&mut self) {
        self.plan = None;
        self.decision = None;
        self.visible_page_indexes.clear();
        self.active_tts_page_index = None;
        self.jump_target_page_index = None;
        self.last_updated = None;
        self.rendered_canvas_pages = 0;
        self.rendered_text_layers = 0;
        self.rendered_overlays = 0;
        self.highlighted_page = None;
        self.highlighted_sentence_idx = None;
        self.overlay_rects.clear();
        self.overlay_alignment_source = None;
        self.overlay_alignment_rects.clear();
    }

    fn updated_age(&self) -> Option<Duration> {
        self.last_updated.map(|instant| instant.elapsed())
    }

    fn overlay_budget_pages(&self) -> usize {
        self.plan
            .as_ref()
            .map(|plan| plan.text_layer_page_indexes.len())
            .unwrap_or(0)
    }

    fn record_render_metrics(&mut self, canvas_pages: usize, text_layers: usize, overlays: usize) {
        self.rendered_canvas_pages = canvas_pages;
        self.rendered_text_layers = text_layers;
        self.rendered_overlays = overlays;
    }

    fn set_highlighted_page(
        &mut self,
        page_index: usize,
        sentence_idx: Option<usize>,
        overlay_rects: Vec<[f32; 4]>,
    ) {
        if self.highlighted_page == Some(page_index)
            && self.highlighted_sentence_idx == sentence_idx
        {
            return;
        }
        self.highlighted_page = Some(page_index);
        self.highlighted_sentence_idx = sentence_idx;
        self.overlay_rects = if overlay_rects.is_empty() {
            Self::generate_overlay_rects(sentence_idx)
        } else {
            overlay_rects
        };
    }

    fn generate_overlay_rects(sentence_idx: Option<usize>) -> Vec<[f32; 4]> {
        let count = sentence_idx.map(|idx| (idx % 3) + 1).unwrap_or(0);
        (0..count)
            .map(|i| {
                let width = 0.8 - (i as f32 * 0.15);
                let height = 0.12;
                let left = 0.1 + (i as f32 * 0.05);
                let top = 0.15 + (i as f32 * 0.18);
                let right = (left + width).min(0.95);
                let bottom = (top + height).min(0.9);
                [left, top, right, bottom]
            })
            .collect()
    }

    fn overlay_rects_for_sentence(
        &mut self,
        source_path: &str,
        sentence_idx: usize,
    ) -> Vec<[f32; 4]> {
        self.ensure_alignment_cache(source_path);
        self.overlay_alignment_rects
            .get(&sentence_idx)
            .cloned()
            .unwrap_or_default()
    }

    fn ensure_alignment_cache(&mut self, source_path: &str) {
        if self.overlay_alignment_source.as_deref() == Some(source_path) {
            return;
        }
        self.overlay_alignment_source = Some(source_path.to_string());
        self.overlay_alignment_rects.clear();
        let path = Path::new(source_path);
        if let Some(artifact) = cache::load_pdf_ocr_alignment_artifact(path) {
            for alignment in artifact.alignments.iter() {
                if alignment.page_idx.is_none() {
                    continue;
                }
                let rects = Self::alignment_rects(alignment);
                if !rects.is_empty() {
                    self.overlay_alignment_rects
                        .insert(alignment.sentence_idx, rects);
                }
            }
        }
    }

    fn alignment_rects(alignment: &crate::cache::PdfOcrSentenceAlignment) -> Vec<[f32; 4]> {
        let geometry = if !alignment.rects.is_empty() {
            &alignment.rects
        } else if !alignment.line_rects.is_empty() {
            &alignment.line_rects
        } else {
            &alignment.block_rects
        };
        geometry
            .iter()
            .map(|rect| {
                let left = rect.left.clamp(0.0, 1.0);
                let top = rect.top.clamp(0.0, 1.0);
                let right = (rect.left + rect.width).clamp(0.0, 1.0);
                let bottom = (rect.top + rect.height).clamp(0.0, 1.0);
                [left, top, right, bottom]
            })
            .collect()
    }
}
