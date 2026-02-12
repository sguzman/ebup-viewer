use super::defaults;
use super::models::{AppConfig, FontFamily, FontWeight, HighlightColor, LogLevel, ThemeMode};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub(super) struct ConfigTables {
    #[serde(default)]
    appearance: AppearanceConfig,
    #[serde(default)]
    window: WindowConfig,
    #[serde(default)]
    reading_behavior: ReadingBehaviorConfig,
    #[serde(default)]
    ui: UiConfig,
    #[serde(default)]
    logging: LoggingConfig,
    #[serde(default)]
    tts: TtsConfig,
    #[serde(default)]
    keybindings: KeybindingsConfig,
}

impl From<ConfigTables> for AppConfig {
    fn from(tables: ConfigTables) -> Self {
        AppConfig {
            theme: tables.appearance.theme,
            font_family: tables.appearance.font_family,
            font_weight: tables.appearance.font_weight,
            font_size: tables.appearance.font_size,
            line_spacing: tables.appearance.line_spacing,
            word_spacing: tables.appearance.word_spacing,
            letter_spacing: tables.appearance.letter_spacing,
            lines_per_page: tables.appearance.lines_per_page,
            margin_horizontal: tables.appearance.margin_horizontal,
            margin_vertical: tables.appearance.margin_vertical,
            window_width: tables.window.width,
            window_height: tables.window.height,
            window_pos_x: tables.window.x,
            window_pos_y: tables.window.y,
            day_highlight: tables.appearance.day_highlight,
            night_highlight: tables.appearance.night_highlight,
            pause_after_sentence: tables.reading_behavior.pause_after_sentence,
            auto_scroll_tts: tables.reading_behavior.auto_scroll_tts,
            center_spoken_sentence: tables.reading_behavior.center_spoken_sentence,
            key_toggle_play_pause: tables.keybindings.toggle_play_pause,
            key_safe_quit: tables.keybindings.safe_quit,
            key_next_sentence: tables.keybindings.next_sentence,
            key_prev_sentence: tables.keybindings.prev_sentence,
            key_repeat_sentence: tables.keybindings.repeat_sentence,
            key_toggle_search: tables.keybindings.toggle_search,
            key_toggle_settings: tables.keybindings.toggle_settings,
            key_toggle_stats: tables.keybindings.toggle_stats,
            key_toggle_tts: tables.keybindings.toggle_tts,
            show_tts: tables.ui.show_tts,
            show_settings: tables.ui.show_settings,
            log_level: tables.logging.log_level,
            tts_model_path: tables.tts.tts_model_path,
            tts_espeak_path: tables.tts.tts_espeak_path,
            tts_speed: tables.tts.tts_speed,
            tts_volume: tables.tts.tts_volume,
            tts_threads: tables.tts.tts_threads,
            tts_progress_log_interval_secs: tables.tts.tts_progress_log_interval_secs,
        }
    }
}

