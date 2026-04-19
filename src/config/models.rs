use serde::Deserialize;
use ts_rs::TS;

/// High-level app configuration; deserializable from TOML.
#[derive(Debug, Clone, Deserialize, serde::Serialize, TS)]
#[ts(export)]
pub struct AppConfig {
    #[serde(default)]
    pub theme: ThemeMode,
    #[serde(default = "crate::config::defaults::default_font_size")]
    pub font_size: u32,
    #[serde(default = "crate::config::defaults::default_chrome_font_scale")]
    pub chrome_font_scale: f32,
    #[serde(default = "crate::config::defaults::default_line_spacing")]
    pub line_spacing: f32,
    #[serde(default = "crate::config::defaults::default_margin_horizontal")]
    pub margin_horizontal: u16,
    #[serde(default = "crate::config::defaults::default_margin_vertical")]
    pub margin_vertical: u16,
    #[serde(default = "crate::config::defaults::default_window_width")]
    pub window_width: f32,
    #[serde(default = "crate::config::defaults::default_window_height")]
    pub window_height: f32,
    #[serde(default)]
    pub window_pos_x: Option<f32>,
    #[serde(default)]
    pub window_pos_y: Option<f32>,
    #[serde(default)]
    pub font_family: FontFamily,
    #[serde(default)]
    pub font_weight: FontWeight,
    #[serde(default)]
    pub word_spacing: u32,
    #[serde(default)]
    pub letter_spacing: u32,
    #[serde(default = "crate::config::defaults::default_tts_model")]
    pub tts_model_path: String,
    #[serde(default = "crate::config::defaults::default_tts_speed")]
    pub tts_speed: f32,
    #[serde(default = "crate::config::defaults::default_tts_volume")]
    pub tts_volume: f32,
    #[serde(default = "crate::config::defaults::default_tts_espeak_path")]
    pub tts_espeak_path: String,
    #[serde(default = "crate::config::defaults::default_tts_threads")]
    pub tts_threads: usize,
    #[serde(default = "crate::config::defaults::default_normalizer_threads")]
    pub normalizer_threads: usize,
    #[serde(default = "crate::config::defaults::default_tts_progress_log_interval_secs")]
    pub tts_progress_log_interval_secs: f32,
    #[serde(default = "crate::config::defaults::default_show_tts")]
    pub show_tts: bool,
    #[serde(default = "crate::config::defaults::default_show_settings")]
    pub show_settings: bool,
    #[serde(default = "crate::config::defaults::default_show_stats")]
    pub show_stats: bool,
    #[serde(default = "crate::config::defaults::default_dual_view_pipeline_enabled")]
    pub dual_view_pipeline_enabled: bool,
    #[serde(default = "crate::config::defaults::default_native_html_pretty_enabled")]
    pub native_html_pretty_enabled: bool,
    #[serde(default = "crate::config::defaults::default_native_html_pagination_mode")]
    pub native_html_pagination_mode: NativeHtmlPaginationMode,
    #[serde(default)]
    pub pretty: PrettyUiConfig,
    #[serde(default = "crate::config::defaults::default_day_highlight")]
    pub day_highlight: HighlightColor,
    #[serde(default = "crate::config::defaults::default_night_highlight")]
    pub night_highlight: HighlightColor,
    #[serde(default = "crate::config::defaults::default_log_level")]
    pub log_level: LogLevel,
    #[serde(default = "crate::config::defaults::default_cache_dir")]
    pub cache_dir: String,
    #[serde(default = "crate::config::defaults::default_browser_tabs_enabled")]
    pub browser_tabs_enabled: bool,
    #[serde(default = "crate::config::defaults::default_browsr_base_url")]
    pub browsr_base_url: String,
    #[serde(default = "crate::config::defaults::default_browsr_timeout_ms")]
    pub browsr_timeout_ms: u64,
    #[serde(default = "crate::config::defaults::default_close_browser_tab_on_recent_delete")]
    pub close_browser_tab_on_recent_delete: bool,
    #[serde(default = "crate::config::defaults::default_lines_per_page")]
    pub lines_per_page: usize,
    #[serde(default = "crate::config::defaults::default_pause_after_sentence")]
    pub pause_after_sentence: f32,
    #[serde(default = "crate::config::defaults::default_auto_scroll_tts")]
    pub auto_scroll_tts: bool,
    #[serde(default = "crate::config::defaults::default_center_spoken_sentence")]
    pub center_spoken_sentence: bool,
    #[serde(default = "crate::config::defaults::default_text_only_show_original_text")]
    pub text_only_show_original_text: bool,
    #[serde(default = "crate::config::defaults::default_tts_pause_resume_behavior")]
    pub tts_pause_resume_behavior: TtsPauseResumeBehavior,
    #[serde(default = "crate::config::defaults::default_time_remaining_display")]
    pub time_remaining_display: TimeRemainingDisplay,
    #[serde(default = "crate::config::defaults::default_key_toggle_play_pause")]
    pub key_toggle_play_pause: String,
    #[serde(default = "crate::config::defaults::default_key_safe_quit")]
    pub key_safe_quit: String,
    #[serde(default = "crate::config::defaults::default_key_next_sentence")]
    pub key_next_sentence: String,
    #[serde(default = "crate::config::defaults::default_key_prev_sentence")]
    pub key_prev_sentence: String,
    #[serde(default = "crate::config::defaults::default_key_repeat_sentence")]
    pub key_repeat_sentence: String,
    #[serde(default = "crate::config::defaults::default_key_toggle_search")]
    pub key_toggle_search: String,
    #[serde(default = "crate::config::defaults::default_key_toggle_settings")]
    pub key_toggle_settings: String,
    #[serde(default = "crate::config::defaults::default_key_toggle_stats")]
    pub key_toggle_stats: String,
    #[serde(default = "crate::config::defaults::default_key_toggle_tts")]
    pub key_toggle_tts: String,
    #[serde(default)]
    pub remote_url: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            theme: ThemeMode::Night,
            font_size: crate::config::defaults::default_font_size(),
            chrome_font_scale: crate::config::defaults::default_chrome_font_scale(),
            line_spacing: crate::config::defaults::default_line_spacing(),
            margin_horizontal: crate::config::defaults::default_margin_horizontal(),
            margin_vertical: crate::config::defaults::default_margin_vertical(),
            window_width: crate::config::defaults::default_window_width(),
            window_height: crate::config::defaults::default_window_height(),
            window_pos_x: None,
            window_pos_y: None,
            font_family: FontFamily::Sans,
            font_weight: FontWeight::Normal,
            word_spacing: 0,
            letter_spacing: 0,
            tts_model_path: crate::config::defaults::default_tts_model(),
            tts_speed: crate::config::defaults::default_tts_speed(),
            tts_volume: crate::config::defaults::default_tts_volume(),
            tts_espeak_path: crate::config::defaults::default_tts_espeak_path(),
            tts_threads: crate::config::defaults::default_tts_threads(),
            normalizer_threads: crate::config::defaults::default_normalizer_threads(),
            tts_progress_log_interval_secs:
                crate::config::defaults::default_tts_progress_log_interval_secs(),
            show_tts: crate::config::defaults::default_show_tts(),
            show_settings: crate::config::defaults::default_show_settings(),
            show_stats: crate::config::defaults::default_show_stats(),
            dual_view_pipeline_enabled: crate::config::defaults::default_dual_view_pipeline_enabled(
            ),
            native_html_pretty_enabled: crate::config::defaults::default_native_html_pretty_enabled(
            ),
            native_html_pagination_mode:
                crate::config::defaults::default_native_html_pagination_mode(),
            pretty: PrettyUiConfig::default(),
            day_highlight: crate::config::defaults::default_day_highlight(),
            night_highlight: crate::config::defaults::default_night_highlight(),
            log_level: crate::config::defaults::default_log_level(),
            cache_dir: crate::config::defaults::default_cache_dir(),
            browser_tabs_enabled: crate::config::defaults::default_browser_tabs_enabled(),
            browsr_base_url: crate::config::defaults::default_browsr_base_url(),
            browsr_timeout_ms: crate::config::defaults::default_browsr_timeout_ms(),
            close_browser_tab_on_recent_delete:
                crate::config::defaults::default_close_browser_tab_on_recent_delete(),
            lines_per_page: crate::config::defaults::default_lines_per_page(),
            pause_after_sentence: crate::config::defaults::default_pause_after_sentence(),
            auto_scroll_tts: crate::config::defaults::default_auto_scroll_tts(),
            center_spoken_sentence: crate::config::defaults::default_center_spoken_sentence(),
            text_only_show_original_text:
                crate::config::defaults::default_text_only_show_original_text(),
            tts_pause_resume_behavior: crate::config::defaults::default_tts_pause_resume_behavior(),
            time_remaining_display: crate::config::defaults::default_time_remaining_display(),
            key_toggle_play_pause: crate::config::defaults::default_key_toggle_play_pause(),
            key_safe_quit: crate::config::defaults::default_key_safe_quit(),
            key_next_sentence: crate::config::defaults::default_key_next_sentence(),
            key_prev_sentence: crate::config::defaults::default_key_prev_sentence(),
            key_repeat_sentence: crate::config::defaults::default_key_repeat_sentence(),
            key_toggle_search: crate::config::defaults::default_key_toggle_search(),
            key_toggle_settings: crate::config::defaults::default_key_toggle_settings(),
            key_toggle_stats: crate::config::defaults::default_key_toggle_stats(),
            key_toggle_tts: crate::config::defaults::default_key_toggle_tts(),
            remote_url: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, serde::Serialize, PartialEq, TS)]
