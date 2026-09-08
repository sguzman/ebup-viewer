use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;
use tracing::{info, warn};

use crate::cancellation::CancellationToken;

use super::{
    CalibreBook, CalibreConfig, CalibreProvider, THUMB_CACHED_PREFETCH_BUDGET,
    THUMB_PREFETCH_BUDGET, THUMB_PREFETCH_LIMIT, build_http_client,
    cache_store::{cache_signature, calibre_download_dir, try_load_cache, write_cache},
    library_id, server_base_url,
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
        let _ = write_cache(config, &signature, &cached);
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
        provider = ?config.provider,
        base_url = ?super::server_base_url(config),
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
                let _ = write_cache(config, &signature, &cached);
            }
            info!(
                book_count = cached.len(),
                elapsed_ms = started.elapsed().as_millis(),
                "Finished calibre catalog load from cache"
            );
            return Ok(cached);
        }
        info!("Calibre cache missing/incompatible; fetching from source via HTTP API");
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
                    let _ = write_cache(config, &signature, &cached);
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
    write_cache(config, &signature, &books)?;
    info!(book_count = books.len(), "Calibre cache file updated");
    info!(
        book_count = books.len(),
        elapsed_ms = started.elapsed().as_millis(),
        "Finished calibre catalog load"
    );
    Ok(books)
}

fn fetch_books(
    config: &CalibreConfig,
    cancel: Option<&CancellationToken>,
) -> Result<Vec<CalibreBook>> {
    match config.provider {
        CalibreProvider::Caliberate => super::caliberate::fetch_books(config, cancel),
        CalibreProvider::Calibre => fetch_legacy_books(config, cancel),
    }
}

