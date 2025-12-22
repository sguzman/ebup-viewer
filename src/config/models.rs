use serde::Deserialize;

/// High-level app configuration; deserializable from TOML.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct AppConfig {
    #[serde(default)]
    pub theme: ThemeMode,
    #[serde(default = "crate::config::defaults::default_font_size")]
    pub font_size: u32,
    #[serde(default = "crate::config::defaults::default_line_spacing")]
    pub line_spacing: f32,
    #[serde(default = "crate::config::defaults::default_margin")]
    pub margin_horizontal: u16,
    #[serde(default = "crate::config::defaults::default_margin")]
    pub margin_vertical: u16,
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
    #[serde(default = "crate::config::defaults::default_tts_espeak_path")]
    pub tts_espeak_path: String,
    #[serde(default = "crate::config::defaults::default_tts_threads")]
    pub tts_threads: usize,
    #[serde(default = "crate::config::defaults::default_show_tts")]
    pub show_tts: bool,
    #[serde(default = "crate::config::defaults::default_show_settings")]
    pub show_settings: bool,
    #[serde(default = "crate::config::defaults::default_day_highlight")]
    pub day_highlight: HighlightColor,
    #[serde(default = "crate::config::defaults::default_night_highlight")]
    pub night_highlight: HighlightColor,
    #[serde(default = "crate::config::defaults::default_log_level")]
    pub log_level: LogLevel,
    #[serde(default = "crate::config::defaults::default_lines_per_page")]
    pub lines_per_page: usize,
    #[serde(default = "crate::config::defaults::default_pause_after_sentence")]
    pub pause_after_sentence: f32,
    #[serde(default = "crate::config::defaults::default_auto_scroll_tts")]
    pub auto_scroll_tts: bool,
    #[serde(default = "crate::config::defaults::default_center_spoken_sentence")]
    pub center_spoken_sentence: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            theme: ThemeMode::Night,
            font_size: crate::config::defaults::default_font_size(),
            line_spacing: crate::config::defaults::default_line_spacing(),
            margin_horizontal: crate::config::defaults::default_margin(),
            margin_vertical: crate::config::defaults::default_margin(),
            font_family: FontFamily::Sans,
            font_weight: FontWeight::Normal,
            word_spacing: 0,
            letter_spacing: 0,
            tts_model_path: crate::config::defaults::default_tts_model(),
            tts_speed: crate::config::defaults::default_tts_speed(),
            tts_espeak_path: crate::config::defaults::default_tts_espeak_path(),
            tts_threads: crate::config::defaults::default_tts_threads(),
            show_tts: crate::config::defaults::default_show_tts(),
            show_settings: crate::config::defaults::default_show_settings(),
            day_highlight: crate::config::defaults::default_day_highlight(),
            night_highlight: crate::config::defaults::default_night_highlight(),
            log_level: crate::config::defaults::default_log_level(),
            lines_per_page: crate::config::defaults::default_lines_per_page(),
            pause_after_sentence: crate::config::defaults::default_pause_after_sentence(),
            auto_scroll_tts: crate::config::defaults::default_auto_scroll_tts(),
            center_spoken_sentence: crate::config::defaults::default_center_spoken_sentence(),
        }
    }
}

/// Theme mode.
#[derive(Debug, Clone, Copy, Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
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

/// Font family options.
#[derive(Debug, Clone, Copy, Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
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
#[derive(Debug, Clone, Copy, Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
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

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
pub struct HighlightColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// Supported logging verbosity levels.
#[derive(Debug, Clone, Copy, Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
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
