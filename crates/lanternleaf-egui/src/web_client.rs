//! WASM "thin client" that renders `ReaderSnapshot` received from the server and
//! sends `SessionCommand` back over WebSocket.
//!
//! This intentionally avoids `lanternleaf-app` and any local filesystem/pipeline work.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use eframe::egui;
use lanternleaf_core::session::{ReaderSnapshot, SessionCommand};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, trace, warn};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{ErrorEvent, HtmlAudioElement, MessageEvent, WebSocket};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientEvent {
    Hello { client_version: String },
    OpenSource { path: String },
    SessionCommand { command: SessionCommand },
    TtsRequestMore { window_after_audio_idx: usize },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerEvent {
    Snapshot { snapshot: ReaderSnapshot },
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

pub fn run_wasm(canvas_id: &str) -> Result<(), JsValue> {
    tracing_wasm::set_as_global_default();

    let canvas_id = canvas_id.to_owned();
    wasm_bindgen_futures::spawn_local(async move {
        let runner = eframe::WebRunner::new();
        let result = runner
            .start(
                &canvas_id,
                eframe::WebOptions::default(),
                Box::new(|cc| Box::new(WebClientApp::new(cc))),
            )
            .await;
        if let Err(err) = result {
            warn!(error = ?err, "Failed to start eframe WebRunner");
        }
    });
    Ok(())
}

struct WebClientApp {
    ws: Option<WebSocket>,
    ws_state: Rc<RefCell<WsState>>,
    audio: Option<HtmlAudioElement>,
}

#[derive(Default)]
struct WsState {
    connected: bool,
    last_error: Option<String>,
    server_url: String,
    open_path: String,
    snapshot: Option<ReaderSnapshot>,
    last_batch: Option<(String, usize, usize, usize)>, // batch_id, page, start, count
    audio_queue: VecDeque<String>,
    audio_playing: bool,
    audio_underflows: u64,
}

impl WebClientApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let ws_state = Rc::new(RefCell::new(WsState {
            // Default to same-origin ws; allow the user to edit.
            server_url: "/api/v1/ws".to_string(),
            ..Default::default()
        }));
        Self {
            ws: None,
            ws_state,
            audio: None,
        }
    }

    fn ensure_ws(&mut self) {
        let url = { self.ws_state.borrow().server_url.clone() };
        if self.ws_state.borrow().connected {
            return;
        }

        let ws_url = if url.starts_with("ws://") || url.starts_with("wss://") {
            url
        } else {
            // Relative path -> same origin.
            let window = web_sys::window().expect("window");
            let loc = window.location();
            let protocol = loc.protocol().unwrap_or_else(|_| "http:".to_string());
            let host = loc.host().unwrap_or_else(|_| "localhost".to_string());
            let ws_scheme = if protocol.starts_with("https") { "wss" } else { "ws" };
            format!("{ws_scheme}://{host}{url}")
        };

        info!(ws_url = %ws_url, "Connecting WebSocket");
        let ws = match WebSocket::new(&ws_url) {
            Ok(ws) => ws,
            Err(err) => {
                self.ws_state.borrow_mut().last_error =
                    Some(format!("WebSocket create failed: {err:?}"));
                return;
            }
        };
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        let state_open = self.ws_state.clone();
        let ws_open = ws.clone();
        let onopen = Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
            info!("WebSocket connected");
            state_open.borrow_mut().connected = true;
            let hello = ClientEvent::Hello {
                client_version: "lanternleaf-web-v1".to_string(),
            };
            let _ = send_ws(&ws_open, &hello);
        });
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        onopen.forget();

        let state_err = self.ws_state.clone();
        let onerror = Closure::<dyn FnMut(ErrorEvent)>::new(move |e: ErrorEvent| {
            warn!(message = %e.message(), "WebSocket error");
            state_err.borrow_mut().last_error = Some(format!("ws error: {}", e.message()));
            state_err.borrow_mut().connected = false;
        });
        ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onerror.forget();

        let state_close = self.ws_state.clone();
        let onclose = Closure::<dyn FnMut(web_sys::CloseEvent)>::new(
            move |_e: web_sys::CloseEvent| {
            warn!("WebSocket closed");
            state_close.borrow_mut().connected = false;
        },
        );
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        onclose.forget();

        let state_msg = self.ws_state.clone();
        let onmessage = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            if let Ok(text) = e.data().dyn_into::<js_sys::JsString>() {
                let text: String = text.into();
                match serde_json::from_str::<ServerEvent>(&text) {
                    Ok(event) => {
                        trace!(?event, "WS server event");
                        handle_server_event(&state_msg, event);
                    }
                    Err(err) => {
                        warn!(error = %err, "Failed to parse server event");
                        state_msg.borrow_mut().last_error =
                            Some(format!("parse error: {err}"));
                    }
                }
            }
        });
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();

        self.ws = Some(ws);
    }

    fn send(&self, msg: &ClientEvent) {
        let Some(ws) = self.ws.as_ref() else {
            return;
        };
        if let Err(err) = send_ws(ws, msg) {
            warn!(error = %err, "Failed to send WS message");
            self.ws_state.borrow_mut().last_error = Some(err);
        }
    }
}

fn send_ws(ws: &WebSocket, msg: &ClientEvent) -> Result<(), String> {
    let payload = serde_json::to_string(msg).map_err(|e| e.to_string())?;
    ws.send_with_str(&payload).map_err(|e| format!("{e:?}"))?;
    Ok(())
}

