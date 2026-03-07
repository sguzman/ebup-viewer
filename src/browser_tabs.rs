use anyhow::{Context, Result, anyhow};
use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{info, warn};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BrowsrHealth {
    pub ok: bool,
    pub extension_connected: bool,
    pub now: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct BrowserWindow {
    pub id: u64,
    pub focused: bool,
    pub height: Option<i32>,
    pub incognito: Option<bool>,
    pub left: Option<i32>,
    pub state: Option<String>,
    pub top: Option<i32>,
    pub r#type: Option<String>,
    pub width: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTab {
    pub id: u64,
    pub window_id: u64,
    pub index: Option<i32>,
    pub active: Option<bool>,
    pub audible: Option<bool>,
    pub pinned: Option<bool>,
    pub status: Option<String>,
    pub title: String,
    pub url: String,
    pub fav_icon_url: Option<String>,
    pub last_accessed: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotTruncationEntry {
    pub bytes: Option<u64>,
    pub max_bytes: Option<u64>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, TS)]
#[ts(export)]
pub struct SnapshotTruncation {
    pub html: SnapshotTruncationEntry,
    pub text: SnapshotTruncationEntry,
    pub selection: SnapshotTruncationEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTabSnapshot {
    pub tab_id: u64,
    pub title: String,
    pub url: String,
    pub lang: Option<String>,
    pub ready_state: Option<String>,
    pub captured_at: Option<String>,
    pub html: Option<String>,
    pub text: Option<String>,
    pub selection: Option<String>,
    #[serde(default)]
    pub truncation: SnapshotTruncation,
}

#[derive(Debug, Clone)]
pub struct BrowsrClient {
    client: reqwest::Client,
    base_url: String,
}

impl BrowsrClient {
    pub fn new(base_url: &str, timeout_ms: u64) -> Result<Self> {
        let normalized = base_url.trim().trim_end_matches('/').to_string();
        if normalized.is_empty() {
            return Err(anyhow!("browsr base URL is empty"));
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms.max(250)))
            .build()
            .context("failed to build browsr reqwest client")?;
        Ok(Self {
            client,
            base_url: normalized,
        })
    }

    pub async fn health(&self) -> Result<BrowsrHealth> {
        let started = std::time::Instant::now();
        let response = self
            .client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .context("failed to request browsr health")?;
        let health = parse_json_response::<BrowsrHealth>(response).await?;
        info!(
            base_url = %self.base_url,
            extension_connected = health.extension_connected,
            elapsed_ms = started.elapsed().as_millis(),
            "Browsr health check completed"
        );
        Ok(health)
    }

    pub async fn list_windows(&self) -> Result<Vec<BrowserWindow>> {
        #[derive(Deserialize)]
        struct Response {
            windows: Vec<BrowserWindow>,
        }
        let started = std::time::Instant::now();
        let response = self
            .client
            .get(format!("{}/v1/windows", self.base_url))
            .send()
            .await
            .context("failed to request browsr windows")?;
        let payload = parse_json_response::<Response>(response).await?;
        info!(
            base_url = %self.base_url,
            count = payload.windows.len(),
            elapsed_ms = started.elapsed().as_millis(),
            "Browsr windows fetch completed"
        );
        Ok(payload.windows)
    }

    pub async fn list_tabs(
        &self,
        window_id: Option<u64>,
        query: Option<&str>,
        refresh: bool,
    ) -> Result<Vec<BrowserTab>> {
        #[derive(Deserialize)]
        struct Response {
            tabs: Vec<BrowserTab>,
        }
        let started = std::time::Instant::now();
        let mut request = self.client.get(format!("{}/v1/tabs", self.base_url));
        if let Some(window_id) = window_id {
            request = request.query(&[("window_id", window_id.to_string())]);
        }
        if let Some(query) = query.filter(|value| !value.trim().is_empty()) {
            request = request.query(&[("q", query.trim())]);
        }
        if refresh {
            request = request.query(&[("refresh", "true")]);
        }
        let response = request
            .send()
            .await
            .context("failed to request browsr tabs")?;
        let payload = parse_json_response::<Response>(response).await?;
        info!(
            base_url = %self.base_url,
            count = payload.tabs.len(),
            window_id,
            refresh,
            elapsed_ms = started.elapsed().as_millis(),
            "Browsr tabs fetch completed"
        );
        Ok(payload.tabs)
    }

    pub async fn snapshot_tab(&self, tab_id: u64) -> Result<BrowserTabSnapshot> {
        let started = std::time::Instant::now();
        let response = self
            .client
            .post(format!("{}/v1/tabs/{tab_id}/snapshot", self.base_url))
            .header(CONTENT_TYPE, "application/json")
            .body(
                serde_json::json!({
                    "include_html": true,
                    "include_text": true,
                    "include_selection": true
                })
                .to_string(),
            )
            .send()
            .await
            .with_context(|| format!("failed to request browsr snapshot for tab {tab_id}"))?;
        let snapshot = parse_json_response::<BrowserTabSnapshot>(response).await?;
        info!(
            base_url = %self.base_url,
            tab_id,
            html_chars = snapshot.html.as_ref().map(|value| value.len()).unwrap_or(0),
            text_chars = snapshot.text.as_ref().map(|value| value.len()).unwrap_or(0),
            html_truncated = snapshot.truncation.html.truncated,
            text_truncated = snapshot.truncation.text.truncated,
            elapsed_ms = started.elapsed().as_millis(),
            "Browsr snapshot completed"
        );
        Ok(snapshot)
    }
}

async fn parse_json_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
) -> Result<T> {
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read browsr response body")?;
    if !status.is_success() {
        let message = extract_error_message(&body).unwrap_or_else(|| body.trim().to_string());
        warn!(status = %status, message = %message, "Browsr request failed");
        return Err(anyhow!(message));
    }
    serde_json::from_str::<T>(&body).with_context(|| {
        format!(
            "failed to parse browsr response JSON (status {}): {}",
            status,
            truncate_body(&body)
        )
    })
}

fn extract_error_message(body: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct ErrorEnvelope {
        error: Option<ErrorPayload>,
    }
    #[derive(Deserialize)]
    struct ErrorPayload {
        code: Option<String>,
        message: Option<String>,
    }
    let parsed = serde_json::from_str::<ErrorEnvelope>(body).ok()?;
    let payload = parsed.error?;
    let code = payload.code.unwrap_or_else(|| "browsr_error".to_string());
    let message = payload
        .message
        .unwrap_or_else(|| "unknown browsr error".to_string());
    Some(format!("{code}: {message}"))
}

fn truncate_body(body: &str) -> String {
    const MAX_CHARS: usize = 280;
    if body.chars().count() <= MAX_CHARS {
        return body.to_string();
    }
    let mut out = body
        .chars()
        .take(MAX_CHARS.saturating_sub(3))
        .collect::<String>();
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    fn spawn_single_response_server(
        status_line: &str,
        response_body: &'static str,
    ) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let addr = listener.local_addr().expect("local addr");
        let (tx, rx) = mpsc::channel();
        let status_line = status_line.to_string();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buf = [0_u8; 8192];
            let read = stream.read(&mut buf).expect("read request");
            let request = String::from_utf8_lossy(&buf[..read]).to_string();
            tx.send(request).expect("send request");
            let response = format!(
                "{status_line}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{response_body}",
                response_body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            stream.flush().expect("flush response");
        });
        (format!("http://{}", addr), rx)
    }

    #[test]
    fn extract_error_message_uses_structured_payload() {
        let body =
            r#"{"error":{"code":"extension_disconnected","message":"extension not connected"}}"#;
        assert_eq!(
            extract_error_message(body).as_deref(),
            Some("extension_disconnected: extension not connected")
        );
    }

    #[test]
    fn truncate_body_limits_large_payloads() {
        let body = "x".repeat(500);
        let truncated = truncate_body(&body);
        assert!(truncated.len() < body.len());
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn snapshot_tab_posts_json_body_and_parses_response() {
        let (base_url, request_rx) = spawn_single_response_server(
            "HTTP/1.1 200 OK",
            r#"{"tabId":42,"title":"Example","url":"https://example.com/article","lang":"en","readyState":"complete","capturedAt":"2026-03-06T20:00:00Z","html":"<article><p>Hello</p></article>","text":"Hello","selection":null,"truncation":{"html":{"bytes":32,"maxBytes":1048576,"truncated":false},"text":{"bytes":5,"maxBytes":1048576,"truncated":false},"selection":{"bytes":0,"maxBytes":1048576,"truncated":false}}}"#,
        );
        let client = BrowsrClient::new(&base_url, 2_000).expect("client");
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let snapshot = runtime.block_on(client.snapshot_tab(42)).expect("snapshot");
        let request = request_rx.recv().expect("captured request");

        assert!(request.starts_with("POST /v1/tabs/42/snapshot HTTP/1.1"));
        assert!(request.contains("content-type: application/json"));
        assert!(request.contains(r#""include_html":true"#));
        assert!(request.contains(r#""include_text":true"#));
        assert_eq!(snapshot.tab_id, 42);
        assert_eq!(snapshot.title, "Example");
        assert_eq!(snapshot.text.as_deref(), Some("Hello"));
    }

    #[test]
    fn health_surfaces_structured_error_messages() {
        let (base_url, _request_rx) = spawn_single_response_server(
            "HTTP/1.1 503 Service Unavailable",
            r#"{"error":{"code":"browsr_unavailable","message":"server offline"}}"#,
        );
        let client = BrowsrClient::new(&base_url, 2_000).expect("client");
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let err = runtime
            .block_on(client.health())
            .expect_err("health must fail");
        let message = format!("{err:#}");
        assert!(message.contains("browsr_unavailable: server offline"));
    }
}