#[ts(export)]
pub struct PrettyUiConfig {
    #[serde(default = "crate::config::defaults::default_pretty_enabled")]
    pub enabled: bool,
    #[serde(default = "crate::config::defaults::default_pretty_base_font_scale")]
    pub base_font_scale: f32,
    #[serde(default = "crate::config::defaults::default_pretty_heading_scale_h1")]
    pub heading_scale_h1: f32,
    #[serde(default = "crate::config::defaults::default_pretty_heading_scale_h2")]
    pub heading_scale_h2: f32,
    #[serde(default = "crate::config::defaults::default_pretty_heading_scale_h3")]
    pub heading_scale_h3: f32,
    #[serde(default = "crate::config::defaults::default_pretty_heading_scale_h4")]
    pub heading_scale_h4: f32,
    #[serde(default = "crate::config::defaults::default_pretty_heading_scale_h5")]
    pub heading_scale_h5: f32,
    #[serde(default = "crate::config::defaults::default_pretty_heading_scale_h6")]
    pub heading_scale_h6: f32,
    #[serde(default = "crate::config::defaults::default_pretty_paragraph_spacing")]
    pub paragraph_spacing: f32,
    #[serde(default = "crate::config::defaults::default_pretty_block_spacing")]
    pub block_spacing: f32,
    #[serde(default = "crate::config::defaults::default_pretty_list_indent")]
    pub list_indent: f32,
    #[serde(default = "crate::config::defaults::default_pretty_list_item_spacing")]
    pub list_item_spacing: f32,
    #[serde(default = "crate::config::defaults::default_pretty_hr_thickness")]
    pub hr_thickness: f32,
    #[serde(default = "crate::config::defaults::default_pretty_hr_margin")]
    pub hr_margin: f32,
    #[serde(default = "crate::config::defaults::default_pretty_code_font_scale")]
    pub code_font_scale: f32,
    #[serde(default = "crate::config::defaults::default_pretty_code_bg_alpha")]
    pub code_bg_alpha: f32,
    #[serde(default = "crate::config::defaults::default_pretty_code_border_alpha")]
    pub code_border_alpha: f32,
    #[serde(default = "crate::config::defaults::default_pretty_link_color")]
    pub link_color: HighlightColor,
    #[serde(default = "crate::config::defaults::default_pretty_image_max_width_pct")]
    pub image_max_width_pct: f32,
    #[serde(default = "crate::config::defaults::default_pretty_image_max_height_px")]
    pub image_max_height_px: f32,
    #[serde(default = "crate::config::defaults::default_pretty_image_cache_max_entries")]
    pub image_cache_max_entries: usize,
    #[serde(default = "crate::config::defaults::default_pretty_table_cell_padding")]
    pub table_cell_padding: f32,
    #[serde(default = "crate::config::defaults::default_pretty_table_border_alpha")]
    pub table_border_alpha: f32,
    #[serde(default = "crate::config::defaults::default_pretty_table_stripe_alpha")]
    pub table_stripe_alpha: f32,
}

