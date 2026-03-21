mod helpers;
mod pdf;
mod pdf_renderer;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use eframe::{
    NativeOptions,
    egui::{
        self, Align, Align2, Button, CentralPanel, CollapsingHeader, Color32, ColorImage, Context,
        FontFamily, FontId, Pos2, Rect, RichText, ScrollArea, Sense, SidePanel, Stroke,
        TextureHandle, TextureOptions, TopBottomPanel, Ui, Vec2, Visuals,
    },
};
use helpers::{app_config_path, bootstrap_config_from_app_config, format_combo};

use crate::pdf::{
    PdfPageRegistryEntry, PdfViewportBudgetDecision, PdfViewportBudgetInput, PdfViewportPlanInput,
    PdfViewportRenderPlan, build_pdf_viewport_render_plan, choose_pdf_viewport_evictions,
};
use crate::pdf_renderer::{NativePdfRenderer, RenderTarget};
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
use tracing::{Level, info, trace, warn};

pub const PDF_CANVAS_BUDGET_PAGES: usize = 2;
pub const PDF_TEXT_LAYER_BUDGET_PAGES: usize = 1;
pub const PDF_CANVAS_TEXTURE_SIZE: [usize; 2] = [320, 450];
pub const PDF_TEXT_TEXTURE_SIZE: [usize; 2] = [300, 420];

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
    _tracing_guard: tracing_appender::non_blocking::WorkerGuard,
    status_log: Vec<String>,
    show_safe_quit_modal: bool,
    show_reader_confirm_modal: bool,
    pending_search_focus: bool,
    last_plan: Option<DispatchPlan>,
    auto_scroll_state: AutoScrollState,
    anchor_diagnostics: AnchorDiagnostics,
    overlay_diagnostics: OverlayDiagnostics,
    scheduler_events: Vec<SchedulerEvent>,
    pdf_render_state: PdfRenderState,
    pdf_renderer: Option<NativePdfRenderer>,
    current_pdf_path: Option<PathBuf>,
    sentence_scroll_offset: Option<Vec2>,
}

