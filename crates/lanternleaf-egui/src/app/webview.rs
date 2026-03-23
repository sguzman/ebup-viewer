use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Instant,
};

use eframe::egui::{Context, Rect as EguiRect};
use http::{Response, StatusCode, header};
use lanternleaf_app::contracts::ReaderSnapshot;
use lanternleaf_core::session::ReaderImageRef;
use lol_html::html_content::ContentType;
use lol_html::{RewriteStrSettings, element, rewrite_str};
use percent_encoding::percent_decode_str;
use tracing::{info, trace, warn};
use wry::{
    Rect as WryRect, WebView, WebViewBuilder,
    dpi::{LogicalPosition, LogicalSize},
};

use raw_window_handle as rwh06;
use raw_window_handle_05 as rwh05;

pub struct FrameHandleSnapshot {
    window: rwh05::RawWindowHandle,
    display: rwh05::RawDisplayHandle,
}

impl FrameHandleSnapshot {
    pub fn from_frame(frame: &eframe::Frame) -> Self {
        use rwh05::{HasRawDisplayHandle as _, HasRawWindowHandle as _};
        Self {
            window: frame.raw_window_handle(),
            display: frame.raw_display_handle(),
        }
    }
}

pub struct WebViewRenderer {
    webview: Option<WebView>,
    resolver: Arc<Mutex<AssetResolver>>,
    prep_tx: mpsc::Sender<HtmlPrepareRequest>,
    prep_rx: mpsc::Receiver<PreparedHtml>,
    pending_html: Option<PreparedHtml>,
    last_source_hash: Option<String>,
    last_html_hash: Option<u64>,
    last_bounds: Option<WryRect>,
    last_highlight: Option<(usize, String)>,
    last_highlight_color: Option<String>,
    pending_highlight: Option<(usize, String)>,
    pending_scroll: Option<usize>,
    requested_html: HashSet<u64>,
}