impl Default for PrettyUiConfig {
    fn default() -> Self {
        Self {
            enabled: crate::config::defaults::default_pretty_enabled(),
            base_font_scale: crate::config::defaults::default_pretty_base_font_scale(),
            heading_scale_h1: crate::config::defaults::default_pretty_heading_scale_h1(),
            heading_scale_h2: crate::config::defaults::default_pretty_heading_scale_h2(),
            heading_scale_h3: crate::config::defaults::default_pretty_heading_scale_h3(),
            heading_scale_h4: crate::config::defaults::default_pretty_heading_scale_h4(),
            heading_scale_h5: crate::config::defaults::default_pretty_heading_scale_h5(),
            heading_scale_h6: crate::config::defaults::default_pretty_heading_scale_h6(),
            paragraph_spacing: crate::config::defaults::default_pretty_paragraph_spacing(),
            block_spacing: crate::config::defaults::default_pretty_block_spacing(),
            list_indent: crate::config::defaults::default_pretty_list_indent(),
            list_item_spacing: crate::config::defaults::default_pretty_list_item_spacing(),
            hr_thickness: crate::config::defaults::default_pretty_hr_thickness(),
            hr_margin: crate::config::defaults::default_pretty_hr_margin(),
            code_font_scale: crate::config::defaults::default_pretty_code_font_scale(),
            code_bg_alpha: crate::config::defaults::default_pretty_code_bg_alpha(),
            code_border_alpha: crate::config::defaults::default_pretty_code_border_alpha(),
            link_color: crate::config::defaults::default_pretty_link_color(),
            image_max_width_pct: crate::config::defaults::default_pretty_image_max_width_pct(),
            image_max_height_px: crate::config::defaults::default_pretty_image_max_height_px(),
            image_cache_max_entries:
                crate::config::defaults::default_pretty_image_cache_max_entries(),
            table_cell_padding: crate::config::defaults::default_pretty_table_cell_padding(),
            table_border_alpha: crate::config::defaults::default_pretty_table_border_alpha(),
            table_stripe_alpha: crate::config::defaults::default_pretty_table_stripe_alpha(),
        }
    }
}

