mod helpers;

use eframe::{
    NativeOptions,
    egui::{
        self, CentralPanel, Context, Key, Layout, Modifiers, SidePanel, TopBottomPanel, Ui, Visuals,
    },
    winit,
};
use helpers::{app_config_path, bootstrap_config_from_app_config, format_combo};

use lanternleaf_app::{
    AppRuntime,
    contracts::{BootstrapConfig, UiMode},
    pipeline::{AppCommand, DispatchPlan, ReaderCommand},
    shortcuts::{ShortcutAction, ShortcutScope, UiShortcutAction},
    tracing::init_tracing,
};
use lanternleaf_core::config;
use tracing::info;

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

    fn handle_shortcuts(&mut self, ctx: &Context, state: &lanternleaf_app::state::AppState) {
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

    fn render_top_bar(&mut self, ctx: &Context, state: &lanternleaf_app::state::AppState) {
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

    fn render_panels(&mut self, ctx: &Context, state: &lanternleaf_app::state::AppState) {
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

    fn render_center(&mut self, ctx: &Context, state: &lanternleaf_app::state::AppState) {
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

    fn render_reader_content(&mut self, ui: &mut Ui, state: &lanternleaf_app::state::AppState) {
        ui.heading("Reader shell");
        ui.label(format!(
            "Current page: {:?} / {:?}",
            state.reader_ui.current_page, state.reader_ui.total_pages
        ));
        ui.horizontal(|ui| {
            if ui
                .button("Play/Pause (ReaderCommand::Session(SessionCommand::TtsTogglePlayPause))")
                .clicked()
            {
                self.execute_reader_command(ReaderCommand::Session(
                    lanternleaf_core::session::SessionCommand::TtsTogglePlayPause,
                ));
            }
            if ui
                .button("Next sentence (ReaderCommand::Session(SessionCommand::TtsSeekNext))")
                .clicked()
            {
                self.execute_reader_command(ReaderCommand::Session(
                    lanternleaf_core::session::SessionCommand::TtsSeekNext,
                ));
            }
            if ui
                .button("Prev sentence (ReaderCommand::Session(SessionCommand::TtsSeekPrev))")
                .clicked()
            {
                self.execute_reader_command(ReaderCommand::Session(
                    lanternleaf_core::session::SessionCommand::TtsSeekPrev,
                ));
            }
        });
        if ui
            .button("Close reader session (AppCommand::CloseReaderSession)")
            .clicked()
        {
            self.execute_command(AppCommand::CloseReaderSession);
            self.show_reader_confirm_modal = true;
        }
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
