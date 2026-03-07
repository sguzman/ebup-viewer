use anyhow::{Context, Result};
use epub::doc::EpubDoc;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use reqwest::StatusCode;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tracing::{debug, info};

use super::{
    CalibreBook, CalibreConfig, THUMB_FETCH_TIMEOUT, THUMB_HEIGHT, THUMB_WIDTH,
    cache_store::calibre_thumb_dir, effective_password, effective_username, sanitized_library_url,
    sanitized_server_urls,
};

pub(super) fn hydrate_book_thumbnails(
    config: &CalibreConfig,
    books: &mut [CalibreBook],
    limit: usize,
    budget: Duration,
    cancel: Option<&crate::cancellation::CancellationToken>,
    allow_remote_fetch: bool,
) -> bool {
    let mut changed = false;
    let started = Instant::now();
    let deadline = started + budget;
    let mut processed = 0usize;
    let mut available = 0usize;
    let prefetch_count = books.len().min(limit);
    for book in books.iter_mut().take(prefetch_count) {
        if let Some(token) = cancel
            && token.is_cancelled()
        {
            info!("Stopping calibre thumbnail prefetch due to cancellation");
            break;
        }
        if started.elapsed() >= budget {
            info!(
                processed,
                available,
                budget_ms = budget.as_millis(),
                "Stopping calibre thumbnail prefetch due to time budget"
            );
            break;
        }
        let current = book.cover_thumbnail.clone();
        let book_started = Instant::now();
        let next = ensure_book_thumbnail(
            config,
            book.id,
            book.path.as_deref(),
            deadline,
            allow_remote_fetch,
        );
        if next != current {
            book.cover_thumbnail = next;
            changed = true;
        }
        let per_book_ms = book_started.elapsed().as_millis();
        if per_book_ms > 200 {
            info!(
                book_id = book.id,
                elapsed_ms = per_book_ms,
                "Slow thumbnail prefetch item"
            );
        }
        processed += 1;
        if book.cover_thumbnail.is_some() {
            available += 1;
        }
        if processed % 25 == 0 {
            info!(
                processed,
                available,
                elapsed_ms = started.elapsed().as_millis(),
                "Calibre thumbnail prefetch progress"
            );
        }
    }
    info!(
        processed,
        available,
        changed,
        elapsed_ms = started.elapsed().as_millis(),
        "Finished calibre thumbnail prefetch pass"
    );
    changed
}

pub(super) fn ensure_thumbnail_for_book(
    config: &CalibreConfig,
    book: &mut CalibreBook,
    allow_remote_fetch: bool,
) -> bool {
    let before = book.cover_thumbnail.clone();
    let deadline = Instant::now() + THUMB_FETCH_TIMEOUT.saturating_mul(6);
    let next = ensure_book_thumbnail(
        config,
        book.id,
        book.path.as_deref(),
        deadline,
        allow_remote_fetch,
    );
    if next != before {
        book.cover_thumbnail = next;
        return true;
    }
    false
}

fn ensure_book_thumbnail(
    config: &CalibreConfig,
    book_id: u64,
    source_path: Option<&Path>,
    deadline: Instant,
    allow_remote_fetch: bool,
) -> Option<PathBuf> {
    let thumb_path = calibre_thumbnail_path(config, book_id);
    if thumb_path.exists() {
        return Some(thumb_path);
    }

    if let Some(dir) = source_path.and_then(Path::parent)
        && let Some(local_cover) = resolve_local_cover_file(dir)
        && let Ok(bytes) = fs::read(&local_cover)
        && write_thumbnail_file(&thumb_path, &bytes).is_ok()
    {
        info!(
            book_id,
            path = %thumb_path.display(),
            source = %local_cover.display(),
            "Hydrated calibre thumbnail from local cover sidecar"
        );
        return Some(thumb_path);
    }

    if let Some(epub_source) = source_path.filter(|path| is_epub_source_path(path))
        && let Some(cover) = extract_epub_cover_bytes(epub_source)
        && write_thumbnail_file(&thumb_path, &cover).is_ok()
    {
        info!(
            book_id,
            path = %thumb_path.display(),
            source = %epub_source.display(),
            "Hydrated calibre thumbnail from EPUB embedded cover"
        );
        return Some(thumb_path);
    }

    if allow_remote_fetch
        && let Some(bytes) = fetch_thumbnail_from_server(config, book_id, deadline)
        && write_thumbnail_file(&thumb_path, &bytes).is_ok()
    {
        return Some(thumb_path);
    }

    None
}

