use super::models::AppConfig;
use super::tables::ConfigTables;
use serde::Deserialize;
use std::fs;
use std::path::Path;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ConfigInput {
    Tables(ConfigTables),
    Flat(AppConfig),
}

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
            return AppConfig::default();
        }
    };

    match parse_config(&contents) {
        Ok(cfg) => {
            debug!("Parsed configuration from disk");
            cfg
        }
        Err(err) => {
            warn!(path = %path.display(), "Invalid config TOML: {err}");
            AppConfig::default()
        }
    }
}

pub fn parse_config(contents: &str) -> Result<AppConfig, toml::de::Error> {
    let cfg = toml::from_str::<ConfigInput>(contents)?;
    Ok(match cfg {
        ConfigInput::Tables(tables) => tables.into(),
        ConfigInput::Flat(flat) => flat,
    })
}

pub fn serialize_config(config: &AppConfig) -> Result<String, toml::ser::Error> {
    toml::to_string(&ConfigTables::from(config))
}
