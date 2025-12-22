use super::Effect;
use super::super::messages::Component;
use super::super::state::{
    App, MAX_LETTER_SPACING, MAX_MARGIN, MAX_WORD_SPACING, apply_component,
};
use crate::pagination::{MAX_FONT_SIZE, MIN_FONT_SIZE};
use tracing::{debug, info};

impl App {
    pub(super) fn handle_font_size_changed(&mut self, size: u32, _effects: &mut Vec<Effect>) {
        let clamped = size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        if clamped != self.config.font_size {
            debug!(old = self.config.font_size, new = clamped, "Font size changed");
            self.config.font_size = clamped;
            self.repaginate();
        }
    }

    pub(super) fn handle_toggle_theme(&mut self, effects: &mut Vec<Effect>) {
        let next = match self.config.theme {
            crate::config::ThemeMode::Night => crate::config::ThemeMode::Day,
            crate::config::ThemeMode::Day => crate::config::ThemeMode::Night,
        };
        info!(night_mode = matches!(next, crate::config::ThemeMode::Night), "Toggled theme");
        self.config.theme = next;
        effects.push(Effect::SaveConfig);
    }

    pub(super) fn handle_toggle_settings(&mut self, effects: &mut Vec<Effect>) {
        debug!("Toggled settings panel");
        self.config.show_settings = !self.config.show_settings;
        effects.push(Effect::SaveConfig);
    }

    pub(super) fn handle_font_family_changed(
        &mut self,
        family: crate::config::FontFamily,
        effects: &mut Vec<Effect>,
    ) {
        debug!(?family, "Font family changed");
        self.config.font_family = family;
        effects.push(Effect::SaveConfig);
    }

    pub(super) fn handle_font_weight_changed(
        &mut self,
        weight: crate::config::FontWeight,
        effects: &mut Vec<Effect>,
    ) {
        debug!(?weight, "Font weight changed");
        self.config.font_weight = weight;
        effects.push(Effect::SaveConfig);
    }

    pub(super) fn handle_line_spacing_changed(&mut self, spacing: f32, effects: &mut Vec<Effect>) {
        self.config.line_spacing = spacing.clamp(0.8, 2.5);
        debug!(line_spacing = self.config.line_spacing, "Line spacing changed");
        effects.push(Effect::SaveConfig);
    }

    pub(super) fn handle_margin_horizontal_changed(
        &mut self,
        margin: u16,
        effects: &mut Vec<Effect>,
    ) {
        self.config.margin_horizontal = margin.min(MAX_MARGIN);
        debug!(
            margin_horizontal = self.config.margin_horizontal,
            "Horizontal margin changed"
        );
        effects.push(Effect::SaveConfig);
    }

    pub(super) fn handle_margin_vertical_changed(
        &mut self,
        margin: u16,
        effects: &mut Vec<Effect>,
    ) {
        self.config.margin_vertical = margin.min(MAX_MARGIN);
        debug!(
            margin_vertical = self.config.margin_vertical,
            "Vertical margin changed"
        );
        effects.push(Effect::SaveConfig);
    }

    pub(super) fn handle_word_spacing_changed(
        &mut self,
        spacing: u32,
        effects: &mut Vec<Effect>,
    ) {
        self.config.word_spacing = spacing.min(MAX_WORD_SPACING);
        debug!(word_spacing = self.config.word_spacing, "Word spacing changed");
        effects.push(Effect::SaveConfig);
    }

    pub(super) fn handle_letter_spacing_changed(
        &mut self,
        spacing: u32,
        effects: &mut Vec<Effect>,
    ) {
        self.config.letter_spacing = spacing.min(MAX_LETTER_SPACING);
        debug!(
            letter_spacing = self.config.letter_spacing,
            "Letter spacing changed"
        );
        effects.push(Effect::SaveConfig);
    }

    pub(super) fn handle_day_highlight_changed(
        &mut self,
        component: Component,
        value: f32,
        effects: &mut Vec<Effect>,
    ) {
        self.config.day_highlight = apply_component(self.config.day_highlight, component, value);
        debug!(?component, value, "Day highlight updated");
        effects.push(Effect::SaveConfig);
    }

    pub(super) fn handle_night_highlight_changed(
        &mut self,
        component: Component,
        value: f32,
        effects: &mut Vec<Effect>,
    ) {
        self.config.night_highlight = apply_component(self.config.night_highlight, component, value);
        debug!(?component, value, "Night highlight updated");
        effects.push(Effect::SaveConfig);
    }
}