impl WebViewRenderer {
    pub fn new() -> Self {
        let (prep_tx, prep_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        thread::spawn(move || html_prepare_worker(prep_rx, result_tx));
        Self {
            webview: None,
            resolver: Arc::new(Mutex::new(AssetResolver::default())),
            prep_tx,
            prep_rx: result_rx,
            pending_html: None,
            last_source_hash: None,
            last_html_hash: None,
            last_bounds: None,
            last_highlight: None,
            last_highlight_color: None,
            pending_highlight: None,
            pending_scroll: None,
            requested_html: HashSet::new(),
        }
    }

    pub fn hide(&mut self) {
        if let Some(webview) = self.webview.as_ref() {
            if let Err(err) = webview.set_visible(false) {
                warn!(error = ?err, "Failed to hide HTML webview");
            }
        }
    }

    pub fn render_html(
        &mut self,
        ctx: &Context,
        frame_handles: Option<&FrameHandleSnapshot>,
        rect: EguiRect,
        snapshot: &ReaderSnapshot,
        highlight_anchor: Option<usize>,
        highlight_sentence: Option<&str>,
        highlight_color: &str,
        scroll_anchor: Option<usize>,
    ) {
        let Some(html) = snapshot.reading_html_page.as_deref() else {
            self.teardown("missing html");
            return;
        };
        let Some(frame_handles) = frame_handles else {
            self.teardown("missing frame handles");
            return;
        };
        self.update_asset_resolver(&snapshot.images);
        self.poll_prepared_html();
        let html_hash = hash_html(html);
        if self.last_html_hash != Some(html_hash) && !self.requested_html.contains(&html_hash) {
            self.requested_html.insert(html_hash);
            let request = HtmlPrepareRequest {
                html: html.to_string(),
                source_hash: hash_source(&snapshot.source_path),
            };
            if let Err(err) = self.prep_tx.send(request) {
                warn!(error = ?err, "Failed to enqueue HTML preprocessing");
            }
        }

        if let Some(prepared) = self.pending_html.take() {
            self.last_source_hash = Some(prepared.source_hash.clone());
            self.last_html_hash = Some(prepared.html_hash);
            self.requested_html.remove(&prepared.html_hash);
            self.last_highlight = None;
            self.last_highlight_color = None;
            self.ensure_webview(frame_handles, ctx, rect, &prepared.html);
        } else if self.webview.is_none() {
            return;
        }

        self.update_bounds(rect);
        if let Some(webview) = self.webview.as_ref() {
            if let Err(err) = webview.set_visible(true) {
                warn!(error = ?err, "Failed to show HTML webview");
            }
        }

        if let (Some(anchor), Some(sentence)) = (highlight_anchor, highlight_sentence) {
            let sentence_owned = sentence.to_string();
            if self.last_highlight.as_ref() != Some(&(anchor, sentence_owned.clone())) {
                self.pending_highlight = Some((anchor, sentence_owned));
            }
        } else if self.last_highlight.is_some() {
            self.clear_highlight();
        }
        if let Some(anchor) = scroll_anchor {
            self.pending_scroll = Some(anchor);
        }

        self.apply_highlight(highlight_color);
        self.apply_scroll();
    }

    pub fn clear(&mut self) {
        self.teardown("clear");
    }

    fn update_asset_resolver(&mut self, images: &[ReaderImageRef]) {
        let mut resolver = self.resolver.lock().unwrap();
        resolver.update(images);
    }

    fn poll_prepared_html(&mut self) {
        while let Ok(prepared) = self.prep_rx.try_recv() {
            self.pending_html = Some(prepared);
        }
    }

    fn ensure_webview(
        &mut self,
        frame_handles: &FrameHandleSnapshot,
        ctx: &Context,
        rect: EguiRect,
        html: &str,
    ) {
        if let Some(webview) = self.webview.as_ref() {
            if let Err(err) = webview.load_html(html) {
                warn!(error = ?err, "Failed to load HTML into webview");
            } else {
                trace!("Reloaded HTML in webview");
            }
            return;
        }

        let Some(handle_wrapper) = WindowHandleWrapper::from_frame(frame_handles) else {
            warn!("Unable to build webview handle wrapper");
            return;
        };
        let bounds = wry_rect_from_egui(rect);
        let resolver = Arc::clone(&self.resolver);
        let builder = WebViewBuilder::new()
            .with_custom_protocol("lanternleaf-asset".to_string(), move |_id, req| {
                resolve_asset_request(req, &resolver)
            })
            .with_bounds(bounds)
            .with_html(html);
        trace!(?bounds, "Creating HTML webview");
        match builder.build_as_child(&handle_wrapper) {
            Ok(webview) => {
                self.webview = Some(webview);
                self.last_bounds = Some(bounds);
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
            Err(err) => {
                warn!(error = ?err, "Failed to build HTML webview");
            }
        }
    }

    fn update_bounds(&mut self, rect: EguiRect) {
        let bounds = wry_rect_from_egui(rect);
        if self.last_bounds.as_ref() == Some(&bounds) {
            return;
        }
        if let Some(webview) = self.webview.as_ref() {
            if let Err(err) = webview.set_bounds(bounds) {
                warn!(error = ?err, "Failed to update webview bounds");
            } else {
                trace!(?bounds, "Updated webview bounds");
                self.last_bounds = Some(bounds);
            }
        }
    }

    fn apply_highlight(&mut self, highlight_color: &str) {
        let Some(webview) = self.webview.as_ref() else {
            return;
        };
        if self.last_highlight_color.as_deref() != Some(highlight_color) {
            let script = format!(
                "window.llSetHighlightColor && window.llSetHighlightColor({});",
                serde_json::to_string(highlight_color).unwrap_or_default()
            );
            if let Err(err) = webview.evaluate_script(&script) {
                warn!(error = ?err, "Failed to update webview highlight color");
            } else {
                self.last_highlight_color = Some(highlight_color.to_string());
            }
        }
        let Some((anchor, sentence)) = self.pending_highlight.take() else {
            return;
        };
        let sentence_json = serde_json::to_string(&sentence).unwrap_or_else(|_| "\"\"".to_string());
        let script = format!(
            "window.llHighlight && window.llHighlight({}, {});",
            anchor, sentence_json
        );
        if let Err(err) = webview.evaluate_script(&script) {
            warn!(error = ?err, "Failed to highlight sentence in webview");
        } else {
            trace!(anchor, "Updated HTML highlight");
            self.last_highlight = Some((anchor, sentence));
        }
    }

    fn clear_highlight(&mut self) {
        let Some(webview) = self.webview.as_ref() else {
            return;
        };
        let script = "window.llClearHighlight && window.llClearHighlight();";
        if let Err(err) = webview.evaluate_script(script) {
            warn!(error = ?err, "Failed to clear webview highlight");
        } else {
            self.last_highlight = None;
            trace!("Cleared HTML highlight");
        }
    }

    fn apply_scroll(&mut self) {
        let Some(webview) = self.webview.as_ref() else {
            return;
        };
        let Some(anchor) = self.pending_scroll.take() else {
            return;
        };
        let script = format!(
            "window.llScrollToAnchor && window.llScrollToAnchor({});",
            anchor
        );
        if let Err(err) = webview.evaluate_script(&script) {
            warn!(error = ?err, "Failed to scroll webview to highlight");
        } else {
            trace!(anchor, "Scrolled HTML webview");
        }
    }

    fn teardown(&mut self, reason: &str) {
        if self.webview.take().is_some() {
            info!(reason, "Torn down HTML webview");
        }
        self.last_bounds = None;
        self.last_highlight = None;
        self.pending_highlight = None;
        self.pending_scroll = None;
    }
}

#[derive(Default)]
struct AssetResolver {
    by_key: HashMap<String, PathBuf>,
    by_basename: HashMap<String, PathBuf>,
}

impl AssetResolver {
    fn update(&mut self, images: &[ReaderImageRef]) {
        self.by_key.clear();
        self.by_basename.clear();
        for image in images {
            let key = normalize_asset_key(&image.raw_path);
            self.by_key.insert(key, PathBuf::from(&image.local_path));
            if let Some(name) = Path::new(&image.raw_path)
                .file_name()
                .and_then(|s| s.to_str())
            {
                let base = normalize_asset_key(name);
                self.by_basename
                    .insert(base, PathBuf::from(&image.local_path));
            }
        }
    }

    fn resolve(&self, raw: &str) -> Option<PathBuf> {
        let key = normalize_asset_key(raw);
        if let Some(path) = self.by_key.get(&key) {
            return Some(path.clone());
        }
        self.by_basename.get(&key).cloned()
    }
}

struct WindowHandleWrapper {
    window: rwh06::RawWindowHandle,
    display: rwh06::RawDisplayHandle,
}

impl WindowHandleWrapper {
    fn from_frame(frame: &FrameHandleSnapshot) -> Option<Self> {
        let window = convert_window_handle(frame.window)?;
        let display = convert_display_handle(frame.display)?;
        Some(Self { window, display })
    }
}

impl rwh06::HasWindowHandle for WindowHandleWrapper {
    fn window_handle(&self) -> Result<rwh06::WindowHandle<'_>, rwh06::HandleError> {
        unsafe { Ok(rwh06::WindowHandle::borrow_raw(self.window)) }
    }
}

impl rwh06::HasDisplayHandle for WindowHandleWrapper {
    fn display_handle(&self) -> Result<rwh06::DisplayHandle<'_>, rwh06::HandleError> {
        unsafe { Ok(rwh06::DisplayHandle::borrow_raw(self.display)) }
    }
}

