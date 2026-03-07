use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use tracing::{info, warn};

use crate::cancellation::CancellationToken;

use super::{
    CALIBRE_DB_TIMEOUT_SECS, CalibreBook, CalibreConfig, THUMB_CACHED_PREFETCH_BUDGET,
    THUMB_PREFETCH_BUDGET, THUMB_PREFETCH_LIMIT,
    cache_store::{
        cache_signature, calibre_download_dir, now_unix_nanos, try_load_cache, write_cache,
    },
    effective_password, effective_username, sanitized_library_url, sanitized_server_urls,
    thumbnails::hydrate_book_thumbnails,
};

pub(super) fn load_cached_books(config: &CalibreConfig) -> Result<Vec<CalibreBook>> {
    if !config.enabled {
        return Ok(Vec::new());
    }

    let signature = cache_signature(config);
    let mut cached = match try_load_cache(config, &signature, false, false)? {
        Some(books) => books,
        None => return Ok(Vec::new()),
    };
    info!(
        book_count = cached.len(),
        budget_ms = THUMB_CACHED_PREFETCH_BUDGET.as_millis(),
        "Hydrating cached calibre thumbnails (local-first pass)"
    );
    let changed = hydrate_book_thumbnails(
        config,
        &mut cached,
        usize::MAX,
        THUMB_CACHED_PREFETCH_BUDGET,
        None,
        false,
    );
    if changed {
        info!(
            book_count = cached.len(),
            "Cached calibre books gained thumbnail updates; rewriting cache"
        );
        let _ = write_cache(&signature, &cached);
    }
    Ok(cached)
}

pub(super) fn load_books_with_cancel(
    config: &CalibreConfig,
    force_refresh: bool,
    cancel: Option<&CancellationToken>,
) -> Result<Vec<CalibreBook>> {
    ensure_not_cancelled(cancel, "calibre_load_start")?;
    if !config.enabled {
        warn!("Calibre integration is disabled in config; returning empty catalogue");
        return Ok(Vec::new());
    }

    let started = Instant::now();
    info!(
        force_refresh,
        list_cache_ttl_secs = config.list_cache_ttl_secs,
        thumb_prefetch_limit = THUMB_PREFETCH_LIMIT,
        thumb_prefetch_budget_ms = THUMB_PREFETCH_BUDGET.as_millis(),
        "Starting calibre catalog load"
    );

    let signature = cache_signature(config);
    if !force_refresh {
        ensure_not_cancelled(cancel, "before_cache_load")?;
        if let Some(mut cached) = try_load_cache(config, &signature, false, false)? {
            info!(book_count = cached.len(), "Using cached calibre catalog");
            let changed = hydrate_book_thumbnails(
                config,
                &mut cached,
                THUMB_PREFETCH_LIMIT,
                THUMB_PREFETCH_BUDGET,
                cancel,
                true,
            );
            if changed {
                let _ = write_cache(&signature, &cached);
            }
            info!(
                book_count = cached.len(),
                elapsed_ms = started.elapsed().as_millis(),
                "Finished calibre catalog load from cache"
            );
            return Ok(cached);
        }
        info!("Calibre cache missing/incompatible; fetching from source");
    }

    let mut books = match fetch_books(config, cancel) {
        Ok(books) => books,
        Err(err) => {
            ensure_not_cancelled(cancel, "after_fetch_books_failed")?;
            if let Some(mut cached) = try_load_cache(config, &signature, false, true)? {
                warn!(
                    error = %err,
                    book_count = cached.len(),
                    "Failed to fetch calibre catalog; falling back to cached catalog"
                );
                let changed = hydrate_book_thumbnails(
                    config,
                    &mut cached,
                    THUMB_PREFETCH_LIMIT,
                    THUMB_PREFETCH_BUDGET,
                    cancel,
                    true,
                );
                if changed {
                    let _ = write_cache(&signature, &cached);
                }
                info!(
                    book_count = cached.len(),
                    elapsed_ms = started.elapsed().as_millis(),
                    "Finished calibre catalog load from fallback cache"
                );
                return Ok(cached);
            }
            return Err(err);
        }
    };

    ensure_not_cancelled(cancel, "after_fetch_books")?;
    info!(
        book_count = books.len(),
        "Fetched calibre catalog from source"
    );
    let _ = hydrate_book_thumbnails(
        config,
        &mut books,
        THUMB_PREFETCH_LIMIT,
        THUMB_PREFETCH_BUDGET,
        cancel,
        true,
    );
    ensure_not_cancelled(cancel, "before_write_cache")?;
    info!(book_count = books.len(), "Writing calibre cache file");
    write_cache(&signature, &books)?;
    info!(book_count = books.len(), "Calibre cache file updated");
    info!(
        book_count = books.len(),
        elapsed_ms = started.elapsed().as_millis(),
        "Finished calibre catalog load"
    );
    Ok(books)
}