fn fetch_legacy_books(
    config: &CalibreConfig,
    cancel: Option<&CancellationToken>,
) -> Result<Vec<CalibreBook>> {
    let base_url = server_base_url(config)
        .ok_or_else(|| anyhow!("library_url is missing or invalid in config"))?;
    let lib_id = library_id(config)
        .ok_or_else(|| anyhow!("library_id (the `#fragment` in library_url) is missing"))?;

    let client = build_http_client(config)?;
    let url = format!(
        "{base_url}/interface-data/books-init?library_id={lib_id}&num=5000&sort=timestamp.desc"
    );

    info!(%url, "Fetching book catalog via HTTP");
    let resp = client
        .get(&url)
        .send()
        .with_context(|| "failed to fetch books-init")?;

    if !resp.status().is_success() {
        return Err(anyhow!("HTTP {} fetching books-init", resp.status()));
    }

    let json: Value =
        serde_json::from_reader(resp).with_context(|| "failed to parse books-init JSON")?;

    let allowed_extensions = config.sanitized_extensions();
    let allowed_set: HashSet<String> = allowed_extensions.iter().cloned().collect();

    let mut books = Vec::new();

    // The books are in the `metadata` object, keyed by ID
    let metadata_obj = json
        .get("metadata")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow!("Missing or invalid 'metadata' object in response"))?;

    for (id_str, row) in metadata_obj {
        ensure_not_cancelled(cancel, "fetch_books_row_loop")?;

        let id = id_str.parse::<u64>().unwrap_or_default();
        if id == 0 {
            continue;
        }

        let title = row
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled")
            .to_string();

        let authors = match row.get("authors") {
            Some(Value::Array(arr)) => {
                let joined = arr
                    .iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                if joined.is_empty() {
                    "Unknown".to_string()
                } else {
                    joined
                }
            }
            Some(Value::String(s)) => s.to_string(),
            _ => "Unknown".to_string(),
        };

        let year = row.get("pubdate").and_then(|v| v.as_str()).and_then(|s| {
            if s.len() >= 4 {
                s[0..4].parse::<i32>().ok()
            } else {
                None
            }
        });

        let formats: Vec<String> = row
            .get("formats")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_lowercase())
                    .collect()
            })
            .unwrap_or_default();

        let selected_ext = allowed_extensions
            .iter()
            .find(|ext| formats.iter().any(|f| f == *ext))
            .cloned();

        let Some(selected_ext) = selected_ext else {
            continue;
        };

        if !allowed_set.contains(&selected_ext) {
            continue;
        }

        let uppercase_ext = selected_ext.to_uppercase();
        let size_from_json = row
            .get("format_sizes")
            .and_then(|sizes| sizes.get(&uppercase_ext))
            .and_then(|v| v.as_u64());
        let file_size_bytes = size_from_json.or_else(|| row.get("size").and_then(|v| v.as_u64()));

        books.push(CalibreBook {
            id,
            title,
            extension: selected_ext,
            authors,
            year,
            file_size_bytes,
            cover_thumbnail: None,
            path: None,
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

pub(super) fn materialize_book_path(config: &CalibreConfig, book: &CalibreBook) -> Result<PathBuf> {
    if matches!(config.provider, CalibreProvider::Caliberate) {
        return super::caliberate::materialize_book_path(config, book);
    }
    materialize_legacy_book_path(config, book)
}

fn materialize_legacy_book_path(config: &CalibreConfig, book: &CalibreBook) -> Result<PathBuf> {
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

    let base_url = server_base_url(config)
        .ok_or_else(|| anyhow!("library_url is missing or invalid in config"))?;
    let lib_id = library_id(config).ok_or_else(|| anyhow!("library_id is missing"))?;

    let client = build_http_client(config)?;
    let dl_url = format!("{base_url}/get/{}/{}/{lib_id}", ext.to_uppercase(), book.id);

    info!(%dl_url, "Downloading book file via HTTP");
    let mut resp = client
        .get(&dl_url)
        .send()
        .with_context(|| "failed to download book")?;

    if !resp.status().is_success() {
        return Err(anyhow!("HTTP {} downloading book", resp.status()));
    }

    let tmp_path = cache_root.join(format!("tmp-{}-{}", book.id, std::process::id()));
    let mut file = fs::File::create(&tmp_path).with_context(|| "failed to create temp file")?;

    resp.copy_to(&mut file)
        .with_context(|| "failed to write downloaded book data")?;
    file.flush()?;

    fs::rename(&tmp_path, &target_path)
        .with_context(|| "failed to rename temp file to final target")?;

    Ok(target_path)
}

fn canonical_extension(raw: &str) -> String {
    let normalized = raw.trim().trim_start_matches('.').to_ascii_lowercase();
    if normalized == "markdown" {
        "md".to_string()
    } else {
        normalized
    }
}

fn short_title_hash(title: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(title.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    hash.chars().take(8).collect()
}

fn ensure_not_cancelled(cancel: Option<&CancellationToken>, stage: &'static str) -> Result<()> {
    if let Some(token) = cancel {
        token.check_cancelled(stage)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calibre::cache_store;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn stale_fallback_never_crosses_provider_boundary() {
        let legacy = CalibreConfig {
            enabled: true,
            provider: CalibreProvider::Calibre,
            library_url: Some("http://127.0.0.1:9/#main".to_string()),
            ..CalibreConfig::default()
        };
        let stale_book = CalibreBook {
            id: 7,
            title: "Legacy only".to_string(),
            extension: "txt".to_string(),
            authors: "Author".to_string(),
            year: Some(2020),
            file_size_bytes: Some(4),
            cover_thumbnail: None,
            path: None,
        };
        let legacy_signature = cache_signature(&legacy);
        write_cache(
            &legacy,
            &legacy_signature,
            std::slice::from_ref(&stale_book),
        )
        .unwrap();

        let caliberate = CalibreConfig {
            enabled: true,
            provider: CalibreProvider::Caliberate,
            library_url: Some("http://127.0.0.1:9".to_string()),
            ..CalibreConfig::default()
        };
        let error = load_books_with_cancel(&caliberate, true, None).unwrap_err();
        assert!(error.to_string().contains("Caliberate"));

        let caliberate_book = CalibreBook {
            title: "Caliberate stale".to_string(),
            ..stale_book
        };
        let caliberate_signature = cache_signature(&caliberate);
        write_cache(
            &caliberate,
            &caliberate_signature,
            std::slice::from_ref(&caliberate_book),
        )
        .unwrap();
        let fallback = load_books_with_cancel(&caliberate, true, None).unwrap();
        assert_eq!(fallback.len(), 1);
        assert_eq!(fallback[0].title, caliberate_book.title);
        assert_eq!(fallback[0].id, caliberate_book.id);
        let _ = fs::remove_file(cache_store::calibre_cache_path_for(&legacy));
        let _ = fs::remove_file(cache_store::calibre_cache_path_for(&caliberate));
    }

    #[test]
    fn legacy_http_catalog_materialization_and_basic_auth_remain_functional() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept legacy request");
                let mut bytes = Vec::new();
                let mut buffer = [0_u8; 4096];
                loop {
                    let count = stream.read(&mut buffer).expect("read legacy request");
                    if count == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buffer[..count]);
                    if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                let request = String::from_utf8(bytes).expect("legacy request is HTTP text");
                let body = if request.starts_with("GET /interface-data/books-init?") {
                    br#"{"metadata":{"7000001":{"title":"Legacy Book","authors":["Legacy Author"],"pubdate":"2021-05-01","formats":["EPUB"],"format_sizes":{"EPUB":17}}}}"#.to_vec()
                } else {
                    assert!(request.starts_with("GET /get/EPUB/7000001/main"));
                    b"legacy-book-bytes".to_vec()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .and_then(|_| stream.write_all(&body))
                    .expect("write legacy response");
                requests.push(request);
            }
            requests
        });

        let config = CalibreConfig {
            enabled: true,
            provider: CalibreProvider::Calibre,
            library_url: Some(format!("{base_url}/#main")),
            server_username: Some("legacy-user".to_string()),
            server_password: Some("legacy-pass".to_string()),
            ..CalibreConfig::default()
        };
        let books = fetch_books(&config, None).expect("legacy catalog should load");
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].title, "Legacy Book");
        assert_eq!(books[0].file_size_bytes, Some(17));

        let expected_path = calibre_download_dir().join(format!(
            "{}-{}.{}",
            books[0].id,
            short_title_hash(&books[0].title),
            books[0].extension
        ));
        let _ = fs::remove_file(&expected_path);
        let path = materialize_book_path(&config, &books[0]).expect("legacy book should download");
        assert_eq!(fs::read(&path).unwrap(), b"legacy-book-bytes");
        let requests = server.join().expect("legacy server should finish");
        assert_eq!(requests.len(), 2);
        let expected_auth = "authorization: basic bGVnYWN5LXVzZXI6bGVnYWN5LXBhc3M=";
        for request in requests {
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains(&expected_auth.to_ascii_lowercase()),
                "request was: {request}"
            );
        }
        let _ = fs::remove_file(path);
    }
}