fn wry_rect_from_egui(rect: EguiRect) -> WryRect {
    WryRect {
        position: LogicalPosition::new(rect.min.x as f64, rect.min.y as f64).into(),
        size: LogicalSize::new(rect.width() as f64, rect.height() as f64).into(),
    }
}

fn resolve_asset_request(
    req: http::Request<Vec<u8>>,
    resolver: &Arc<Mutex<AssetResolver>>,
) -> Response<std::borrow::Cow<'static, [u8]>> {
    let uri = req.uri().clone();
    let host = uri.host().unwrap_or_default();
    let path = uri.path().trim_start_matches('/');
    let mut candidates = Vec::new();
    if !host.is_empty() && !path.is_empty() {
        candidates.push(format!("{}/{}", host, path));
    } else if !host.is_empty() {
        candidates.push(host.to_string());
    }
    if !path.is_empty() {
        candidates.push(path.to_string());
    }
    for candidate in candidates {
        let decoded = percent_decode_str(&candidate).decode_utf8_lossy();
        let resolved = resolver.lock().unwrap().resolve(&decoded);
        if let Some(path) = resolved {
            if let Ok(bytes) = std::fs::read(&path) {
                let mime = mime_for_path(&path);
                return Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, mime)
                    .body(std::borrow::Cow::Owned(bytes))
                    .unwrap_or_else(|_| empty_response(StatusCode::OK));
            }
        }
    }
    empty_response(StatusCode::NOT_FOUND)
}

