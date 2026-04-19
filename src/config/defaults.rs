pub(crate) fn default_font_size() -> u32 {
    22
}

pub(crate) fn default_chrome_font_scale() -> f32 {
    // Keep the main app chrome compact even if the reader font is large.
    0.62
}

pub(crate) fn default_line_spacing() -> f32 {
    1.2
}

pub(crate) fn default_margin_horizontal() -> u16 {
    100
}

pub(crate) fn default_margin_vertical() -> u16 {
    12
}

pub(crate) fn default_window_width() -> f32 {
    1024.0
}

pub(crate) fn default_window_height() -> f32 {
    768.0
}

pub(crate) fn default_tts_model() -> String {
    "/home/admin/Music/models/piper/en-US/female/en_US-amy-medium.onnx".to_string()
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

pub(crate) fn default_normalizer_threads() -> usize {
    std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(4)
        .max(1)
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

pub(crate) fn default_show_stats() -> bool {
    false
}

pub(crate) fn default_dual_view_pipeline_enabled() -> bool {
    true
}

pub(crate) fn default_native_html_pretty_enabled() -> bool {
    true
}

pub(crate) fn default_native_html_pagination_mode() -> crate::config::NativeHtmlPaginationMode {
    crate::config::NativeHtmlPaginationMode::SentenceWindow
}

pub(crate) fn default_pretty_enabled() -> bool {
    true
}

pub(crate) fn default_pretty_base_font_scale() -> f32 {
    0.9
}

pub(crate) fn default_pretty_heading_scale_h1() -> f32 {
    1.7
}

pub(crate) fn default_pretty_heading_scale_h2() -> f32 {
    1.45
}

pub(crate) fn default_pretty_heading_scale_h3() -> f32 {
    1.25
}

pub(crate) fn default_pretty_heading_scale_h4() -> f32 {
    1.1
}

pub(crate) fn default_pretty_heading_scale_h5() -> f32 {
    1.0
}

pub(crate) fn default_pretty_heading_scale_h6() -> f32 {
    0.95
}

pub(crate) fn default_pretty_paragraph_spacing() -> f32 {
    6.0
}

pub(crate) fn default_pretty_block_spacing() -> f32 {
    10.0
}

pub(crate) fn default_pretty_list_indent() -> f32 {
    20.0
}

pub(crate) fn default_pretty_list_item_spacing() -> f32 {
    4.0
}

pub(crate) fn default_pretty_hr_thickness() -> f32 {
    1.0
}

pub(crate) fn default_pretty_hr_margin() -> f32 {
    10.0
}

pub(crate) fn default_pretty_code_font_scale() -> f32 {
    0.9
}

pub(crate) fn default_pretty_code_bg_alpha() -> f32 {
    0.08
}

pub(crate) fn default_pretty_code_border_alpha() -> f32 {
    0.18
}

pub(crate) fn default_pretty_link_color() -> crate::config::HighlightColor {
    crate::config::HighlightColor {
        r: 0.30,
        g: 0.55,
        b: 0.95,
        a: 1.0,
    }
}

pub(crate) fn default_pretty_image_max_width_pct() -> f32 {
    95.0
}

pub(crate) fn default_pretty_image_max_height_px() -> f32 {
    520.0
}

pub(crate) fn default_pretty_image_cache_max_entries() -> usize {
    128
}

pub(crate) fn default_pretty_table_cell_padding() -> f32 {
    6.0
}

pub(crate) fn default_pretty_table_border_alpha() -> f32 {
    0.16
}

pub(crate) fn default_pretty_table_stripe_alpha() -> f32 {
    0.05
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

pub(crate) fn default_cache_dir() -> String {
    ".cache".to_string()
}

pub(crate) fn default_browser_tabs_enabled() -> bool {
    true
}

pub(crate) fn default_browsr_base_url() -> String {
    "http://127.0.0.1:17373".to_string()
}

pub(crate) fn default_browsr_timeout_ms() -> u64 {
    8000
}

pub(crate) fn default_close_browser_tab_on_recent_delete() -> bool {
    true
}

pub(crate) fn default_lines_per_page() -> usize {
    700
}

pub(crate) fn default_pause_after_sentence() -> f32 {
    0.06
}

pub(crate) fn default_auto_scroll_tts() -> bool {
    false
}

pub(crate) fn default_center_spoken_sentence() -> bool {
    true
}

pub(crate) fn default_text_only_show_original_text() -> bool {
    false
}

pub(crate) fn default_tts_pause_resume_behavior() -> crate::config::TtsPauseResumeBehavior {
    crate::config::TtsPauseResumeBehavior::ResumeFromPausePoint
}

pub(crate) fn default_time_remaining_display() -> crate::config::TimeRemainingDisplay {
    crate::config::TimeRemainingDisplay::Adaptive
}

pub(crate) fn default_key_toggle_play_pause() -> String {
    "space".to_string()
}

pub(crate) fn default_key_safe_quit() -> String {
    "q".to_string()
}

pub(crate) fn default_key_next_sentence() -> String {
    "f".to_string()
}

pub(crate) fn default_key_prev_sentence() -> String {
    "s".to_string()
}

pub(crate) fn default_key_repeat_sentence() -> String {
    "r".to_string()
}

pub(crate) fn default_key_toggle_search() -> String {
    "ctrl+f".to_string()
}

pub(crate) fn default_key_toggle_settings() -> String {
    "ctrl+t".to_string()
}

pub(crate) fn default_key_toggle_stats() -> String {
    "ctrl+g".to_string()
}

pub(crate) fn default_key_toggle_tts() -> String {
    "ctrl+y".to_string()
}
