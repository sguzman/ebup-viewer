pub(crate) fn default_font_size() -> u32 {
    16
}

pub(crate) fn default_line_spacing() -> f32 {
    1.2
}

pub(crate) fn default_margin() -> u16 {
    12
}

pub(crate) fn default_window_width() -> f32 {
    1024.0
}

pub(crate) fn default_window_height() -> f32 {
    768.0
}

pub(crate) fn default_tts_model() -> String {
    "/usr/share/piper-voices/en/en_US/ryan/high/en_US-ryan-high.onnx".to_string()
}

pub(crate) fn default_tts_speed() -> f32 {
    2.5
}

pub(crate) fn default_tts_volume() -> f32 {
    1.0
}

pub(crate) fn default_tts_espeak_path() -> String {
    "/usr/share".to_string()
}

pub(crate) fn default_tts_threads() -> usize {
    16
}

pub(crate) fn default_tts_progress_log_interval_secs() -> f32 {
    5.0
}

pub(crate) fn default_show_tts() -> bool {
    true
}

pub(crate) fn default_show_settings() -> bool {
    true
}

pub(crate) fn default_day_highlight() -> crate::config::HighlightColor {
    crate::config::HighlightColor {
        r: 0.2,
        g: 0.4,
        b: 0.7,
        a: 0.15,
    }
}

pub(crate) fn default_night_highlight() -> crate::config::HighlightColor {
    crate::config::HighlightColor {
        r: 0.8,
        g: 0.8,
        b: 0.5,
        a: 0.2,
    }
}

pub(crate) fn default_log_level() -> crate::config::LogLevel {
    crate::config::LogLevel::Debug
}

pub(crate) fn default_lines_per_page() -> usize {
    28
}

pub(crate) fn default_pause_after_sentence() -> f32 {
    0.2
}

pub(crate) fn default_auto_scroll_tts() -> bool {
    false
}

pub(crate) fn default_center_spoken_sentence() -> bool {
    true
}