fn empty_response(status: StatusCode) -> Response<std::borrow::Cow<'static, [u8]>> {
    Response::builder()
        .status(status)
        .body(std::borrow::Cow::Borrowed(&[] as &[u8]))
        .unwrap_or_else(|_| {
            Response::builder()
                .status(status)
                .body(std::borrow::Cow::Borrowed(&[] as &[u8]))
                .unwrap()
        })
}

fn mime_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        _ => "application/octet-stream",
    }
}

fn normalize_asset_key(raw: &str) -> String {
    let trimmed = raw.trim().trim_matches('"').trim_matches('\'');
    let mut key = trimmed.replace('\\', "/");
    if let Some(idx) = key.find('?') {
        key.truncate(idx);
    }
    if let Some(idx) = key.find('#') {
        key.truncate(idx);
    }
    let mut parts = Vec::new();
    for part in key.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            let _ = parts.pop();
        } else {
            parts.push(part);
        }
    }
    let joined = parts.join("/");
    joined.trim_start_matches('/').to_ascii_lowercase()
}

struct HtmlPrepareRequest {
    html: String,
    source_hash: String,
}

struct PreparedHtml {
    html: String,
    source_hash: String,
    html_hash: u64,
}

fn html_prepare_worker(rx: mpsc::Receiver<HtmlPrepareRequest>, tx: mpsc::Sender<PreparedHtml>) {
    for req in rx {
        let start = Instant::now();
        let html_hash = hash_html(&req.html);
        match prepare_html(&req.html, &req.source_hash) {
            Ok(html) => {
                let elapsed = start.elapsed();
                trace!(
                    html_hash,
                    prep_ms = elapsed.as_millis(),
                    "Prepared HTML for webview"
                );
                let _ = tx.send(PreparedHtml {
                    html,
                    source_hash: req.source_hash,
                    html_hash,
                });
            }
            Err(err) => {
                warn!(error = ?err, "Failed to prepare HTML");
            }
        }
    }
}

fn prepare_html(html: &str, source_hash: &str) -> Result<String, lol_html::errors::RewritingError> {
    let base_tag = format!("<base href=\"lanternleaf-asset://{}/\">", source_hash);
    let helpers = format!(
        "{base}<style>{style}</style><script>{script}</script>",
        base = base_tag,
        style = STYLE_HELPER,
        script = SCRIPT_HELPER
    );
    let injected_head = Arc::new(Mutex::new(false));
    let anchor_counter = Arc::new(Mutex::new(0usize));
    let injected_head_clone = Arc::clone(&injected_head);
    let anchor_clone = Arc::clone(&anchor_counter);
    let helpers_head = helpers.clone();
    let settings = RewriteStrSettings {
        element_content_handlers: vec![
            element!("head", move |el| {
                el.append(&helpers_head, ContentType::Html);
                *injected_head_clone.lock().unwrap() = true;
                Ok(())
            }),
            element!(ANCHOR_SELECTOR, move |el| {
                let mut idx = anchor_clone.lock().unwrap();
                let anchor = *idx;
                let _ = el.set_attribute("data-ll-anchor", &anchor.to_string());
                let _ = el.set_attribute("id", &format!("ll-anchor-{anchor}"));
                *idx += 1;
                Ok(())
            }),
        ],
        ..RewriteStrSettings::default()
    };
    let rewritten = rewrite_str(html, settings)?;
    let lower = html.to_ascii_lowercase();
    let has_html_tag = lower.contains("<html");
    let injected = *injected_head.lock().unwrap();
    if has_html_tag {
        if injected {
            Ok(rewritten)
        } else {
            Ok(format!("{helpers}{rewritten}"))
        }
    } else {
        Ok(format!(
            "<!doctype html><html><head>{helpers}</head><body>{rewritten}</body></html>"
        ))
    }
}