pub(super) fn materialize_book_path(config: &CalibreConfig, book: &CalibreBook) -> Result<PathBuf> {
    if let Some(path) = book.path.as_ref().filter(|path| path.exists()) {
        return Ok(path.clone());
    }

    let ext = canonical_extension(&book.extension);
    let cache_root = calibre_download_dir();
    fs::create_dir_all(&cache_root)
        .with_context(|| format!("failed to create {}", cache_root.display()))?;

    let file_name = format!("{}-{}.{}", book.id, short_title_hash(&book.title), ext);
    let target_path = cache_root.join(file_name);
    if target_path.exists() {
        return Ok(target_path);
    }

    let mut last_err = None;
    for target in calibre_targets(config) {
        let tmp_dir = cache_root.join(format!(
            "tmp-{}-{}-{}",
            book.id,
            std::process::id(),
            now_unix_nanos()
        ));
        if let Err(err) = fs::create_dir_all(&tmp_dir) {
            last_err = Some(format!("failed to create {}: {err}", tmp_dir.display()));
            continue;
        }

        let export_result = run_calibredb_export(config, &target, book.id, &ext, &tmp_dir)
            .and_then(|_| {
                find_exported_file(&tmp_dir, &ext).ok_or_else(|| {
                    anyhow!(
                        "export completed but no .{ext} file was found in {}",
                        tmp_dir.display()
                    )
                })
            })
            .and_then(|found| {
                fs::copy(&found, &target_path).with_context(|| {
                    format!(
                        "failed to copy exported file {} -> {}",
                        found.display(),
                        target_path.display()
                    )
                })?;
                Ok(())
            });

        let _ = fs::remove_dir_all(&tmp_dir);

        match export_result {
            Ok(()) => return Ok(target_path),
            Err(err) => last_err = Some(format!("{}: {err}", target.label)),
        }
    }

    Err(anyhow!(
        "failed to materialize book id={} ext={} via calibre targets. {}",
        book.id,
        ext,
        last_err.unwrap_or_else(|| "no export targets succeeded".to_string())
    ))
}

fn fetch_books(
    config: &CalibreConfig,
    cancel: Option<&CancellationToken>,
) -> Result<Vec<CalibreBook>> {
    ensure_not_cancelled(cancel, "fetch_books_start")?;
    let rows = fetch_rows_from_targets(config, cancel)?;
    let allowed_extensions = config.sanitized_extensions();
    let allowed_set: HashSet<String> = allowed_extensions.iter().cloned().collect();
    let library = config.state_path.clone().or(config.library_path.clone());
    let id_dir_index = library
        .as_deref()
        .map(index_library_book_dirs)
        .unwrap_or_default();

    let mut books = Vec::new();
    for row in rows {
        ensure_not_cancelled(cancel, "fetch_books_row_loop")?;
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

        let path =
            resolve_book_file_path(library.as_deref(), &id_dir_index, &row, id, &selected_ext);
        let size_from_fs = path
            .as_ref()
            .and_then(|resolved| fs::metadata(resolved).ok().map(|m| m.len()));
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
            cover_thumbnail: None,
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

fn fetch_rows_from_targets(
    config: &CalibreConfig,
    cancel: Option<&CancellationToken>,
) -> Result<Vec<Value>> {
    ensure_not_cancelled(cancel, "fetch_rows_start")?;
    let mut last_err = None;
    for target in calibre_targets(config) {
        ensure_not_cancelled(cancel, "fetch_rows_target_loop")?;
        info!(target = %target.label, "Attempting calibre target");
        match run_calibredb_list(config, &target, cancel) {
            Ok(rows) => {
                info!(target = %target.label, row_count = rows.len(), "Calibre target responded");
                return Ok(rows);
            }
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
    cancel: Option<&CancellationToken>,
) -> Result<Vec<Value>> {
    ensure_not_cancelled(cancel, "before_calibredb_list")?;
    let mut cmd = Command::new(&config.calibredb_bin);
    cmd.arg("--timeout")
        .arg(CALIBRE_DB_TIMEOUT_SECS.to_string());
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
        .arg("id,title,authors,pubdate,formats,size");

    let output = cmd
        .output()
        .with_context(|| format!("failed to run calibredb list against {}", target.label))?;
    ensure_not_cancelled(cancel, "after_calibredb_list")?;
    if !output.status.success() {
        return Err(anyhow!(
            String::from_utf8_lossy(&output.stderr).trim().to_string()
        ));
    }
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "failed to parse calibredb JSON output from {}",
            target.label
        )
    })?;
    Ok(rows)
}

fn run_calibredb_export(
    config: &CalibreConfig,
    target: &CalibreTarget,
    book_id: u64,
    extension: &str,
    out_dir: &Path,
) -> Result<()> {
    let mut cmd = Command::new(&config.calibredb_bin);
    cmd.arg("--timeout")
        .arg((CALIBRE_DB_TIMEOUT_SECS * 4).to_string());
    cmd.arg("--with-library").arg(&target.with_library);
    if let Some(username) = &target.username {
        cmd.arg("--username").arg(username);
    }
    if let Some(password) = &target.password {
        cmd.arg("--password").arg(password);
    }
    cmd.arg("export")
        .arg("--single-dir")
        .arg("--dont-write-opf")
        .arg("--dont-save-cover")
        .arg("--dont-save-extra-files")
        .arg("--to-dir")
        .arg(out_dir)
        .arg("--formats")
        .arg(extension)
        .arg(book_id.to_string());

    let output = cmd.output().with_context(|| {
        format!(
            "failed to run calibredb export for id {book_id} against {}",
            target.label
        )
    })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            String::from_utf8_lossy(&output.stderr).trim().to_string()
        ))
    }
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

