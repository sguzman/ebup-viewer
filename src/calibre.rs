use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::warn;

const DEFAULT_CALIBRE_CONFIG_PATH: &str = "conf/calibre.toml";
const CALIBRE_CACHE_PATH: &str = ".cache/calibre-books.toml";
const CALIBRE_CACHE_REV: &str = "calibre-cache-v1";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct CalibreConfig {
    pub enabled: bool,
    pub library_path: Option<PathBuf>,
    pub library_url: Option<String>,
    pub state_path: Option<PathBuf>,
    pub content_server: ContentServerConfig,
    pub calibredb_bin: String,
    pub server_urls: Vec<String>,
    pub server_username: Option<String>,
    pub server_password: Option<String>,
    pub allow_local_library_fallback: bool,
    pub allowed_extensions: Vec<String>,
    pub columns: Vec<String>,
    pub list_cache_ttl_secs: u64,
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
            enabled: false,
            library_path: None,
            library_url: Some("http://127.0.0.1:8080".to_string()),
            state_path: None,
            content_server: ContentServerConfig::default(),
            calibredb_bin: "calibredb".to_string(),
            server_urls: vec![
                "http://127.0.0.1:8080".to_string(),
                "http://localhost:8080".to_string(),
            ],
            server_username: None,
            server_password: None,
            allow_local_library_fallback: false,
            allowed_extensions: vec!["epub".to_string(), "md".to_string(), "txt".to_string()],
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
    pub path: PathBuf,
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

#[derive(Debug, Deserialize, Serialize)]
struct CachedBookList {
    rev: String,
    generated_unix_secs: u64,
    signature: String,
    books: Vec<CalibreBook>,
}

impl CalibreConfig {
    pub fn load_default() -> Self {
        let path = PathBuf::from(DEFAULT_CALIBRE_CONFIG_PATH);
        let Ok(contents) = fs::read_to_string(&path) else {
            return Self::default();
        };
        match toml::from_str::<CalibreFile>(&contents) {
            Ok(file) => file
                .calibre
                .or(file.calibred)
                .unwrap_or_else(CalibreConfig::default),
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
                "txt" => "txt",
                "md" | "markdown" => "md",
                _ => continue,
            };
            if !out.iter().any(|e| e == mapped) {
                out.push(mapped.to_string());
            }
        }
        if out.is_empty() {
            vec!["epub".to_string(), "md".to_string(), "txt".to_string()]
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

pub fn load_books(config: &CalibreConfig, force_refresh: bool) -> Result<Vec<CalibreBook>> {
    if !config.enabled {
        return Ok(Vec::new());
    }

    let signature = cache_signature(config);
    if !force_refresh {
        if let Some(cached) = try_load_cache(config, &signature)? {
            return Ok(cached);
        }
    }

    let books = fetch_books(config)?;
    write_cache(&signature, &books)?;
    Ok(books)
}

fn fetch_books(config: &CalibreConfig) -> Result<Vec<CalibreBook>> {
    let rows = fetch_rows_from_targets(config)?;
    let allowed_extensions = config.sanitized_extensions();
    let allowed_set: HashSet<String> = allowed_extensions.iter().cloned().collect();
    let library = config.state_path.clone().or(config.library_path.clone());

    let mut books = Vec::new();
    for row in rows {
        let id = parse_u64_field(&row, "id");
        let Some(id) = id else {
            continue;
        };

        let title = parse_string_field(&row, "title").unwrap_or_else(|| "Untitled".to_string());
        let authors = parse_authors(&row);
        let year = parse_year_field(&row, "pubdate");

        let formats = parse_formats(&row);
        let selected_ext = allowed_extensions
            .iter()
            .find(|ext| formats.iter().any(|f| f == *ext))
            .cloned();
        let Some(selected_ext) = selected_ext else {
            continue;
        };

        let Some(path) = resolve_book_file_path(library.as_deref(), &row, &selected_ext) else {
            continue;
        };
        let size_from_fs = fs::metadata(&path).ok().map(|m| m.len());
        let file_size_bytes = size_from_fs.or_else(|| parse_u64_field(&row, "size"));

        if !allowed_set.contains(&selected_ext) {
            continue;
        }

        books.push(CalibreBook {
            id,
            title,
            extension: selected_ext,
            authors,
            year,
            file_size_bytes,
            cover_thumbnail: resolve_cover_thumbnail(path.parent()),
            path,
        });
    }

    books.sort_by(|a, b| {
        a.title
            .to_ascii_lowercase()
            .cmp(&b.title.to_ascii_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(books)
}

fn fetch_rows_from_targets(config: &CalibreConfig) -> Result<Vec<serde_json::Value>> {
    let mut last_err = None;
    for target in calibre_targets(config) {
        match run_calibredb_list(config, &target) {
            Ok(rows) => return Ok(rows),
            Err(err) => last_err = Some(format!("{}: {err}", target.label)),
        }
    }
    Err(anyhow!(
        "no server detected (checked configured/default calibre content-server URLs). {}",
        last_err.unwrap_or_else(|| "no targets were available".to_string())
    ))
}

fn run_calibredb_list(
    config: &CalibreConfig,
    target: &CalibreTarget,
) -> Result<Vec<serde_json::Value>> {
    let mut cmd = Command::new(&config.calibredb_bin);
    cmd.arg("--with-library").arg(&target.with_library);
    if let Some(username) = &target.username {
        cmd.arg("--username").arg(username);
    }
    if let Some(password) = &target.password {
        cmd.arg("--password").arg(password);
    }
    cmd.arg("list")
        .arg("--for-machine")
        .arg("--fields")
        .arg("id,title,authors,pubdate,formats,size,path");

    let output = cmd
        .output()
        .with_context(|| format!("failed to run calibredb list against {}", target.label))?;
    if !output.status.success() {
        return Err(anyhow!(
            String::from_utf8_lossy(&output.stderr).trim().to_string()
        ));
    }
    let rows: Vec<serde_json::Value> =
        serde_json::from_slice(&output.stdout).with_context(|| {
            format!(
                "failed to parse calibredb JSON output from {}",
                target.label
            )
        })?;
    Ok(rows)
}

struct CalibreTarget {
    label: String,
    with_library: String,
    username: Option<String>,
    password: Option<String>,
}

fn calibre_targets(config: &CalibreConfig) -> Vec<CalibreTarget> {
    let mut targets = Vec::new();
    if let Some(url) = sanitized_library_url(config) {
        targets.push(CalibreTarget {
            label: format!("server:{url}"),
            with_library: url,
            username: effective_username(config),
            password: effective_password(config),
        });
    }
    for url in sanitized_server_urls(config) {
        if targets.iter().any(|t| t.with_library == url) {
            continue;
        }
        targets.push(CalibreTarget {
            label: format!("server:{url}"),
            with_library: url,
            username: effective_username(config),
            password: effective_password(config),
        });
    }
    if config.allow_local_library_fallback {
        if let Some(path) = config.state_path.as_ref().or(config.library_path.as_ref()) {
            targets.push(CalibreTarget {
                label: format!("local:{}", path.display()),
                with_library: path.to_string_lossy().to_string(),
                username: None,
                password: None,
            });
        }
    }
    targets
}

fn sanitized_server_urls(config: &CalibreConfig) -> Vec<String> {
    let mut urls = Vec::new();
    for raw in &config.server_urls {
        let url = raw.trim().trim_end_matches('/').to_string();
        if url.starts_with("http://") || url.starts_with("https://") {
            if !urls.iter().any(|u| u == &url) {
                urls.push(url);
            }
        }
    }
    if urls.is_empty() {
        vec![
            "http://127.0.0.1:8080".to_string(),
            "http://localhost:8080".to_string(),
        ]
    } else {
        urls
    }
}

fn sanitized_library_url(config: &CalibreConfig) -> Option<String> {
    config
        .library_url
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| v.starts_with("http://") || v.starts_with("https://"))
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

fn parse_u64_field(row: &serde_json::Value, key: &str) -> Option<u64> {
    row.get(key).and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_i64().map(|n| n.max(0) as u64))
            .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
    })
}

fn parse_string_field(row: &serde_json::Value, key: &str) -> Option<String> {
    row.get(key).and_then(|value| {
        value
            .as_str()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    })
}

fn parse_year_field(row: &serde_json::Value, key: &str) -> Option<i32> {
    let raw = parse_string_field(row, key)?;
    let year = raw.chars().take(4).collect::<String>();
    if year.chars().all(|c| c.is_ascii_digit()) {
        year.parse::<i32>().ok()
    } else {
        None
    }
}

fn parse_authors(row: &serde_json::Value) -> String {
    match row.get("authors") {
        Some(serde_json::Value::Array(values)) => {
            let joined = values
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            if joined.is_empty() {
                "Unknown".to_string()
            } else {
                joined
            }
        }
        Some(value) => value
            .as_str()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Unknown".to_string()),
        None => "Unknown".to_string(),
    }
}

fn parse_formats(row: &serde_json::Value) -> Vec<String> {
    let Some(value) = row.get("formats") else {
        return Vec::new();
    };
    match value {
        serde_json::Value::Array(values) => values
            .iter()
            .filter_map(|v| v.as_str())
            .map(normalize_format_value)
            .filter(|s| !s.is_empty())
            .collect(),
        serde_json::Value::String(raw) => raw
            .split(',')
            .map(normalize_format_value)
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn normalize_format_value(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    Path::new(trimmed)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .or_else(|| Some(trimmed.to_ascii_lowercase()))
        .unwrap_or_default()
}

fn resolve_book_file_path(
    library: Option<&Path>,
    row: &serde_json::Value,
    extension: &str,
) -> Option<PathBuf> {
    let rel_dir = parse_string_field(row, "path");
    let base = match (library, rel_dir.as_deref()) {
        (Some(root), Some(rel)) => root.join(rel),
        _ => return None,
    };
    let entries = fs::read_dir(&base).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())?;
        let normalized_ext = if ext == "markdown" {
            "md"
        } else {
            ext.as_str()
        };
        if normalized_ext == extension {
            return Some(path);
        }
    }
    None
}

fn resolve_cover_thumbnail(book_dir: Option<&Path>) -> Option<PathBuf> {
    let Some(dir) = book_dir else {
        return None;
    };
    for name in ["cover.jpg", "cover.jpeg", "cover.png", "cover.webp"] {
        let candidate = dir.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn try_load_cache(config: &CalibreConfig, signature: &str) -> Result<Option<Vec<CalibreBook>>> {
    let cache_path = PathBuf::from(CALIBRE_CACHE_PATH);
    let contents = match fs::read_to_string(&cache_path) {
        Ok(contents) => contents,
        Err(_) => return Ok(None),
    };
    let parsed: CachedBookList = match toml::from_str(&contents) {
        Ok(parsed) => parsed,
        Err(_) => return Ok(None),
    };
    if parsed.rev != CALIBRE_CACHE_REV || parsed.signature != signature {
        return Ok(None);
    }

    let now = now_unix_secs();
    if now.saturating_sub(parsed.generated_unix_secs) > config.list_cache_ttl_secs {
        return Ok(None);
    }

    Ok(Some(parsed.books))
}

fn write_cache(signature: &str, books: &[CalibreBook]) -> Result<()> {
    let cache_path = PathBuf::from(CALIBRE_CACHE_PATH);
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let payload = CachedBookList {
        rev: CALIBRE_CACHE_REV.to_string(),
        generated_unix_secs: now_unix_secs(),
        signature: signature.to_string(),
        books: books.to_vec(),
    };
    let serialized =
        toml::to_string(&payload).with_context(|| "failed to serialize calibre cache")?;
    fs::write(&cache_path, serialized)
        .with_context(|| format!("failed to write {}", cache_path.display()))?;
    Ok(())
}

fn cache_signature(config: &CalibreConfig) -> String {
    let mut hasher = Sha256::new();
    hasher.update(CALIBRE_CACHE_REV.as_bytes());
    hasher.update(config.calibredb_bin.as_bytes());
    if let Some(url) = sanitized_library_url(config) {
        hasher.update(url.as_bytes());
        hasher.update([0u8]);
    }
    for url in sanitized_server_urls(config) {
        hasher.update(url.as_bytes());
        hasher.update([0u8]);
    }
    if let Some(path) = &config.state_path {
        hasher.update(path.to_string_lossy().as_bytes());
    }
    if let Some(path) = &config.library_path {
        hasher.update(path.to_string_lossy().as_bytes());
    }
    hasher.update([config.allow_local_library_fallback as u8]);
    for ext in config.sanitized_extensions() {
        hasher.update(ext.as_bytes());
        hasher.update([0_u8]);
    }
    format!("{:x}", hasher.finalize())
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
