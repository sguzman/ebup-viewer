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
        FontFamily, FontId, Pos2, Rect, RichText, ScrollArea, Sense, SidePanel, Slider, Stroke,
        TextureHandle, TextureOptions, TopBottomPanel, Ui, Vec2, Visuals,
    },
};
use helpers::{app_config_path, bootstrap_config_from_app_config, format_combo};

use crate::pdf::{
    PdfPageRegistryEntry, PdfViewportBudgetDecision, PdfViewportBudgetInput, PdfViewportPlanInput,
    PdfViewportRenderPlan, build_pdf_viewport_render_plan, choose_pdf_viewport_evictions,
};
use crate::pdf_renderer::{
    NativePdfRenderer, NativeRenderEviction, NativeRenderSpan, RenderTarget,
};
use lanternleaf_app::{
    AppRuntime,
    contracts::{PrettyKind, ReaderSnapshot, UiMode},
    pipeline::{AppCommand, DispatchPlan, PersistenceTrigger, ReaderCommand},
    shortcuts::{ShortcutAction, ShortcutScope, UiShortcutAction},
    state::AppState,
    tracing::init_tracing,
};
use lanternleaf_core::{
    cache, config,
    session::{ReaderSettingsPatch, SessionCommand, TtsPlaybackState},
};
use serde_json::json;
use tracing::{Level, info, trace, warn};

pub const PDF_CANVAS_BUDGET_PAGES: usize = 2;
pub const PDF_TEXT_LAYER_BUDGET_PAGES: usize = 1;
pub const PDF_CANVAS_TEXTURE_SIZE: [usize; 2] = [320, 450];
pub const PDF_TEXT_TEXTURE_SIZE: [usize; 2] = [300, 420];
const REGRESSION_EVENT_WINDOW: Duration = Duration::from_secs(3);
const READER_RENDR_ROADMAP_URL: &str = "https://github.com/sguzman/lantern-leaf/blob/main/docs/roadmaps/egui-reader-rendering-roadmap.md";
const PDF_SUBSYSTEM_ROADMAP_URL: &str =
    "https://github.com/sguzman/lantern-leaf/blob/main/docs/roadmaps/egui-native-pdf-roadmap.md";
const PRIORITIZATION_ROADMAP_URL: &str = "https://github.com/sguzman/lantern-leaf/blob/main/docs/roadmaps/implementation-prioritization-roadmap.md";
const TTS_ROADMAP_URL: &str = "https://github.com/sguzman/lantern-leaf/blob/main/docs/roadmaps/egui-tts-audio-and-playback-roadmap.md";
const SETTINGS_ROADMAP_URL: &str = "https://github.com/sguzman/lantern-leaf/blob/main/docs/roadmaps/egui-config-cache-and-persistence-roadmap.md";
const PERSISTENCE_ROADMAP_URL: &str = SETTINGS_ROADMAP_URL;
const QA_REGRESSION_URL: &str = "https://github.com/sguzman/lantern-leaf/blob/main/docs/roadmaps/egui-testing-and-parity-roadmap.md";

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
    status_log: Vec<StatusLogEntry>,
    show_safe_quit_modal: bool,
    show_reader_confirm_modal: bool,
    pending_search_focus: bool,
    last_plan: Option<DispatchPlan>,
    auto_scroll_state: AutoScrollState,
    anchor_diagnostics: AnchorDiagnostics,
    overlay_diagnostics: OverlayDiagnostics,
    audio_diagnostics: AudioDiagnostics,
    settings_trace_events: Vec<SettingsTraceEvent>,
    settings_trace_next_id: usize,
    persistence_trace_events: Vec<PersistenceTraceEvent>,
    persistence_trace_next_id: usize,
    regression_snapshots: Vec<RegressionSnapshot>,
    regression_snapshot_next_id: usize,
    overlay_pressure_focus: bool,
    scheduler_events: Vec<SchedulerEvent>,
    pdf_render_state: PdfRenderState,
    pdf_renderer: Option<NativePdfRenderer>,
    current_pdf_path: Option<PathBuf>,
    sentence_scroll_offset: Option<Vec2>,
    overlay_eviction_warning_at: Option<Instant>,
}