const ANCHOR_SELECTOR: &str = "h1,h2,h3,h4,h5,h6,p,li,blockquote,pre,img";

const STYLE_HELPER: &str = r#"
:root { --ll-highlight: rgba(255, 215, 0, 0.35); }
.ll-highlight { background: var(--ll-highlight); }
img { max-width: 100%; height: auto; }
"#;

const SCRIPT_HELPER: &str = r#"
window.llSetHighlightColor = function(color) {
  if (color) {
    document.documentElement.style.setProperty('--ll-highlight', color);
  }
};
window.llClearHighlight = function() {
  if (window.__llHighlightSpan && window.__llHighlightSpan.parentNode) {
    const span = window.__llHighlightSpan;
    const parent = span.parentNode;
    while (span.firstChild) {
      parent.insertBefore(span.firstChild, span);
    }
    parent.removeChild(span);
    window.__llHighlightSpan = null;
  }
  if (window.__llHighlightEl) {
    window.__llHighlightEl.classList.remove('ll-highlight');
    window.__llHighlightEl = null;
  }
};
function llNormalize(text) {
  return (text || '').replace(/\s+/g, ' ').trim();
}
function llFindRange(el, sentence) {
  const target = llNormalize(sentence);
  if (!target) return null;
  const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, null);
  let nodes = [];
  let raw = '';
  while (walker.nextNode()) {
    const node = walker.currentNode;
    const text = node.nodeValue || '';
    for (let i = 0; i < text.length; i++) {
      const ch = text[i];
      if (/\s/.test(ch)) {
        if (!raw.endsWith(' ')) {
          raw += ' ';
          nodes.push({ node, offset: i });
        }
      } else {
        raw += ch;
        nodes.push({ node, offset: i });
      }
    }
  }
  const hay = raw.toLowerCase();
  const needle = target.toLowerCase();
  const idx = hay.indexOf(needle);
  if (idx < 0) return null;
  const start = nodes[idx];
  const end = nodes[idx + needle.length - 1];
  if (!start || !end) return null;
  const range = document.createRange();
  range.setStart(start.node, start.offset);
  range.setEnd(end.node, end.offset + 1);
  return range;
}
window.llHighlight = function(anchorIdx, sentence) {
  window.llClearHighlight();
  const el = document.querySelector('[data-ll-anchor=\"' + anchorIdx + '\"]');
  if (!el) return false;
  const range = llFindRange(el, sentence);
  if (range) {
    const span = document.createElement('span');
    span.className = 'll-highlight';
    span.appendChild(range.extractContents());
    range.insertNode(span);
    window.__llHighlightSpan = span;
    return true;
  }
  el.classList.add('ll-highlight');
  window.__llHighlightEl = el;
  return false;
};
window.llScrollToAnchor = function(anchorIdx) {
  const el = document.querySelector('[data-ll-anchor=\"' + anchorIdx + '\"]');
  if (!el) return false;
  el.scrollIntoView({ block: 'center', behavior: 'auto' });
  return true;
};
"#;

