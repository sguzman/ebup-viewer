//! WASM web client: renders a reader UI from server-provided `ReaderSnapshot`s
//! and sends `SessionCommand`s back over WebSocket.
//!
//! The server owns the `ReaderSession` and TTS generation/cache; the browser renders
//! and plays streamed audio.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use eframe::egui;
use lanternleaf_core::session::{PrettyKind, ReaderSnapshot, SessionCommand, TtsPlaybackState};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, trace, warn};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    Document, Element, ErrorEvent, HtmlAudioElement, HtmlDivElement, MessageEvent, WebSocket,
};

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
        let canvas_id_for_app = canvas_id.clone();
        let result = runner
            .start(
                &canvas_id,
                eframe::WebOptions::default(),
                Box::new(move |_cc| Box::new(WebClientApp::new(canvas_id_for_app.clone()))),
            )
            .await;
        if let Err(err) = result {
            warn!(error = ?err, "Failed to start eframe WebRunner");
        }
    });
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiMode {
    Starter,
    Reader,
}

struct WebClientApp {
    ws: Option<WebSocket>,
    ws_state: Rc<RefCell<WsState>>,
    audio: Option<HtmlAudioElement>,
    canvas_id: String,
    mode: UiMode,
    pretty_dom: PrettyDom,
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
    fn new(canvas_id: String) -> Self {
        let ws_state = Rc::new(RefCell::new(WsState {
            // Default to same-origin ws; allow the user to edit.
            server_url: "/api/v1/ws".to_string(),
            ..Default::default()
        }));
        Self {
            ws: None,
            ws_state,
            audio: None,
            canvas_id,
            mode: UiMode::Starter,
            pretty_dom: PrettyDom::new(),
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
                        state_msg.borrow_mut().last_error = Some(format!("parse error: {err}"));
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

    fn ensure_audio(&mut self) {
        if self.audio.is_some() {
            return;
        }
        let audio = HtmlAudioElement::new().ok();
        let Some(audio) = audio else {
            self.ws_state.borrow_mut().last_error =
                Some("Failed to create HtmlAudioElement".into());
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
                if let Ok(promise) = audio_clone.play() {
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
        let Some(audio) = self.audio.as_ref() else {
            return;
        };
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
        let Some(url) = next else {
            return;
        };
        trace!(url = %url, "Starting audio playback");
        audio.set_src(&url);
        if let Ok(promise) = audio.play() {
            wasm_bindgen_futures::spawn_local(async move {
                let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
            });
        }
    }

    fn render_top_bar(&mut self, ui: &mut egui::Ui, connected: bool, last_error: Option<&str>) {
        let mut server_url = { self.ws_state.borrow().server_url.clone() };
        ui.horizontal(|ui| {
            ui.label("Server:");
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
            if let Some(err) = last_error {
                ui.separator();
                ui.colored_label(egui::Color32::RED, err);
            }
        });
    }

    fn render_starter(&mut self, ui: &mut egui::Ui) {
        ui.heading("LanternLeaf");
        ui.add_space(8.0);
        ui.label("Open a source by server filesystem path:");
        ui.add_space(4.0);

        let mut open_path = self.ws_state.borrow().open_path.clone();
        ui.horizontal(|ui| {
            ui.label("Path:");
            if ui.text_edit_singleline(&mut open_path).changed() {
                self.ws_state.borrow_mut().open_path = open_path.clone();
            }
            if ui.button("Open").clicked() && !open_path.trim().is_empty() {
                info!(path = %open_path, "Requesting open_source");
                self.send(&ClientEvent::OpenSource { path: open_path });
            }
        });
        ui.add_space(8.0);
        ui.separator();
        ui.label("Tip: the server must be able to read the path you enter.");
    }

    fn render_reader_controls(&mut self, ui: &mut egui::Ui, snapshot: &ReaderSnapshot) {
        ui.heading("Reader");
        ui.add_space(8.0);
        ui.label(format!("Source: {}", snapshot.source_name));
        ui.label(format!(
            "Page {}/{}",
            snapshot.current_page + 1,
            snapshot.total_pages
        ));
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if ui.button("Prev").clicked() {
                self.send(&ClientEvent::SessionCommand {
                    command: SessionCommand::PrevPage,
                });
            }
            if ui.button("Next").clicked() {
                self.send(&ClientEvent::SessionCommand {
                    command: SessionCommand::NextPage,
                });
            }
        });

        ui.add_space(8.0);
        ui.separator();
        ui.label("TTS");
        ui.horizontal(|ui| {
            if ui.button("Play/Pause").clicked() {
                self.send(&ClientEvent::SessionCommand {
                    command: SessionCommand::TtsTogglePlayPause,
                });
            }
            if ui.button("Prev sentence").clicked() {
                self.send(&ClientEvent::SessionCommand {
                    command: SessionCommand::TtsSeekPrev,
                });
            }
            if ui.button("Next sentence").clicked() {
                self.send(&ClientEvent::SessionCommand {
                    command: SessionCommand::TtsSeekNext,
                });
            }
            if ui.button("Stop").clicked() {
                self.send(&ClientEvent::SessionCommand {
                    command: SessionCommand::TtsStop,
                });
            }
        });
        ui.label(format!("State: {:?}", snapshot.tts.state));
        if let Some(text) = snapshot.tts_current_sentence_text.as_ref() {
            ui.label(format!("Now: {}", truncate(text, 80)));
        }

        ui.add_space(8.0);
        ui.separator();
        ui.label("View");
        ui.horizontal(|ui| {
            if ui
                .button(if snapshot.text_only_mode {
                    "Switch to Pretty"
                } else {
                    "Switch to Text-only"
                })
                .clicked()
            {
                self.send(&ClientEvent::SessionCommand {
                    command: SessionCommand::ToggleTextOnly,
                });
            }
            ui.label(format!("pretty={:?}", snapshot.pretty_kind));
        });
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

        if snapshot.is_some() && self.mode != UiMode::Reader {
            info!("Switching to reader mode (snapshot received)");
            self.mode = UiMode::Reader;
        }

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            self.render_top_bar(ui, connected, last_error.as_deref());
        });

        match self.mode {
            UiMode::Starter => {
                self.pretty_dom.hide();
                egui::CentralPanel::default().show(ctx, |ui| {
                    self.render_starter(ui);
                });
            }
            UiMode::Reader => {
                let Some(snapshot) = snapshot.as_ref() else {
                    self.mode = UiMode::Starter;
                    self.pretty_dom.hide();
                    return;
                };

                egui::SidePanel::left("left_controls")
                    .resizable(true)
                    .default_width(340.0)
                    .show(ctx, |ui| {
                        if ui.button("Back").clicked() {
                            self.mode = UiMode::Starter;
                            self.ws_state.borrow_mut().snapshot = None;
                            self.pretty_dom.hide();
                        }
                        ui.separator();
                        self.render_reader_controls(ui, snapshot);
                    });

                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading(&snapshot.source_name);
                    });
                    ui.separator();

                    let highlight_idx = snapshot.highlighted_sentence_idx;
                    let pretty_html = snapshot.reading_html_page.as_deref();

                    if !snapshot.text_only_mode && snapshot.pretty_kind == PrettyKind::Html {
                        let rect = ui.available_rect_before_wrap();
                        ui.allocate_rect(rect, egui::Sense::hover());
                        self.pretty_dom
                            .show_html(&self.canvas_id, rect, pretty_html);
                        self.pretty_dom.update_highlight(snapshot, highlight_idx);

                        if snapshot.tts.state == TtsPlaybackState::Playing {
                            ctx.request_repaint();
                        }
                    } else {
                        self.pretty_dom.hide();
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                for (idx, sentence) in snapshot.sentences.iter().enumerate() {
                                    let selected = highlight_idx == Some(idx);
                                    let mut text = egui::RichText::new(sentence);
                                    if selected {
                                        text =
                                            text.background_color(egui::Color32::from_rgb(60, 80, 130));
                                    }
                                    if ui.selectable_label(selected, text).clicked() {
                                        self.send(&ClientEvent::SessionCommand {
                                            command: SessionCommand::SentenceClick { sentence_idx: idx },
                                        });
                                        self.send(&ClientEvent::SessionCommand {
                                            command: SessionCommand::TtsPlayFromHighlight,
                                        });
                                    }
                                }
                            });
                    }
                });
            }
        }

        // Keep egui driving while the server is sending snapshots and audio is playing.
        ctx.request_repaint();
    }
}