impl From<&AppConfig> for ConfigTables {
    fn from(config: &AppConfig) -> Self {
        ConfigTables {
            appearance: AppearanceConfig {
                theme: config.theme,
                font_family: config.font_family,
                font_weight: config.font_weight,
                font_size: config.font_size,
                line_spacing: config.line_spacing,
                word_spacing: config.word_spacing,
                letter_spacing: config.letter_spacing,
                lines_per_page: config.lines_per_page,
                margin_horizontal: config.margin_horizontal,
                margin_vertical: config.margin_vertical,
                day_highlight: config.day_highlight,
                night_highlight: config.night_highlight,
            },
            window: WindowConfig {
                width: config.window_width,
                height: config.window_height,
                x: config.window_pos_x,
                y: config.window_pos_y,
            },
            reading_behavior: ReadingBehaviorConfig {
                pause_after_sentence: config.pause_after_sentence,
                auto_scroll_tts: config.auto_scroll_tts,
                center_spoken_sentence: config.center_spoken_sentence,
            },
            ui: UiConfig {
                show_tts: config.show_tts,
                show_settings: config.show_settings,
            },
            logging: LoggingConfig {
                log_level: config.log_level,
            },
            tts: TtsConfig {
                tts_model_path: config.tts_model_path.clone(),
                tts_espeak_path: config.tts_espeak_path.clone(),
                tts_speed: config.tts_speed,
                tts_volume: config.tts_volume,
                tts_threads: config.tts_threads,
                tts_progress_log_interval_secs: config.tts_progress_log_interval_secs,
            },
            keybindings: KeybindingsConfig {
                toggle_play_pause: config.key_toggle_play_pause.clone(),
                safe_quit: config.key_safe_quit.clone(),
                next_sentence: config.key_next_sentence.clone(),
                prev_sentence: config.key_prev_sentence.clone(),
                repeat_sentence: config.key_repeat_sentence.clone(),
                toggle_search: config.key_toggle_search.clone(),
                toggle_settings: config.key_toggle_settings.clone(),
                toggle_stats: config.key_toggle_stats.clone(),
                toggle_tts: config.key_toggle_tts.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
struct AppearanceConfig {
    #[serde(default)]
    theme: ThemeMode,
    #[serde(default)]
    font_family: FontFamily,
    #[serde(default)]
    font_weight: FontWeight,
    #[serde(default = "defaults::default_font_size")]
    font_size: u32,
    #[serde(default = "defaults::default_line_spacing")]
    line_spacing: f32,
    #[serde(default)]
    word_spacing: u32,
    #[serde(default)]
    letter_spacing: u32,
    #[serde(default = "defaults::default_lines_per_page")]
    lines_per_page: usize,
    #[serde(default = "defaults::default_margin")]
    margin_horizontal: u16,
    #[serde(default = "defaults::default_margin")]
    margin_vertical: u16,
    #[serde(default = "defaults::default_day_highlight")]
    day_highlight: HighlightColor,
    #[serde(default = "defaults::default_night_highlight")]
    night_highlight: HighlightColor,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        AppearanceConfig {
            theme: ThemeMode::default(),
            font_family: FontFamily::default(),
            font_weight: FontWeight::default(),
            font_size: defaults::default_font_size(),
            line_spacing: defaults::default_line_spacing(),
            word_spacing: 0,
            letter_spacing: 0,
            lines_per_page: defaults::default_lines_per_page(),
            margin_horizontal: defaults::default_margin(),
            margin_vertical: defaults::default_margin(),
            day_highlight: defaults::default_day_highlight(),
            night_highlight: defaults::default_night_highlight(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
struct WindowConfig {
    #[serde(default = "defaults::default_window_width")]
    width: f32,
    #[serde(default = "defaults::default_window_height")]
    height: f32,
    #[serde(default)]
    x: Option<f32>,
    #[serde(default)]
    y: Option<f32>,
}

impl Default for WindowConfig {
    fn default() -> Self {
        WindowConfig {
            width: defaults::default_window_width(),
            height: defaults::default_window_height(),
            x: None,
            y: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
struct ReadingBehaviorConfig {
    #[serde(default = "defaults::default_pause_after_sentence")]
    pause_after_sentence: f32,
    #[serde(default = "defaults::default_auto_scroll_tts")]
    auto_scroll_tts: bool,
    #[serde(default = "defaults::default_center_spoken_sentence")]
    center_spoken_sentence: bool,
}

impl Default for ReadingBehaviorConfig {
    fn default() -> Self {
        ReadingBehaviorConfig {
            pause_after_sentence: defaults::default_pause_after_sentence(),
            auto_scroll_tts: defaults::default_auto_scroll_tts(),
            center_spoken_sentence: defaults::default_center_spoken_sentence(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
struct UiConfig {
    #[serde(default = "defaults::default_show_tts")]
    show_tts: bool,
    #[serde(default = "defaults::default_show_settings")]
    show_settings: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        UiConfig {
            show_tts: defaults::default_show_tts(),
            show_settings: defaults::default_show_settings(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
struct LoggingConfig {
    #[serde(default = "defaults::default_log_level")]
    log_level: LogLevel,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        LoggingConfig {
            log_level: defaults::default_log_level(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
struct TtsConfig {
    #[serde(default = "defaults::default_tts_model")]
    tts_model_path: String,
    #[serde(default = "defaults::default_tts_espeak_path")]
    tts_espeak_path: String,
    #[serde(default = "defaults::default_tts_speed")]
    tts_speed: f32,
    #[serde(default = "defaults::default_tts_volume")]
    tts_volume: f32,
    #[serde(default = "defaults::default_tts_threads")]
    tts_threads: usize,
    #[serde(default = "defaults::default_tts_progress_log_interval_secs")]
    tts_progress_log_interval_secs: f32,
}

impl Default for TtsConfig {
    fn default() -> Self {
        TtsConfig {
            tts_model_path: defaults::default_tts_model(),
            tts_espeak_path: defaults::default_tts_espeak_path(),
            tts_speed: defaults::default_tts_speed(),
            tts_volume: defaults::default_tts_volume(),
            tts_threads: defaults::default_tts_threads(),
            tts_progress_log_interval_secs: defaults::default_tts_progress_log_interval_secs(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
struct KeybindingsConfig {
    #[serde(default = "defaults::default_key_toggle_play_pause")]
    toggle_play_pause: String,
    #[serde(default = "defaults::default_key_safe_quit")]
    safe_quit: String,
    #[serde(default = "defaults::default_key_next_sentence")]
    next_sentence: String,
    #[serde(default = "defaults::default_key_prev_sentence")]
    prev_sentence: String,
    #[serde(default = "defaults::default_key_repeat_sentence")]
    repeat_sentence: String,
    #[serde(default = "defaults::default_key_toggle_search")]
    toggle_search: String,
    #[serde(default = "defaults::default_key_toggle_settings")]
    toggle_settings: String,
    #[serde(default = "defaults::default_key_toggle_stats")]
    toggle_stats: String,
    #[serde(default = "defaults::default_key_toggle_tts")]
    toggle_tts: String,
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        KeybindingsConfig {
            toggle_play_pause: defaults::default_key_toggle_play_pause(),
            safe_quit: defaults::default_key_safe_quit(),
            next_sentence: defaults::default_key_next_sentence(),
            prev_sentence: defaults::default_key_prev_sentence(),
            repeat_sentence: defaults::default_key_repeat_sentence(),
            toggle_search: defaults::default_key_toggle_search(),
            toggle_settings: defaults::default_key_toggle_settings(),
            toggle_stats: defaults::default_key_toggle_stats(),
            toggle_tts: defaults::default_key_toggle_tts(),
        }
    }
}
