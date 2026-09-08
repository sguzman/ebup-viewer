use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    CALIBRE_CACHE_FILE, CALIBRE_CACHE_REV, CALIBRE_DOWNLOAD_SUBDIR, CALIBRE_THUMB_SUBDIR,
    CalibreBook, CalibreConfig, CalibreProvider, server_base_url,
};

#[derive(Debug, Deserialize, Serialize)]
struct CachedBookList {
    rev: String,
    generated_unix_secs: u64,
    signature: String,
    #[serde(default)]
    provider: Option<CalibreProvider>,
    books: Vec<CalibreBook>,
}

pub(super) fn try_load_cache(
    config: &CalibreConfig,
    signature: &str,
    check_ttl: bool,
    allow_signature_mismatch: bool,
) -> Result<Option<Vec<CalibreBook>>> {
    let cache_path = calibre_cache_path_for(config);
    let contents = match fs::read_to_string(&cache_path) {
        Ok(contents) => contents,
        Err(_) => return Ok(None),
    };
    let parsed: CachedBookList = match toml::from_str(&contents) {
        Ok(parsed) => parsed,
        Err(_) => return Ok(None),
    };
    if parsed.rev != CALIBRE_CACHE_REV {
        return Ok(None);
    }
    if !allow_signature_mismatch && parsed.signature != signature {
        return Ok(None);
    }
    if allow_signature_mismatch && parsed.provider != Some(config.provider) {
        return Ok(None);
    }

    if check_ttl {
        let now = now_unix_secs();
        if now.saturating_sub(parsed.generated_unix_secs) > config.list_cache_ttl_secs {
            return Ok(None);
        }
    }

    Ok(Some(parsed.books))
}

pub(super) fn write_cache(
    config: &CalibreConfig,
    signature: &str,
    books: &[CalibreBook],
) -> Result<()> {
    let cache_path = calibre_cache_path_for(config);
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let payload = CachedBookList {
        rev: CALIBRE_CACHE_REV.to_string(),
        generated_unix_secs: now_unix_secs(),
        signature: signature.to_string(),
        provider: Some(config.provider),
        books: books.to_vec(),
    };
    let serialized =
        toml::to_string(&payload).with_context(|| "failed to serialize calibre cache")?;
    fs::write(&cache_path, serialized)
        .with_context(|| format!("failed to write {}", cache_path.display()))?;
    Ok(())
}

pub(super) fn cache_signature(config: &CalibreConfig) -> String {
    let mut hasher = Sha256::new();
    hasher.update(CALIBRE_CACHE_REV.as_bytes());
    hasher.update(match config.provider {
        CalibreProvider::Caliberate => b"caliberate" as &[u8],
        CalibreProvider::Calibre => b"calibre",
    });
    hasher.update([0u8]);
    hasher.update(config.calibredb_bin.as_bytes());
    if let Some(url) = server_base_url(config) {
        hasher.update(url.as_bytes());
        hasher.update([0u8]);
    }
    for url in &config.server_urls {
        hasher.update(url.trim().trim_end_matches('/').as_bytes());
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

pub(super) fn calibre_cache_path() -> PathBuf {
    crate::cache::cache_root().join(CALIBRE_CACHE_FILE)
}

pub(super) fn calibre_cache_path_for(config: &CalibreConfig) -> PathBuf {
    let provider = match config.provider {
        CalibreProvider::Caliberate => "caliberate",
        CalibreProvider::Calibre => "calibre",
    };
    let signature = cache_signature(config);
    let scope = signature.chars().take(16).collect::<String>();
    crate::cache::cache_root().join(format!("{CALIBRE_CACHE_FILE}-{provider}-{scope}"))
}

pub(super) fn calibre_download_dir() -> PathBuf {
    crate::cache::cache_root().join(CALIBRE_DOWNLOAD_SUBDIR)
}

pub(super) fn calibre_thumb_dir() -> PathBuf {
    crate::cache::cache_root().join(CALIBRE_THUMB_SUBDIR)
}

pub(super) fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(super) fn now_unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}
