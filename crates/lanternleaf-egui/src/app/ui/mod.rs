pub(crate) mod format;
mod starter;
mod reader;

use eframe::egui::{self, CentralPanel, Color32, Context, RichText, SidePanel, TopBottomPanel};
use lanternleaf_app::contracts::{ReaderSnapshot, UiMode};
use lanternleaf_app::state::AppState;

use super::LanternLeafApp;

impl LanternLeafApp {
    pub(crate) fn render_navigation_row(&mut self, ctx: &Context, state: &AppState) {
        if !self.layout_policy.show_status_row || !self.layout_policy.is_narrow() {
            return;
        }
        TopBottomPanel::top("nav_status_row").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Mode: {:?}", self.shell_state.active_mode));
                if state.app_shell.busy {
                    ui.label("Busy");
                }
                if state.app_shell.operations.source_open {
                    ui.label("Opening source");
                }
                if state.app_shell.operations.calibre_load {
                    ui.label("Loading Calibre");
                }
                if state.app_shell.operations.browser_tab_refresh {
                    ui.label("Refreshing browser tabs");
                }
            });
        });
    }

    pub(crate) fn render_top_bar(&mut self, ctx: &Context, state: &AppState) {
        TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.heading("LanternLeaf (egui)");
                    let current_theme =
                        self.resolve_theme(state, state.reader_document.snapshot.as_ref());
                    let next_theme = match current_theme {
                        lanternleaf_core::config::ThemeMode::Day => {
                            lanternleaf_core::config::ThemeMode::Night
                        }
                        lanternleaf_core::config::ThemeMode::Night => {
                            lanternleaf_core::config::ThemeMode::Day
                        }
                    };
                    let label = match next_theme {
                        lanternleaf_core::config::ThemeMode::Day => "Day",
                        lanternleaf_core::config::ThemeMode::Night => "Night",
                    };
                    if ui.button(label).clicked() {
                        self.theme_override = Some(next_theme);
                        if let Some(_snapshot) = state.reader_document.snapshot.as_ref() {
                            self.apply_reader_settings_patch(
                                lanternleaf_core::session::ReaderSettingsPatch {
                                    theme: Some(next_theme),
                                    ..Default::default()
                                },
                                "theme_toggle",
                            );
                        }
                        self.execute_command(lanternleaf_app::pipeline::AppCommand::ToggleTheme);
                    }
                    ui.separator();
                    let allow_recents = !state.app_shell.operations.source_open;
                    if ui
                        .add_enabled(
                            allow_recents,
                            egui::Button::new("Refresh recents (AppCommand::RefreshRecents)"),
                        )
                        .clicked()
                    {
                        self.execute_command(lanternleaf_app::pipeline::AppCommand::RefreshRecents {
                            limit: Some(10),
                        });
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
                            .strong(),
                        );
                    }
                }
            });
        });
    }

    pub(crate) fn render_panels(
        &mut self,
        ctx: &Context,
        state: &AppState,
        reader_snapshot: Option<&ReaderSnapshot>,
    ) {
        let panels = state
            .session
            .session
            .as_ref()
            .map(|session| session.panels)
            .unwrap_or_default();
        let show_search_panel = self.pending_search_focus
            || !state.reader_ui.search_query.trim().is_empty()
            || !state.reader_ui.search_matches.is_empty();
        SidePanel::left("panel_toggle").show(ctx, |ui| {
            ui.heading("Panels");
            if ui
                .button("Toggle settings (AppCommand::ToggleSettingsPanel)")
                .clicked()
            {
                self.execute_command(lanternleaf_app::pipeline::AppCommand::ToggleSettingsPanel);
            }
            if ui
                .button("Toggle stats (AppCommand::ToggleStatsPanel)")
                .clicked()
            {
                self.execute_command(lanternleaf_app::pipeline::AppCommand::ToggleStatsPanel);
            }
            if ui
                .button("Toggle TTS (AppCommand::ToggleTtsPanel)")
                .clicked()
            {
                self.execute_command(lanternleaf_app::pipeline::AppCommand::ToggleTtsPanel);
            }
            ui.label(format!("Settings: {}", panels.show_settings));
            ui.label(format!("Stats: {}", panels.show_stats));
            ui.label(format!("TTS: {}", panels.show_tts));
            ui.label(format!("Search: {}", show_search_panel));
            if panels.show_settings {
                ui.separator();
                ui.heading("Settings");
                self.render_settings_sidebar(ui, reader_snapshot);
            }
            if panels.show_stats {
                ui.separator();
                ui.heading("Stats");
                self.render_stats_panel(ui, reader_snapshot);
            }
            if show_search_panel {
                ui.separator();
                ui.heading("Search");
                self.render_search_panel(ui, state);
            }
            if panels.show_tts {
                ui.separator();
                ui.heading("TTS");
                if let Some(snapshot) = reader_snapshot {
                    self.render_tts_widget(ui, snapshot);
                } else {
                    ui.label("No reader session.");
                }
            }
            ui.separator();
            ui.heading("Status diagnostics");
            self.render_status_diagnostics_panel(ui, state);
            self.render_anchor_diagnostics(ui, reader_snapshot);
        });
        SidePanel::right("shortcuts").show(ctx, |ui| {
            ui.heading("Shortcut registry");
            for binding in self.runtime.shortcut_registry().bindings() {
                ui.label(format!("{} → {:?}", binding.combo, binding.action));
            }
        });
    }

    pub(crate) fn render_center(&mut self, ctx: &Context, state: &AppState) {
        CentralPanel::default().show(ctx, |ui| {
            match state.session.session.as_ref().map(|session| session.mode) {
                Some(UiMode::Reader) => self.render_reader_content(ui, state),
                _ => {
                    self.webview_renderer.clear();
                    self.render_starter_content(ui, state);
                }
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

    pub(crate) fn render_modals(&mut self, ctx: &Context, reader_snapshot: Option<&ReaderSnapshot>) {
        if self.show_safe_quit_modal {
            self.render_safe_quit_modal(ctx);
        }
        if self.show_reader_confirm_modal {
            self.render_reader_confirm_modal(ctx, reader_snapshot);
        }
    }

    pub(crate) fn render_status(&mut self, ctx: &Context) {
        TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Status log:");
                for entry in &self.status_log {
                    ui.label(format!("{} ({:.1}s)", entry.message, entry.age_secs()));
                }
                if self.shell_state.screen_lock_active {
                    ui.colored_label(Color32::YELLOW, "Screen lock active");
                }
            });
            if !self.shell_state.notifications.is_empty() {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Notifications:");
                    for note in &self.shell_state.notifications {
                        let color = match note.level {
                            crate::shell::NotificationLevel::Info => Color32::LIGHT_GRAY,
                            crate::shell::NotificationLevel::Warn => Color32::YELLOW,
                            crate::shell::NotificationLevel::Error => Color32::RED,
                        };
                        ui.colored_label(color, &note.message);
                    }
                });
            }
        });
    }
}
