use std::fs;
use std::path::Path;

use crate::config;
use tracing::{info, warn};

pub trait ConfigService: Send + Sync {
    fn save_base_config(&self, path: &Path, config: &config::AppConfig) -> Result<(), String>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FilesystemConfigService;

impl ConfigService for FilesystemConfigService {
    fn save_base_config(&self, path: &Path, config: &config::AppConfig) -> Result<(), String> {
        let Some(parent) = path.parent() else {
            return Err("config path has no parent directory".to_string());
        };
        if let Err(err) = fs::create_dir_all(parent) {
            return Err(format!(
                "failed to create config directory {}: {err}",
                parent.display()
            ));
        }
        let serialized =
            config::serialize_config(config).map_err(|err| format!("serialize failed: {err}"))?;
        if let Err(err) = fs::write(path, serialized.as_bytes()) {
            warn!(path = %path.display(), "Failed to write config: {err}");
            return Err(format!("failed to write config {}: {err}", path.display()));
        }
        info!(path = %path.display(), "Saved base config");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_config_path() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("lanternleaf_config_{nanos}.toml"))
    }

    #[test]
    fn save_base_config_writes_file() {
        let path = temp_config_path();
        let mut config = config::AppConfig::default();
        config.tts_speed = 2.5;
        let service = FilesystemConfigService;
        service
            .save_base_config(&path, &config)
            .expect("save config");
        let written = fs::read_to_string(&path).expect("read config");
        assert!(written.contains("tts_speed"));
        let _ = fs::remove_file(&path);
    }
}