fn handle_server_event(state: &Rc<RefCell<WsState>>, event: ServerEvent) {
    let mut guard = state.borrow_mut();
    match event {
        ServerEvent::Snapshot { snapshot } => {
            guard.snapshot = Some(snapshot);
        }
        ServerEvent::TtsBatch {
            batch_id,
            page,
            start_idx,
            items,
        } => {
            debug!(
                batch_id = %batch_id,
                page = page + 1,
                start_idx,
                count = items.len(),
                "Received TTS batch"
            );
            guard.last_batch = Some((batch_id, page, start_idx, items.len()));
            for item in items {
                guard.audio_queue.push_back(item.url);
            }
        }
        ServerEvent::Error { code, message } => {
            warn!(code = %code, message = %message, "Server error");
            guard.last_error = Some(format!("{code}: {message}"));
        }
    }
}

impl eframe::App for WebClientApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ensure_ws();
        self.ensure_audio();
        self.maybe_start_audio();
        let (connected, last_error, snapshot) = {
            let state = self.ws_state.borrow();
            (state.connected, state.last_error.clone(), state.snapshot.clone())
        };

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            let mut server_url = {
                let state = self.ws_state.borrow();
                state.server_url.clone()
            };
            ui.horizontal(|ui| {
                ui.label("Server WS:");
                let edited = ui.text_edit_singleline(&mut server_url).changed();
                if ui.button("Reconnect").clicked() {
                    self.ws = None;
                    let mut state = self.ws_state.borrow_mut();
                    state.connected = false;
                }
                if edited {
                    self.ws_state.borrow_mut().server_url = server_url;
                }
                ui.separator();
                ui.label(if connected { "connected" } else { "disconnected" });
                if let Some(err) = last_error.as_ref() {
                    ui.separator();
                    ui.colored_label(egui::Color32::RED, err);
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("LanternLeaf Web Client (thin)");
            let mut open_path = {
                let state = self.ws_state.borrow();
                state.open_path.clone()
            };
            ui.horizontal(|ui| {
                ui.label("Open path:");
                let edited = ui.text_edit_singleline(&mut open_path).changed();
                if ui.button("Open").clicked() {
                    let path = open_path.clone();
                    self.send(&ClientEvent::OpenSource { path });
                }
                if edited {
                    self.ws_state.borrow_mut().open_path = open_path.clone();
                }
            });

            if let Some(snapshot) = snapshot.as_ref() {
                ui.separator();
                ui.label(format!("Source: {}", snapshot.source_name));
                ui.label(format!("Page: {} / {}", snapshot.current_page + 1, snapshot.total_pages));
                ui.label(format!("Pretty: {:?} text_only={}", snapshot.pretty_kind, snapshot.text_only_mode));
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Prev").clicked() {
                        self.send(&ClientEvent::SessionCommand { command: SessionCommand::PrevPage });
                    }
                    if ui.button("Next").clicked() {
                        self.send(&ClientEvent::SessionCommand { command: SessionCommand::NextPage });
                    }
                    if ui.button("Play/Pause").clicked() {
                        self.send(&ClientEvent::SessionCommand { command: SessionCommand::TtsTogglePlayPause });
                    }
                    if ui.button("Stop").clicked() {
                        self.send(&ClientEvent::SessionCommand { command: SessionCommand::TtsStop });
                    }
                    if ui.button("Clear audio queue").clicked() {
                        let mut state = self.ws_state.borrow_mut();
                        state.audio_queue.clear();
                        state.audio_playing = false;
                    }
                });

                ui.separator();
                let highlight = snapshot.highlighted_sentence_idx;
                let sentences = snapshot.sentences.clone();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (idx, sentence) in sentences.iter().enumerate() {
                        let selected = highlight == Some(idx);
                        if ui.selectable_label(selected, sentence).clicked() {
                            self.send(&ClientEvent::SessionCommand {
                                command: SessionCommand::SentenceClick { sentence_idx: idx },
                            });
                            self.send(&ClientEvent::SessionCommand {
                                command: SessionCommand::TtsPlayFromHighlight,
                            });
                        }
                    }
                });
            } else {
                ui.label("No snapshot yet. Use 'Open path' to load a source.");
            }
        });

        ctx.request_repaint();
    }
}

impl WebClientApp {
    fn ensure_audio(&mut self) {
        if self.audio.is_some() {
            return;
        }
        let audio = HtmlAudioElement::new().ok();
        let Some(audio) = audio else {
            self.ws_state.borrow_mut().last_error = Some("Failed to create HtmlAudioElement".into());
            return;
        };
        audio.set_autoplay(false);
        audio.set_preload("auto");

        let state = self.ws_state.clone();
        let audio_clone = audio.clone();
        let onended = Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
            let mut guard = state.borrow_mut();
            if let Some(next) = guard.audio_queue.pop_front() {
                trace!(url = %next, "Audio ended; playing next url");
                guard.audio_playing = true;
                audio_clone.set_src(&next);
                let promise = audio_clone.play().ok();
                if let Some(promise) = promise {
                    wasm_bindgen_futures::spawn_local(async move {
                        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                    });
                }
            } else {
                guard.audio_playing = false;
                guard.audio_underflows += 1;
                debug!(
                    underflows = guard.audio_underflows,
                    "Audio queue underflow (no next item)"
                );
            }
        });
        audio.set_onended(Some(onended.as_ref().unchecked_ref()));
        onended.forget();

        self.audio = Some(audio);
    }

    fn maybe_start_audio(&mut self) {
        let Some(audio) = self.audio.as_ref() else { return; };
        let next = {
            let mut state = self.ws_state.borrow_mut();
            if state.audio_playing {
                return;
            }
            state.audio_queue.pop_front().map(|url| {
                state.audio_playing = true;
                url
            })
        };
        let Some(url) = next else { return; };
        trace!(url = %url, "Starting audio playback");
        audio.set_src(&url);
        if let Ok(promise) = audio.play() {
            wasm_bindgen_futures::spawn_local(async move {
                let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
            });
        }
    }
}