struct PrettyDom {
    container: Option<HtmlDivElement>,
    last_html_hash: u64,
    last_anchor_idx: Option<usize>,
}

impl PrettyDom {
    fn new() -> Self {
        Self {
            container: None,
            last_html_hash: 0,
            last_anchor_idx: None,
        }
    }

    fn hide(&mut self) {
        if let Some(div) = self.container.as_ref() {
            let _ = div.style().set_property("display", "none");
        }
    }

    fn show_html(&mut self, canvas_id: &str, rect: egui::Rect, html: Option<&str>) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Some(document) = window.document() else {
            return;
        };
        let div = self.ensure_container(&document);

        let Some((left, top, width, height)) = canvas_rect_px(&document, canvas_id, rect) else {
            return;
        };

        let style = div.style();
        let _ = style.set_property("display", "block");
        let _ = style.set_property("position", "absolute");
        let _ = style.set_property("left", &format!("{left}px"));
        let _ = style.set_property("top", &format!("{top}px"));
        let _ = style.set_property("width", &format!("{width}px"));
        let _ = style.set_property("height", &format!("{height}px"));
        let _ = style.set_property("overflow", "auto");
        let _ = style.set_property("pointer-events", "auto");

        if let Some(html) = html {
            let hash = fnv1a64(html.as_bytes());
            if hash != self.last_html_hash {
                trace!(len = html.len(), "Updating pretty HTML DOM");
                div.set_inner_html(html);
                self.last_html_hash = hash;
                self.last_anchor_idx = None;
            }
        } else {
            div.set_inner_html("<div style=\"padding: 16px;\">No HTML available for this source.</div>");
            self.last_html_hash = 0;
            self.last_anchor_idx = None;
        }
    }

    fn update_highlight(&mut self, snapshot: &ReaderSnapshot, sentence_idx: Option<usize>) {
        let Some(div) = self.container.as_ref() else {
            return;
        };
        let Some(sentence_idx) = sentence_idx else {
            return;
        };

        let anchor_idx = snapshot
            .sentence_anchor_map
            .get(sentence_idx)
            .copied()
            .flatten();
        let Some(anchor_idx) = anchor_idx else {
            return;
        };
        if self.last_anchor_idx == Some(anchor_idx) {
            return;
        }
        self.last_anchor_idx = Some(anchor_idx);
        highlight_nth_anchor(div, anchor_idx);
    }

    fn ensure_container(&mut self, document: &Document) -> HtmlDivElement {
        if let Some(existing) = self.container.as_ref() {
            return existing.clone();
        }

        let div: HtmlDivElement = document
            .create_element("div")
            .unwrap()
            .dyn_into()
            .unwrap();
        div.set_id("lanternleaf_pretty_dom");
        let style = div.style();
        let _ = style.set_property("z-index", "10");
        let _ = style.set_property("display", "none");
        let _ = style.set_property("background", "transparent");
        let _ = style.set_property("color", "inherit");

        // Inject minimal highlight CSS.
        div.set_inner_html(
            r#"<style>
.ll-highlight { outline: 2px solid rgba(120, 160, 255, 0.9); background: rgba(80, 110, 200, 0.15); }
</style>"#,
        );

        document.body().unwrap().append_child(&div).unwrap();
        self.container = Some(div.clone());
        div
    }
}

