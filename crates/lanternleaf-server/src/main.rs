use axum::{
    Router,
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::{IntoResponse, Response},
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use lanternleaf_core::{cache, config, normalizer, session, tts};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::services::ServeDir;
use tracing::{debug, info, trace, warn};

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the LanternLeaf server (default).
    Serve {
        /// Port to listen on
        #[arg(short, long, default_value_t = 3030)]
        port: u16,

        /// Interface to bind to
        #[arg(short, long, default_value = "0.0.0.0")]
        bind: String,

        /// Directory containing built web assets (Trunk dist/)
        #[arg(long, default_value = "crates/lanternleaf-web/dist")]
        web_dist: String,
    },
}

#[derive(Clone)]
struct AppState {
    normalizer: Arc<normalizer::TextNormalizer>,
    config: Arc<config::AppConfig>,
    tts_engine: Arc<tts::TtsEngine>,
    tts_cache_root: PathBuf,
    // batch_id -> (audio_idx -> wav_path)
    tts_batches: Arc<Mutex<HashMap<String, HashMap<usize, PathBuf>>>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Default to `info` logging unless the user overrides via `RUST_LOG=...`.
    // Without this, `EnvFilter::from_default_env()` defaults to a very restrictive filter,
    // and important diagnostics (like web-dist existence) disappear.
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    let args = Args::parse();
    match args.command.unwrap_or(Commands::Serve {
        port: 3030,
        bind: "0.0.0.0".to_string(),
        web_dist: "crates/lanternleaf-web/dist".to_string(),
    }) {
        Commands::Serve {
            port,
            bind,
            web_dist,
        } => serve(port, &bind, &web_dist).await,
    }
}

