use anyhow::{Context, Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use tracing::{debug, info, warn};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowsrImportBundleJob {
    pub job_id: String,
    pub tab_id: u64,
    pub status: String,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub error: Option<BrowsrImportBundleError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowsrImportBundleError {
    pub code: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowsrImportBundleManifestResponse {
    pub job_id: String,
    pub tab_id: u64,
    pub status: String,
    pub bundle: BrowsrImportBundleManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowsrImportBundleWaitResult {
    pub job: BrowsrImportBundleJob,
    #[serde(default)]
    pub manifest: Option<BrowsrImportBundleManifestResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowsrImportBundleWaitResponse {
    pub started: BrowsrImportBundleJob,
    pub result: BrowsrImportBundleWaitResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowsrImportBundleManifest {
    pub tab: BrowsrImportBundleTab,
    pub document: Option<BrowsrImportBundleDocument>,
    #[serde(default)]
    pub capture: Option<Value>,
    #[serde(default)]
    pub screenshot: Option<Value>,
    #[serde(default)]
    pub assets: Vec<BrowsrImportBundleAssetRef>,
    #[serde(default)]
    pub export: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowsrImportBundleTab {
    pub id: u64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowsrImportBundleDocument {
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub html: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub selection: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowsrImportBundleAssetRef {
    pub asset_id: String,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub document_url: Option<String>,
    #[serde(default)]
    pub resource_type: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub status: Option<u16>,
    #[serde(default)]
    pub asset_ordinal: Option<u32>,
    #[serde(default)]
    pub served_from_cache: Option<bool>,
    #[serde(default)]
    pub from_disk_cache: Option<bool>,
    #[serde(default)]
    pub from_service_worker: Option<bool>,
    #[serde(default)]
    pub base64_encoded: Option<bool>,
    #[serde(default)]
    pub bytes: Option<usize>,
    #[serde(default)]
    pub headers: Option<Value>,
    #[serde(default)]
    pub body_available: bool,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BrowserTabBundleAssetPayload {
    pub asset_id: String,
    pub url: String,
    pub mime_type: Option<String>,
    pub resource_type: Option<String>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct BrowserTabBundleCapture {
    pub tab_id: u64,
    pub title: String,
    pub url: String,
    pub captured_at: Option<String>,
    pub html: String,
    pub text: Option<String>,
    pub selection: Option<String>,
    pub assets: Vec<BrowserTabBundleAssetPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowsrImportBundleAssetResponse {
    pub job_id: String,
    #[serde(default)]
    pub asset_id: Option<String>,
    #[serde(default)]
    pub asset: Option<BrowsrImportBundleAssetRef>,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub base64_encoded: bool,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct BrowsrClient {
    client: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Clone)]
pub struct BrowsrBlockingClient {
    client: reqwest::blocking::Client,
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

    pub async fn close_tab(&self, tab_id: u64) -> Result<()> {
        let started = std::time::Instant::now();
        let response = self
            .client
            .post(format!("{}/v1/tabs/{tab_id}/close", self.base_url))
            .send()
            .await
            .with_context(|| format!("failed to request browsr close for tab {tab_id}"))?;
        let _: Value = parse_json_response(response).await?;
        info!(
            base_url = %self.base_url,
            tab_id,
            elapsed_ms = started.elapsed().as_millis(),
            "Browsr close-tab completed"
        );
        Ok(())
    }

    pub async fn start_import_bundle_and_wait(
        &self,
        tab_id: u64,
    ) -> Result<BrowsrImportBundleWaitResponse> {
        const BUNDLE_SETTLE_TIMEOUT_MS: u64 = 90_000;
        const BUNDLE_WAIT_TIMEOUT_MS: u64 = 180_000;
        let started = std::time::Instant::now();
        let response = self
            .client
            .post(format!(
                "{}/v1/tabs/{tab_id}/import-bundles/wait",
                self.base_url
            ))
            .timeout(Duration::from_millis(BUNDLE_WAIT_TIMEOUT_MS + 10_000))
            .header(CONTENT_TYPE, "application/json")
            .body(
                serde_json::json!({
                    "reload": true,
                    "capture_html": true,
                    "capture_assets": true,
                    "capture_text": true,
                    "capture_selection": true,
                    "capture_screenshot": false,
                    "wait_for_network_idle_ms": 1500,
                    "settle_timeout_ms": BUNDLE_SETTLE_TIMEOUT_MS,
                    "max_asset_bytes": 5_000_000,
                    "max_total_bytes": 75_000_000,
                    "wait_timeout_ms": BUNDLE_WAIT_TIMEOUT_MS,
                    "poll_interval_ms": 500,
                    "include_manifest": true
                })
                .to_string(),
            )
            .send()
            .await
            .with_context(|| {
                format!("failed to start/wait browsr import bundle for tab {tab_id}")
            })?;
        let waited = parse_json_response::<BrowsrImportBundleWaitResponse>(response).await?;
        info!(
            base_url = %self.base_url,
            tab_id,
            job_id = %waited.result.job.job_id,
            status = %waited.result.job.status,
            elapsed_ms = started.elapsed().as_millis(),
            "Browsr import bundle start/wait completed"
        );
        Ok(waited)
    }

    pub async fn get_import_bundle_manifest(
        &self,
        job_id: &str,
    ) -> Result<BrowsrImportBundleManifestResponse> {
        let started = std::time::Instant::now();
        let response = self
            .client
            .get(format!(
                "{}/v1/import-bundles/{job_id}/manifest",
                self.base_url
            ))
            .send()
            .await
            .with_context(|| format!("failed to get browsr import bundle manifest for {job_id}"))?;
        let manifest = parse_json_response::<BrowsrImportBundleManifestResponse>(response).await?;
        info!(
            base_url = %self.base_url,
            job_id,
            asset_count = manifest.bundle.assets.len(),
            elapsed_ms = started.elapsed().as_millis(),
            "Browsr import bundle manifest fetched"
        );
        Ok(manifest)
    }

    pub async fn get_import_bundle_asset(
        &self,
        job_id: &str,
        asset_id: &str,
    ) -> Result<BrowserTabBundleAssetPayload> {
        let started = std::time::Instant::now();
        let response = self
            .client
            .get(format!(
                "{}/v1/import-bundles/{job_id}/assets/{asset_id}",
                self.base_url
            ))
            .send()
            .await
            .with_context(|| {
                format!("failed to get browsr import bundle asset {asset_id} for {job_id}")
            })?;
        let payload = parse_json_response::<BrowsrImportBundleAssetResponse>(response).await?;
        let asset_ref = payload.asset.clone();
        let url = asset_ref
            .as_ref()
            .map(|value| value.url.clone())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_default();
        let mime_type = payload
            .content_type
            .clone()
            .or_else(|| asset_ref.as_ref().and_then(|value| value.mime_type.clone()));
        let resource_type = asset_ref
            .as_ref()
            .and_then(|value| value.resource_type.clone());
        let body = decode_bundle_asset_body(&payload)?;
        debug!(
            base_url = %self.base_url,
            job_id,
            asset_id,
            bytes = body.len(),
            content_type = ?mime_type,
            elapsed_ms = started.elapsed().as_millis(),
            "Browsr import bundle asset fetched"
        );
        Ok(BrowserTabBundleAssetPayload {
            asset_id: asset_id.to_string(),
            url,
            mime_type,
            resource_type,
            body,
        })
    }
}

impl BrowsrBlockingClient {
    pub fn new(base_url: &str, timeout_ms: u64) -> Result<Self> {
        let normalized = base_url.trim().trim_end_matches('/').to_string();
        if normalized.is_empty() {
            return Err(anyhow!("browsr base URL is empty"));
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(timeout_ms.max(250)))
            .build()
            .context("failed to build browsr blocking reqwest client")?;
        Ok(Self {
            client,
            base_url: normalized,
        })
    }

    pub fn health(&self) -> Result<BrowsrHealth> {
        let started = std::time::Instant::now();
        let response = self
            .client
            .get(format!("{}/health", self.base_url))
            .send()
            .context("failed to request browsr health")?;
        let health = parse_json_response_blocking::<BrowsrHealth>(response)?;
        info!(
            base_url = %self.base_url,
            extension_connected = health.extension_connected,
            elapsed_ms = started.elapsed().as_millis(),
            "Browsr health check completed"
        );
        Ok(health)
    }

    pub fn list_windows(&self) -> Result<Vec<BrowserWindow>> {
        #[derive(Deserialize)]
        struct Response {
            windows: Vec<BrowserWindow>,
        }
        let started = std::time::Instant::now();
        let response = self
            .client
            .get(format!("{}/v1/windows", self.base_url))
            .send()
            .context("failed to request browsr windows")?;
        let payload = parse_json_response_blocking::<Response>(response)?;
        info!(
            base_url = %self.base_url,
            count = payload.windows.len(),
            elapsed_ms = started.elapsed().as_millis(),
            "Browsr windows fetch completed"
        );
        Ok(payload.windows)
    }

    pub fn list_tabs(
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
            .context("failed to request browsr tabs")?;
        let payload = parse_json_response_blocking::<Response>(response)?;
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

    pub fn snapshot_tab(&self, tab_id: u64) -> Result<BrowserTabSnapshot> {
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
            .with_context(|| format!("failed to request browsr snapshot for tab {tab_id}"))?;
        let snapshot = parse_json_response_blocking::<BrowserTabSnapshot>(response)?;
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

    pub fn close_tab(&self, tab_id: u64) -> Result<()> {
        let started = std::time::Instant::now();
        let response = self
            .client
            .post(format!("{}/v1/tabs/{tab_id}/close", self.base_url))
            .send()
            .with_context(|| format!("failed to request browsr close for tab {tab_id}"))?;
        let _: Value = parse_json_response_blocking(response)?;
        info!(
            base_url = %self.base_url,
            tab_id,
            elapsed_ms = started.elapsed().as_millis(),
            "Browsr close-tab completed"
        );
        Ok(())
    }

    pub fn start_import_bundle_and_wait(&self, tab_id: u64) -> Result<BrowsrImportBundleWaitResponse> {
        const BUNDLE_SETTLE_TIMEOUT_MS: u64 = 90_000;
        const BUNDLE_WAIT_TIMEOUT_MS: u64 = 180_000;
        let started = std::time::Instant::now();
        let response = self
            .client
            .post(format!(
                "{}/v1/tabs/{tab_id}/import-bundles/wait",
                self.base_url
            ))
            .timeout(Duration::from_millis(BUNDLE_WAIT_TIMEOUT_MS + 10_000))
            .header(CONTENT_TYPE, "application/json")
            .body(
                serde_json::json!({
                    "reload": true,
                    "capture_html": true,
                    "capture_assets": true,
                    "capture_text": true,
                    "capture_selection": true,
                    "capture_screenshot": false,
                    "wait_for_network_idle_ms": 1500,
                    "settle_timeout_ms": BUNDLE_SETTLE_TIMEOUT_MS,
                    "max_asset_bytes": 5_000_000,
                    "max_total_bytes": 75_000_000,
                    "wait_timeout_ms": BUNDLE_WAIT_TIMEOUT_MS,
                    "poll_interval_ms": 500,
                    "include_manifest": true
                })
                .to_string(),
            )
            .send()
            .with_context(|| {
                format!("failed to start/wait browsr import bundle for tab {tab_id}")
            })?;
        let waited = parse_json_response_blocking::<BrowsrImportBundleWaitResponse>(response)?;
        info!(
            base_url = %self.base_url,
            tab_id,
            job_id = %waited.result.job.job_id,
            status = %waited.result.job.status,
            elapsed_ms = started.elapsed().as_millis(),
            "Browsr import bundle start/wait completed"
        );
        Ok(waited)
    }

    pub fn get_import_bundle_asset(
        &self,
        job_id: &str,
        asset_id: &str,
    ) -> Result<BrowserTabBundleAssetPayload> {
        let started = std::time::Instant::now();
        let response = self
            .client
            .get(format!(
                "{}/v1/import-bundles/{job_id}/assets/{asset_id}",
                self.base_url
            ))
            .send()
            .with_context(|| {
                format!("failed to get browsr import bundle asset {asset_id} for {job_id}")
            })?;
        let payload = parse_json_response_blocking::<BrowsrImportBundleAssetResponse>(response)?;
        let asset_ref = payload.asset.clone();
        let url = asset_ref
            .as_ref()
            .map(|value| value.url.clone())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_default();
        let mime_type = payload
            .content_type
            .clone()
            .or_else(|| asset_ref.as_ref().and_then(|value| value.mime_type.clone()));
        let resource_type = asset_ref
            .as_ref()
            .and_then(|value| value.resource_type.clone());
        let body = decode_bundle_asset_body(&payload)?;
        debug!(
            base_url = %self.base_url,
            job_id,
            asset_id,
            bytes = body.len(),
            content_type = ?mime_type,
            elapsed_ms = started.elapsed().as_millis(),
            "Browsr import bundle asset fetched"
        );
        Ok(BrowserTabBundleAssetPayload {
            asset_id: asset_id.to_string(),
            url,
            mime_type,
            resource_type,
            body,
        })
    }
}

fn decode_bundle_asset_body(payload: &BrowsrImportBundleAssetResponse) -> Result<Vec<u8>> {
    if payload.base64_encoded {
        return BASE64_STANDARD
            .decode(payload.body.as_bytes())
            .context("failed to decode browsr import bundle asset body");
    }
    Ok(payload.body.as_bytes().to_vec())
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

fn parse_json_response_blocking<T: for<'de> Deserialize<'de>>(
    response: reqwest::blocking::Response,
) -> Result<T> {
    let status = response.status();
    let body = response
        .text()
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