impl LanternLeafApp {
    const OVERLAY_EVICTION_SNACK_DURATION: Duration = Duration::from_secs(5);
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
            audio_diagnostics: AudioDiagnostics::default(),
            settings_trace_events: Vec::new(),
            settings_trace_next_id: 0,
            persistence_trace_events: Vec::new(),
            persistence_trace_next_id: 0,
            regression_snapshots: Vec::new(),
            regression_snapshot_next_id: 0,
            overlay_pressure_focus: false,
            scheduler_events: Vec::new(),
            pdf_render_state: PdfRenderState::default(),
            pdf_renderer,
            current_pdf_path: None,
            sentence_scroll_offset: None,
            overlay_eviction_warning_at: None,
        }
    }

    fn execute_command(&mut self, command: AppCommand) {
        let state_snapshot = self.runtime.state_snapshot();
        let reader_snapshot = state_snapshot.reader_document.snapshot.as_ref();
        self.maybe_record_audio_command(&command, reader_snapshot);
        let plan = self.runtime.plan_command(command);
        self.log_plan(&plan);
        self.last_plan = Some(plan);
    }

    fn execute_reader_command(&mut self, command: ReaderCommand) {
        self.execute_command(AppCommand::Reader(command));
    }

    fn log_plan(&mut self, plan: &DispatchPlan) {
        let entry = format!("Planned {} ({})", plan.action, plan.effects.len());
        self.push_status(entry);
    }

    fn push_status(&mut self, message: String) {
        self.status_log.push(StatusLogEntry {
            timestamp: Instant::now(),
            message,
        });
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
                self.push_status("Shortcut: focus search".to_string());
            }
        }
    }

    fn render_top_bar(&mut self, ctx: &Context, state: &AppState) {
        TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.vertical(|ui| {
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
                if let Some(elapsed) = self.overlay_eviction_warning_age() {
                    if let Some(alert) = self
                        .pdf_render_state
                        .recent_overlay_pressure_alerts()
                        .last()
                    {
                        ui.label(
                            RichText::new(format!(
                                "Overlay eviction warning: {} ({:.1}s ago)",
                                alert.describe(),
                                elapsed.as_secs_f32()
                            ))
                            .color(Color32::from_rgb(255, 130, 90))
                            .small()
                            .strong(),
                        );
                    }
                }
                self.render_overlay_pressure_toast(ui);
            });
        });
    }

    fn render_overlay_pressure_toast(&mut self, ui: &mut Ui) {
        if let Some(alert) = self
            .pdf_render_state
            .recent_overlay_pressure_alerts()
            .last()
            .cloned()
        {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(self.overlay_pressure_badge(&alert));
                ui.label(
                    RichText::new(format!(
                        "{} (span #{}, {:.1}s ago, budget {} pages)",
                        alert.describe(),
                        alert.id(),
                        alert.age_secs(),
                        alert.overlay_budget_pages
                    ))
                    .small()
                    .weak(),
                );
                if ui.small_button("Copy QA JSON").clicked() {
                    let summary = self.overlay_pressure_span_summary(&alert);
                    self.log_qa_span_copy(&alert, &summary);
                }
                if ui.small_button("Open overlay diagnostics").clicked() {
                    self.overlay_pressure_focus = true;
                    ui.ctx().request_repaint();
                }
            });
        }
    }

    fn overlay_eviction_warning_age(&mut self) -> Option<Duration> {
        let now = Instant::now();
        if let Some(start) = self.overlay_eviction_warning_at {
            let elapsed = now.duration_since(start);
            if elapsed < Self::OVERLAY_EVICTION_SNACK_DURATION {
                return Some(elapsed);
            }
            self.overlay_eviction_warning_at = None;
        }
        None
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
            self.render_settings_sidebar(ui, reader_snapshot);
        });
        SidePanel::right("shortcuts").show(ctx, |ui| {
            ui.heading("Shortcut registry");
            for binding in self.runtime.shortcut_registry().bindings() {
                ui.label(format!("{} → {:?}", binding.combo, binding.action));
            }
        });
    }

    fn render_settings_sidebar(&mut self, ui: &mut Ui, snapshot: Option<&ReaderSnapshot>) {
        CollapsingHeader::new("Settings & persistence")
            .id_source("settings-sidebar")
            .default_open(false)
            .show(ui, |ui| {
                let snapshot = match snapshot {
                    Some(snapshot) => snapshot,
                    None => {
                        ui.label("Open a reader session to adjust settings.");
                        return;
                    }
                };
                let settings = &snapshot.settings;
                ui.label(format!("Theme: {:?}", settings.theme));
                ui.horizontal(|ui| {
                    let mut auto_scroll = settings.auto_scroll_tts;
                    if ui
                        .checkbox(&mut auto_scroll, "Auto-scroll TTS playback")
                        .changed()
                    {
                        self.apply_reader_settings_patch(
                            ReaderSettingsPatch {
                                auto_scroll_tts: Some(auto_scroll),
                                ..Default::default()
                            },
                            "auto_scroll_tts",
                        );
                    }
                    let mut center_spoken = settings.center_spoken_sentence;
                    if ui
                        .checkbox(&mut center_spoken, "Center spoken sentence")
                        .changed()
                    {
                        self.apply_reader_settings_patch(
                            ReaderSettingsPatch {
                                center_spoken_sentence: Some(center_spoken),
                                ..Default::default()
                            },
                            "center_spoken_sentence",
                        );
                    }
                });
                ui.horizontal(|ui| {
                    let mut show_original = settings.text_only_show_original_text;
                    if ui
                        .checkbox(&mut show_original, "Text-only shows original text")
                        .changed()
                    {
                        self.apply_reader_settings_patch(
                            ReaderSettingsPatch {
                                text_only_show_original_text: Some(show_original),
                                ..Default::default()
                            },
                            "text_only_show_original_text",
                        );
                    }
                });
                ui.add_space(4.0);
                let mut line_spacing = settings.line_spacing;
                if ui
                    .add(
                        Slider::new(&mut line_spacing, 1.0..=2.5)
                            .text("Line spacing")
                            .prefix("Line: "),
                    )
                    .changed()
                {
                    self.apply_reader_settings_patch(
                        ReaderSettingsPatch {
                            line_spacing: Some(line_spacing),
                            ..Default::default()
                        },
                        "line_spacing",
                    );
                }
                let mut pause_after = settings.pause_after_sentence;
                if ui
                    .add(
                        Slider::new(&mut pause_after, 0.1..=3.0)
                            .text("Pause after sentence")
                            .suffix("s"),
                    )
                    .changed()
                {
                    self.apply_reader_settings_patch(
                        ReaderSettingsPatch {
                            pause_after_sentence: Some(pause_after),
                            ..Default::default()
                        },
                        "pause_after_sentence",
                    );
                }
                let mut tts_speed = settings.tts_speed;
                if ui
                    .add(
                        Slider::new(&mut tts_speed, 0.5..=2.5)
                            .text("TTS speed")
                            .suffix("x"),
                    )
                    .changed()
                {
                    self.apply_reader_settings_patch(
                        ReaderSettingsPatch {
                            tts_speed: Some(tts_speed),
                            ..Default::default()
                        },
                        "tts_speed",
                    );
                }
                let mut tts_volume = settings.tts_volume;
                if ui
                    .add(
                        Slider::new(&mut tts_volume, 0.0..=2.0)
                            .text("TTS volume")
                            .suffix("x"),
                    )
                    .changed()
                {
                    self.apply_reader_settings_patch(
                        ReaderSettingsPatch {
                            tts_volume: Some(tts_volume),
                            ..Default::default()
                        },
                        "tts_volume",
                    );
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Persist settings now").clicked() {
                        self.trigger_persistence_flush(
                            PersistenceTrigger::RuntimeConfigChange,
                            "manual_settings_persist",
                        );
                    }
                    if ui.button("Flush persistence caches").clicked() {
                        self.trigger_persistence_flush(
                            PersistenceTrigger::ReaderCommand,
                            "manual_cache_flush",
                        );
                    }
                });
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

    fn render_reader_summary(&mut self, ui: &mut Ui, snapshot: &ReaderSnapshot) {
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
        if snapshot.pretty_kind == PrettyKind::Pdf {
            let overlay_budget = self.pdf_render_state.overlay_budget_pages();
            let highlight_ready = self.highlight_page_has_text_layer();
            let overlay_status = if overlay_budget > 0 {
                "Overlay budget available"
            } else {
                "Overlay budget exhausted"
            };
            let overlay_color = if highlight_ready && overlay_budget > 0 {
                Color32::from_rgb(130, 210, 170)
            } else {
                Color32::from_rgb(220, 130, 110)
            };
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} — cached rects: {}, highlight layer ready: {}",
                        overlay_status,
                        self.pdf_render_state.overlay_rects.len(),
                        highlight_ready
                    ))
                    .color(overlay_color)
                    .strong(),
                );
                if overlay_budget == 0 {
                    ui.label(
                        RichText::new("Budget blocked")
                            .color(Color32::from_rgb(220, 180, 120))
                            .small(),
                    );
                }
            });
        }
        if let Some(alert) = self
            .pdf_render_state
            .recent_overlay_pressure_alerts()
            .last()
            .cloned()
        {
            ui.horizontal(|ui| {
                ui.label(self.overlay_pressure_badge(&alert));
                ui.label(
                    RichText::new(format!(
                        "Overlay pressure on page {} (budget {} pages, {:.1}s ago)",
                        alert.kind.page_index() + 1,
                        alert.overlay_budget_pages,
                        alert.age_secs()
                    ))
                    .small(),
                );
                if ui
                    .small_button("Inspect PDF diagnostics")
                    .on_hover_text("Highlight the diagnostics panel to replay the pressure span")
                    .clicked()
                {
                    self.overlay_pressure_focus = true;
                }
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

    fn maybe_record_overlay_retry(
        &mut self,
        decision: &OverlayDecisionSnapshot,
        snapshot: &ReaderSnapshot,
    ) {
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
        self.record_regression_snapshot(
            RegressionScenario::OverlayBacklog { reason },
            Some(snapshot),
            Some(decision.clone()),
        );
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

    fn audio_budget_span(&self, event: &AudioBudgetEvent) -> tracing::span::Span {
        tracing::span!(
            Level::TRACE,
            "JumpToSentence",
            budget_plan = "shell.performance_budget",
            audio_command = event.command,
            target_sentence = ?event.target_sentence,
            anchor_path = event.fallback.label(),
            anchor_index = ?event.anchor,
            auto_scroll = event.auto_scroll,
            overlay_budget_pages = event.overlay_snapshot.budget_pages,
            overlay_budget_allowed = event.overlay_snapshot.allowed,
            overlay_rect_count = event.overlay_snapshot.overlay_rects_available,
            overlay_alignment_reason = ?event.overlay_snapshot.overlay_reason.as_deref(),
            highlight_page = ?event.highlight_page,
        )
    }

    fn replay_audio_event(&self, event: &AudioBudgetEvent) {
        let span = self.audio_budget_span(event);
        let _enter = span.enter();
        trace!(event = %event.describe(), "Replayed audio budget span for QA");
    }

    fn audio_event_payload(&self, event: &AudioBudgetEvent, summary: &str) -> String {
        let payload = json!({
            "id": event.id,
            "command": event.command,
            "auto_scroll": event.auto_scroll,
            "target_sentence": event.target_sentence,
            "anchor_fallback": event.fallback.label(),
            "anchor_index": event.anchor,
            "overlay_budget_pages": event.overlay_snapshot.budget_pages,
            "overlay_allowed": event.overlay_snapshot.allowed,
            "highlight_page": event.highlight_page,
            "overlay_rects_cached": event.overlay_snapshot.overlay_rects_available,
            "overlay_reason": event.overlay_snapshot.overlay_reason,
            "summary": summary,
        });
        serde_json::to_string(&payload).unwrap_or_else(|_| summary.to_string())
    }

    fn log_qa_audio_copy(&mut self, event: &AudioBudgetEvent, summary: &str) {
        let payload = self.audio_event_payload(event, summary);
        self.push_status(format!("QA audio span copy: {}", payload));
    }

    fn apply_reader_settings_patch(
        &mut self,
        patch: ReaderSettingsPatch,
        description: &'static str,
    ) {
        let summary = format!("{:?}", patch);
        let span = tracing::span!(
            Level::TRACE,
            "ReaderSettingsChange",
            budget_plan = "shell.performance_budget",
            settings_action = description,
            patch = %summary,
        );
        let _enter = span.enter();
        self.record_settings_event(description, summary.clone());
        self.execute_reader_command(ReaderCommand::Session(SessionCommand::ApplySettings {
            patch,
        }));
    }

    fn record_settings_event(&mut self, description: &'static str, summary: String) {
        const MAX_EVENTS: usize = 12;
        let event = SettingsTraceEvent {
            id: self.settings_trace_next_id,
            timestamp: Instant::now(),
            description,
            summary,
            roadmap_url: SETTINGS_ROADMAP_URL,
        };
        self.settings_trace_next_id = self.settings_trace_next_id.wrapping_add(1);
        self.settings_trace_events.push(event);
        if self.settings_trace_events.len() > MAX_EVENTS {
            self.settings_trace_events.remove(0);
        }
    }

    fn replay_settings_event(&self, event: &SettingsTraceEvent) {
        let span = tracing::span!(
            Level::TRACE,
            "ReaderSettingsReplay",
            budget_plan = "shell.performance_budget",
            description = event.description,
            summary = event.summary.as_str(),
        );
        let _enter = span.enter();
        trace!(event = event.describe(), "Replayed settings change for QA");
    }

    fn settings_event_payload(&self, event: &SettingsTraceEvent) -> String {
        let payload = json!({
            "id": event.id,
            "description": event.description,
            "summary": event.summary,
            "roadmap_url": event.roadmap_url,
            "age_secs": event.age_secs(),
        });
        serde_json::to_string(&payload).unwrap_or_else(|_| event.summary.clone())
    }

    fn log_settings_trace_copy(&mut self, event: &SettingsTraceEvent) {
        let payload = self.settings_event_payload(event);
        self.push_status(format!("QA settings span copy: {}", payload));
    }

    fn trigger_persistence_flush(
        &mut self,
        trigger: PersistenceTrigger,
        description: &'static str,
    ) {
        let span = tracing::span!(
            Level::TRACE,
            "PersistenceFlush",
            budget_plan = "shell.performance_budget",
            trigger = ?trigger,
            description = description,
        );
        let _enter = span.enter();
        self.record_persistence_event(trigger, description);
        self.execute_command(AppCommand::FlushPersistence { trigger });
    }

    fn record_persistence_event(&mut self, trigger: PersistenceTrigger, description: &'static str) {
        const MAX_EVENTS: usize = 12;
        let event = PersistenceTraceEvent {
            id: self.persistence_trace_next_id,
            timestamp: Instant::now(),
            trigger,
            description,
            roadmap_url: PERSISTENCE_ROADMAP_URL,
        };
        self.persistence_trace_next_id = self.persistence_trace_next_id.wrapping_add(1);
        self.persistence_trace_events.push(event);
        if self.persistence_trace_events.len() > MAX_EVENTS {
            self.persistence_trace_events.remove(0);
        }
    }

    fn replay_persistence_event(&self, event: &PersistenceTraceEvent) {
        let span = tracing::span!(
            Level::TRACE,
            "PersistenceReplay",
            budget_plan = "shell.performance_budget",
            trigger = ?event.trigger,
            description = event.description,
        );
        let _enter = span.enter();
        trace!(event = event.describe(), "Replayed persistence span for QA");
    }

    fn persistence_event_payload(&self, event: &PersistenceTraceEvent) -> String {
        let payload = json!({
            "id": event.id,
            "description": event.description,
            "trigger": format!("{:?}", event.trigger),
            "roadmap_url": event.roadmap_url,
            "age_secs": event.age_secs(),
        });
        serde_json::to_string(&payload).unwrap_or_else(|_| event.describe())
    }

    fn log_persistence_trace_copy(&mut self, event: &PersistenceTraceEvent) {
        let payload = self.persistence_event_payload(event);
        self.push_status(format!("QA persistence span copy: {}", payload));
    }

    fn replay_regression_snapshot(&self, snapshot: &RegressionSnapshot) {
        let span = tracing::span!(
            Level::TRACE,
            "RegressionSnapshotReplay",
            budget_plan = "shell.performance_budget",
            scenario = snapshot.scenario.label(),
            snapshot_id = snapshot.id,
        );
        let _enter = span.enter();
        trace!(snapshot = %snapshot.describe(), "Replayed regression snapshot for QA");
    }

    fn regression_snapshot_payload(&self, snapshot: &RegressionSnapshot) -> String {
        let payload = json!({
            "id": snapshot.id,
            "scenario": snapshot.scenario.label(),
            "description": snapshot.describe(),
            "source_path": snapshot.source_path,
            "page": snapshot.current_page.map(|page| page + 1),
            "highlighted_sentence": snapshot.highlighted_sentence.map(|idx| idx + 1),
            "overlay_budget_pages": snapshot.overlay_snapshot.as_ref().map(|overlay| overlay.budget_pages),
            "overlay_reason": snapshot.overlay_snapshot.as_ref().and_then(|overlay| overlay.overlay_reason.clone()),
            "persistence_trigger": snapshot.scenario.persistence_trigger().map(|trigger| format!("{:?}", trigger)),
            "roadmap_url": snapshot.scenario.roadmap_url(),
            "qa_checklist": QA_REGRESSION_URL,
            "age_secs": snapshot.age_secs(),
        });
        serde_json::to_string(&payload).unwrap_or_else(|_| snapshot.describe())
    }

    fn log_regression_snapshot_copy(&mut self, snapshot: &RegressionSnapshot, payload: &str) {
        self.push_status(format!(
            "QA regression snapshot copy ({}): {}",
            snapshot.scenario.label(),
            payload
        ));
    }

    fn record_regression_snapshot(
        &mut self,
        scenario: RegressionScenario,
        snapshot: Option<&ReaderSnapshot>,
        overlay_snapshot: Option<OverlayDecisionSnapshot>,
    ) {
        const MAX_SNAPSHOTS: usize = 10;
        let now = Instant::now();
        let scenario_clone = scenario.clone();
        if let Some(last) = self.regression_snapshots.last() {
            if last.scenario == scenario_clone
                && now.duration_since(last.timestamp) < Duration::from_secs(8)
            {
                return;
            }
        }
        let (source_path, current_page, highlighted_sentence) = snapshot
            .map(|snapshot| {
                (
                    Some(snapshot.source_path.clone()),
                    Some(snapshot.current_page),
                    snapshot.highlighted_sentence_idx,
                )
            })
            .unwrap_or((None, None, None));
        let entry = RegressionSnapshot {
            id: self.regression_snapshot_next_id,
            timestamp: now,
            scenario,
            source_path,
            current_page,
            highlighted_sentence,
            overlay_snapshot,
        };
        self.regression_snapshot_next_id = self.regression_snapshot_next_id.wrapping_add(1);
        self.regression_snapshots.push(entry.clone());
        if self.regression_snapshots.len() > MAX_SNAPSHOTS {
            self.regression_snapshots.remove(0);
        }
        trace!(
            regression_snapshot = entry.describe(),
            id = entry.id,
            "Captured regression snapshot for QA"
        );
    }

    fn maybe_record_audio_command(
        &mut self,
        command: &AppCommand,
        snapshot: Option<&ReaderSnapshot>,
    ) {
        let session_cmd = match command {
            AppCommand::Reader(ReaderCommand::Session(session_cmd)) => session_cmd,
            _ => return,
        };
        let label = match Self::audio_command_label(session_cmd) {
            Some(label) => label,
            None => return,
        };
        let snapshot = match snapshot {
            Some(snapshot) => snapshot,
            None => return,
        };
        let target_sentence = snapshot.highlighted_sentence_idx;
        let (anchor, fallback) = target_sentence
            .map(|idx| LanternLeafApp::resolve_sentence_anchor(snapshot, idx))
            .unwrap_or((None, AnchorFallback::Missing));
        let overlay_snapshot = self.capture_overlay_decision();
        let highlight_page = self.pdf_render_state.highlighted_page;
        let event = AudioBudgetEvent {
            id: self.audio_diagnostics.allocate_event_id(),
            timestamp: Instant::now(),
            command: label,
            auto_scroll: Self::audio_command_auto_scroll(session_cmd),
            target_sentence,
            anchor,
            fallback,
            overlay_snapshot: overlay_snapshot.clone(),
            highlight_page,
        };
        let span = self.audio_budget_span(&event);
        let _enter = span.enter();
        trace!(
            audio_command = event.command,
            target_sentence = ?event.target_sentence,
            auto_scroll = event.auto_scroll,
            budget_pages = event.overlay_snapshot.budget_pages,
            "Recorded audio JumpToSentence decision"
        );
        self.audio_diagnostics.record(event);
    }

    fn audio_command_label(command: &SessionCommand) -> Option<&'static str> {
        match command {
            SessionCommand::TtsPlay => Some("tts.play"),
            SessionCommand::TtsPause => Some("tts.pause"),
            SessionCommand::TtsTogglePlayPause => Some("tts.toggle_play_pause"),
            SessionCommand::TtsPlayFromPageStart => Some("tts.play_page_start"),
            SessionCommand::TtsPlayFromHighlight => Some("tts.play_from_highlight"),
            SessionCommand::TtsSeekNext => Some("tts.seek_next"),
            SessionCommand::TtsSeekPrev => Some("tts.seek_prev"),
            SessionCommand::TtsRepeatSentence => Some("tts.repeat_sentence"),
            SessionCommand::TtsStop => Some("tts.stop"),
            _ => None,
        }
    }

    fn audio_command_auto_scroll(command: &SessionCommand) -> bool {
        matches!(
            command,
            SessionCommand::TtsPlay
                | SessionCommand::TtsPlayFromPageStart
                | SessionCommand::TtsPlayFromHighlight
                | SessionCommand::TtsSeekNext
                | SessionCommand::TtsSeekPrev
                | SessionCommand::TtsRepeatSentence
        )
    }

    fn capture_overlay_pressure_from_native_render_span(&mut self, span: &NativeRenderSpan) {
        if span.target != RenderTarget::TextLayer || span.cache_hit {
            return;
        }
        let overlay_budget_pages = self.pdf_render_state.overlay_budget_pages();
        if overlay_budget_pages == 0 {
            return;
        }
        let highlight_page = self.pdf_render_state.highlighted_page;
        let reason_text = if highlight_page == Some(span.page_index) {
            "Highlight text layer rendered while overlay budget contested"
        } else {
            "Neighbor text layer render consumed the overlay budget"
        };
        let alert_id = self.pdf_render_state.allocate_overlay_alert_id();
        let alert = OverlayPressureAlert::new(
            alert_id,
            OverlayPressureKind::NativeRender {
                span: span.clone(),
                reason_text: reason_text.to_string(),
            },
            overlay_budget_pages,
            highlight_page,
        );
        self.pdf_render_state.record_overlay_pressure_alert(alert);
    }

    fn capture_overlay_pressure_from_native_eviction(&mut self, eviction: &NativeRenderEviction) {
        if eviction.target != RenderTarget::TextLayer {
            return;
        }
        let highlight_page = self.pdf_render_state.highlighted_page;
        if highlight_page != Some(eviction.page_index) {
            return;
        }
        let alert_id = self.pdf_render_state.allocate_overlay_alert_id();
        let alert = OverlayPressureAlert::new(
            alert_id,
            OverlayPressureKind::NativeEviction {
                eviction: eviction.clone(),
                reason_text: "Highlight text layer evicted by budget pressure".to_string(),
            },
            self.pdf_render_state.overlay_budget_pages(),
            highlight_page,
        );
        self.pdf_render_state.record_overlay_pressure_alert(alert);
        self.overlay_eviction_warning_at = Some(Instant::now());
        let eviction_span = tracing::span!(
            Level::WARN,
            "OverlayEvictionWarning",
            budget_plan = "shell.performance_budget",
            page = eviction.page_index + 1,
            highlight_page = highlight_page.is_some(),
            overlay_budget_pages = self.pdf_render_state.overlay_budget_pages(),
            reason = eviction.reason,
            target = ?eviction.target,
        );
        let _enter = eviction_span.enter();
        trace!(event = %eviction.describe(), "Overlay eviction logged for QA");
    }

    fn overlay_pressure_badge(&self, alert: &OverlayPressureAlert) -> RichText {
        let (color, label) = alert.kind.badge_info();
        RichText::new(label).color(color).small().strong()
    }

    fn overlay_pressure_span_summary(&self, alert: &OverlayPressureAlert) -> String {
        match &alert.kind {
            OverlayPressureKind::NativeRender { span, reason_text } => format!(
                "[OverlayBudget][Render] page={} target={} cache_hit={} duration_ms={:.2} budget={} reason={}",
                span.page_index + 1,
                span.target.label(),
                span.cache_hit,
                span.duration.as_secs_f32() * 1000.0,
                alert.overlay_budget_pages,
                reason_text,
            ),
            OverlayPressureKind::NativeEviction {
                eviction,
                reason_text,
            } => format!(
                "[OverlayBudget][Eviction] page={} target={} reason={} budget={}",
                eviction.page_index + 1,
                eviction.target.label(),
                reason_text,
                alert.overlay_budget_pages,
            ),
        }
    }

    fn overlay_pressure_span_payload(&self, alert: &OverlayPressureAlert, summary: &str) -> String {
        let kind_info = match &alert.kind {
            OverlayPressureKind::NativeRender { span, reason_text } => json!({
                "type": "native_render",
                "page": span.page_index + 1,
                "target": span.target.label(),
                "cache_hit": span.cache_hit,
                "duration_ms": span.duration.as_secs_f32() * 1000.0,
                "reason": reason_text,
            }),
            OverlayPressureKind::NativeEviction {
                eviction,
                reason_text,
            } => json!({
                "type": "native_eviction",
                "page": eviction.page_index + 1,
                "target": eviction.target.label(),
                "eviction_reason": eviction.reason,
                "reason": reason_text,
            }),
        };
        let payload = json!({
            "id": alert.id(),
            "tranche": alert.tranche_label(),
            "tranche_url": alert.tranche_url(),
            "overlay_budget_pages": alert.overlay_budget_pages,
            "highlight_page": alert.highlight_page,
            "age_secs": alert.age_secs(),
            "span": kind_info,
            "summary": summary,
        });
        serde_json::to_string(&payload).unwrap_or_else(|_| summary.to_string())
    }

    fn log_qa_span_copy(&mut self, alert: &OverlayPressureAlert, summary: &str) {
        let payload = self.overlay_pressure_span_payload(alert, summary);
        self.push_status(format!("QA span copy: {}", payload));
    }

    fn replay_pdf_render_event(&self, event: &PdfRenderEvent) {
        let highlight_page = self.pdf_render_state.highlighted_page == Some(event.page_index);
        let replay_span = tracing::span!(
            Level::TRACE,
            "PdfRenderEventReplay",
            budget_plan = "shell.performance_budget",
            page = event.page_index + 1,
            kind = ?event.kind,
            highlight_page = highlight_page,
            overlay_budget_pages = event.overlay_budget_pages,
            overlays_drawn = event.overlays_drawn,
            overlay_reason = ?event.overlay_reason.as_deref(),
        );
        let _enter = replay_span.enter();
        trace!(event = ?event.describe(), "Replayed PDF render event for QA");
    }

    fn replay_native_render_span(&self, span: &NativeRenderSpan) {
        let highlight_page = self.pdf_render_state.highlighted_page == Some(span.page_index);
        let replay_span = tracing::span!(
            Level::TRACE,
            "PdfNativeRenderReplay",
            budget_plan = "shell.performance_budget",
            target = ?span.target,
            page = span.page_index + 1,
            highlight_page = highlight_page,
            cache_hit = span.cache_hit,
            duration_ms = span.duration.as_secs_f32(),
            overlay_budget_pages = self.pdf_render_state.overlay_budget_pages(),
        );
        let _enter = replay_span.enter();
        trace!(span = ?span.describe(), "Replayed native render span for QA");
    }

    fn replay_native_eviction(&self, event: &NativeRenderEviction) {
        let highlight_page = self.pdf_render_state.highlighted_page == Some(event.page_index);
        let replay_span = tracing::span!(
            Level::TRACE,
            "PdfNativeEvictionReplay",
            budget_plan = "shell.performance_budget",
            target = ?event.target,
            page = event.page_index + 1,
            highlight_page = highlight_page,
            reason = event.reason,
            overlay_budget_pages = self.pdf_render_state.overlay_budget_pages(),
        );
        let _enter = replay_span.enter();
        trace!(event = ?event.describe(), "Replayed native eviction for QA");
    }

    fn replay_overlay_pressure_alert(&self, alert: &OverlayPressureAlert) {
        match &alert.kind {
            OverlayPressureKind::NativeRender { span, .. } => self.replay_native_render_span(span),
            OverlayPressureKind::NativeEviction { eviction, .. } => {
                self.replay_native_eviction(eviction)
            }
        }
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

    fn regression_snapshot_event_links(&self, snapshot: &RegressionSnapshot) -> RegressionSnapshotEventLinks {
        let render_events = self
            .pdf_render_state
            .recent_render_events()
            .iter()
            .filter(|event| Self::matches_snapshot_page(snapshot, event.page_index))
            .filter(|event| {
                Self::within_snapshot_window(
                    snapshot.timestamp,
                    event.timestamp,
                    REGRESSION_EVENT_WINDOW,
                )
            })
            .cloned()
            .collect();
        let throttle_events = self
            .pdf_render_state
            .recent_throttle_events()
            .iter()
            .filter(|event| Self::matches_snapshot_page(snapshot, event.page_index))
            .filter(|event| {
                Self::within_snapshot_window(
                    snapshot.timestamp,
                    event.timestamp,
                    REGRESSION_EVENT_WINDOW,
                )
            })
            .cloned()
            .collect();
        let status_entries = self
            .status_log
            .iter()
            .filter(|entry| {
                Self::within_snapshot_window(
                    snapshot.timestamp,
                    entry.timestamp,
                    REGRESSION_EVENT_WINDOW,
                )
            })
            .cloned()
            .collect();
        RegressionSnapshotEventLinks {
            render_events,
            throttle_events,
            status_entries,
        }
    }

    fn regression_snapshot_timeline_entries(
        &self,
        snapshot: &RegressionSnapshot,
        event_links: &RegressionSnapshotEventLinks,
    ) -> Vec<RegressionSnapshotTimelineEntry> {
        let mut entries = Vec::new();
        for alert in self
            .pdf_render_state
            .recent_overlay_pressure_alerts()
            .iter()
            .filter(|alert| {
                Self::within_snapshot_window(
                    snapshot.timestamp,
                    alert.timestamp,
                    REGRESSION_EVENT_WINDOW,
                )
            })
            .filter(|alert| {
                alert
                    .highlight_page
                    .map_or(true, |page| Self::matches_snapshot_page(snapshot, page))
            })
        {
            entries.push(RegressionSnapshotTimelineEntry {
                kind: RegressionSnapshotTimelineKind::OverlayAlert(alert.clone()),
                timestamp: alert.timestamp,
            });
        }
        for event in event_links.render_events.iter() {
            entries.push(RegressionSnapshotTimelineEntry {
                kind: RegressionSnapshotTimelineKind::PdfRenderEvent(event.clone()),
                timestamp: event.timestamp,
            });
        }
        for event in event_links.throttle_events.iter() {
            entries.push(RegressionSnapshotTimelineEntry {
                kind: RegressionSnapshotTimelineKind::PdfThrottleEvent(event.clone()),
                timestamp: event.timestamp,
            });
        }
        for status in event_links.status_entries.iter() {
            entries.push(RegressionSnapshotTimelineEntry {
                kind: RegressionSnapshotTimelineKind::Status(status.clone()),
                timestamp: status.timestamp,
            });
        }
        entries.sort_by_key(|entry| entry.timestamp);
        entries
    }

    fn matches_snapshot_page(snapshot: &RegressionSnapshot, page_index: usize) -> bool {
        snapshot.current_page.map_or(true, |page| page == page_index)
    }

    fn within_snapshot_window(snapshot_ts: Instant, event_ts: Instant, window: Duration) -> bool {
        if event_ts >= snapshot_ts {
            event_ts.duration_since(snapshot_ts) <= window
        } else {
            snapshot_ts.duration_since(event_ts) <= window
        }
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
        let focus_request = self.overlay_pressure_focus;
        let mut overlay_warning_rect: Option<Rect> = None;
        CollapsingHeader::new("PDF diagnostics")
            .id_source("pdf-diagnostics")
            .default_open(false)
            .open(if focus_request { Some(true) } else { None })
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
                ui.separator();
                ui.label("Native render traces:");
                let native_spans = self.pdf_render_state.recent_native_render_spans();
                if native_spans.is_empty() {
                    ui.label("(No native renders yet)");
                } else {
                    for span in native_spans.iter().rev() {
                        ui.label(
                            RichText::new(format!(
                                "{} ({:.2?} ago)",
                                span.describe(),
                                Instant::now().saturating_duration_since(span.timestamp)
                            ))
                            .small()
                            .weak(),
                        );
                    }
                }
                ui.separator();
                ui.label("Native render evictions:");
                let evictions = self.pdf_render_state.recent_native_evictions();
                if evictions.is_empty() {
                    ui.label("(No evictions yet)");
                } else {
                    for event in evictions.iter().rev() {
                        ui.label(
                            RichText::new(format!(
                                "{} ({:.2?} ago)",
                                event.describe(),
                                Instant::now().saturating_duration_since(event.timestamp)
                            ))
                            .small()
                            .weak(),
                        );
                    }
                }
                ui.separator();
                let warning_label = ui.label("Overlay pressure warnings:");
                overlay_warning_rect = Some(warning_label.rect);
                let overlay_warnings = self
                    .pdf_render_state
                    .recent_overlay_pressure_alerts()
                    .to_vec();
                if overlay_warnings.is_empty() {
                    ui.label("(No overlay pressure warnings yet)");
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Related tranches:");
                        ui.hyperlink_to("Reader Rendering Core", READER_RENDR_ROADMAP_URL);
                        ui.hyperlink_to("PDF Subsystem", PDF_SUBSYSTEM_ROADMAP_URL);
                        ui.hyperlink_to(
                            "Implementation prioritization",
                            PRIORITIZATION_ROADMAP_URL,
                        );
                    });
                    for alert in overlay_warnings.iter().rev() {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(self.overlay_pressure_badge(alert));
                                ui.label(
                                    RichText::new(format!(
                                        "{} ({:.1}s ago)",
                                        alert.describe(),
                                        alert.age_secs()
                                    ))
                                    .small()
                                    .weak(),
                                );
                                ui.label(
                                    RichText::new(format!("[span id: {}]", alert.id()))
                                        .small()
                                        .weak(),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label("Tranche link:");
                                ui.hyperlink_to(alert.tranche_label(), alert.tranche_url());
                            });
                            ui.horizontal(|ui| {
                                if ui.button("Replay pressure span").clicked() {
                                    self.replay_overlay_pressure_alert(alert);
                                }
                                if ui.button("Copy span data").clicked() {
                                    let summary = self.overlay_pressure_span_summary(alert);
                                    ui.ctx()
                                        .output_mut(|output| output.copied_text = summary.clone());
                                    trace!(span_summary = %summary, "Copied overlay pressure span for QA");
                                    self.log_qa_span_copy(alert, &summary);
                                }
                                if ui.button("Log QA JSON").clicked() {
                                    let summary = self.overlay_pressure_span_summary(alert);
                                    self.log_qa_span_copy(alert, &summary);
                                }
                            });
                        });
                    }
                }
                ui.separator();
                ui.label("Audio budget traces:");
                let audio_events = self.audio_diagnostics.recent_events().to_vec();
                if audio_events.is_empty() {
                    ui.label("(No audio JumpToSentence events yet)");
                } else {
                ui.horizontal(|ui| {
                    ui.label("Related tranches:");
                    ui.hyperlink_to("Audio & TTS integration", TTS_ROADMAP_URL);
                        ui.hyperlink_to("Reader Rendering Core", READER_RENDR_ROADMAP_URL);
                        ui.hyperlink_to("Implementation prioritization", PRIORITIZATION_ROADMAP_URL);
                    });
                    for event in audio_events.iter().rev() {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{} ({:.1}s ago)",
                                        event.describe(),
                                        event.age_secs()
                                    ))
                                    .small()
                                    .weak(),
                                );
                                ui.label(
                                    RichText::new(format!("[span id: {}]", event.id))
                                        .small()
                                        .weak(),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(format!(
                                    "Anchor: {} (auto_scroll: {}, overlay budget {} pages)",
                                    event.fallback.label(),
                                    event.auto_scroll,
                                    event.overlay_snapshot.budget_pages
                                ));
                            });
                            ui.horizontal(|ui| {
                                if ui.button("Replay audio span").clicked() {
                                    self.replay_audio_event(event);
                                }
                                if ui.button("Copy QA JSON").clicked() {
                                    let summary = event.describe();
                                    ui.ctx()
                                        .output_mut(|output| output.copied_text = summary.clone());
                                    trace!(span_summary = %summary, "Copied audio budget span for QA");
                                    self.log_qa_audio_copy(event, &summary);
                                }
                                if ui.button("Log QA JSON").clicked() {
                                    let summary = event.describe();
                                    self.log_qa_audio_copy(event, &summary);
                                }
                            });
                        });
                    }
                }
                ui.separator();
                ui.label("Settings trace events:");
                let settings_events = self.settings_trace_events.clone();
                if settings_events.is_empty() {
                    ui.label("(No settings spans captured yet)");
                } else {
                    for event in settings_events.iter().rev() {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "{} ({:.1}s ago)",
                                    event.describe(),
                                    event.age_secs()
                                ))
                                .small()
                                .weak(),
                            );
                            ui.label(
                                RichText::new(format!("[span id: {}]", event.id))
                                    .small()
                                    .weak(),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.hyperlink_to("Settings roadmap", event.roadmap_url);
                            if ui.button("Replay settings span").clicked() {
                                self.replay_settings_event(event);
                            }
                            if ui.button("Copy QA JSON").clicked() {
                                let summary = event.summary.clone();
                                ui.ctx()
                                    .output_mut(|output| output.copied_text = summary.clone());
                                trace!(span_summary = %summary, "Copied settings span for QA");
                                self.log_settings_trace_copy(event);
                            }
                        });
                    }
                }
                ui.separator();
                ui.label("Persistence trace events:");
                let persistence_events = self.persistence_trace_events.clone();
                if persistence_events.is_empty() {
                    ui.label("(No persistence spans yet)");
                } else {
                    for event in persistence_events.iter().rev() {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "{} ({:.1}s ago)",
                                    event.describe(),
                                    event.age_secs()
                                ))
                                .small()
                                .weak(),
                            );
                            ui.label(
                                RichText::new(format!("[span id: {}]", event.id))
                                    .small()
                                    .weak(),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.hyperlink_to("Persistence roadmap", event.roadmap_url);
                            if ui.button("Replay persistence span").clicked() {
                                self.replay_persistence_event(event);
                            }
                            if ui.button("Copy QA JSON").clicked() {
                                let summary = event.describe();
                                ui.ctx()
                                    .output_mut(|output| output.copied_text = summary.clone());
                                trace!(span_summary = %summary, "Copied persistence span for QA");
                                self.log_persistence_trace_copy(event);
                            }
                        });
                    }
                }
                ui.separator();
                ui.label("Regression watchlist:");
                let regression_snapshots = self.regression_snapshots.clone();
                if regression_snapshots.is_empty() {
                    ui.label("(No regression snapshots captured yet)");
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Related QA resources:");
                        ui.hyperlink_to("QA checklist", QA_REGRESSION_URL);
                        ui.hyperlink_to("Settings/persistence roadmap (Tranche 6)", SETTINGS_ROADMAP_URL);
                    });
                    for snapshot in regression_snapshots.iter().rev() {
                        let event_links = self.regression_snapshot_event_links(snapshot);
                        let timeline_entries =
                            self.regression_snapshot_timeline_entries(snapshot, &event_links);
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{} ({:.1}s ago) [id {}]",
                                        snapshot.describe(),
                                        snapshot.age_secs(),
                                        snapshot.id
                                    ))
                                    .small()
                                    .weak(),
                                );
                            });
                            if let Some(source_path) = snapshot.source_path.as_ref() {
                                ui.horizontal(|ui| {
                                    ui.label("Source:");
                                    ui.label(
                                        RichText::new(source_path)
                                            .small()
                                            .weak()
                                            .monospace(),
                                    );
                                });
                            }
                            if let Some(page) = snapshot.current_page {
                                ui.label(format!("Page: {}", page + 1));
                            }
                            if let Some(sentence) = snapshot.highlighted_sentence {
                                ui.label(format!("Highlighted sentence: {}", sentence + 1));
                            }
                            if let Some(overlay) = snapshot.overlay_snapshot.as_ref() {
                                let overlay_reason = overlay
                                    .overlay_reason
                                    .as_deref()
                                    .unwrap_or("unknown");
                                ui.label(format!(
                                    "Overlay budget {} pages (reason {})",
                                    overlay.budget_pages, overlay_reason
                                ));
                            }
                            if !timeline_entries.is_empty() {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("Timeline:");
                                    for entry in timeline_entries.iter() {
                                        let button = Button::new(entry.badge_label(snapshot.timestamp))
                                            .rounding(6.0)
                                            .fill(entry.badge_color());
                                        if ui.add(button).clicked() {
                                            let kind = entry.kind.clone();
                                            match kind {
                                                RegressionSnapshotTimelineKind::OverlayAlert(
                                                    alert,
                                                ) => {
                                                    self.overlay_pressure_focus = true;
                                                    self.overlay_eviction_warning_at =
                                                        Some(Instant::now());
                                                    self.push_status(format!(
                                                        "QA timeline overlay alert: {}",
                                                        alert.describe()
                                                    ));
                                                    self.replay_overlay_pressure_alert(&alert);
                                                }
                                                RegressionSnapshotTimelineKind::PdfRenderEvent(
                                                    event,
                                                ) => {
                                                    self.replay_pdf_render_event(&event);
                                                }
                                                RegressionSnapshotTimelineKind::PdfThrottleEvent(
                                                    event,
                                                ) => {
                                                    self.replay_throttle_span(&event);
                                                }
                                                RegressionSnapshotTimelineKind::Status(status) => {
                                                    self.push_status(format!(
                                                        "QA timeline status: {}",
                                                        status.message
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                });
                            }
                            ui.horizontal(|ui| {
                                ui.label("Related docs:");
                                ui.hyperlink_to("QA checklist", QA_REGRESSION_URL);
                                ui.hyperlink_to(
                                    snapshot.scenario.label(),
                                    snapshot.scenario.roadmap_url(),
                                );
                            });
                            ui.horizontal(|ui| {
                                if ui.button("Replay regression snapshot").clicked() {
                                    self.replay_regression_snapshot(snapshot);
                                }
                                if ui.button("Copy QA JSON").clicked() {
                                    let payload = self.regression_snapshot_payload(snapshot);
                                    ui.ctx()
                                        .output_mut(|output| output.copied_text = payload.clone());
                                    trace!(
                                        span_summary = %snapshot.describe(),
                                        "Copied regression snapshot QA JSON"
                                    );
                                    self.log_regression_snapshot_copy(snapshot, &payload);
                                }
                                if ui.button("Log QA JSON").clicked() {
                                    let payload = self.regression_snapshot_payload(snapshot);
                                    self.log_regression_snapshot_copy(snapshot, &payload);
                                }
                            });
                            if !event_links.render_events.is_empty() {
                                ui.label("Related PDF render spans:");
                                for event in event_links.render_events.iter() {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(event.describe())
                                                .small()
                                                .weak(),
                                        );
                                        if ui.button("Replay render span").clicked() {
                                            self.replay_pdf_render_event(event);
                                        }
                                    });
                                }
                            }
                            if !event_links.throttle_events.is_empty() {
                                ui.label("Related throttle spans:");
                                for event in event_links.throttle_events.iter() {
                                    ui.horizontal(|ui| {
                                        ui.label(Self::throttle_badge(event.kind));
                                        ui.label(
                                            RichText::new(event.describe())
                                                .small()
                                                .weak(),
                                        );
                                        if ui.button("Replay throttle span").clicked() {
                                            self.replay_throttle_span(event);
                                        }
                                    });
                                }
                            }
                            if let Some(overlay_decision) = snapshot.overlay_snapshot.clone() {
                                let scenario_label = snapshot.scenario.label();
                                ui.horizontal(|ui| {
                                    if ui.button("Replay overlay decision").clicked() {
                                        self.replay_overlay_span(
                                            scenario_label,
                                            overlay_decision.clone(),
                                        );
                                    }
                                });
                            }
                        });
                    }
                }
                if focus_request {
                    if let Some(rect) = overlay_warning_rect {
                        ui.scroll_to_rect(rect, Some(Align::Center));
                    }
                    ui.label(
                        RichText::new("Overlay pressure focus requested.")
                            .color(Color32::from_rgb(220, 200, 120))
                            .small()
                            .strong(),
                    );
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
        self.maybe_record_overlay_retry(&overlay_snapshot, snapshot);
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
        let render_result = {
            let renderer = self.pdf_renderer.as_mut()?;
            let outcome = match target {
                RenderTarget::Canvas => renderer.render_canvas(source_path, page_index),
                RenderTarget::TextLayer => renderer.render_text_layer(source_path, page_index),
            };
            match outcome {
                Ok(outcome) => {
                    let render_span = NativeRenderSpan {
                        timestamp: Instant::now(),
                        target,
                        page_index,
                        duration: outcome.duration,
                        cache_hit: outcome.cache_hit,
                    };
                    let evictions = renderer.drain_eviction_events();
                    Ok((outcome, render_span, evictions))
                }
                Err(err) => Err(err),
            }
        };
        match render_result {
            Ok((outcome, render_span, evictions)) => {
                let span = tracing::span!(
                    Level::TRACE,
                    "PdfNativeRender",
                    budget_plan = "shell.performance_budget",
                    target = ?target,
                    page = page_index + 1,
                    cache_hit = render_span.cache_hit,
                    duration_ms = render_span.duration.as_secs_f32(),
                );
                let _enter = span.enter();
                self.capture_overlay_pressure_from_native_render_span(&render_span);
                self.pdf_render_state.record_native_render_span(render_span);
                for eviction in evictions {
                    self.capture_overlay_pressure_from_native_eviction(&eviction);
                    self.pdf_render_state.record_native_eviction(eviction);
                }
                Some(outcome.image)
            }
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

    fn render_modals(&mut self, ctx: &Context, reader_snapshot: Option<&ReaderSnapshot>) {
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
            self.record_persistence_event(PersistenceTrigger::SafeQuit, "safe_quit_flow");
            self.record_regression_snapshot(
                RegressionScenario::BookmarkRestore {
                    trigger: PersistenceTrigger::SafeQuit,
                },
                reader_snapshot,
                None,
            );
            self.execute_command(AppCommand::SafeQuit);
        }
        if close_reader_confirm_modal {
            show_reader_confirm_modal = false;
        }
        self.show_reader_confirm_modal = show_reader_confirm_modal;
        if return_confirmed {
            self.record_persistence_event(PersistenceTrigger::SessionClose, "reader_close_flow");
            self.record_regression_snapshot(
                RegressionScenario::BookmarkRestore {
                    trigger: PersistenceTrigger::SessionClose,
                },
                reader_snapshot,
                None,
            );
            self.execute_command(AppCommand::ReturnToStarter);
        }
    }

    fn render_status(&mut self, ctx: &Context) {
        TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Status log:");
                for entry in &self.status_log {
                    ui.label(format!("{} ({:.1}s)", entry.message, entry.age_secs()));
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
        self.render_modals(ctx, reader_snapshot);
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

#[derive(Clone, Debug)]
struct AudioBudgetEvent {
    id: usize,
    timestamp: Instant,
    command: &'static str,
    auto_scroll: bool,
    target_sentence: Option<usize>,
    anchor: Option<usize>,
    fallback: AnchorFallback,
    overlay_snapshot: OverlayDecisionSnapshot,
    highlight_page: Option<usize>,
}

impl AudioBudgetEvent {
    fn describe(&self) -> String {
        let sentence_label = self
            .target_sentence
            .map(|idx| format!("{}", idx + 1))
            .unwrap_or_else(|| "unknown".to_string());
        let fallback_label = self.fallback.label();
        format!(
            "{} → sentence {} ({fallback_label}, budget {})",
            self.command, sentence_label, self.overlay_snapshot.budget_pages
        )
    }

    fn age_secs(&self) -> f32 {
        self.timestamp.elapsed().as_secs_f32()
    }
}

#[derive(Default)]
struct AudioDiagnostics {
    events: Vec<AudioBudgetEvent>,
    next_id: usize,
}

impl AudioDiagnostics {
    fn record(&mut self, event: AudioBudgetEvent) {
        const MAX_AUDIO_EVENTS: usize = 16;
        self.events.push(event);
        if self.events.len() > MAX_AUDIO_EVENTS {
            self.events.remove(0);
        }
    }

    fn recent_events(&self) -> &[AudioBudgetEvent] {
        &self.events
    }

    fn allocate_event_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        id
    }
}

#[derive(Clone, Debug)]
struct SettingsTraceEvent {
    id: usize,
    timestamp: Instant,
    description: &'static str,
    summary: String,
    roadmap_url: &'static str,
}

impl SettingsTraceEvent {
    fn describe(&self) -> String {
        format!("{} — {}", self.description, self.summary)
    }

    fn age_secs(&self) -> f32 {
        self.timestamp.elapsed().as_secs_f32()
    }
}

#[derive(Clone, Debug)]
struct PersistenceTraceEvent {
    id: usize,
    timestamp: Instant,
    trigger: PersistenceTrigger,
    description: &'static str,
    roadmap_url: &'static str,
}

impl PersistenceTraceEvent {
    fn describe(&self) -> String {
        format!("{} (trigger={:?})", self.description, self.trigger)
    }

    fn age_secs(&self) -> f32 {
        self.timestamp.elapsed().as_secs_f32()
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum RegressionScenario {
    OverlayBacklog {
        reason: &'static str,
    },
    BookmarkRestore {
        trigger: PersistenceTrigger,
    },
}

impl RegressionScenario {
    fn label(&self) -> &'static str {
        match self {
            RegressionScenario::OverlayBacklog { .. } => "Overlay backlog",
            RegressionScenario::BookmarkRestore { .. } => "Bookmark restore",
        }
    }

    fn roadmap_url(&self) -> &'static str {
        match self {
            RegressionScenario::OverlayBacklog { .. } => READER_RENDR_ROADMAP_URL,
            RegressionScenario::BookmarkRestore { .. } => SETTINGS_ROADMAP_URL,
        }
    }

    fn persistence_trigger(&self) -> Option<PersistenceTrigger> {
        match self {
            RegressionScenario::BookmarkRestore { trigger } => Some(*trigger),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
struct RegressionSnapshot {
    id: usize,
    timestamp: Instant,
    scenario: RegressionScenario,
    source_path: Option<String>,
    current_page: Option<usize>,
    highlighted_sentence: Option<usize>,
    overlay_snapshot: Option<OverlayDecisionSnapshot>,
}

impl RegressionSnapshot {
    fn describe(&self) -> String {
        match &self.scenario {
            RegressionScenario::OverlayBacklog { reason } => {
                let page_label = self
                    .current_page
                    .map(|page| format!("page {}", page + 1))
                    .unwrap_or_else(|| "page unknown".to_string());
                let overlay_reason = self
                    .overlay_snapshot
                    .as_ref()
                    .and_then(|overlay| overlay.overlay_reason.as_deref())
                    .unwrap_or("unknown");
                format!(
                    "{} ({}) on {} (budget {} pages, overlay reason {})",
                    self.scenario.label(),
                    reason,
                    page_label,
                    self.overlay_snapshot
                        .as_ref()
                        .map(|overlay| overlay.budget_pages)
                        .unwrap_or(0),
                    overlay_reason
                )
            }
            RegressionScenario::BookmarkRestore { trigger } => {
                let page_label = self
                    .current_page
                    .map(|page| format!("page {}", page + 1))
                    .unwrap_or_else(|| "page unknown".to_string());
                let sentence_label = self
                    .highlighted_sentence
                    .map(|idx| format!("{}", idx + 1))
                    .unwrap_or_else(|| "unknown sentence".to_string());
                format!(
                    "{} after {:?} on {} (highlighted sentence {})",
                    self.scenario.label(),
                    trigger,
                    page_label,
                    sentence_label
                )
            }
        }
    }

    fn age_secs(&self) -> f32 {
        self.timestamp.elapsed().as_secs_f32()
    }
}

#[derive(Clone, Debug)]
struct StatusLogEntry {
    timestamp: Instant,
    message: String,
}

impl StatusLogEntry {
    fn age_secs(&self) -> f32 {
        self.timestamp.elapsed().as_secs_f32()
    }
}

#[derive(Clone, Debug)]
struct RegressionSnapshotTimelineEntry {
    kind: RegressionSnapshotTimelineKind,
    timestamp: Instant,
}

#[derive(Clone, Debug)]
enum RegressionSnapshotTimelineKind {
    OverlayAlert(OverlayPressureAlert),
    PdfRenderEvent(PdfRenderEvent),
    PdfThrottleEvent(PdfRenderThrottleEvent),
    Status(StatusLogEntry),
}

impl RegressionSnapshotTimelineEntry {
    fn badge_label(&self, reference: Instant) -> String {
        format!(
            "{} {:.1}s",
            self.kind_label(),
            Self::relative_secs(reference, self.timestamp)
        )
    }

    fn kind_label(&self) -> &'static str {
        match &self.kind {
            RegressionSnapshotTimelineKind::OverlayAlert(_) => "Overlay",
            RegressionSnapshotTimelineKind::PdfRenderEvent(_) => "Canvas/Text",
            RegressionSnapshotTimelineKind::PdfThrottleEvent(_) => "Throttle",
            RegressionSnapshotTimelineKind::Status(_) => "Status",
        }
    }

    fn badge_color(&self) -> Color32 {
        match &self.kind {
            RegressionSnapshotTimelineKind::OverlayAlert(_) => Color32::from_rgb(222, 163, 91),
            RegressionSnapshotTimelineKind::PdfRenderEvent(_) => Color32::from_rgb(130, 190, 230),
            RegressionSnapshotTimelineKind::PdfThrottleEvent(_) => Color32::from_rgb(200, 120, 120),
            RegressionSnapshotTimelineKind::Status(_) => Color32::from_rgb(110, 170, 200),
        }
    }

    fn relative_secs(reference: Instant, timestamp: Instant) -> f32 {
        if timestamp >= reference {
            timestamp.duration_since(reference).as_secs_f32()
        } else {
            reference.duration_since(timestamp).as_secs_f32()
        }
    }
}

#[derive(Clone, Debug)]
struct RegressionSnapshotEventLinks {
    render_events: Vec<PdfRenderEvent>,
    throttle_events: Vec<PdfRenderThrottleEvent>,
    status_entries: Vec<StatusLogEntry>,
}

#[derive(Clone, Debug)]
struct OverlayPressureAlert {
    id: usize,
    timestamp: Instant,
    overlay_budget_pages: usize,
    highlight_page: Option<usize>,
    kind: OverlayPressureKind,
}

impl OverlayPressureAlert {
    fn new(
        id: usize,
        kind: OverlayPressureKind,
        overlay_budget_pages: usize,
        highlight_page: Option<usize>,
    ) -> Self {
        Self {
            id,
            timestamp: Instant::now(),
            overlay_budget_pages,
            highlight_page,
            kind,
        }
    }

    fn id(&self) -> usize {
        self.id
    }

    fn tranche_url(&self) -> &'static str {
        self.kind.tranche_url()
    }

    fn age_secs(&self) -> f32 {
        self.timestamp.elapsed().as_secs_f32()
    }

    fn describe(&self) -> String {
        let highlight_note = self
            .highlight_page
            .map(|page| format!(" (highlight page {})", page + 1))
            .unwrap_or_default();
        format!(
            "{}: {}{}",
            self.kind.label(),
            self.kind.detail(),
            highlight_note
        )
    }

    fn tranche_label(&self) -> &'static str {
        match self.kind {
            OverlayPressureKind::NativeRender { .. } => "PDF Subsystem (Tranche 4)",
            OverlayPressureKind::NativeEviction { .. } => "PDF Subsystem (Tranche 4)",
        }
    }
}

#[derive(Clone, Debug)]
enum OverlayPressureKind {
    NativeRender {
        span: NativeRenderSpan,
        reason_text: String,
    },
    NativeEviction {
        eviction: NativeRenderEviction,
        reason_text: String,
    },
}

impl OverlayPressureKind {
    fn label(&self) -> &'static str {
        match self {
            OverlayPressureKind::NativeRender { .. } => "Native render pressure",
            OverlayPressureKind::NativeEviction { .. } => "Eviction pressure",
        }
    }

    fn page_index(&self) -> usize {
        match self {
            OverlayPressureKind::NativeRender { span, .. } => span.page_index,
            OverlayPressureKind::NativeEviction { eviction, .. } => eviction.page_index,
        }
    }

    fn detail(&self) -> String {
        match self {
            OverlayPressureKind::NativeRender { span, reason_text } => format!(
                "{} (cache hit: {}, duration {:.2?})",
                reason_text, span.cache_hit, span.duration
            ),
            OverlayPressureKind::NativeEviction {
                eviction,
                reason_text,
            } => format!(
                "{} (target {} {}, reason {})",
                reason_text,
                eviction.target.label(),
                eviction.page_index + 1,
                eviction.reason
            ),
        }
    }

    fn badge_info(&self) -> (Color32, &'static str) {
        match self {
            OverlayPressureKind::NativeRender { .. } => (Color32::from_rgb(220, 140, 80), "RENDER"),
            OverlayPressureKind::NativeEviction { .. } => (Color32::from_rgb(220, 90, 90), "EVICT"),
        }
    }

    fn tranche_url(&self) -> &'static str {
        match self {
            OverlayPressureKind::NativeRender { .. } => PDF_SUBSYSTEM_ROADMAP_URL,
            OverlayPressureKind::NativeEviction { .. } => PDF_SUBSYSTEM_ROADMAP_URL,
        }
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
    native_render_spans: Vec<NativeRenderSpan>,
    native_eviction_events: Vec<NativeRenderEviction>,
    overlay_pressure_alerts: Vec<OverlayPressureAlert>,
    next_overlay_alert_id: usize,
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
        self.native_render_spans.clear();
        self.native_eviction_events.clear();
        self.overlay_pressure_alerts.clear();
        self.next_overlay_alert_id = 0;
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

    fn record_native_render_span(&mut self, span: NativeRenderSpan) {
        const MAX_NATIVE_SPANS: usize = 16;
        self.native_render_spans.push(span);
        if self.native_render_spans.len() > MAX_NATIVE_SPANS {
            self.native_render_spans.remove(0);
        }
    }

    fn record_native_eviction(&mut self, event: NativeRenderEviction) {
        const MAX_NATIVE_EVICTIONS: usize = 12;
        self.native_eviction_events.push(event);
        if self.native_eviction_events.len() > MAX_NATIVE_EVICTIONS {
            self.native_eviction_events.remove(0);
        }
    }

    fn recent_render_events(&self) -> &[PdfRenderEvent] {
        &self.render_events
    }

    fn recent_native_render_spans(&self) -> &[NativeRenderSpan] {
        &self.native_render_spans
    }

    fn recent_native_evictions(&self) -> &[NativeRenderEviction] {
        &self.native_eviction_events
    }

    fn record_overlay_pressure_alert(&mut self, alert: OverlayPressureAlert) {
        const MAX_OVERLAY_PRESSURE_ALERTS: usize = 12;
        self.overlay_pressure_alerts.push(alert);
        if self.overlay_pressure_alerts.len() > MAX_OVERLAY_PRESSURE_ALERTS {
            self.overlay_pressure_alerts.remove(0);
        }
    }

    fn allocate_overlay_alert_id(&mut self) -> usize {
        let id = self.next_overlay_alert_id;
        self.next_overlay_alert_id = self.next_overlay_alert_id.wrapping_add(1);
        id
    }

    fn recent_overlay_pressure_alerts(&self) -> &[OverlayPressureAlert] {
        &self.overlay_pressure_alerts
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
