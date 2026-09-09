use super::models::AppConfig;
use super::tables::ConfigTables;
use serde::Deserialize;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
use std::path::Path;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ConfigInput {
    Tables(ConfigTables),
    Flat(AppConfig),
}

#[cfg(not(target_arch = "wasm32"))]
/// Load configuration from the given path, falling back to defaults on error.
pub fn load_config(path: &Path) -> AppConfig {
    let contents = match fs::read_to_string(path) {
        Ok(data) => {
            info!(path = %path.display(), "Loaded base config");
            data
        }
        Err(err) => {
            warn!(
                path = %path.display(),
                "Falling back to default config: {err}"
            );
            return apply_qa_overrides(AppConfig::default());
        }
    };

    match parse_config(&contents) {
        Ok(cfg) => {
            debug!("Parsed configuration from disk");
            let cfg = apply_qa_overrides(cfg);
            info!(backend = ?cfg.tts_backend, "Effective TTS backend");
            cfg
        }
        Err(err) => {
            warn!(path = %path.display(), "Invalid config TOML: {err}");
            let cfg = apply_qa_overrides(AppConfig::default());
            info!(backend = ?cfg.tts_backend, "Effective TTS backend");
            cfg
        }
    }
}

/// Apply an explicit repo-native QA override after parsing the staged config.
/// This is intentionally environment-scoped: normal user configuration, including
/// explicit Piper selection on Windows, remains untouched.
fn apply_qa_overrides(mut cfg: AppConfig) -> AppConfig {
    if let Ok(value) = std::env::var("LANTERNLEAF_QA_TTS_BACKEND") {
        match value.trim().to_ascii_lowercase().as_str() {
            "windows" => {
                cfg.tts_backend = super::models::TtsBackend::Windows;
                debug!(backend = ?cfg.tts_backend, "Applied Windows QA TTS backend override");
            }
            "piper" => {
                cfg.tts_backend = super::models::TtsBackend::Piper;
                debug!(backend = ?cfg.tts_backend, "Applied Piper QA TTS backend override");
            }
            other => warn!(value = other, "Ignoring unknown QA TTS backend override"),
        }
    }
    cfg
}

#[cfg(target_arch = "wasm32")]
pub fn load_config(_path: &Path) -> AppConfig {
    AppConfig::default()
}

pub fn parse_config(contents: &str) -> Result<AppConfig, toml::de::Error> {
    let cfg = toml::from_str::<ConfigInput>(contents)?;
    Ok(match cfg {
        ConfigInput::Tables(tables) => normalize_config(tables.into()),
        ConfigInput::Flat(flat) => normalize_config(flat),
    })
}

pub fn serialize_config(config: &AppConfig) -> Result<String, toml::ser::Error> {
    toml::to_string(&ConfigTables::from(config))
}

fn normalize_config(mut cfg: AppConfig) -> AppConfig {
    cfg.pretty.base_font_scale = cfg.pretty.base_font_scale.clamp(0.4, 2.0);
    cfg.chrome_font_scale = cfg.chrome_font_scale.clamp(0.4, 1.2);
    cfg.pretty.heading_scale_h1 = cfg.pretty.heading_scale_h1.clamp(0.5, 5.0);
    cfg.pretty.heading_scale_h2 = cfg.pretty.heading_scale_h2.clamp(0.5, 5.0);
    cfg.pretty.heading_scale_h3 = cfg.pretty.heading_scale_h3.clamp(0.5, 5.0);
    cfg.pretty.heading_scale_h4 = cfg.pretty.heading_scale_h4.clamp(0.5, 5.0);
    cfg.pretty.heading_scale_h5 = cfg.pretty.heading_scale_h5.clamp(0.5, 5.0);
    cfg.pretty.heading_scale_h6 = cfg.pretty.heading_scale_h6.clamp(0.5, 5.0);
    cfg.pretty.paragraph_spacing = cfg.pretty.paragraph_spacing.clamp(0.0, 64.0);
    cfg.pretty.block_spacing = cfg.pretty.block_spacing.clamp(0.0, 96.0);
    cfg.pretty.list_indent = cfg.pretty.list_indent.clamp(0.0, 128.0);
    cfg.pretty.list_item_spacing = cfg.pretty.list_item_spacing.clamp(0.0, 64.0);
    cfg.pretty.hr_thickness = cfg.pretty.hr_thickness.clamp(0.5, 6.0);
    cfg.pretty.hr_margin = cfg.pretty.hr_margin.clamp(0.0, 96.0);
    cfg.pretty.code_font_scale = cfg.pretty.code_font_scale.clamp(0.4, 2.0);
    cfg.pretty.code_bg_alpha = cfg.pretty.code_bg_alpha.clamp(0.0, 1.0);
    cfg.pretty.code_border_alpha = cfg.pretty.code_border_alpha.clamp(0.0, 1.0);
    cfg.pretty.link_color.r = cfg.pretty.link_color.r.clamp(0.0, 1.0);
    cfg.pretty.link_color.g = cfg.pretty.link_color.g.clamp(0.0, 1.0);
    cfg.pretty.link_color.b = cfg.pretty.link_color.b.clamp(0.0, 1.0);
    cfg.pretty.link_color.a = cfg.pretty.link_color.a.clamp(0.0, 1.0);
    cfg.pretty.image_max_width_pct = cfg.pretty.image_max_width_pct.clamp(10.0, 100.0);
    cfg.pretty.image_max_height_px = cfg.pretty.image_max_height_px.clamp(64.0, 4096.0);
    cfg.pretty.image_cache_max_entries = cfg.pretty.image_cache_max_entries.clamp(1, 2048);
    cfg.pretty.table_cell_padding = cfg.pretty.table_cell_padding.clamp(0.0, 64.0);
    cfg.pretty.table_border_alpha = cfg.pretty.table_border_alpha.clamp(0.0, 1.0);
    cfg.pretty.table_stripe_alpha = cfg.pretty.table_stripe_alpha.clamp(0.0, 1.0);
    cfg
}

#[cfg(test)]
mod tests {
    use super::parse_config;
    use crate::config::TtsBackend;

    #[test]
    fn omitted_tts_backend_uses_platform_default() {
        let config = parse_config("[tts]\n").expect("minimal table config should parse");
        let expected = if cfg!(windows) {
            TtsBackend::Windows
        } else {
            TtsBackend::Piper
        };
        assert_eq!(config.tts_backend, expected);
    }

    #[test]
    fn explicit_tts_backend_is_preserved_across_platforms() {
        let piper = parse_config("[tts]\ntts_backend = \"piper\"\n")
            .expect("explicit Piper config should parse");
        assert_eq!(piper.tts_backend, TtsBackend::Piper);

        let windows = parse_config("[tts]\ntts_backend = \"windows\"\n")
            .expect("explicit Windows config should parse");
        assert_eq!(windows.tts_backend, TtsBackend::Windows);
    }
}