fn parse_u64_field(row: &Value, key: &str) -> Option<u64> {
    row.get(key).and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_i64().map(|n| n.max(0) as u64))
            .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
    })
}

fn parse_string_field(row: &Value, key: &str) -> Option<String> {
    row.get(key).and_then(|value| {
        value
            .as_str()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    })
}

fn parse_year_field(row: &Value, key: &str) -> Option<i32> {
    let raw = parse_string_field(row, key)?;
    let year = raw.chars().take(4).collect::<String>();
    if year.chars().all(|c| c.is_ascii_digit()) {
        year.parse::<i32>().ok()
    } else {
        None
    }
}

fn parse_authors(row: &Value) -> String {
    match row.get("authors") {
        Some(Value::Array(values)) => {
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

fn parse_formats(row: &Value) -> Vec<String> {
    let Some(value) = row.get("formats") else {
        return Vec::new();
    };
    match value {
        Value::Array(values) => values
            .iter()
            .filter_map(|v| v.as_str())
            .map(normalize_format_value)
            .filter(|s| !s.is_empty())
            .collect(),
        Value::String(raw) => raw
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

fn canonical_extension(raw: &str) -> String {
    let normalized = raw.trim().trim_start_matches('.').to_ascii_lowercase();
    if normalized == "markdown" {
        "md".to_string()
    } else {
        normalized
    }
}

fn find_exported_file(dir: &Path, extension: &str) -> Option<PathBuf> {
    let wanted = canonical_extension(extension);
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(canonical_extension)?;
        if ext == wanted {
            return Some(path);
        }
    }
    None
}

fn short_title_hash(title: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(title.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    hash.chars().take(8).collect()
}

fn resolve_book_file_path(
    library: Option<&Path>,
    id_dir_index: &HashMap<u64, PathBuf>,
    row: &Value,
    book_id: u64,
    extension: &str,
) -> Option<PathBuf> {
    let rel_dir = parse_string_field(row, "path");
    let base = match (library, rel_dir.as_deref()) {
        (Some(root), Some(rel)) => root.join(rel),
        (Some(_), None) => id_dir_index.get(&book_id)?.clone(),
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

fn index_library_book_dirs(root: &Path) -> HashMap<u64, PathBuf> {
    let mut out = HashMap::new();
    collect_book_dirs(root, 0, &mut out);
    out
}

fn collect_book_dirs(dir: &Path, depth: usize, out: &mut HashMap<u64, PathBuf>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_dir() {
            continue;
        }
        if let Some(book_id) = parse_book_id_from_dir_name(&path) {
            out.entry(book_id).or_insert(path.clone());
        }
        collect_book_dirs(&path, depth + 1, out);
    }
}

fn parse_book_id_from_dir_name(path: &Path) -> Option<u64> {
    let name = path.file_name()?.to_str()?;
    let start = name.rfind('(')?;
    if !name.ends_with(')') || start + 1 >= name.len() - 1 {
        return None;
    }
    name[start + 1..name.len() - 1].trim().parse::<u64>().ok()
}

fn ensure_not_cancelled(cancel: Option<&CancellationToken>, stage: &'static str) -> Result<()> {
    if let Some(token) = cancel {
        token.check_cancelled(stage)?;
    }
    Ok(())
}
