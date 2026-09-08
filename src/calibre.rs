#[path = "calibre/cache_store.rs"]
mod cache_store;
#[path = "calibre/caliberate.rs"]
mod caliberate;
#[path = "calibre/catalog.rs"]
mod catalog;
#[path = "calibre/thumbnails.rs"]
mod thumbnails;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tracing::warn;

const DEFAULT_CALIBRE_CONFIG_PATH: &str = "conf/calibre.toml";
const CALIBRE_CACHE_FILE: &str = "calibre-books.toml";
const CALIBRE_CACHE_REV: &str = "calibre-cache-v1";
const CALIBRE_DOWNLOAD_SUBDIR: &str = "calibre-downloads";
const CALIBRE_THUMB_SUBDIR: &str = "calibre-thumbs";
const THUMB_WIDTH: u32 = 68;
const THUMB_HEIGHT: u32 = 100;
const THUMB_PREFETCH_LIMIT: usize = 200;
const THUMB_PREFETCH_BUDGET: Duration = Duration::from_secs(2);
const THUMB_CACHED_PREFETCH_BUDGET: Duration = Duration::from_secs(4);
const THUMB_FETCH_TIMEOUT: Duration = Duration::from_millis(350);
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct CalibreConfig {
    pub provider: CalibreProvider,
    pub enabled: bool,
    pub library_path: Option<PathBuf>,
    pub library_url: Option<String>,
    pub state_path: Option<PathBuf>,
    pub content_server: ContentServerConfig,
    pub calibredb_bin: String,
    pub server_urls: Vec<String>,
    pub server_username: Option<String>,
    pub server_password: Option<String>,
    pub api_key: Option<String>,
    pub allow_local_library_fallback: bool,
    pub allowed_extensions: Vec<String>,
    pub columns: Vec<String>,
    pub list_cache_ttl_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CalibreProvider {
    Caliberate,
    Calibre,
}

impl Default for CalibreProvider {
    fn default() -> Self {
        Self::Caliberate
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct ContentServerConfig {
    pub username: Option<String>,
    pub password: Option<String>,
}

impl Default for CalibreConfig {
    fn default() -> Self {
        Self {
            provider: CalibreProvider::Caliberate,
            enabled: false,
            library_path: None,
            library_url: Some("http://127.0.0.1:8181".to_string()),
            state_path: None,
            content_server: ContentServerConfig::default(),
            calibredb_bin: "calibredb".to_string(),
            server_urls: vec![
                "http://127.0.0.1:8181".to_string(),
                "http://localhost:8181".to_string(),
            ],
            server_username: None,
            server_password: None,
            api_key: None,
            allow_local_library_fallback: false,
            allowed_extensions: vec![
                "epub".to_string(),
                "pdf".to_string(),
                "html".to_string(),
                "md".to_string(),
                "txt".to_string(),
            ],
            columns: vec![
                "title".to_string(),
                "extension".to_string(),
                "author".to_string(),
                "year".to_string(),
                "size".to_string(),
            ],
            list_cache_ttl_secs: 600,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
struct CalibreFile {
    calibre: Option<CalibreConfig>,
    calibred: Option<CalibreConfig>,
}

impl Default for CalibreFile {
    fn default() -> Self {
        Self {
            calibre: Some(CalibreConfig::default()),
            calibred: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CalibreBook {
    pub id: u64,
    pub title: String,
    pub extension: String,
    pub authors: String,
    pub year: Option<i32>,
    pub file_size_bytes: Option<u64>,
    pub path: Option<PathBuf>,
    pub cover_thumbnail: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibreColumn {
    Title,
    Extension,
    Author,
    Year,
    Size,
}

impl CalibreConfig {
    pub fn load_default() -> Self {
        let Some(path) = resolve_config_path() else {
            return Self::default();
        };
        let Ok(contents) = fs::read_to_string(&path) else {
            return Self::default();
        };
        match toml::from_str::<CalibreFile>(&contents) {
            Ok(file) => {
                let mut config = file
                    .calibre
                    .or(file.calibred)
                    .unwrap_or_else(CalibreConfig::default);
                let provider_is_explicit = toml::from_str::<toml::Value>(&contents)
                    .ok()
                    .map(|value| {
                        value
                            .get("calibre")
                            .or_else(|| value.get("calibred"))
                            .and_then(|table| table.get("provider"))
                            .is_some()
                    })
                    .unwrap_or(false);
                if !provider_is_explicit && library_id(&config).is_some() {
                    config.provider = CalibreProvider::Calibre;
                }
                config
            }
            Err(err) => {
                warn!(
                    path = %path.display(),
                    "Invalid calibre config TOML; falling back to defaults: {err}"
                );
                CalibreConfig::default()
            }
        }
    }

    pub fn sanitized_extensions(&self) -> Vec<String> {
        let mut out = Vec::new();
        for ext in &self.allowed_extensions {
            let normalized = ext.trim().trim_start_matches('.').to_ascii_lowercase();
            let mapped = match normalized.as_str() {
                "epub" => "epub",
                "pdf" => "pdf",
                "txt" => "txt",
                "html" => "html",
                "md" | "markdown" => "md",
                _ => continue,
            };
            if !out.iter().any(|e| e == mapped) {
                out.push(mapped.to_string());
            }
        }
        if out.is_empty() {
            vec![
                "epub".to_string(),
                "pdf".to_string(),
                "html".to_string(),
                "md".to_string(),
                "txt".to_string(),
            ]
        } else {
            out
        }
    }

    pub fn sanitized_columns(&self) -> Vec<CalibreColumn> {
        let mut out = Vec::new();
        for column in &self.columns {
            let normalized = column.trim().to_ascii_lowercase();
            let mapped = match normalized.as_str() {
                "title" => CalibreColumn::Title,
                "ext" | "extension" | "format" => CalibreColumn::Extension,
                "author" | "authors" => CalibreColumn::Author,
                "year" | "pub-year" | "published" => CalibreColumn::Year,
                "size" | "file-size" => CalibreColumn::Size,
                _ => continue,
            };
            if !out.contains(&mapped) {
                out.push(mapped);
            }
        }
        if out.is_empty() {
            vec![
                CalibreColumn::Title,
                CalibreColumn::Extension,
                CalibreColumn::Author,
                CalibreColumn::Year,
                CalibreColumn::Size,
            ]
        } else {
            out
        }
    }
}

fn resolve_config_path() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("CALIBRE_CONFIG_PATH") {
        let candidate = PathBuf::from(value);
        if candidate.exists() {
            return Some(candidate);
        }
        warn!(
            path = %candidate.display(),
            "CALIBRE_CONFIG_PATH is set but file does not exist; falling back to defaults/search paths"
        );
    }

    let mut candidates = Vec::new();
    candidates.push(PathBuf::from(DEFAULT_CALIBRE_CONFIG_PATH));

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(DEFAULT_CALIBRE_CONFIG_PATH));
        if cwd
            .file_name()
            .map(|name| name == std::ffi::OsStr::new("src-tauri"))
            .unwrap_or(false)
        {
            if let Some(parent) = cwd.parent() {
                candidates.push(parent.join(DEFAULT_CALIBRE_CONFIG_PATH));
            }
        }
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    candidates.push(manifest_dir.join(DEFAULT_CALIBRE_CONFIG_PATH));

    if let Some(parent) = manifest_dir.parent() {
        candidates.push(parent.join(DEFAULT_CALIBRE_CONFIG_PATH));
        if let Some(grand_parent) = parent.parent() {
            candidates.push(grand_parent.join(DEFAULT_CALIBRE_CONFIG_PATH));
        }
    }

    candidates.into_iter().find(|candidate| candidate.exists())
}

pub fn load_books(config: &CalibreConfig, force_refresh: bool) -> Result<Vec<CalibreBook>> {
    catalog::load_books_with_cancel(config, force_refresh, None)
}

pub fn load_cached_books(config: &CalibreConfig) -> Result<Vec<CalibreBook>> {
    catalog::load_cached_books(config)
}

pub fn load_books_with_cancel(
    config: &CalibreConfig,
    force_refresh: bool,
    cancel: Option<&crate::cancellation::CancellationToken>,
) -> Result<Vec<CalibreBook>> {
    catalog::load_books_with_cancel(config, force_refresh, cancel)
}

pub fn materialize_book_path(config: &CalibreConfig, book: &CalibreBook) -> Result<PathBuf> {
    catalog::materialize_book_path(config, book)
}

pub fn ensure_thumbnail_for_book(
    config: &CalibreConfig,
    book: &mut CalibreBook,
    allow_remote_fetch: bool,
) -> bool {
    thumbnails::ensure_thumbnail_for_book(config, book, allow_remote_fetch)
}

pub fn server_base_url(config: &CalibreConfig) -> Option<String> {
    config
        .library_url
        .as_ref()
        .map(|v| v.trim())
        .filter(|v| v.starts_with("http://") || v.starts_with("https://"))
        .map(|v| {
            v.split('#')
                .next()
                .unwrap_or(v)
                .trim_end_matches('/')
                .to_string()
        })
}

pub fn library_id(config: &CalibreConfig) -> Option<String> {
    config
        .library_url
        .as_ref()
        .map(|v| v.trim())
        .and_then(|v| v.split('#').nth(1))
        .map(|v| v.to_string())
}

pub fn build_http_client(config: &CalibreConfig) -> Result<reqwest::blocking::Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    if matches!(config.provider, CalibreProvider::Calibre)
        && let (Some(user), Some(pass)) = (effective_username(config), effective_password(config))
    {
        let auth = format!("{user}:{pass}");
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(auth.as_bytes());
        if let Ok(val) = reqwest::header::HeaderValue::from_str(&format!("Basic {b64}")) {
            headers.insert(reqwest::header::AUTHORIZATION, val);
        }
    }
    if matches!(config.provider, CalibreProvider::Caliberate)
        && let Some(key) = config
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
    {
        if let Ok(value) = reqwest::header::HeaderValue::from_str(key) {
            headers.insert("x-api-key", value);
        }
    }
    reqwest::blocking::Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(Into::into)
}

fn effective_username(config: &CalibreConfig) -> Option<String> {
    config
        .content_server
        .username
        .as_ref()
        .or(config.server_username.as_ref())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn effective_password(config: &CalibreConfig) -> Option<String> {
    config
        .content_server
        .password
        .as_ref()
        .or(config.server_password.as_ref())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_file(name: &str, extension: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("lanternleaf_calibre_{name}_{nanos}.{extension}"))
    }

    #[test]
    fn load_default_reads_env_override_file() {
        let key = "CALIBRE_CONFIG_PATH";
        let previous = std::env::var_os(key);
        let path = unique_temp_file("load_default", "toml");
        fs::write(
            &path,
            r#"
[calibre]
enabled = true
server_urls = ["http://0.0.0.0:1"]
allowed_extensions = ["epub", "pdf", "txt"]
"#,
        )
        .expect("write calibre override");

        // SAFETY: test-scoped env mutation; restored before test exits.
        unsafe {
            std::env::set_var(key, &path);
        }
        let config = CalibreConfig::load_default();
        assert!(config.enabled);
        assert_eq!(config.server_urls, vec!["http://0.0.0.0:1".to_string()]);

        match previous {
            Some(value) => {
                // SAFETY: test-scoped env mutation restore.
                unsafe {
                    std::env::set_var(key, value);
                }
            }
            None => {
                // SAFETY: test-scoped env mutation restore.
                unsafe {
                    std::env::remove_var(key);
                }
            }
        }

        let _ = fs::remove_file(path);
    }

    #[test]
    fn load_default_discovers_workspace_config_without_env_override() {
        let key = "CALIBRE_CONFIG_PATH";
        let previous = std::env::var_os(key);

        // SAFETY: test-scoped env mutation; restored before test exits.
        unsafe {
            std::env::remove_var(key);
        }

        let resolved =
            resolve_config_path().expect("workspace calibre config should be discoverable");
        assert!(resolved.exists());
        assert_eq!(
            resolved.file_name().and_then(|name| name.to_str()),
            Some("calibre.toml")
        );

        match previous {
            Some(value) => {
                // SAFETY: test-scoped env mutation restore.
                unsafe {
                    std::env::set_var(key, value);
                }
            }
            None => {
                // SAFETY: test-scoped env mutation restore.
                unsafe {
                    std::env::remove_var(key);
                }
            }
        }
    }

    #[test]
    fn calibre_paths_follow_cache_root_override() {
        let key = crate::cache::CACHE_DIR_ENV;
        let previous = std::env::var_os(key);
        let override_path = std::env::temp_dir().join(format!(
            "lanternleaf_calibre_cache_root_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));

        // SAFETY: test-scoped env mutation; restored before test exits.
        unsafe {
            std::env::set_var(key, &override_path);
        }

        assert_eq!(
            cache_store::calibre_cache_path_for(&CalibreConfig::default()),
            override_path.join("lantern-leaf").join(format!(
                "{CALIBRE_CACHE_FILE}-caliberate-{}",
                cache_store::cache_signature(&CalibreConfig::default())
                    .chars()
                    .take(16)
                    .collect::<String>()
            ))
        );
        assert_eq!(
            cache_store::calibre_download_dir(),
            override_path
                .join("lantern-leaf")
                .join(CALIBRE_DOWNLOAD_SUBDIR)
        );
        assert_eq!(
            cache_store::calibre_thumb_dir(),
            override_path
                .join("lantern-leaf")
                .join(CALIBRE_THUMB_SUBDIR)
        );

        match previous {
            Some(value) => {
                // SAFETY: test-scoped env mutation restore.
                unsafe {
                    std::env::set_var(key, value);
                }
            }
            None => {
                // SAFETY: test-scoped env mutation restore.
                unsafe {
                    std::env::remove_var(key);
                }
            }
        }
    }

    #[test]
    fn load_books_honors_cancellation_token() {
        let mut config = CalibreConfig::default();
        config.enabled = true;
        let token = crate::cancellation::CancellationToken::new();
        token.cancel();

        let err = load_books_with_cancel(&config, false, Some(&token))
            .expect_err("cancelled calibre load should return an error");
        assert!(
            err.to_string().contains("operation cancelled"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn default_config_targets_caliberate_without_a_library_fragment() {
        let config = CalibreConfig::default();
        assert_eq!(config.provider, CalibreProvider::Caliberate);
        assert_eq!(
            server_base_url(&config).as_deref(),
            Some("http://127.0.0.1:8181")
        );
        assert_eq!(library_id(&config), None);
    }

    #[test]
    fn legacy_config_without_provider_is_preserved_by_library_fragment() {
        let key = "CALIBRE_CONFIG_PATH";
        let previous = std::env::var_os(key);
        let path = unique_temp_file("legacy_provider", "toml");
        fs::write(
            &path,
            r#"
[calibre]
enabled = true
library_url = "http://127.0.0.1:8083/#main"
"#,
        )
        .unwrap();
        unsafe { std::env::set_var(key, &path) };
        assert_eq!(
            CalibreConfig::load_default().provider,
            CalibreProvider::Calibre
        );
        match previous {
            Some(value) => unsafe { std::env::set_var(key, value) },
            None => unsafe { std::env::remove_var(key) },
        }
        let _ = fs::remove_file(path);
    }

    #[test]
    fn provider_is_part_of_catalog_and_thumbnail_cache_identity() {
        let mut caliberate = CalibreConfig::default();
        let mut legacy = caliberate.clone();
        legacy.provider = CalibreProvider::Calibre;
        assert_ne!(
            cache_store::cache_signature(&caliberate),
            cache_store::cache_signature(&legacy)
        );
        caliberate.api_key = Some("not included in cache identity".to_string());
        assert_ne!(
            cache_store::cache_signature(&caliberate),
            cache_store::cache_signature(&legacy)
        );
    }
}
