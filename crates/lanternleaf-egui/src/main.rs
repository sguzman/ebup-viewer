mod helpers;

use eframe::{
    NativeOptions,
    egui::{
        self, Align, Button, CentralPanel, CollapsingHeader, Color32, Context, Layout, Modifiers,
        RichText, ScrollArea, SidePanel, TopBottomPanel, Ui, Visuals,
    },
    winit,
};
use helpers::{app_config_path, bootstrap_config_from_app_config, format_combo};

use lanternleaf_app::{
    AppRuntime,
    contracts::{BootstrapConfig, ReaderSnapshot, UiMode},
    pipeline::{AppCommand, DispatchPlan, ReaderCommand},
    shortcuts::{ShortcutAction, ShortcutScope, UiShortcutAction},
    state::AppState,
    tracing::init_tracing,
};
use lanternleaf_core::{
    config,
    session::{SessionCommand, TtsPlaybackState},
};
use tracing::{info, trace};

fn main() {
    let config_path = app_config_path();
    let app_config = config::load_config(&config_path);
    let bootstrap_config = bootstrap_config_from_app_config(&app_config);
    let tracing_guard = init_tracing(&bootstrap_config.log_level);

    let runtime = AppRuntime::with_bootstrap_config(&bootstrap_config);
    let mut options = NativeOptions::default();
    options.initial_window_size = Some(egui::vec2(
        app_config.window_width as f32,
        app_config.window_height as f32,
    ));

    info!("Starting LanternLeaf egui shell");

    eframe::run_native(
        "LanternLeaf",
        options,
        Box::new(|cc| {
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
        for event in &ctx.input().events {
            if let egui::Event::Key(key_event) = event {
                if !key_event.pressed {
                    continue;
                }
                if let Some(combo) = format_combo(key_event.key, key_event.modifiers) {
                    let matches = self.runtime.shortcut_registry().matches(&combo, mode_scope);
                    for binding in matches {
                        self.execute_shortcut_action(&binding.action);
                    }
                }
            }
        }
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

    fn render_panels(&mut self, ctx: &Context, state: &AppState) {
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
        });
        SidePanel::right("shortcuts").show(ctx, |ui| {
            ui.heading("Shortcut registry");
            for binding in self.runtime.shortcut_registry().bindings() {
                ui.label(format!("{} → {:?}", binding.combo, binding.action));
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
        let auto_scroll_enabled = self.should_auto_scroll(snapshot);
        if !auto_scroll_enabled {
            self.auto_scroll_state.reset();
        }
        let auto_scroll_align = if snapshot.settings.center_spoken_sentence {
            Align::Center
        } else {
            Align::Min
        };
        ScrollArea::vertical()
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
                    let mut label_text = format!("{}: {}", idx + 1, sentence);
                    if is_search_match {
                        label_text.push_str(" (search match)");
                    }
                    let mut button = Button::new(RichText::new(label_text).size(14.0))
                        .fill(if is_highlighted {
                            highlight_color
                        } else {
                            ui.visuals().widgets.inactive.bg_fill
                        })
                        .wrap(true);
                    if is_highlighted {
                        button = button.text_style(egui::TextStyle::Body);
                    }
                    let response = ui.add(button);
                    if is_highlighted && auto_scroll_enabled {
                        let (resolved_anchor, fallback) =
                            Self::resolve_sentence_anchor(snapshot, idx);
                        if self.auto_scroll_state.should_scroll(idx, fallback) {
                            trace!(
                                jump_to_sentence = idx,
                                highlight_anchor = fallback.label(),
                                canonical_anchor = resolved_anchor,
                                "JumpToSentence: auto-scrolling highlighted sentence"
                            );
                            response.scroll_to_me(Some(auto_scroll_align));
                            self.auto_scroll_state.record(idx, fallback);
                        }
                    }
                    if response.clicked() {
                        trace!(sentence_idx = idx, anchor = ?anchor_idx, "reader sentence clicked");
                        self.execute_reader_command(ReaderCommand::Session(
                            SessionCommand::SentenceClick { sentence_idx: idx },
                        ));
                    }
                    if let Some((anchor, canonical)) = canonical_preview {
                        ui.label(
                            RichText::new(format!("anchor {} → {}", anchor, canonical))
                                .small()
                                .italics()
                                .weak(),
                        );
                    } else if anchor_idx.is_none() {
                        ui.label(RichText::new("anchor missing").small().italics().weak());
                    }
                    ui.separator();
                }
            });
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
        egui::Window::new("Safe quit confirmation")
            .open(&mut self.show_safe_quit_modal)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Are you sure you want to quit?");
                ui.horizontal(|ui| {
                    if ui.button("Yes").clicked() {
                        self.execute_command(AppCommand::SafeQuit);
                        self.show_safe_quit_modal = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_safe_quit_modal = false;
                    }
                });
            });
        egui::Window::new("Reader close confirmation")
            .open(&mut self.show_reader_confirm_modal)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Return to starter after closing reader?");
                ui.horizontal(|ui| {
                    if ui.button("Confirm").clicked() {
                        self.execute_command(AppCommand::ReturnToStarter);
                        self.show_reader_confirm_modal = false;
                    }
                    if ui.button("Dismiss").clicked() {
                        self.show_reader_confirm_modal = false;
                    }
                });
            });
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
        ctx.set_visuals(Visuals::dark());
        self.handle_shortcuts(ctx, &snapshot);
        self.render_top_bar(ctx, &snapshot);
        self.render_panels(ctx, &snapshot);
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
    fn label(self) -> &'static str {
        match self {
            AnchorFallback::Exact => "exact",
            AnchorFallback::Nearest => "nearest",
            AnchorFallback::Missing => "missing",
        }
    }
}

#[derive(Default)]
struct AutoScrollState {
    last_highlighted: Option<(usize, AnchorFallback)>,
}

impl AutoScrollState {
    fn should_scroll(&self, idx: usize, fallback: AnchorFallback) -> bool {
        self.last_highlighted != Some((idx, fallback))
    }

    fn record(&mut self, idx: usize, fallback: AnchorFallback) {
        self.last_highlighted = Some((idx, fallback));
    }

    fn reset(&mut self) {
        self.last_highlighted = None;
    }
}