/// Theme mode.
#[derive(Debug, Clone, Copy, Deserialize, serde::Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum ThemeMode {
    Day,
    Night,
}

impl Default for ThemeMode {
    fn default() -> Self {
        ThemeMode::Night
    }
}

impl std::fmt::Display for ThemeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            ThemeMode::Day => "Day",
            ThemeMode::Night => "Night",
        };
        write!(f, "{}", label)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, serde::Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TtsPauseResumeBehavior {
    ResumeFromPausePoint,
    RestartSentence,
}

impl Default for TtsPauseResumeBehavior {
    fn default() -> Self {
        TtsPauseResumeBehavior::ResumeFromPausePoint
    }
}

#[derive(Debug, Clone, Copy, Deserialize, serde::Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum TimeRemainingDisplay {
    Adaptive,
    MinutesSeconds,
}

impl Default for TimeRemainingDisplay {
    fn default() -> Self {
        TimeRemainingDisplay::Adaptive
    }
}

#[derive(Debug, Clone, Copy, Deserialize, serde::Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum NativeHtmlPaginationMode {
    SentenceWindow,
    ChapterSection,
}

impl Default for NativeHtmlPaginationMode {
    fn default() -> Self {
        NativeHtmlPaginationMode::SentenceWindow
    }
}

/// Font family options.
#[derive(Debug, Clone, Copy, Deserialize, serde::Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum FontFamily {
    Sans,
    Serif,
    Monospace,
    Lexend,
    FiraCode,
    AtkinsonHyperlegible,
    AtkinsonHyperlegibleNext,
    LexicaUltralegible,
    Courier,
    FrankGothic,
    Hermit,
    Hasklug,
    NotoSans,
}

impl Default for FontFamily {
    fn default() -> Self {
        FontFamily::Sans
    }
}

impl std::fmt::Display for FontFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            FontFamily::Sans => "Sans",
            FontFamily::Serif => "Serif",
            FontFamily::Monospace => "Monospace",
            FontFamily::Lexend => "Lexend",
            FontFamily::FiraCode => "Fira Code",
            FontFamily::AtkinsonHyperlegible => "Atkinson Hyperlegible",
            FontFamily::AtkinsonHyperlegibleNext => "Atkinson Hyperlegible Next",
            FontFamily::LexicaUltralegible => "Lexica Ultralegible",
            FontFamily::Courier => "Courier",
            FontFamily::FrankGothic => "Frank Gothic",
            FontFamily::Hermit => "Hermit",
            FontFamily::Hasklug => "Hasklug",
            FontFamily::NotoSans => "Noto Sans",
        };
        write!(f, "{}", label)
    }
}

/// Font weight options.
#[derive(Debug, Clone, Copy, Deserialize, serde::Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum FontWeight {
    Light,
    Normal,
    Bold,
}

impl Default for FontWeight {
    fn default() -> Self {
        FontWeight::Normal
    }
}

impl std::fmt::Display for FontWeight {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            FontWeight::Light => "Light",
            FontWeight::Normal => "Normal",
            FontWeight::Bold => "Bold",
        };
        write!(f, "{}", label)
    }
}

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize, PartialEq, TS)]
#[ts(export)]
pub struct HighlightColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// Supported logging verbosity levels.
#[derive(Debug, Clone, Copy, Deserialize, serde::Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Default for LogLevel {
    fn default() -> Self {
        LogLevel::Debug
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        };
        write!(f, "{}", label)
    }
}

impl LogLevel {
    pub fn as_filter_str(self) -> &'static str {
        match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}