fn is_epub_source_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("epub"))
        .unwrap_or(false)
}

fn extract_epub_cover_bytes(source_path: &Path) -> Option<Vec<u8>> {
    let mut doc = EpubDoc::new(source_path).ok()?;
    let (cover, _mime) = doc.get_cover()?;
    if cover.is_empty() {
        return None;
    }
    Some(cover)
}

fn resolve_local_cover_file(book_dir: &Path) -> Option<PathBuf> {
    for name in ["cover.jpg", "cover.jpeg", "cover.png", "cover.webp"] {
        let candidate = book_dir.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn fetch_thumbnail_from_server(
    config: &CalibreConfig,
    book_id: u64,
    deadline: Instant,
) -> Option<Vec<u8>> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining < Duration::from_millis(40) {
        return None;
    }
    let timeout = remaining.min(THUMB_FETCH_TIMEOUT);
    let client = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .ok()?;
    let username = effective_username(config);
    let password = effective_password(config);
    let endpoints = [
        format!("get/thumb/{book_id}"),
        format!("get/cover/{book_id}"),
    ];

    for base in cover_server_urls(config).into_iter().take(1) {
        if Instant::now() >= deadline {
            return None;
        }
        for endpoint in &endpoints {
            if Instant::now() >= deadline {
                return None;
            }
            let url = format!("{base}/{endpoint}");
            let mut request = client.get(&url);
            if let Some(user) = username.as_ref() {
                request = request.basic_auth(user, password.clone());
            }

            let Ok(response) = request.send() else {
                continue;
            };
            if response.status() != StatusCode::OK {
                continue;
            }
            let Ok(bytes) = response.bytes() else {
                continue;
            };
            if bytes.is_empty() {
                continue;
            }
            return Some(bytes.to_vec());
        }
    }

    None
}

fn calibre_thumbnail_path(config: &CalibreConfig, book_id: u64) -> PathBuf {
    let key = thumbnail_scope_key(config);
    calibre_thumb_dir().join(key).join(format!("{book_id}.jpg"))
}

fn thumbnail_scope_key(config: &CalibreConfig) -> String {
    let mut hasher = Sha256::new();
    if let Some(url) = sanitized_library_url(config) {
        hasher.update(url.as_bytes());
    }
    if let Some(path) = config.state_path.as_ref().or(config.library_path.as_ref()) {
        hasher.update(path.to_string_lossy().as_bytes());
    }
    for url in sanitized_server_urls(config) {
        hasher.update(url.as_bytes());
    }
    let digest = format!("{:x}", hasher.finalize());
    digest.chars().take(16).collect()
}

fn cover_server_urls(config: &CalibreConfig) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(raw) = sanitized_library_url(config)
        && let Some(base) = normalize_server_base_url(&raw)
    {
        out.push(base);
    }
    for raw in sanitized_server_urls(config) {
        if let Some(base) = normalize_server_base_url(&raw)
            && !out.iter().any(|known| known == &base)
        {
            out.push(base);
        }
    }
    out
}

fn normalize_server_base_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return None;
    }
    let no_fragment = trimmed.split('#').next()?.split('?').next()?.trim();
    let normalized = no_fragment.trim_end_matches('/').to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn write_thumbnail_file(path: &Path, raw_image: &[u8]) -> Result<()> {
    let image = image::load_from_memory(raw_image).context("decoding thumbnail image")?;
    let thumb = image.resize(THUMB_WIDTH, THUMB_HEIGHT, FilterType::Triangle);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create thumbnail dir {}", parent.display()))?;
    }
    let mut encoded = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(Cursor::new(&mut encoded), 80);
    encoder
        .encode_image(&thumb)
        .context("encoding thumbnail as jpeg")?;
    fs::write(path, encoded)
        .with_context(|| format!("failed to write thumbnail {}", path.display()))?;
    debug!(path = %path.display(), "cached calibre thumbnail");
    Ok(())
}