fn hash_source(source_path: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source_path.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn hash_html(html: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    html.hash(&mut hasher);
    hasher.finish()
}

fn convert_window_handle(handle: rwh05::RawWindowHandle) -> Option<rwh06::RawWindowHandle> {
    use rwh05::RawWindowHandle as R05;
    use rwh06::RawWindowHandle as R06;
    match handle {
        R05::Win32(raw) => {
            let hwnd = std::num::NonZeroIsize::new(raw.hwnd as isize)?;
            let mut out = rwh06::Win32WindowHandle::new(hwnd);
            out.hinstance = std::num::NonZeroIsize::new(raw.hinstance as isize);
            Some(R06::Win32(out))
        }
        R05::Xlib(raw) => {
            let mut out = rwh06::XlibWindowHandle::new(raw.window);
            out.visual_id = raw.visual_id;
            Some(R06::Xlib(out))
        }
        R05::Xcb(raw) => {
            let window = std::num::NonZeroU32::new(raw.window)?;
            let mut out = rwh06::XcbWindowHandle::new(window);
            out.visual_id = std::num::NonZeroU32::new(raw.visual_id);
            Some(R06::Xcb(out))
        }
        R05::Wayland(raw) => {
            let surface = std::ptr::NonNull::new(raw.surface)?;
            let out = rwh06::WaylandWindowHandle::new(surface);
            Some(R06::Wayland(out))
        }
        R05::AppKit(raw) => {
            let ns_view = std::ptr::NonNull::new(raw.ns_view)?;
            let out = rwh06::AppKitWindowHandle::new(ns_view);
            Some(R06::AppKit(out))
        }
        _ => None,
    }
}

fn convert_display_handle(handle: rwh05::RawDisplayHandle) -> Option<rwh06::RawDisplayHandle> {
    use rwh05::RawDisplayHandle as R05;
    use rwh06::RawDisplayHandle as R06;
    match handle {
        R05::Xlib(raw) => {
            let display = std::ptr::NonNull::new(raw.display);
            let mut out = rwh06::XlibDisplayHandle::new(display, raw.screen);
            out.screen = raw.screen;
            Some(R06::Xlib(out))
        }
        R05::Xcb(raw) => {
            let connection = std::ptr::NonNull::new(raw.connection);
            let mut out = rwh06::XcbDisplayHandle::new(connection, raw.screen);
            out.screen = raw.screen;
            Some(R06::Xcb(out))
        }
        R05::Wayland(raw) => {
            let display = std::ptr::NonNull::new(raw.display)?;
            let out = rwh06::WaylandDisplayHandle::new(display);
            Some(R06::Wayland(out))
        }
        R05::AppKit(_) => {
            let out = rwh06::AppKitDisplayHandle::new();
            Some(R06::AppKit(out))
        }
        R05::Windows(_) => {
            let out = rwh06::WindowsDisplayHandle::new();
            Some(R06::Windows(out))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_preprocessor_injects_anchors_in_order() {
        let html = "<h1>Title</h1><p>First</p><p>Second</p><li>Item</li>";
        let out = prepare_html(html, "abc123").expect("prepare html");
        assert!(out.contains("lanternleaf-asset://abc123/"));
        let mut anchors = Vec::new();
        let mut cursor = out.as_str();
        let marker = "data-ll-anchor=\"";
        while let Some(idx) = cursor.find(marker) {
            let start = idx + marker.len();
            let rest = &cursor[start..];
            let end = rest.find('"').unwrap_or(0);
            if end == 0 {
                break;
            }
            if let Ok(anchor) = rest[..end].parse::<usize>() {
                anchors.push(anchor);
            }
            cursor = &rest[end..];
        }
        assert_eq!(anchors, vec![0, 1, 2, 3]);
    }

    #[test]
    fn normalize_asset_key_resolves_relative_segments() {
        let key = normalize_asset_key("../Images/Cover.PNG?cache=1#frag");
        assert_eq!(key, "images/cover.png");
    }

    #[test]
    fn asset_resolver_matches_basename() {
        let mut resolver = AssetResolver::default();
        resolver.update(&[ReaderImageRef {
            raw_path: "images/cover.png".to_string(),
            local_path: "/tmp/cover.png".to_string(),
        }]);
        let resolved = resolver.resolve("cover.png");
        assert_eq!(resolved, Some(PathBuf::from("/tmp/cover.png")));
    }
}