impl LanternLeafApp {
    fn new(
        _cc: &eframe::CreationContext<'_>,
        runtime: AppRuntime,
        tracing_guard: tracing_appender::non_blocking::WorkerGuard,
    ) -> Self {
        let pdf_renderer = match NativePdfRenderer::new() {
            Ok(renderer) => Some(renderer),
            Err(err) => {
                warn!(error = ?err, "Failed to initialize native PDF renderer");
                None
            }
        };
        Self {
            runtime,
            _tracing_guard: tracing_guard,
            status_log: Vec::new(),
            show_safe_quit_modal: false,
            show_reader_confirm_modal: false,
            pending_search_focus: false,
            last_plan: None,
            auto_scroll_state: AutoScrollState::default(),
            anchor_diagnostics: AnchorDiagnostics::default(),
            overlay_diagnostics: OverlayDiagnostics::default(),
            scheduler_events: Vec::new(),
            pdf_render_state: PdfRenderState::default(),
            pdf_renderer,
            current_pdf_path: None,
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
                if let Some(decision) = self.overlay_diagnostics.preview_decision() {
                    if !decision.allowed {
                        let reason = if !decision.highlight_page_has_text_layer {
                            "no text layer"
                        } else {
                            "overlay budget exhausted"
                        };
                        ui.label(
                            RichText::new(format!(
                                "Overlay warning: {} (budget {} pages)",
                                reason, decision.budget_pages
                            ))
                            .color(Color32::from_rgb(255, 190, 110))
                            .strong(),
                        );
                    }
                }
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
        if let Some(decision) = self.overlay_diagnostics.preview_decision() {
            let reason = if decision.allowed {
                "overlays rendering"
            } else if !decision.highlight_page_has_text_layer {
                "no text layer to honor overlay budget"
            } else {
                "budget exhausted"
            };
            let badge = if decision.allowed { "✅" } else { "⚠️" };
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{} Overlay budget: {} pages, {} overlays drawn ({})",
                    badge, decision.budget_pages, decision.overlays_drawn, reason
                ));
            });
        }
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
                        let overlay_geometry = Self::global_sentence_index(snapshot, idx)
                            .and_then(|global_idx| {
                                self.pdf_render_state
                                    .overlay_geometry_for_sentence(&snapshot.source_path, global_idx)
                            });
                        let overlay_rects = overlay_geometry
                            .as_ref()
                            .map(|entry| entry.rects.clone())
                            .unwrap_or_default();
                        let overlay_reason = overlay_geometry
                            .as_ref()
                            .and_then(|entry| entry.reason.clone());
                        self.pdf_render_state.set_highlighted_page(
                            highlight_page,
                            Some(idx),
                            overlay_rects,
                            overlay_reason.clone(),
                        );
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
                                let overlay_span =
                                    self.overlay_budget_span("auto-scroll", &overlay_snapshot);
                                let _overlay_enter = overlay_span.enter();
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
                                self.overlay_diagnostics.record_jump("auto-scroll", overlay_snapshot);
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
                        let overlay_span =
                            self.overlay_budget_span("sentence-click", &overlay_snapshot);
                        let _overlay_enter = overlay_span.enter();
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
            let overlay_span = self.overlay_budget_span("manual-scroll", &overlay_snapshot);
            let _overlay_enter = overlay_span.enter();
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
        self.pdf_render_state
            .highlighted_page
            .and_then(|page| {
                self.pdf_render_state
                    .surface_for_page(page)
                    .map(|surface| surface.text_layer_ready)
            })
            .unwrap_or(false)
    }

    fn capture_overlay_decision(&self) -> OverlayDecisionSnapshot {
        let highlight_has_text_layer = self.highlight_page_has_text_layer();
        let budget_pages = self.pdf_render_state.overlay_budget_pages();
        let overlay_rects_available = self.pdf_render_state.overlay_rects.len();
        let overlay_reason = self.pdf_render_state.overlay_alignment_reason.clone();
        OverlayDecisionSnapshot {
            allowed: highlight_has_text_layer && budget_pages > 0,
            budget_pages,
            overlays_drawn: self.pdf_render_state.rendered_overlays,
            highlight_page_has_text_layer: highlight_has_text_layer,
            highlight_page: self.pdf_render_state.highlighted_page,
            overlay_rects_available,
            overlay_reason,
        }
    }

    fn maybe_record_overlay_retry(&mut self, decision: &OverlayDecisionSnapshot) {
        if decision.allowed {
            return;
        }
        let reason = if !decision.highlight_page_has_text_layer {
            "text_layer_missing"
        } else if decision.budget_pages == 0 {
            "budget_exhausted"
        } else {
            "overlay_blocked"
        };
        self.record_scheduler_event(SchedulerEventKind::RetryOverlay {
            reason,
            highlight_page: decision.highlight_page,
            budget_pages: decision.budget_pages,
            overlay_reason: decision.overlay_reason.clone(),
        });
    }

    fn record_scheduler_event(&mut self, kind: SchedulerEventKind) {
        let event = SchedulerEvent {
            timestamp: Instant::now(),
            kind,
        };
        if self
            .scheduler_events
            .last()
            .map(|last| last.kind == event.kind)
            .unwrap_or(false)
        {
            return;
        }
        self.scheduler_events.push(event);
        if self.scheduler_events.len() > 8 {
            self.scheduler_events.remove(0);
        }
    }

    fn overlay_budget_span(
        &self,
        event: &'static str,
        decision: &OverlayDecisionSnapshot,
    ) -> tracing::span::Span {
        tracing::span!(
            Level::TRACE,
            "OverlayBudgetDecision",
            budget_plan = "shell.performance_budget",
            overlay_budget_pages = decision.budget_pages,
            overlay_budget_allowed = decision.allowed,
            overlay_budget_drawn = decision.overlays_drawn,
            highlight_page = ?decision.highlight_page,
            highlight_page_text_layer = decision.highlight_page_has_text_layer,
            overlay_rect_count = decision.overlay_rects_available,
            overlay_alignment_reason = ?decision.overlay_reason.as_deref(),
            event = event,
        )
    }

    fn replay_overlay_span(&self, event: &'static str, decision: OverlayDecisionSnapshot) {
        let span = self.overlay_budget_span(event, &decision);
        let _enter = span.enter();
        trace!(decision = ?decision, "Replayed overlay budget decision for QA");
    }

    fn replay_throttle_span(&self, event: &PdfRenderThrottleEvent) {
        let highlight_page = self.pdf_render_state.highlighted_page == Some(event.page_index);
        let span = tracing::span!(
            Level::TRACE,
            "PdfRenderThrottle",
            budget_plan = "shell.performance_budget",
            page = (event.page_index + 1),
            highlight_page = highlight_page,
            kind = ?event.kind,
            reason = event.reason.as_str(),
            overlay_budget_pages = self.pdf_render_state.overlay_budget_pages(),
        );
        let _enter = span.enter();
        trace!(event = ?event, "Replayed throttle span for QA");
    }

    fn log_render_throttle(
        &mut self,
        kind: PdfRenderThrottleKind,
        page_index: usize,
        highlight_page: bool,
        overlay_budget_pages: usize,
        reason: &'static str,
    ) {
        let event = PdfRenderThrottleEvent::new(kind, page_index, reason.to_string());
        self.pdf_render_state.record_throttle_event(event.clone());
        let span = tracing::span!(
            Level::TRACE,
            "PdfRenderThrottle",
            budget_plan = "shell.performance_budget",
            page = (page_index + 1),
            highlight_page = highlight_page,
            kind = ?kind,
            reason = reason,
            overlay_budget_pages = overlay_budget_pages,
        );
        let _enter = span.enter();
        trace!(
            page = (page_index + 1),
            kind = ?kind,
            reason = reason,
            highlight_page = highlight_page,
            "PDF render stage throttled/skipped"
        );
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
                        if let Some(reason) = &decision.overlay_reason {
                            ui.label(format!("Overlay geometry reason: {}", reason));
                        }
                        ui.label(format!(
                            "Cached overlay rects: {}",
                            decision.overlay_rects_available
                        ));
                        if ui.button("Replay preview overlay span").clicked() {
                            self.replay_overlay_span("preview", decision);
                        }
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
                        if let Some(reason) = &decision.overlay_reason {
                            ui.label(format!("Overlay geometry reason: {}", reason));
                        }
                        ui.label(format!(
                            "Cached overlay rects: {}",
                            decision.overlay_rects_available
                        ));
                        if ui.button("Replay last overlay span").clicked() {
                            self.replay_overlay_span(event, decision);
                        }
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
                ui.separator();
                ui.label("Scheduler events:");
                if self.scheduler_events.is_empty() {
                    ui.label("(No scheduler events logged yet)");
                } else {
                    for event in self.scheduler_events.iter().rev() {
                        ui.label(
                            RichText::new(format!(
                                "{} ({:.1}s ago)",
                                event.kind.describe(),
                                event.age_secs()
                            ))
                            .small()
                            .weak(),
                        );
                    }
                }
                ui.separator();
                ui.label("Render throttle timeline:");
                let throttle_events = self.pdf_render_state.recent_throttle_events();
                if throttle_events.is_empty() {
                    ui.label("(No throttle events yet)");
                } else {
                    for event in throttle_events.iter().rev() {
                        ui.horizontal(|ui| {
                            ui.label(LanternLeafApp::throttle_badge(event.kind));
                            ui.label(
                                RichText::new(format!(
                                    "{} ({:.1}s ago)",
                                    event.describe(),
                                    event.age_secs()
                                ))
                                .small(),
                            );
                            if ui.button("Replay throttle span").clicked() {
                                self.replay_throttle_span(event);
                            }
                        });
                    }
                }
                ui.separator();
                ui.label("Render events:");
                let render_events = self.pdf_render_state.recent_render_events();
                if render_events.is_empty() {
                    ui.label("(No render activity yet)");
                } else {
                    for event in render_events.iter().rev() {
                        ui.label(
                            RichText::new(format!(
                                "{} ({:.1}s ago)",
                                event.describe(),
                                event.age_secs()
                            ))
                            .small()
                            .weak(),
                        );
                    }
                }
                let budget_rejections = self
                    .scheduler_events
                    .iter()
                    .filter(|event| matches!(event.kind, SchedulerEventKind::RetryOverlay { .. }))
                    .count();
                if budget_rejections > 0 {
                    ui.separator();
                    ui.label("Overlay budget rejections:");
                    for event in self.scheduler_events.iter().rev() {
                        if let SchedulerEventKind::RetryOverlay { .. } = &event.kind {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("BUDGET REJECTION")
                                        .color(Color32::from_rgb(220, 180, 120))
                                        .strong()
                                        .small(),
                                );
                                ui.label(
                                    RichText::new(format!(
                                        "{} ({:.1}s ago)",
                                        event.kind.describe(),
                                        event.age_secs()
                                    ))
                                    .small()
                                    .weak(),
                                );
                            });
                        }
                    }
                }
            });
    }

    fn throttle_badge(kind: PdfRenderThrottleKind) -> RichText {
        match kind {
            PdfRenderThrottleKind::Canvas => RichText::new("CANVAS")
                .color(Color32::from_rgb(150, 190, 230))
                .small()
                .strong(),
            PdfRenderThrottleKind::TextLayer => RichText::new("TEXT")
                .color(Color32::from_rgb(130, 210, 170))
                .small()
                .strong(),
            PdfRenderThrottleKind::Overlay => RichText::new("OVERLAY")
                .color(Color32::from_rgb(220, 170, 100))
                .small()
                .strong(),
        }
    }

    fn render_pdf_preview(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
        if snapshot.total_pages == 0 {
            ui.label("PDF preview will appear once the document is ready.");
            return;
        }
        let plan = match self.pdf_render_state.plan.as_ref().cloned() {
            Some(plan) => plan,
            None => {
                ui.label("Viewport preview waiting for scheduler updates...");
                return;
            }
        };
        self.prepare_pdf_textures(ui.ctx());
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
        let highlight_page_text_ready = highlight_page
            .and_then(|page| {
                self.pdf_render_state
                    .surface_for_page(page)
                    .map(|surface| surface.text_layer_ready)
            })
            .unwrap_or(false);
        let overlays_allowed = highlight_page_text_ready && overlay_budget > 0;

        let mut canvas_drawn = 0;
        let mut text_drawn = 0;
        let mut overlays_drawn = 0;

        for &page in &preview_pages {
            let page_rect = Rect::from_min_max(
                Pos2::new(current_x, content_rect.top()),
                Pos2::new(current_x + page_width, content_rect.bottom()),
            );
            current_x += page_width + gap;
            let is_highlight_page = Some(page) == highlight_page;
            let is_priority = plan.priority_page_indexes.contains(&page);
            let (
                canvas_allowed,
                text_allowed,
                canvas_texture,
                text_texture,
                overlays_source,
                overlay_reason,
            ) = {
                let surface = self.pdf_render_state.surface_for_page(page);
                let canvas_allowed = surface.map(|surface| surface.canvas_ready).unwrap_or(false);
                let text_allowed = surface
                    .map(|surface| surface.text_layer_ready)
                    .unwrap_or(false);
                let canvas_texture = surface
                    .and_then(|surface| surface.canvas_texture.as_ref())
                    .map(|texture| texture.id());
                let text_texture = surface
                    .and_then(|surface| surface.text_layer_texture.as_ref())
                    .map(|texture| texture.id());
                let overlays_source = surface
                    .map(|surface| surface.overlay_rects.clone())
                    .unwrap_or_else(|| self.pdf_render_state.overlay_rects.clone());
                let overlay_reason = surface
                    .and_then(|surface| surface.overlay_reason.clone())
                    .or_else(|| self.pdf_render_state.overlay_alignment_reason.clone());
                (
                    canvas_allowed,
                    text_allowed,
                    canvas_texture,
                    text_texture,
                    overlays_source,
                    overlay_reason,
                )
            };
            let has_canvas_intent = plan.canvas_page_indexes.contains(&page);
            let has_text_intent = plan.text_layer_page_indexes.contains(&page);
            let uv_rect = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));

            if canvas_allowed {
                let canvas_span = tracing::span!(
                    Level::TRACE,
                    "PdfRenderCanvas",
                    budget_plan = "shell.performance_budget",
                    page = (page + 1),
                    highlight_page = is_highlight_page,
                    priority_page = is_priority,
                    text_layer_available = text_allowed,
                    overlay_budget_pages = overlay_budget,
                );
                let _canvas_enter = canvas_span.enter();
                self.pdf_render_state
                    .record_render_event(PdfRenderEvent::canvas(
                        page,
                        is_highlight_page,
                        overlay_budget,
                    ));
                canvas_drawn += 1;
            } else if has_canvas_intent {
                let reason = if self.pdf_render_state.is_canvas_evicted(page) {
                    "evicted_from_budget"
                } else {
                    "not_ready"
                };
                self.log_render_throttle(
                    PdfRenderThrottleKind::Canvas,
                    page,
                    is_highlight_page,
                    overlay_budget,
                    reason,
                );
            } else {
                self.log_render_throttle(
                    PdfRenderThrottleKind::Canvas,
                    page,
                    is_highlight_page,
                    overlay_budget,
                    "not_scheduled",
                );
            }

            let fill_color = if !canvas_allowed {
                Color32::from_gray(10)
            } else if is_highlight_page {
                Color32::from_rgb(38, 105, 170)
            } else if has_canvas_intent {
                Color32::from_rgb(25, 25, 25)
            } else {
                Color32::from_rgb(15, 15, 15)
            };
            let border_color = if is_priority {
                Color32::from_rgb(220, 190, 120)
            } else if canvas_allowed || has_canvas_intent {
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
            if canvas_allowed {
                if let Some(texture) = canvas_texture {
                    painter.image(texture, page_rect, uv_rect, Color32::WHITE);
                }
            }
            let inner = page_rect.shrink(4.0);
            if text_allowed {
                text_drawn += 1;
                let text_span = tracing::span!(
                    Level::TRACE,
                    "PdfRenderTextLayer",
                    budget_plan = "shell.performance_budget",
                    page = (page + 1),
                    highlight_page = is_highlight_page,
                    overlay_budget_pages = overlay_budget,
                );
                let _text_enter = text_span.enter();
                trace!(
                    page = (page + 1),
                    highlight_page = is_highlight_page,
                    "Drawing text layer"
                );
                self.pdf_render_state
                    .record_render_event(PdfRenderEvent::text_layer(
                        page,
                        is_highlight_page,
                        overlay_budget,
                    ));
                let text_layer_rect = inner.shrink(2.0);
                if let Some(texture) = text_texture {
                    painter.image(
                        texture,
                        text_layer_rect,
                        uv_rect,
                        Color32::from_white_alpha(200),
                    );
                } else {
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
            } else if has_text_intent {
                let reason = if self.pdf_render_state.is_text_layer_evicted(page) {
                    "budget_exhausted"
                } else {
                    "not_ready"
                };
                self.log_render_throttle(
                    PdfRenderThrottleKind::TextLayer,
                    page,
                    is_highlight_page,
                    overlay_budget,
                    reason,
                );
            }
            painter.text(
                Pos2::new(page_rect.center().x, page_rect.bottom() - 12.0),
                Align2::CENTER_BOTTOM,
                format!("Pg {}", page + 1),
                font.clone(),
                Color32::WHITE,
            );
            if Some(page) == highlight_page {
                let mut page_overlay_drawn = 0;
                if overlays_allowed {
                    for (idx, rect) in overlays_source.iter().enumerate() {
                        if idx >= overlay_budget {
                            break;
                        }
                        overlays_drawn += 1;
                        page_overlay_drawn += 1;
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
                if page_overlay_drawn > 0 {
                    let overlay_span = tracing::span!(
                        Level::TRACE,
                        "PdfRenderOverlay",
                        budget_plan = "shell.performance_budget",
                        page = (page + 1),
                        highlight_page = true,
                        overlays_drawn = page_overlay_drawn,
                        overlay_budget_pages = overlay_budget,
                        overlay_alignment_reason = ?overlay_reason.as_deref(),
                    );
                    let _overlay_enter = overlay_span.enter();
                    trace!(
                        page = (page + 1),
                        overlays = page_overlay_drawn,
                        "Rendered highlight overlays"
                    );
                    self.pdf_render_state
                        .record_render_event(PdfRenderEvent::overlay(
                            page,
                            page_overlay_drawn,
                            overlay_budget,
                            overlay_reason.clone(),
                        ));
                }
                if !highlight_page_text_ready && !self.pdf_render_state.overlay_rects.is_empty() {
                    self.log_render_throttle(
                        PdfRenderThrottleKind::Overlay,
                        page,
                        true,
                        overlay_budget,
                        "text_layer_missing",
                    );
                } else if highlight_page_text_ready && overlay_budget == 0 {
                    self.log_render_throttle(
                        PdfRenderThrottleKind::Overlay,
                        page,
                        true,
                        overlay_budget,
                        "budget_exhausted",
                    );
                } else if overlays_allowed
                    && !self.pdf_render_state.overlay_rects.is_empty()
                    && self.pdf_render_state.overlay_rects.len() > overlay_budget
                {
                    self.log_render_throttle(
                        PdfRenderThrottleKind::Overlay,
                        page,
                        true,
                        overlay_budget,
                        "budget_exhausted",
                    );
                }
            }
        }

        self.pdf_render_state
            .record_render_metrics(canvas_drawn, text_drawn, overlays_drawn);
        let overlay_snapshot = self.capture_overlay_decision();
        self.maybe_record_overlay_retry(&overlay_snapshot);
        self.overlay_diagnostics
            .record_preview(overlay_snapshot.clone());
        let overlay_span = self.overlay_budget_span("preview", &overlay_snapshot);
        let _overlay_enter = overlay_span.enter();
        let preview_span = tracing::span!(
            Level::TRACE,
            "PdfPreviewRender",
            budget_plan = "shell.performance_budget",
            highlight_page = ?highlight_page,
            highlight_page_text_layer = highlight_page_text_ready,
            overlay_budget_pages = overlay_snapshot.budget_pages,
            overlay_budget_allowed = overlay_snapshot.allowed,
            overlay_rect_count = overlay_snapshot.overlay_rects_available,
            overlay_alignment_reason = ?overlay_snapshot.overlay_reason.as_deref(),
            canvas_drawn = canvas_drawn,
            text_drawn = text_drawn,
            overlays = overlays_drawn,
        );
        let _enter = preview_span.enter();
        trace!(
            preview_pages = ?preview_pages,
            canvas = canvas_drawn,
            text_layers = text_drawn,
            overlays = overlays_drawn,
            overlay_rects = self.pdf_render_state.overlay_rects.len(),
            overlay_reason = ?self.pdf_render_state.overlay_alignment_reason.as_deref(),
            "Rendered simplified PDF preview"
        );
    }

    fn prepare_pdf_textures(&mut self, ctx: &Context) {
        if self.pdf_render_state.plan.is_none() {
            return;
        }
        for idx in 0..self.pdf_render_state.viewport_surfaces.len() {
            let (page_index, canvas_ready, canvas_missing, text_ready, text_missing) = {
                let surface = &self.pdf_render_state.viewport_surfaces[idx];
                (
                    surface.page_index,
                    surface.canvas_ready,
                    surface.canvas_texture.is_none(),
                    surface.text_layer_ready,
                    surface.text_layer_texture.is_none(),
                )
            };
            if canvas_ready && canvas_missing {
                let image = self
                    .render_pdf_texture(page_index, RenderTarget::Canvas)
                    .unwrap_or_else(|| Self::build_canvas_color_image(page_index));
                let texture = ctx.load_texture(
                    format!("pdf-{}-{}", RenderTarget::Canvas.label(), page_index),
                    image,
                    TextureOptions::LINEAR,
                );
                self.pdf_render_state.viewport_surfaces[idx].canvas_texture = Some(texture);
            }
            if text_ready && text_missing {
                let image = self
                    .render_pdf_texture(page_index, RenderTarget::TextLayer)
                    .unwrap_or_else(|| Self::build_text_layer_color_image(page_index));
                let texture = ctx.load_texture(
                    format!("pdf-{}-{}", RenderTarget::TextLayer.label(), page_index),
                    image,
                    TextureOptions::LINEAR,
                );
                self.pdf_render_state.viewport_surfaces[idx].text_layer_texture = Some(texture);
            }
        }
    }

    fn build_canvas_color_image(page_index: usize) -> ColorImage {
        Self::build_placeholder_texture(
            page_index,
            PDF_CANVAS_TEXTURE_SIZE,
            [180, 190, 205],
            [60, 80, 110],
        )
    }

    fn build_text_layer_color_image(page_index: usize) -> ColorImage {
        Self::build_placeholder_texture(
            page_index,
            PDF_TEXT_TEXTURE_SIZE,
            [230, 230, 210],
            [80, 90, 70],
        )
    }

    fn build_placeholder_texture(
        page_index: usize,
        size: [usize; 2],
        base: [u8; 3],
        accent: [u8; 3],
    ) -> ColorImage {
        let (width, height) = (size[0], size[1]);
        let mut data = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            let stripe = (y + page_index * 3) % 32 < 18;
            for x in 0..width {
                let pattern = ((x * 3 + y * 5 + page_index * 7) % 256) as u8;
                let extra = if stripe { 24 } else { 0 };
                let r = base[0]
                    .saturating_add((pattern / 16) as u8)
                    .saturating_add(extra);
                let g = base[1]
                    .saturating_add((pattern / 20) as u8)
                    .saturating_add(extra / 2);
                let b = accent[2].saturating_add((pattern / 24) as u8);
                data.extend_from_slice(&[r, g, b, 255u8]);
            }
        }
        ColorImage::from_rgba_unmultiplied([width, height], &data)
    }

    fn render_pdf_texture(
        &mut self,
        page_index: usize,
        target: RenderTarget,
    ) -> Option<ColorImage> {
        let source_path = self.current_pdf_path.as_deref()?;
        let renderer = self.pdf_renderer.as_mut()?;
        let result = match target {
            RenderTarget::Canvas => renderer.render_canvas(source_path, page_index),
            RenderTarget::TextLayer => renderer.render_text_layer(source_path, page_index),
        };
        match result {
            Ok(image) => Some(image),
            Err(err) => {
                warn!(
                    pdf_path = %source_path.display(),
                    page = page_index + 1,
                    target = ?target,
                    error = ?err,
                    "native PDF renderer failed"
                );
                None
            }
        }
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
                self.current_pdf_path = Some(PathBuf::from(&snapshot.source_path));
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
                let mut registry_pages = plan.canvas_page_indexes.clone();
                registry_pages.extend(plan.text_layer_page_indexes.iter().copied());
                registry_pages.sort_unstable();
                registry_pages.dedup();
                let entries = registry_pages
                    .into_iter()
                    .map(|page_index| PdfPageRegistryEntry {
                        page_index,
                        last_touched_at: (snapshot.current_page as u64)
                            .saturating_add(page_index as u64),
                        rendered_zoom: if plan.canvas_page_indexes.contains(&page_index) {
                            Some(1.0)
                        } else {
                            None
                        },
                        text_layer_zoom: if plan.text_layer_page_indexes.contains(&page_index) {
                            Some(1.0)
                        } else {
                            None
                        },
                    })
                    .collect::<Vec<_>>();
                let mut keep_canvas_page_indexes = plan.priority_page_indexes.clone();
                keep_canvas_page_indexes.push(highlighted_page);
                keep_canvas_page_indexes.sort_unstable();
                keep_canvas_page_indexes.dedup();
                let mut keep_text_layer_page_indexes = keep_canvas_page_indexes.clone();
                keep_text_layer_page_indexes.extend(plan.text_layer_page_indexes.iter().copied());
                keep_text_layer_page_indexes.sort_unstable();
                keep_text_layer_page_indexes.dedup();
                let decision = choose_pdf_viewport_evictions(&PdfViewportBudgetInput {
                    entries,
                    keep_canvas_page_indexes,
                    keep_text_layer_page_indexes,
                    max_canvas_pages: PDF_CANVAS_BUDGET_PAGES.max(1),
                    max_text_layer_pages: PDF_TEXT_LAYER_BUDGET_PAGES.max(1),
                });
                if !decision.evict_canvas_page_indexes.is_empty()
                    || !decision.evict_text_layer_page_indexes.is_empty()
                {
                    self.record_scheduler_event(SchedulerEventKind::Eviction {
                        evicted_canvas_pages: decision.evict_canvas_page_indexes.clone(),
                        evicted_text_layer_pages: decision.evict_text_layer_page_indexes.clone(),
                    });
                }
                trace!(
                    pdf_plan = ?plan,
                    evicted_canvases = ?decision.evict_canvas_page_indexes,
                    evicted_text_layers = ?decision.evict_text_layer_page_indexes,
                    highlighted_page,
                    canvas_budget = PDF_CANVAS_BUDGET_PAGES,
                    text_layer_budget = PDF_TEXT_LAYER_BUDGET_PAGES,
                    "PDF scheduler updated"
                );
                self.pdf_render_state.plan = Some(plan.clone());
                self.pdf_render_state.update_surfaces(&plan);
                self.pdf_render_state.decision = Some(decision.clone());
                self.pdf_render_state.apply_budget_evictions(&decision);
                self.pdf_render_state.visible_page_indexes = visible_page_indexes;
                self.pdf_render_state.active_tts_page_index = plan_input.active_tts_page_index;
                self.pdf_render_state.jump_target_page_index = plan_input.jump_target_page_index;
                self.pdf_render_state.last_updated = Some(Instant::now());
                return;
            }
        }
        self.current_pdf_path = None;
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