async fn serve(port: u16, bind: &str, web_dist: &str) -> anyhow::Result<()> {
    let config_path = default_config_path();
    let app_config = config::load_config(&config_path);
    info!(path = %config_path.display(), "Loaded app config for server");

    let normalizer = Arc::new(normalizer::TextNormalizer::load_default());
    let model_path = resolve_config_path(&app_config.tts_model_path);
    let espeak_path = resolve_config_path(&app_config.tts_espeak_path);
    let tts_engine = Arc::new(tts::TtsEngine::new(model_path, espeak_path)?);

    let tts_cache_root = cache::cache_root().join("tts-server");
    let state = AppState {
        normalizer,
        config: Arc::new(app_config),
        tts_engine,
        tts_cache_root,
        tts_batches: Arc::new(Mutex::new(HashMap::new())),
    };

    // Resolve web asset dir relative to the workspace root, not process CWD.
    // `cargo run`/systemd/etc can change CWD, and a relative web-dist would silently break.
    let web_dist_path = {
        let path = PathBuf::from(web_dist);
        if path.is_absolute() {
            path
        } else {
            let root = lanternleaf_core::workspace::workspace_root_from_cwd()
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            root.join(path)
        }
    };
    let web_dist_exists = web_dist_path.exists();
    info!(
        web_dist = web_dist,
        web_dist_abs = %web_dist_path.display(),
        web_dist_exists,
        "Configured web asset directory"
    );
    if !web_dist_exists {
        warn!(
            web_dist_abs = %web_dist_path.display(),
            "Web asset directory does not exist; run `cd crates/lanternleaf-web && trunk build`"
        );
    }

    let api = Router::new()
        .route("/api/v1/ws", get(ws_handler))
        .route("/api/v1/tts/audio/:batch_id/:audio_idx", get(get_tts_audio))
        .with_state(state.clone());

    // Serve the web client from the same origin.
    // Use an explicit `/` route so even if directory-index behavior changes, `/` works.
    let index_path = web_dist_path.join("index.html");
    if let Ok(index_html) = std::fs::read_to_string(&index_path) {
        if index_html.contains("lanternleaf_web.js") {
            // This is a common failure mode when the user forgot to rerun `trunk build`:
            // the old index referenced a non-existent fixed JS filename instead of Trunk's
            // fingerprinted output.
            warn!(
                index_path = %index_path.display(),
                "Web dist/index.html looks stale (references `lanternleaf_web.js`); rerun `cd crates/lanternleaf-web && trunk build`"
            );
        }
    } else {
        warn!(
            index_path = %index_path.display(),
            "Missing dist/index.html; run `cd crates/lanternleaf-web && trunk build`"
        );
    }
    let web = ServeDir::new(&web_dist_path).append_index_html_on_directories(true);

    let index_path_for_handler = index_path.clone();
    let app = Router::new()
        .merge(api)
        .route("/", get(move || serve_index_html(index_path_for_handler.clone())))
        .fallback_service(web);

    let addr = format!("{bind}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!(addr = %addr, web_dist, "LanternLeaf server listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn serve_index_html(index_path: PathBuf) -> Response {
    // If the user forgot to build the web client, returning a hard 404 is confusing.
    // Provide a small HTML help page instead.
    if tokio::fs::try_exists(&index_path).await.unwrap_or(false) {
        match tokio::fs::read(&index_path).await {
            Ok(bytes) => {
                let mut resp: Response = bytes.into_response();
                resp.headers_mut().insert(
                    axum::http::header::CONTENT_TYPE,
                    axum::http::HeaderValue::from_static("text/html"),
                );
                return resp;
            }
            Err(err) => {
                warn!(index_path = %index_path.display(), "Failed to read index.html: {err}");
            }
        }
    }

    let body = format!(
        r#"<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>LanternLeaf - Web Assets Missing</title>
    <style>
      body {{ font-family: ui-sans-serif, system-ui, sans-serif; padding: 24px; line-height: 1.4; }}
      code, pre {{ background: #f5f5f5; padding: 2px 4px; border-radius: 4px; }}
      pre {{ padding: 12px; overflow: auto; }}
    </style>
  </head>
  <body>
    <h1>LanternLeaf web assets missing</h1>
    <p>The server is running, but it could not find <code>index.html</code> at:</p>
    <pre>{}</pre>
    <p>Build the web client with:</p>
    <pre>cd crates/lanternleaf-web
trunk build</pre>
    <p>Then restart the server with <code>--web-dist crates/lanternleaf-web/dist</code>.</p>
  </body>
</html>"#,
        index_path.display()
    );
    let mut resp: Response = body.into_response();
    resp.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("text/html"),
    );
    resp
}

fn default_config_path() -> PathBuf {
    lanternleaf_core::workspace::workspace_root_from_cwd()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("conf")
        .join("config.toml")
}

fn resolve_config_path(value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        return path;
    }
    lanternleaf_core::workspace::workspace_root_from_cwd()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(path)
}

async fn get_tts_audio(
    Path((batch_id, audio_idx)): Path<(String, usize)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let guard = state.tts_batches.lock().await;
    let Some(map) = guard.get(&batch_id) else {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    };
    let Some(path) = map.get(&audio_idx) else {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    };
    // We serve the WAV bytes directly (v1).
    match tokio::fs::read(path).await {
        Ok(bytes) => {
            let mut resp = Response::new(bytes.into());
            resp.headers_mut().insert(
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static("audio/wav"),
            );
            resp
        }
        Err(err) => {
            warn!(batch_id, audio_idx, path = %path.display(), "Failed to read wav: {err}");
            axum::http::StatusCode::NOT_FOUND.into_response()
        }
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientEvent {
    Hello { client_version: String },
    OpenSource { path: String },
    SessionCommand { command: session::SessionCommand },
    TtsRequestMore { window_after_audio_idx: usize },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerEvent {
    Snapshot { snapshot: session::ReaderSnapshot },
    TtsBatch {
        batch_id: String,
        page: usize,
        start_idx: usize,
        items: Vec<TtsBatchItem>,
    },
    Error { code: String, message: String },
}

#[derive(Debug, Serialize, Deserialize)]
struct TtsBatchItem {
    audio_idx: usize,
    url: String,
    duration_ms: u64,
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Message>();

    let client_id = format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    info!(client_id, "WS client connected");

    let mut session_state = ClientSessionState::new(client_id.clone(), state.clone(), tx.clone());

    let mut send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ClientEvent>(&text) {
                            Ok(event) => {
                                if let Err(err) = session_state.handle_event(event).await {
                                    warn!(client_id = %session_state.client_id, "WS event handling error: {err}");
                                }
                            }
                            Err(err) => {
                                warn!(client_id = %session_state.client_id, "Failed to parse client event: {err}");
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break;
                    }
                    _ => {}
                }
            }
            _ = async {
                if let Some(schedule) = session_state.tts_schedule.as_mut() {
                    schedule.sleep.as_mut().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {
                if let Err(err) = session_state.tick_tts().await {
                    warn!(client_id = %session_state.client_id, "TTS tick error: {err}");
                }
            }
        }
    }

    send_task.abort();

    info!(client_id, "WS client disconnected");
}

struct ClientSessionState {
    client_id: String,
    state: AppState,
    tx: tokio::sync::mpsc::UnboundedSender<Message>,
    panels: session::PanelState,
    reader: Option<session::ReaderSession>,
    current_batch_id: Option<String>,
    tts_schedule: Option<TtsSchedule>,
}

struct TtsSchedule {
    // Absolute audio indices for this batch window.
    start_audio_idx: usize,
    durations_ms: Vec<u64>,
    next_offset: usize,
    sleep: Pin<Box<tokio::time::Sleep>>,
}

impl ClientSessionState {
    fn new(
        client_id: String,
        state: AppState,
        tx: tokio::sync::mpsc::UnboundedSender<Message>,
    ) -> Self {
        Self {
            client_id,
            state,
            tx,
            panels: session::PanelState::default(),
            reader: None,
            current_batch_id: None,
            tts_schedule: None,
        }
    }

    async fn handle_event(&mut self, event: ClientEvent) -> anyhow::Result<()> {
        match event {
            ClientEvent::Hello { client_version } => {
                info!(client_id = %self.client_id, client_version, "Client hello");
                self.emit_snapshot("hello").await?;
            }
            ClientEvent::OpenSource { path } => {
                info!(client_id = %self.client_id, path, "Open source request");
                self.open_source(PathBuf::from(path)).await?;
            }
            ClientEvent::SessionCommand { command } => {
                self.apply_command(command).await?;
            }
            ClientEvent::TtsRequestMore { window_after_audio_idx: _ } => {
                // v1: batches are prepared on play; client can request more later.
            }
        }
        Ok(())
    }

    async fn open_source(&mut self, source_path: PathBuf) -> anyhow::Result<()> {
        self.tts_schedule = None;
        let runtime_config = (*self.state.config).clone();
        let bookmark = cache::load_bookmark(&source_path);
        let reader = session::ReaderSession::load(
            source_path.clone(),
            runtime_config,
            &self.state.normalizer,
            bookmark,
        )
        .map_err(anyhow::Error::msg)?;
        self.reader = Some(reader);
        self.emit_snapshot("open_source").await?;
        Ok(())
    }

    async fn apply_command(&mut self, command: session::SessionCommand) -> anyhow::Result<()> {
        let Some(reader) = self.reader.as_mut() else {
            self.emit_error("no_session", "No active reader session.").await;
            return Ok(());
        };

        let started = std::time::Instant::now();
        trace!(
            client_id = %self.client_id,
            command = ?command,
            "Applying session command"
        );

        let event = reader.apply_command(command.clone(), self.panels, &self.state.normalizer);

        let elapsed_ms = started.elapsed().as_millis();
        debug!(
            client_id = %self.client_id,
            action = event.action,
            elapsed_ms,
            "Applied session command"
        );

        // If TTS transitioned to playing, prepare an initial batch window and send it.
        if matches!(
            command,
            session::SessionCommand::TtsPlay
                | session::SessionCommand::TtsTogglePlayPause
                | session::SessionCommand::TtsPlayFromPageStart
                | session::SessionCommand::TtsPlayFromHighlight
        ) && event.snapshot.tts.state == session::TtsPlaybackState::Playing
        {
            self.prepare_and_emit_tts_batch().await?;
            // schedule ticks using the synthesized durations we just prepared
            self.reset_tts_schedule_from_current_batch().await?;
        }
        if matches!(command, session::SessionCommand::TtsPause | session::SessionCommand::TtsStop) {
            self.tts_schedule = None;
        }

        self.emit(ServerEvent::Snapshot {
            snapshot: event.snapshot,
        })
        .await?;
        Ok(())
    }

    async fn prepare_and_emit_tts_batch(&mut self) -> anyhow::Result<()> {
        let (current_page, start_idx, chunk_end, batch_id, windowed_sentences) = {
            let Some(reader) = self.reader.as_mut() else {
                return Ok(());
            };
            let current_page = reader.current_page;
            let (sentences, start_idx) = reader.current_tts_audio_slice(&self.state.normalizer);
            if sentences.is_empty() {
                return Ok(());
            }
            let batch_id = format!("{}-{}", self.client_id, start_idx);
            self.current_batch_id = Some(batch_id.clone());

            let window = 100usize;
            let chunk_end = (start_idx + window).min(sentences.len());
            let windowed_sentences: Vec<String> = sentences[start_idx..chunk_end].to_vec();
            (current_page, start_idx, chunk_end, batch_id, windowed_sentences)
        };

        info!(
            client_id = %self.client_id,
            batch_id = %batch_id,
            start_idx,
            chunk_end,
            total = chunk_end,
            "Preparing server-side TTS batch"
        );

        let prepared = self.state.tts_engine.prepare_batch(
            self.state.tts_cache_root.clone(),
            windowed_sentences,
            0,
            self.state.config.tts_threads,
            std::time::Duration::from_secs_f32(self.state.config.tts_progress_log_interval_secs),
        )?;

        let mut map: HashMap<usize, PathBuf> = HashMap::new();
        let mut items = Vec::new();
        let mut durations_ms = Vec::new();
        for (offset, (path, dur)) in prepared.into_iter().enumerate() {
            let audio_idx = start_idx + offset;
            if audio_idx >= chunk_end {
                break;
            }
            map.insert(audio_idx, path.clone());
            durations_ms.push(dur.as_millis() as u64);
            items.push(TtsBatchItem {
                audio_idx,
                url: format!("/api/v1/tts/audio/{batch_id}/{audio_idx}"),
                duration_ms: dur.as_millis() as u64,
            });
        }

        {
            let mut guard = self.state.tts_batches.lock().await;
            guard.insert(batch_id.clone(), map);
        }

        self.emit(ServerEvent::TtsBatch {
            batch_id,
            page: current_page,
            start_idx,
            items,
        })
        .await?;

        // Store schedule info for cursor driving.
        self.tts_schedule = Some(TtsSchedule {
            start_audio_idx: start_idx,
            durations_ms,
            next_offset: 0,
            sleep: Box::pin(tokio::time::sleep(std::time::Duration::from_millis(0))),
        });

        Ok(())
    }

    async fn reset_tts_schedule_from_current_batch(&mut self) -> anyhow::Result<()> {
        // prepare_and_emit_tts_batch already placed the schedule; just arm initial timer.
        if let Some(schedule) = self.tts_schedule.as_mut() {
            let first = schedule.durations_ms.get(0).copied().unwrap_or(0);
            schedule.sleep =
                Box::pin(tokio::time::sleep(std::time::Duration::from_millis(first)));
        }
        Ok(())
    }

    async fn emit_snapshot(&mut self, reason: &str) -> anyhow::Result<()> {
        let Some(reader) = self.reader.as_mut() else {
            trace!(client_id = %self.client_id, reason, "Snapshot requested with no session");
            return Ok(());
        };
        let snapshot = reader.snapshot(self.panels, &self.state.normalizer);
        trace!(client_id = %self.client_id, reason, "Emitting snapshot");
        self.emit(ServerEvent::Snapshot { snapshot }).await?;
        Ok(())
    }

    async fn emit_error(&self, code: &str, message: &str) {
        let _ = self
            .tx
            .send(Message::Text(
                serde_json::to_string(&ServerEvent::Error {
                    code: code.to_string(),
                    message: message.to_string(),
                })
                .unwrap_or_else(|_| "{\"type\":\"error\",\"code\":\"serde\",\"message\":\"serialization failed\"}".to_string()),
            ));
    }

    async fn emit(&self, event: ServerEvent) -> anyhow::Result<()> {
        let json = serde_json::to_string(&event)?;
        self.tx
            .send(Message::Text(json))
            .map_err(|_| anyhow::anyhow!("client channel closed"))?;
        Ok(())
    }

    async fn tick_tts(&mut self) -> anyhow::Result<()> {
        // Take the schedule out to avoid holding a mutable borrow across `.await`s.
        let Some(mut schedule) = self.tts_schedule.take() else {
            return Ok(());
        };
        let Some(reader) = self.reader.as_mut() else {
            self.tts_schedule = None;
            return Ok(());
        };

        // Stop driving if TTS is no longer playing.
        if reader.snapshot(self.panels, &self.state.normalizer).tts.state
            != session::TtsPlaybackState::Playing
        {
            self.tts_schedule = None;
            return Ok(());
        }

        // If we've exhausted this window, prepare the next batch starting from the current cursor.
        if schedule.next_offset >= schedule.durations_ms.len() {
            trace!(client_id = %self.client_id, "TTS window exhausted; preparing next batch");
            self.prepare_and_emit_tts_batch().await?;
            self.reset_tts_schedule_from_current_batch().await?;
            return Ok(());
        }

        // Advance cursor.
        let (snapshot, pause_ms) = {
            let event = reader.apply_command(
                session::SessionCommand::TtsSeekNext,
                self.panels,
                &self.state.normalizer,
            );
            let pause_ms = (reader.config.pause_after_sentence.max(0.0) * 1000.0) as u64;
            (event.snapshot, pause_ms)
        };
        self.emit(ServerEvent::Snapshot { snapshot }).await?;

        schedule.next_offset += 1;
        let next_ms = schedule
            .durations_ms
            .get(schedule.next_offset)
            .copied()
            .unwrap_or(0);
        let wait_ms = next_ms.saturating_add(pause_ms);
        schedule.sleep = Box::pin(tokio::time::sleep(std::time::Duration::from_millis(
            wait_ms,
        )));
        self.tts_schedule = Some(schedule);
        Ok(())
    }
}