fn canvas_rect_px(
    document: &Document,
    canvas_id: &str,
    rect: egui::Rect,
) -> Option<(f64, f64, f64, f64)> {
    let canvas = document.get_element_by_id(canvas_id)?;
    let client_rect = canvas.get_bounding_client_rect();
    let left = client_rect.left() + rect.min.x as f64;
    let top = client_rect.top() + rect.min.y as f64;
    let width = rect.width() as f64;
    let height = rect.height() as f64;
    Some((left, top, width, height))
}

fn highlight_nth_anchor(container: &HtmlDivElement, anchor_idx: usize) {
    let selector = "h1,h2,h3,h4,h5,h6,p,li,blockquote,pre,img";
    let Ok(container_el) = container.clone().dyn_into::<Element>() else {
        return;
    };
    let Ok(list) = container_el.query_selector_all(selector) else {
        return;
    };

    for i in 0..list.length() {
        if let Some(el) = list.item(i) {
            if let Ok(el) = el.dyn_into::<Element>() {
                let _ = el.class_list().remove_1("ll-highlight");
            }
        }
    }

    let Some(el) = list.item(anchor_idx as u32) else {
        return;
    };
    let Ok(el) = el.dyn_into::<Element>() else {
        return;
    };
    let _ = el.class_list().add_1("ll-highlight");
    el.scroll_into_view();
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut out = s[..max].to_string();
    out.push_str("...");
    out
}
