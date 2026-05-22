use eframe::egui::{Key, Modifiers};
use lanternleaf_app::contracts::BootstrapConfig;
use lanternleaf_core::config;
use std::path::PathBuf;

pub use lanternleaf_core::workspace::workspace_root_from_cwd;

pub fn app_config_path() -> PathBuf {
    if let Some(value) = std::env::var_os("LANTERNLEAF_CONFIG_PATH") {
        let candidate = PathBuf::from(value);
        return if candidate.is_absolute() {
            candidate
        } else if let Some(root) = workspace_root_from_cwd() {
            root.join(candidate)
        } else {
            candidate
        };
    }
    if let Some(root) = workspace_root_from_cwd() {
        root.join("conf/config.toml")
    } else {
        PathBuf::from("conf/config.toml")
    }
}

pub fn bootstrap_config_from_app_config(app_cfg: &config::AppConfig) -> BootstrapConfig {
    BootstrapConfig {
        theme: app_cfg.theme,
        font_family: app_cfg.font_family,
        font_weight: app_cfg.font_weight,
        day_highlight: app_cfg.day_highlight,
        night_highlight: app_cfg.night_highlight,
        log_level: app_cfg.log_level.as_filter_str().to_string(),
        default_font_size: app_cfg.font_size,
        default_lines_per_page: app_cfg.lines_per_page,
        default_tts_speed: app_cfg.tts_speed,
        default_pause_after_sentence: app_cfg.pause_after_sentence,
        key_toggle_play_pause: app_cfg.key_toggle_play_pause.clone(),
        key_next_sentence: app_cfg.key_next_sentence.clone(),
        key_prev_sentence: app_cfg.key_prev_sentence.clone(),
        key_repeat_sentence: app_cfg.key_repeat_sentence.clone(),
        key_toggle_search: app_cfg.key_toggle_search.clone(),
        key_safe_quit: app_cfg.key_safe_quit.clone(),
        key_toggle_settings: app_cfg.key_toggle_settings.clone(),
        key_toggle_stats: app_cfg.key_toggle_stats.clone(),
        key_toggle_tts: app_cfg.key_toggle_tts.clone(),
        browser_tabs_enabled: app_cfg.browser_tabs_enabled,
        close_browser_tab_on_recent_delete: app_cfg.close_browser_tab_on_recent_delete,
        remote_url: app_cfg.remote_url.clone(),
    }
}

pub fn format_combo(key: Key, modifiers: Modifiers) -> Option<String> {
    let mut parts = Vec::new();
    if modifiers.ctrl {
        parts.push("ctrl");
    }
    if modifiers.alt {
        parts.push("alt");
    }
    if modifiers.shift {
        parts.push("shift");
    }
    if let Some(label) = key_label(key) {
        parts.push(label);
    } else {
        return None;
    }
    Some(parts.join("+"))
}

fn key_label(key: Key) -> Option<&'static str> {
    match key {
        Key::A => Some("a"),
        Key::B => Some("b"),
        Key::C => Some("c"),
        Key::D => Some("d"),
        Key::E => Some("e"),
        Key::F => Some("f"),
        Key::G => Some("g"),
        Key::H => Some("h"),
        Key::I => Some("i"),
        Key::J => Some("j"),
        Key::K => Some("k"),
        Key::L => Some("l"),
        Key::M => Some("m"),
        Key::N => Some("n"),
        Key::O => Some("o"),
        Key::P => Some("p"),
        Key::Q => Some("q"),
        Key::R => Some("r"),
        Key::S => Some("s"),
        Key::T => Some("t"),
        Key::U => Some("u"),
        Key::V => Some("v"),
        Key::W => Some("w"),
        Key::X => Some("x"),
        Key::Y => Some("y"),
        Key::Z => Some("z"),
        Key::Space => Some("space"),
        Key::Backslash => Some("/"),
        Key::Escape => Some("escape"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Key, Modifiers};

    #[test]
    fn format_combo_with_ctrl() {
        let combo = format_combo(Key::S, Modifiers::CTRL);
        assert_eq!(combo.as_deref(), Some("ctrl+s"));
    }

    #[test]
    fn format_combo_without_modifier() {
        let combo = format_combo(Key::Space, Modifiers::default());
        assert_eq!(combo.as_deref(), Some("space"));
    }
}