#[derive(Clone, Debug)]
struct OverlayDecisionSnapshot {
    allowed: bool,
    budget_pages: usize,
    overlays_drawn: usize,
    highlight_page_has_text_layer: bool,
    highlight_page: Option<usize>,
    overlay_rects_available: usize,
    overlay_reason: Option<String>,
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
        self.preview_decision.clone()
    }

    fn last_jump_decision(&self) -> Option<(&'static str, OverlayDecisionSnapshot)> {
        self.last_jump_decision.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SchedulerEventKind {
    Eviction {
        evicted_canvas_pages: Vec<usize>,
        evicted_text_layer_pages: Vec<usize>,
    },
    RetryOverlay {
        reason: &'static str,
        highlight_page: Option<usize>,
        budget_pages: usize,
        overlay_reason: Option<String>,
    },
}

#[derive(Clone, Debug)]
struct SchedulerEvent {
    timestamp: Instant,
    kind: SchedulerEventKind,
}

impl SchedulerEvent {
    fn age_secs(&self) -> f32 {
        self.timestamp.elapsed().as_secs_f32()
    }
}

impl SchedulerEventKind {
    fn describe(&self) -> String {
        match self {
            SchedulerEventKind::Eviction {
                evicted_canvas_pages,
                evicted_text_layer_pages,
            } => format!(
                "Evicted canvases: {}, text layers: {}",
                LanternLeafApp::format_pdf_page_list(evicted_canvas_pages),
                LanternLeafApp::format_pdf_page_list(evicted_text_layer_pages)
            ),
            SchedulerEventKind::RetryOverlay {
                reason,
                highlight_page,
                budget_pages,
                overlay_reason,
            } => format!(
                "Overlay retry ({}): page {}, budget {}, geometry {}",
                reason,
                highlight_page
                    .map(|idx| idx + 1)
                    .map_or("unknown".to_string(), |page| page.to_string()),
                budget_pages,
                overlay_reason.as_deref().unwrap_or("unknown")
            ),
        }
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
    overlay_alignment_reason: Option<String>,
    overlay_alignment_source: Option<String>,
    overlay_alignment_rects: HashMap<usize, OverlayGeometryEntry>,
    render_events: Vec<PdfRenderEvent>,
    viewport_surfaces: Vec<PdfViewportSurface>,
    throttle_events: Vec<PdfRenderThrottleEvent>,
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
        self.overlay_alignment_reason = None;
        self.overlay_alignment_source = None;
        self.overlay_alignment_rects.clear();
        self.render_events.clear();
        self.viewport_surfaces.clear();
        self.throttle_events.clear();
    }

    fn updated_age(&self) -> Option<Duration> {
        self.last_updated.map(|instant| instant.elapsed())
    }

    fn overlay_budget_pages(&self) -> usize {
        self.viewport_surfaces
            .iter()
            .filter(|surface| surface.text_layer_ready)
            .count()
    }

    fn record_render_metrics(&mut self, canvas_pages: usize, text_layers: usize, overlays: usize) {
        self.rendered_canvas_pages = canvas_pages;
        self.rendered_text_layers = text_layers;
        self.rendered_overlays = overlays;
    }

    fn record_render_event(&mut self, event: PdfRenderEvent) {
        const MAX_RENDER_EVENTS: usize = 16;
        self.render_events.push(event);
        if self.render_events.len() > MAX_RENDER_EVENTS {
            self.render_events.remove(0);
        }
    }

    fn recent_render_events(&self) -> &[PdfRenderEvent] {
        &self.render_events
    }

    fn record_throttle_event(&mut self, event: PdfRenderThrottleEvent) {
        const MAX_THROTTLE_EVENTS: usize = 12;
        self.throttle_events.push(event);
        if self.throttle_events.len() > MAX_THROTTLE_EVENTS {
            self.throttle_events.remove(0);
        }
    }

    fn recent_throttle_events(&self) -> &[PdfRenderThrottleEvent] {
        &self.throttle_events
    }

    fn update_surfaces(&mut self, plan: &PdfViewportRenderPlan) {
        let mut surfaces_map: HashMap<usize, PdfViewportSurface> = self
            .viewport_surfaces
            .drain(..)
            .map(|surface| (surface.page_index, surface))
            .collect();
        for &page in plan.canvas_page_indexes.iter() {
            surfaces_map
                .entry(page)
                .or_insert_with(|| PdfViewportSurface::new(page))
                .canvas_ready = true;
        }
        for &page in plan.text_layer_page_indexes.iter() {
            surfaces_map
                .entry(page)
                .or_insert_with(|| PdfViewportSurface::new(page))
                .text_layer_ready = true;
        }
        for &page in plan.priority_page_indexes.iter() {
            surfaces_map
                .entry(page)
                .or_insert_with(|| PdfViewportSurface::new(page));
        }
        let mut surfaces = surfaces_map.into_values().collect::<Vec<_>>();
        surfaces.sort_by_key(|surface| surface.page_index);
        self.viewport_surfaces = surfaces;
    }

    fn apply_budget_evictions(&mut self, decision: &PdfViewportBudgetDecision) {
        for surface in self.viewport_surfaces.iter_mut() {
            if decision
                .evict_canvas_page_indexes
                .contains(&surface.page_index)
            {
                surface.canvas_ready = false;
                surface.canvas_texture = None;
            }
            if decision
                .evict_text_layer_page_indexes
                .contains(&surface.page_index)
            {
                surface.text_layer_ready = false;
                surface.text_layer_texture = None;
            }
        }
    }

    fn surface_for_page(&self, page: usize) -> Option<&PdfViewportSurface> {
        self.viewport_surfaces
            .iter()
            .find(|surface| surface.page_index == page)
    }

    fn is_canvas_evicted(&self, page_index: usize) -> bool {
        self.decision
            .as_ref()
            .map(|decision| decision.evict_canvas_page_indexes.contains(&page_index))
            .unwrap_or(false)
    }

    fn is_text_layer_evicted(&self, page_index: usize) -> bool {
        self.decision
            .as_ref()
            .map(|decision| decision.evict_text_layer_page_indexes.contains(&page_index))
            .unwrap_or(false)
    }

    fn set_highlighted_page(
        &mut self,
        page_index: usize,
        sentence_idx: Option<usize>,
        overlay_rects: Vec<[f32; 4]>,
        overlay_reason: Option<String>,
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
        self.overlay_alignment_reason = if self.overlay_rects.is_empty() {
            None
        } else {
            overlay_reason
        };
        if let Some(surface) = self
            .viewport_surfaces
            .iter_mut()
            .find(|surface| surface.page_index == page_index)
        {
            surface.overlay_rects = self.overlay_rects.clone();
            surface.overlay_reason = self.overlay_alignment_reason.clone();
        }
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

    fn overlay_geometry_for_sentence(
        &mut self,
        source_path: &str,
        sentence_idx: usize,
    ) -> Option<OverlayGeometryEntry> {
        self.ensure_alignment_cache(source_path);
        self.overlay_alignment_rects.get(&sentence_idx).cloned()
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
                if let Some(entry) = OverlayGeometryEntry::from_alignment(alignment) {
                    self.overlay_alignment_rects
                        .insert(alignment.sentence_idx, entry);
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

#[derive(Clone)]
struct OverlayGeometryEntry {
    rects: Vec<[f32; 4]>,
    reason: Option<String>,
}

impl OverlayGeometryEntry {
    fn new(rects: Vec<[f32; 4]>, reason: Option<String>) -> Self {
        Self { rects, reason }
    }

    fn from_alignment(alignment: &crate::cache::PdfOcrSentenceAlignment) -> Option<Self> {
        let rects = PdfRenderState::alignment_rects(alignment);
        if rects.is_empty() {
            return None;
        }
        let reason_text = alignment.fallback_reason.trim();
        let reason = if reason_text.is_empty() {
            None
        } else {
            Some(reason_text.to_string())
        };
        Some(Self::new(rects, reason))
    }
}

#[derive(Clone)]
struct PdfViewportSurface {
    page_index: usize,
    canvas_ready: bool,
    text_layer_ready: bool,
    canvas_texture: Option<TextureHandle>,
    text_layer_texture: Option<TextureHandle>,
    overlay_rects: Vec<[f32; 4]>,
    overlay_reason: Option<String>,
}

impl PdfViewportSurface {
    fn new(page_index: usize) -> Self {
        Self {
            page_index,
            canvas_ready: false,
            text_layer_ready: false,
            canvas_texture: None,
            text_layer_texture: None,
            overlay_rects: Vec::new(),
            overlay_reason: None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum PdfRenderEventKind {
    Canvas,
    TextLayer,
    Overlay,
}

#[derive(Clone, Debug)]
struct PdfRenderEvent {
    timestamp: Instant,
    kind: PdfRenderEventKind,
    page_index: usize,
    highlight_page: bool,
    overlay_budget_pages: usize,
    overlays_drawn: usize,
    overlay_reason: Option<String>,
}

impl PdfRenderEvent {
    fn canvas(page_index: usize, highlight_page: bool, overlay_budget_pages: usize) -> Self {
        Self {
            timestamp: Instant::now(),
            kind: PdfRenderEventKind::Canvas,
            page_index,
            highlight_page,
            overlay_budget_pages,
            overlays_drawn: 0,
            overlay_reason: None,
        }
    }

    fn text_layer(page_index: usize, highlight_page: bool, overlay_budget_pages: usize) -> Self {
        Self {
            timestamp: Instant::now(),
            kind: PdfRenderEventKind::TextLayer,
            page_index,
            highlight_page,
            overlay_budget_pages,
            overlays_drawn: 0,
            overlay_reason: None,
        }
    }

    fn overlay(
        page_index: usize,
        overlays_drawn: usize,
        overlay_budget_pages: usize,
        overlay_reason: Option<String>,
    ) -> Self {
        Self {
            timestamp: Instant::now(),
            kind: PdfRenderEventKind::Overlay,
            page_index,
            highlight_page: true,
            overlay_budget_pages,
            overlays_drawn,
            overlay_reason,
        }
    }

    fn describe(&self) -> String {
        match self.kind {
            PdfRenderEventKind::Canvas => format!(
                "Canvas render: page {}{} (budget {} pages)",
                self.page_index + 1,
                if self.highlight_page {
                    " (highlight)"
                } else {
                    ""
                },
                self.overlay_budget_pages
            ),
            PdfRenderEventKind::TextLayer => format!(
                "Text layer render: page {}{} (budget {} pages)",
                self.page_index + 1,
                if self.highlight_page {
                    " (highlight)"
                } else {
                    ""
                },
                self.overlay_budget_pages
            ),
            PdfRenderEventKind::Overlay => format!(
                "Overlay render: page {} (rects {}, reason {}, budget {} pages)",
                self.page_index + 1,
                self.overlays_drawn,
                self.overlay_reason.as_deref().unwrap_or("unknown"),
                self.overlay_budget_pages
            ),
        }
    }

    fn age_secs(&self) -> f32 {
        self.timestamp.elapsed().as_secs_f32()
    }
}

#[derive(Clone, Copy, Debug)]
enum PdfRenderThrottleKind {
    Canvas,
    TextLayer,
    Overlay,
}

#[derive(Clone, Debug)]
struct PdfRenderThrottleEvent {
    timestamp: Instant,
    kind: PdfRenderThrottleKind,
    page_index: usize,
    reason: String,
}

impl PdfRenderThrottleEvent {
    fn new(kind: PdfRenderThrottleKind, page_index: usize, reason: String) -> Self {
        Self {
            timestamp: Instant::now(),
            kind,
            page_index,
            reason,
        }
    }

    fn describe(&self) -> String {
        format!(
            "{} throttle: page {}, {}",
            match self.kind {
                PdfRenderThrottleKind::Canvas => "Canvas",
                PdfRenderThrottleKind::TextLayer => "Text layer",
                PdfRenderThrottleKind::Overlay => "Overlay",
            },
            self.page_index + 1,
            self.reason
        )
    }

    fn age_secs(&self) -> f32 {
        self.timestamp.elapsed().as_secs_f32()
    }
}
