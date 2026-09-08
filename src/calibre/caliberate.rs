use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

use crate::cancellation::CancellationToken;

use super::{CalibreBook, CalibreConfig, build_http_client, cache_store::calibre_download_dir};

const PAGE_LIMIT: usize = 500;

#[derive(Debug, Deserialize)]
struct BooksPage {
    items: Vec<BookRow>,
    total: usize,
    offset: usize,
    limit: usize,
}

#[derive(Debug, Deserialize)]
struct BookRow {
    id: u64,
    title: String,
    #[serde(default)]
    authors: Vec<String>,
    #[serde(default)]
    pubdate: Option<String>,
    #[serde(default)]
    primary_format: Option<String>,
    #[serde(default)]
    formats: Vec<FormatRow>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum FormatRow {
    Detailed {
        #[serde(alias = "extension")]
        format: String,
        #[serde(default)]
        size_bytes: Option<u64>,
    },
    Name(String),
}

pub(super) fn fetch_books(
    config: &CalibreConfig,
    cancel: Option<&CancellationToken>,
) -> Result<Vec<CalibreBook>> {
    let base_url = super::server_base_url(config)
        .ok_or_else(|| anyhow!("Caliberate library_url is missing or invalid"))?;
    let client = build_http_client(config)?;
    let allowed = config.sanitized_extensions();
    let mut books = Vec::new();
    let mut offset = 0usize;

    loop {
        ensure_not_cancelled(cancel, "caliberate_before_page")?;
        let url = format!(
            "{base_url}/api/v1/books?offset={offset}&limit={PAGE_LIMIT}&sort=title&direction=asc"
        );
        info!(
            provider = "caliberate",
            offset,
            limit = PAGE_LIMIT,
            "Fetching Caliberate catalog page"
        );
        let response = client.get(&url).send().with_context(|| {
            format!("failed to fetch Caliberate catalog page at offset {offset}")
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(anyhow!(
                "Caliberate catalog returned HTTP {status} at offset {offset}"
            ));
        }
        let body = response.text().with_context(|| {
            format!("failed to read Caliberate catalog page at offset {offset}")
        })?;
        let page: BooksPage = serde_json::from_str(&body).with_context(|| {
            format!("failed to parse Caliberate catalog page at offset {offset}")
        })?;
        if page.offset != offset {
            return Err(anyhow!(
                "Caliberate catalog returned offset {} for requested offset {offset}",
                page.offset
            ));
        }
        let page_count = page.items.len();
        for row in page.items {
            ensure_not_cancelled(cancel, "caliberate_row_mapping")?;
            if let Some(book) = map_book(row, &allowed) {
                books.push(book);
            }
        }
        info!(
            provider = "caliberate",
            offset,
            page_count,
            total = page.total,
            "Mapped Caliberate catalog page"
        );
        if page_count == 0 || offset.saturating_add(page_count) >= page.total || page.limit == 0 {
            break;
        }
        offset = offset.saturating_add(page_count);
    }

    books.sort_by(|a, b| {
        a.title
            .to_ascii_lowercase()
            .cmp(&b.title.to_ascii_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    info!(
        provider = "caliberate",
        book_count = books.len(),
        "Finished Caliberate catalog mapping"
    );
    Ok(books)
}

pub(super) fn materialize_book_path(config: &CalibreConfig, book: &CalibreBook) -> Result<PathBuf> {
    let cache_root = calibre_download_dir().join("caliberate");
    fs::create_dir_all(&cache_root)
        .with_context(|| format!("failed to create {}", cache_root.display()))?;
    download_book(config, book, &cache_root)
}

fn download_book(config: &CalibreConfig, book: &CalibreBook, cache_root: &Path) -> Result<PathBuf> {
    let ext = canonical_extension(&book.extension);
    let target_path = cache_root.join(format!(
        "{}-{}.{}",
        book.id,
        short_title_hash(&book.title),
        ext
    ));
    if target_path.exists() {
        return Ok(target_path);
    }
    let base_url = super::server_base_url(config)
        .ok_or_else(|| anyhow!("Caliberate library_url is missing or invalid"))?;
    let url = format!("{base_url}/api/v1/books/{}/content/{ext}", book.id);
    let mut response = build_http_client(config)?
        .get(&url)
        .send()
        .with_context(|| format!("failed to materialize Caliberate book {}", book.id))?;
    let status = response.status();
    if !status.is_success() {
        return Err(anyhow!(
            "Caliberate content returned HTTP {status} for book {}",
            book.id
        ));
    }
    let temp_path = cache_root.join(format!(
        ".tmp-{}-{}-{}",
        book.id,
        std::process::id(),
        now_nanos()
    ));
    let mut file = fs::File::create(&temp_path).with_context(|| {
        format!(
            "failed to create partial Caliberate download {}",
            temp_path.display()
        )
    })?;
    response
        .copy_to(&mut file)
        .with_context(|| format!("failed to write Caliberate book {}", book.id))?;
    file.flush()?;
    drop(file);
    fs::rename(&temp_path, &target_path).with_context(|| {
        format!(
            "failed to atomically materialize Caliberate book to {}",
            target_path.display()
        )
    })?;
    info!(provider = "caliberate", book_id = book.id, path = %target_path.display(), "Materialized Caliberate book");
    Ok(target_path)
}

fn map_book(row: BookRow, allowed: &[String]) -> Option<CalibreBook> {
    let selected = select_format(&row, allowed)?;
    let authors = if row.authors.is_empty() {
        "Unknown".to_string()
    } else {
        row.authors.join(", ")
    };
    Some(CalibreBook {
        id: row.id,
        title: row.title,
        extension: selected.0,
        authors,
        year: row.pubdate.as_deref().and_then(parse_year),
        file_size_bytes: selected.1,
        path: None,
        cover_thumbnail: None,
    })
}

fn select_format(row: &BookRow, allowed: &[String]) -> Option<(String, Option<u64>)> {
    let formats: Vec<(String, Option<u64>)> = row
        .formats
        .iter()
        .map(|format| match format {
            FormatRow::Detailed { format, size_bytes } => (format.clone(), *size_bytes),
            FormatRow::Name(format) => (format.clone(), None),
        })
        .collect();
    for preferred in allowed {
        if let Some((_format, size)) = formats
            .iter()
            .find(|(format, _)| canonical_extension(format) == *preferred)
        {
            return Some((preferred.clone(), *size));
        }
    }
    row.primary_format
        .as_deref()
        .map(canonical_extension)
        .filter(|format| allowed.iter().any(|allowed| allowed == format))
        .map(|format| {
            let size = formats
                .iter()
                .find(|(candidate, _)| canonical_extension(candidate) == format)
                .and_then(|(_, size)| *size);
            (format, size)
        })
}

fn parse_year(value: &str) -> Option<i32> {
    value.get(..4)?.parse().ok()
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
    format!("{:x}", hasher.finalize()).chars().take(8).collect()
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0)
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
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    #[test]
    fn selects_allowed_priority_and_primary_fallback() {
        let row: BookRow = serde_json::from_str(
            r#"{"id":7,"title":"A Book","authors":["Ada","Lin"],"pubdate":"2024-05-01","formats":[{"format":"pdf","size_bytes":10},{"format":"EPUB","size_bytes":20}]}"#,
        ).unwrap();
        let book = map_book(row, &["epub".to_string(), "pdf".to_string()]).unwrap();
        assert_eq!(book.extension, "epub");
        assert_eq!(book.authors, "Ada, Lin");
        assert_eq!(book.year, Some(2024));
        assert_eq!(book.file_size_bytes, Some(20));

        let fallback: BookRow = serde_json::from_str(
            r#"{"id":8,"title":"Fallback","primary_format":"txt","formats":[]}"#,
        )
        .unwrap();
        assert_eq!(
            map_book(fallback, &["txt".to_string()]).unwrap().extension,
            "txt"
        );
    }

    fn response(stream: &mut TcpStream, body: &[u8], status: &str) {
        let header = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\nContent-Type: application/json\r\n\r\n",
            body.len()
        );
        stream.write_all(header.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
    }

    #[test]
    fn fetches_all_pages_with_api_key_and_materializes_versioned_content() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let page_one = br#"{"items":[{"id":7,"title":"Alpha","authors":["Ada"],"pubdate":"2024-01-01","formats":[{"format":"EPUB","size_bytes":12}]}],"total":2,"offset":0,"limit":500}"#;
        let page_two = br#"{"items":[{"id":8,"title":"Beta","authors":[],"primary_format":"txt","formats":[]}],"total":2,"offset":1,"limit":500}"#;
        let content = b"representative book bytes";
        let server = thread::spawn(move || {
            for (index, expected) in [
                "/api/v1/books?offset=0",
                "/api/v1/books?offset=1",
                "/api/v1/books/7/content/epub",
            ]
            .into_iter()
            .enumerate()
            {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut buffer = [0u8; 4096];
                loop {
                    let read = stream.read(&mut buffer).unwrap();
                    request.extend_from_slice(&buffer[..read]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                let request_text = String::from_utf8_lossy(&request);
                assert!(
                    request_text.contains(expected),
                    "request did not contain {expected}: {request_text}"
                );
                assert!(request_text.contains("x-api-key: secret-token"));
                match index {
                    0 => response(&mut stream, page_one, "200 OK"),
                    1 => response(&mut stream, page_two, "200 OK"),
                    _ => {
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            content.len()
                        );
                        stream.write_all(header.as_bytes()).unwrap();
                        stream.write_all(content).unwrap();
                    }
                }
            }
        });

        let mut config = CalibreConfig::default();
        config.provider = super::super::CalibreProvider::Caliberate;
        config.library_url = Some(format!("http://{}", address));
        config.api_key = Some("secret-token".to_string());
        config.allowed_extensions = vec!["epub".to_string(), "txt".to_string()];
        let books = fetch_books(&config, None).unwrap();
        assert_eq!(books.len(), 2);
        assert_eq!(books[0].title, "Alpha");
        assert_eq!(books[1].extension, "txt");

        let temp =
            std::env::temp_dir().join(format!("lanternleaf-caliberate-download-{}", now_nanos()));
        fs::create_dir_all(&temp).unwrap();
        let path = download_book(&config, &books[0], &temp).unwrap();
        assert_eq!(fs::read(&path).unwrap(), content);
        assert!(!temp.join(".tmp-7").exists());
        let _ = fs::remove_dir_all(&temp);
        server.join().unwrap();
    }

    #[test]
    fn unreachable_service_returns_actionable_error() {
        let mut config = CalibreConfig::default();
        config.library_url = Some("http://127.0.0.1:1".to_string());
        let error = fetch_books(&config, None).expect_err("unreachable service must fail");
        assert!(error.to_string().contains("Caliberate"));
    }
}
