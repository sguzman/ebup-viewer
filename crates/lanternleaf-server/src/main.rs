use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use axum::{
    extract::{Path, State, ws::{Message, WebSocket, WebSocketUpgrade}},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use tracing::info;
use lanternleaf_app::contracts::ReaderPlaybackState;
use lanternleaf_core::{cache, config};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BookState {
    pub bookmark: Option<cache::Bookmark>,
    pub config: Option<config::AppConfig>,
    pub playback: Option<ReaderPlaybackState>,
}

#[derive(Default)]
struct ServerState {
    books: HashMap<String, BookState>,
    clients: Vec<tokio::sync::mpsc::UnboundedSender<Message>>,
}

type SharedState = Arc<Mutex<ServerState>>;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the LanternLeaf synchronization server (default)
    Serve {
        /// Port to listen on
        #[arg(short, long, default_value_t = 3030)]
        port: u16,

        /// Interface to bind to
        #[arg(short, long, default_value = "0.0.0.0")]
        bind: String,
    },
    /// List all books currently in memory
    ListBooks,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let state = SharedState::default();

    match args.command.unwrap_or(Commands::Serve {
        port: 3030,
        bind: "0.0.0.0".to_string(),
    }) {
        Commands::Serve { port, bind } => {
            let app = Router::new()
                .route("/api/v1/book/:hash", get(get_book).post(update_book))
                .route("/api/v1/ws", get(ws_handler))
                .with_state(state);

            let addr = format!("{}:{}", bind, port);
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            info!("Server listening on {}", addr);
            axum::serve(listener, app).await?;
        }
        Commands::ListBooks => {
            info!("Querying books in memory (not implemented as persistence is transient in this version)");
        }
    }

    Ok(())
}

async fn get_book(
    Path(hash): Path<String>,
    State(state): State<SharedState>,
) -> impl IntoResponse {
    let guard = state.lock().unwrap();
    if let Some(book) = guard.books.get(&hash) {
        Json(book.clone()).into_response()
    } else {
        axum::http::StatusCode::NOT_FOUND.into_response()
    }
}

async fn update_book(
    Path(hash): Path<String>,
    State(state): State<SharedState>,
    Json(update): Json<BookState>,
) -> impl IntoResponse {
    let mut guard = state.lock().unwrap();
    guard.books.insert(hash.clone(), update.clone());
    
    // Broadcast update to all clients
    let msg = Message::Text(serde_json::to_string(&ServerEvent::BookUpdated {
        hash,
        state: update,
    }).unwrap());
    
    broadcast(&mut guard, msg);
    
    axum::http::StatusCode::OK
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: SharedState) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    
    {
        let mut guard = state.lock().unwrap();
        guard.clients.push(tx);
    }
    
    let mut send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });
    
    let state_clone = state.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(text) = msg {
                if let Ok(event) = serde_json::from_str::<ClientEvent>(&text) {
                    handle_client_event(event, &state_clone);
                }
            }
        }
    });
    
    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };
    
    // Cleanup client on disconnect
    // (In a real app we'd need a better way to remove the specific client)
}

fn handle_client_event(event: ClientEvent, state: &SharedState) {
    match event {
        ClientEvent::PlaybackUpdated { hash, playback } => {
            let mut guard = state.lock().unwrap();
            if let Some(book) = guard.books.get_mut(&hash) {
                book.playback = Some(playback.clone());
            }
            
            let msg = Message::Text(serde_json::to_string(&ServerEvent::PlaybackUpdated {
                hash,
                playback,
            }).unwrap());
            broadcast(&mut guard, msg);
        }
    }
}

fn broadcast(state: &mut ServerState, msg: Message) {
    state.clients.retain(|tx| {
        tx.send(msg.clone()).is_ok()
    });
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientEvent {
    PlaybackUpdated {
        hash: String,
        playback: ReaderPlaybackState,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerEvent {
    BookUpdated {
        hash: String,
        state: BookState,
    },
    PlaybackUpdated {
        hash: String,
        playback: ReaderPlaybackState,
    },
}
