use super::{
    BROWSER_TAB_ASSETS_SUBDIR, BROWSER_TAB_FETCH_USER_AGENT, BROWSER_TAB_HTML_FILE,
    BROWSER_TAB_MANIFEST_FILE, BROWSER_TAB_MANIFEST_VERSION, BROWSER_TAB_RAW_HTML_FILE,
    BROWSER_TAB_TEXT_FILE, BROWSER_TABS_SUBDIR, cache_root,
};
use crate::browser_tabs::{BrowserTab, BrowserTabSnapshot};
use anyhow::Result as AnyhowResult;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::Url;
use reqwest::header::{ACCEPT, ACCEPT_LANGUAGE, REFERER, USER_AGENT};
use scraper::{ElementRef, Html, Selector};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{debug, info, warn};

static RE_LINK_TAG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?is)<link\b[^>]*>"#).expect("valid browser tab link tag regex"));
static RE_HTML_ATTR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?is)\b([a-zA-Z_:][-a-zA-Z0-9_:.]*)\s*=\s*["']([^"']*)["']"#)
        .expect("valid html attr regex")
});
static RE_IMG_SRC: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?is)<img\b[^>]*?\bsrc\s*=\s*["']([^"']+)["'][^>]*>"#)
        .expect("valid browser tab image regex")
});
static RE_SVG_IMAGE_HREF: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?is)<image\b[^>]*?\b(?:xlink:href|href)\s*=\s*["']([^"']+)["'][^>]*>"#)
        .expect("valid browser tab svg image regex")
});
static RE_SOURCE_SRC: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?is)<source\b[^>]*?\bsrc\s*=\s*["']([^"']+)["'][^>]*>"#)
        .expect("valid browser tab source src regex")
});
static RE_SOURCE_SRCSET: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?is)<source\b[^>]*?\bsrcset\s*=\s*["']([^"']+)["'][^>]*>"#)
        .expect("valid browser tab source srcset regex")
});
static RE_STYLE_ATTR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?is)\bstyle\s*=\s*["']([^"']+)["']"#)
        .expect("valid browser tab style attr regex")
});
static RE_STYLE_BLOCK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?is)<style\b[^>]*>(.*?)</style>"#).expect("valid browser tab style block regex")
});
static RE_CSS_URL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?is)url\(([^)]+)\)"#).expect("valid css url regex"));

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BrowserTabSourceManifest {
    #[serde(default)]
    pub manifest_version: u32,
    pub tab_id: u64,
    pub window_id: Option<u64>,
    pub title: String,
    pub url: String,
    pub lang: Option<String>,
    pub ready_state: Option<String>,
    pub captured_at: Option<String>,
    pub favicon_url: Option<String>,
    pub active: Option<bool>,
    pub audible: Option<bool>,
    pub pinned: Option<bool>,
    #[serde(default)]
    pub raw_html_path: Option<PathBuf>,
    pub html_path: PathBuf,
    pub text_path: PathBuf,
    #[serde(default)]
    pub asset_dir: Option<PathBuf>,
    #[serde(default)]
    pub assets: Vec<BrowserTabAsset>,
    #[serde(default)]
    pub html_truncated: bool,
    #[serde(default)]
    pub text_truncated: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BrowserTabAsset {
    pub raw_path: String,
    pub local_path: PathBuf,
    #[serde(default)]
    pub kind: String,
}

pub fn persist_browser_tab_source(
    snapshot: &BrowserTabSnapshot,
    tab: Option<&BrowserTab>,
) -> std::result::Result<PathBuf, String> {
    let stable_key = snapshot.tab_id.to_string();
    let mut hasher = Sha256::new();
    hasher.update(stable_key.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    let dir = cache_root().join(BROWSER_TABS_SUBDIR).join(&digest);
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;

    let html_path = dir.join(BROWSER_TAB_HTML_FILE);
    let raw_html_path = dir.join(BROWSER_TAB_RAW_HTML_FILE);
    let text_path = dir.join(BROWSER_TAB_TEXT_FILE);
    let manifest_path = dir.join(BROWSER_TAB_MANIFEST_FILE);
    let asset_dir = dir.join(BROWSER_TAB_ASSETS_SUBDIR);

    let html = snapshot
        .html
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(
            "<article><p>No structured HTML content was captured for this tab.</p></article>",
        );
    let prepared = prepare_browser_tab_bundle(html, snapshot.url.trim(), &asset_dir)
        .map_err(|err| err.to_string())?;
    let text = if !prepared.text.trim().is_empty() {
        prepared.text.as_str()
    } else {
        snapshot
            .text
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("No textual content found in this browser tab.")
    };

    fs::write(&raw_html_path, html).map_err(|err| err.to_string())?;
    fs::write(&html_path, prepared.html).map_err(|err| err.to_string())?;
    fs::write(&text_path, text).map_err(|err| err.to_string())?;

    let manifest = BrowserTabSourceManifest {
        manifest_version: BROWSER_TAB_MANIFEST_VERSION,
        tab_id: snapshot.tab_id,
        window_id: tab.map(|value| value.window_id),
        title: snapshot.title.trim().to_string(),
        url: snapshot.url.trim().to_string(),
        lang: snapshot.lang.clone(),
        ready_state: snapshot.ready_state.clone(),
        captured_at: snapshot.captured_at.clone(),
        favicon_url: tab.and_then(|value| value.fav_icon_url.clone()),
        active: tab.and_then(|value| value.active),
        audible: tab.and_then(|value| value.audible),
        pinned: tab.and_then(|value| value.pinned),
        raw_html_path: Some(raw_html_path.clone()),
        html_path: html_path.clone(),
        text_path: text_path.clone(),
        asset_dir: (!prepared.assets.is_empty()).then_some(asset_dir.clone()),
        assets: prepared.assets,
        html_truncated: snapshot.truncation.html.truncated,
        text_truncated: snapshot.truncation.text.truncated,
    };
    let manifest_raw = toml::to_string(&manifest).map_err(|err| err.to_string())?;
    fs::write(&manifest_path, manifest_raw).map_err(|err| err.to_string())?;

    info!(
        path = %manifest_path.display(),
        tab_id = snapshot.tab_id,
        title = %snapshot.title,
        url = %snapshot.url,
        html_chars = html.len(),
        text_chars = text.len(),
        asset_count = manifest.assets.len(),
        html_truncated = snapshot.truncation.html.truncated,
        text_truncated = snapshot.truncation.text.truncated,
        "Persisted browser-tab cache snapshot"
    );

    Ok(manifest_path)
}

#[derive(Debug, Default)]
struct PreparedBrowserTabBundle {
    html: String,
    text: String,
    assets: Vec<BrowserTabAsset>,
}

fn prepare_browser_tab_bundle(
    html: &str,
    base_url: &str,
    asset_dir: &Path,
) -> AnyhowResult<PreparedBrowserTabBundle> {
    let focused_html = focus_browser_tab_html(html, base_url);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent(BROWSER_TAB_FETCH_USER_AGENT)
        .build()?;
    let mut asset_map = HashMap::<String, BrowserTabAsset>::new();
    let html_with_styles =
        inline_browser_tab_stylesheets(&focused_html, base_url, asset_dir, &client, &mut asset_map);
    collect_browser_tab_html_assets(
        &html_with_styles,
        base_url,
        asset_dir,
        &client,
        &mut asset_map,
    );
    let text = browser_tab_text_from_html(&html_with_styles);
    Ok(PreparedBrowserTabBundle {
        html: html_with_styles,
        text,
        assets: asset_map.into_values().collect(),
    })
}

fn focus_browser_tab_html(raw_html: &str, page_url: &str) -> String {
    let document = Html::parse_document(raw_html);
    let head_styles = collect_browser_tab_head_nodes(&document, "style, link[rel~='stylesheet']");
    let title = collect_browser_tab_title(&document);
    let html_classes = collect_browser_tab_attr(&document, "html", "class");
    let body_classes = collect_browser_tab_attr(&document, "body", "class");
    let html_style = collect_browser_tab_attr(&document, "html", "style");
    let body_style = collect_browser_tab_attr(&document, "body", "style");

    let candidate_selectors = [
        ".mw-parser-output",
        ".mw-body-content",
        "main article",
        "main#content",
        "article",
        "[role='main']",
        "main",
        "#content",
        ".entry-content",
        ".post-content",
        ".article-content",
    ];
    let candidates = candidate_selectors
        .iter()
        .filter_map(|selector| select_browser_tab_candidate(&document, selector))
        .collect::<Vec<_>>();
    let focused = candidates
        .iter()
        .find(|candidate| candidate.text_len >= 600)
        .or_else(|| candidates.iter().max_by_key(|candidate| candidate.text_len))
        .map(|candidate| candidate.html.clone())
        .unwrap_or_else(|| raw_html.to_string());

    let mut classes = Vec::<String>::new();
    classes.push("ll-browser-tab-root".to_string());
    if let Some(value) = html_classes {
        classes.extend(value.split_whitespace().map(str::to_string));
    }
    if let Some(value) = body_classes {
        classes.extend(value.split_whitespace().map(str::to_string));
    }
    classes.sort();
    classes.dedup();
    let style = [html_style.as_deref(), body_style.as_deref()]
        .into_iter()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    let title_markup = title
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("<h1>{}</h1>", escape_html_attr(&value)))
        .unwrap_or_default();
    let style_attr = if style.trim().is_empty() {
        String::new()
    } else {
        format!(r#" style="{}""#, escape_html_attr(&style))
    };
    format!(
        r#"<div data-ll-browser-tab-focused="1" data-ll-page-url="{}" class="{}"{}>{head_styles}{title_markup}{focused}</div>"#,
        escape_html_attr(page_url),
        classes.join(" "),
        style_attr
    )
}

struct BrowserTabCandidate {
    html: String,
    text_len: usize,
}

fn select_browser_tab_candidate(
    document: &Html,
    selector_raw: &str,
) -> Option<BrowserTabCandidate> {
    let selector = Selector::parse(selector_raw).ok()?;
    let element = document.select(&selector).next()?;
    if element.value().name() == "article"
        && let Some(candidate) = refine_browser_tab_article_candidate(element)
    {
        return Some(candidate);
    }
    let element = refine_browser_tab_element(element);
    let text_len = browser_tab_element_text_len(&element);
    Some(BrowserTabCandidate {
        html: element.html(),
        text_len,
    })
}

fn refine_browser_tab_element<'a>(element: ElementRef<'a>) -> ElementRef<'a> {
    let parent_len = browser_tab_element_text_len(&element);
    let best_child = element
        .children()
        .filter_map(ElementRef::wrap)
        .filter(|child| matches!(child.value().name(), "section" | "main" | "article" | "div"))
        .map(|child| (browser_tab_element_text_len(&child), child))
        .max_by_key(|(len, _)| *len);
    if let Some((child_len, child)) = best_child
        && child_len >= 400
        && child_len * 2 >= parent_len
    {
        return child;
    }
    element
}

fn refine_browser_tab_article_candidate(element: ElementRef<'_>) -> Option<BrowserTabCandidate> {
    let parent_len = browser_tab_element_text_len(&element);
    let children = element
        .children()
        .filter_map(ElementRef::wrap)
        .collect::<Vec<_>>();
    let (main_idx, main_len) = children
        .iter()
        .enumerate()
        .filter(|(_, child)| matches!(child.value().name(), "section" | "main" | "article" | "div"))
        .map(|(idx, child)| (idx, browser_tab_element_text_len(child)))
        .max_by_key(|(_, len)| *len)?;
    if main_len < 400 || main_len * 2 < parent_len {
        return None;
    }
    let mut kept_children = String::new();
    for (idx, child) in children.iter().enumerate() {
        if idx > main_idx {
            break;
        }
        if idx == main_idx || !browser_tab_child_is_noise(child) {
            kept_children.push_str(&child.html());
        }
    }
    if kept_children.trim().is_empty() {
        return None;
    }
    let html = format!(
        "{}{}</article>",
        browser_tab_open_tag(&element),
        kept_children
    );
    Some(BrowserTabCandidate {
        text_len: parent_len.min(main_len + 800),
        html,
    })
}

fn browser_tab_open_tag(element: &ElementRef<'_>) -> String {
    let mut out = format!("<{}", element.value().name());
    for (name, value) in element.value().attrs() {
        out.push(' ');
        out.push_str(name);
        out.push_str(r#"=""#);
        out.push_str(&escape_html_attr(value));
        out.push('"');
    }
    out.push('>');
    out
}

fn browser_tab_child_is_noise(element: &ElementRef<'_>) -> bool {
    let tag = element.value().name();
    if matches!(tag, "nav" | "aside" | "footer" | "button") {
        return true;
    }
    let id = element
        .value()
        .attr("id")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let class_name = element
        .value()
        .attr("class")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let attrs = format!("{id} {class_name}");
    if [
        "sponsor",
        "comment",
        "recirculation",
        "editors-picks",
        "edpick",
        "bottom-sheet",
        "share",
        "newsletter",
        "promo",
        "subscribe",
        "toolbar",
    ]
    .iter()
    .any(|needle| attrs.contains(needle))
    {
        return true;
    }
    let text = element
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let lower = text.to_ascii_lowercase();
    text.len() < 200
        && (lower.contains("skip advertisement")
            || lower.contains("advertisement")
            || lower.contains("you have been granted access"))
}

fn browser_tab_element_text_len(element: &ElementRef<'_>) -> usize {
    element
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .len()
}

fn collect_browser_tab_title(document: &Html) -> Option<String> {
    let selector = Selector::parse("title").ok()?;
    document
        .select(&selector)
        .next()
        .map(|element| element.text().collect::<String>().trim().to_string())
}

fn collect_browser_tab_attr(document: &Html, selector_raw: &str, attr: &str) -> Option<String> {
    let selector = Selector::parse(selector_raw).ok()?;
    document
        .select(&selector)
        .next()
        .and_then(|element| element.value().attr(attr))
        .map(str::to_string)
}

fn collect_browser_tab_head_nodes(document: &Html, selector_raw: &str) -> String {
    let selector = match Selector::parse(selector_raw) {
        Ok(selector) => selector,
        Err(_) => return String::new(),
    };
    document
        .select(&selector)
        .map(|element| element.html())
        .collect::<Vec<_>>()
        .join("")
}

fn browser_tab_text_from_html(html: &str) -> String {
    match html2text::from_read(html.as_bytes(), 10_000) {
        Ok(text) => {
            let normalized = text
                .lines()
                .map(|line| line.trim())
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            if normalized.trim().is_empty() {
                "No textual content found in this browser tab.".to_string()
            } else {
                normalized
            }
        }
        Err(_) => "No textual content found in this browser tab.".to_string(),
    }
}

fn inline_browser_tab_stylesheets(
    html: &str,
    base_url: &str,
    asset_dir: &Path,
    client: &reqwest::blocking::Client,
    asset_map: &mut HashMap<String, BrowserTabAsset>,
) -> String {
    let mut out = String::with_capacity(html.len());
    let mut last = 0usize;
    for full in RE_LINK_TAG.find_iter(html) {
        let tag = full.as_str();
        let attrs = parse_html_attrs(tag);
        let rel = attrs
            .get("rel")
            .map(|value| value.to_ascii_lowercase())
            .unwrap_or_default();
        if !rel.split_whitespace().any(|value| value == "stylesheet") {
            continue;
        }
        let Some(href) = attrs.get("href").cloned() else {
            continue;
        };
        out.push_str(&html[last..full.start()]);
        let replacement = resolve_browser_tab_url(&href, base_url)
            .and_then(|stylesheet_url| fetch_stylesheet_text(client, &stylesheet_url))
            .map(|(stylesheet_url, css)| {
                let rewritten = rewrite_css_urls_for_import(
                    &css,
                    &stylesheet_url,
                    asset_dir,
                    client,
                    asset_map,
                );
                format!(
                    "<style data-ll-origin-href=\"{}\">{}</style>",
                    escape_html_attr(&stylesheet_url),
                    rewritten
                )
            })
            .unwrap_or_default();
        out.push_str(&replacement);
        last = full.end();
    }
    out.push_str(&html[last..]);
    out
}

fn parse_html_attrs(tag: &str) -> HashMap<String, String> {
    RE_HTML_ATTR
        .captures_iter(tag)
        .filter_map(|caps| {
            let name = caps.get(1)?.as_str().trim().to_ascii_lowercase();
            let value = decode_html_entities(caps.get(2)?.as_str());
            Some((name, value))
        })
        .collect()
}

fn collect_browser_tab_html_assets(
    html: &str,
    base_url: &str,
    asset_dir: &Path,
    client: &reqwest::blocking::Client,
    asset_map: &mut HashMap<String, BrowserTabAsset>,
) {
    for captures in RE_IMG_SRC.captures_iter(html) {
        if let Some(raw) = captures.get(1).map(|value| value.as_str()) {
            let _ = fetch_browser_tab_asset(raw, base_url, "image", asset_dir, client, asset_map);
        }
    }
    for captures in RE_SVG_IMAGE_HREF.captures_iter(html) {
        if let Some(raw) = captures.get(1).map(|value| value.as_str()) {
            let _ = fetch_browser_tab_asset(raw, base_url, "image", asset_dir, client, asset_map);
        }
    }
    for captures in RE_SOURCE_SRC.captures_iter(html) {
        if let Some(raw) = captures.get(1).map(|value| value.as_str()) {
            let _ = fetch_browser_tab_asset(raw, base_url, "image", asset_dir, client, asset_map);
        }
    }
    for captures in RE_SOURCE_SRCSET.captures_iter(html) {
        if let Some(raw) = captures.get(1).map(|value| value.as_str()) {
            for candidate in parse_srcset_urls(raw) {
                let _ = fetch_browser_tab_asset(
                    &candidate, base_url, "image", asset_dir, client, asset_map,
                );
            }
        }
    }
    for captures in RE_STYLE_ATTR.captures_iter(html) {
        if let Some(css) = captures.get(1).map(|value| value.as_str()) {
            let _ = rewrite_css_urls_for_import(css, base_url, asset_dir, client, asset_map);
        }
    }
    for captures in RE_STYLE_BLOCK.captures_iter(html) {
        if let Some(css) = captures.get(1).map(|value| value.as_str()) {
            let _ = rewrite_css_urls_for_import(css, base_url, asset_dir, client, asset_map);
        }
    }
}

fn rewrite_css_urls_for_import(
    css: &str,
    stylesheet_url: &str,
    asset_dir: &Path,
    client: &reqwest::blocking::Client,
    asset_map: &mut HashMap<String, BrowserTabAsset>,
) -> String {
    RE_CSS_URL
        .replace_all(css, |caps: &regex::Captures<'_>| {
            let raw = caps
                .get(1)
                .map(|value| value.as_str())
                .unwrap_or_default()
                .trim()
                .trim_matches('\'')
                .trim_matches('"');
            if raw.is_empty() || raw.starts_with("data:") || raw.starts_with('#') {
                return format!("url({raw})");
            }
            let absolute =
                resolve_browser_tab_url(raw, stylesheet_url).unwrap_or_else(|| raw.to_string());
            let _ = fetch_browser_tab_asset(
                &absolute,
                stylesheet_url,
                "image",
                asset_dir,
                client,
                asset_map,
            );
            format!("url(\"{}\")", absolute)
        })
        .into_owned()
}

fn fetch_stylesheet_text(
    client: &reqwest::blocking::Client,
    stylesheet_url: &str,
) -> Option<(String, String)> {
    let response = browser_tab_request(client, stylesheet_url, stylesheet_url)
        .send()
        .ok()?;
    if !response.status().is_success() {
        warn!(url = %stylesheet_url, status = %response.status(), "Browser tab stylesheet fetch failed");
        return None;
    }
    let css = response.text().ok()?;
    Some((stylesheet_url.to_string(), css))
}

fn fetch_browser_tab_asset(
    raw: &str,
    base_url: &str,
    kind: &str,
    asset_dir: &Path,
    client: &reqwest::blocking::Client,
    asset_map: &mut HashMap<String, BrowserTabAsset>,
) -> Option<BrowserTabAsset> {
    let absolute = resolve_browser_tab_url(raw, base_url)?;
    if absolute.starts_with("data:") {
        return None;
    }
    if let Some(existing) = asset_map.get(&absolute) {
        return Some(existing.clone());
    }
    fs::create_dir_all(asset_dir).ok()?;
    let response = browser_tab_request(client, &absolute, base_url)
        .send()
        .ok()?;
    if !response.status().is_success() {
        warn!(url = %absolute, status = %response.status(), kind, "Browser tab asset fetch failed");
        return None;
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());
    let bytes = response.bytes().ok()?;
    let output = browser_tab_asset_output_path(asset_dir, &absolute, content_type.as_deref());
    if let Some(parent) = output.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&output, &bytes).ok()?;
    let asset = BrowserTabAsset {
        raw_path: absolute.clone(),
        local_path: output,
        kind: kind.to_string(),
    };
    asset_map.insert(absolute, asset.clone());
    Some(asset)
}

fn resolve_browser_tab_url(raw: &str, base_url: &str) -> Option<String> {
    let trimmed = decode_html_entities(raw);
    let trimmed = trimmed.trim().trim_matches('\'').trim_matches('"');
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    if trimmed.starts_with("data:") {
        return Some(trimmed.to_string());
    }
    if let Ok(url) = Url::parse(trimmed) {
        return Some(url.to_string());
    }
    let base = Url::parse(base_url).ok()?;
    base.join(trimmed).ok().map(|value| value.to_string())
}

fn browser_tab_request<'a>(
    client: &'a reqwest::blocking::Client,
    url: &'a str,
    referer: &'a str,
) -> reqwest::blocking::RequestBuilder {
    client
        .get(url)
        .header(USER_AGENT, BROWSER_TAB_FETCH_USER_AGENT)
        .header(ACCEPT, "*/*")
        .header(ACCEPT_LANGUAGE, "en-US,en;q=0.9")
        .header(REFERER, decode_html_entities(referer))
}

fn decode_html_entities(raw: &str) -> String {
    raw.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn parse_srcset_urls(raw: &str) -> Vec<String> {
    raw.split(',')
        .filter_map(|part| {
            let candidate = part.trim().split_whitespace().next()?.trim();
            (!candidate.is_empty()).then_some(candidate.to_string())
        })
        .collect()
}

fn browser_tab_asset_output_path(
    asset_dir: &Path,
    raw_url: &str,
    content_type: Option<&str>,
) -> PathBuf {
    let digest = {
        let mut hasher = Sha256::new();
        hasher.update(raw_url.as_bytes());
        format!("{:x}", hasher.finalize())
    };
    let parsed = Url::parse(raw_url).ok();
    let name = parsed
        .as_ref()
        .and_then(|url| {
            url.path_segments()
                .and_then(|mut segments| segments.next_back())
                .filter(|segment| !segment.is_empty())
                .map(|segment| segment.to_string())
        })
        .unwrap_or_else(|| "asset".to_string());
    let safe_name = sanitize_browser_tab_asset_name(&name);
    let ext = Path::new(&safe_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_string())
        .or_else(|| browser_tab_extension_from_content_type(content_type).map(str::to_string));
    let stem = Path::new(&safe_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("asset");
    let file_name = if let Some(ext) = ext {
        format!("{stem}-{digest}.{ext}")
    } else {
        format!("{stem}-{digest}")
    };
    asset_dir.join(file_name)
}

fn sanitize_browser_tab_asset_name(raw: &str) -> String {
    let trimmed = raw.trim();
    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "asset".to_string()
    } else {
        out
    }
}

fn browser_tab_extension_from_content_type(content_type: Option<&str>) -> Option<&'static str> {
    let mime = content_type?.split(';').next()?.trim().to_ascii_lowercase();
    match mime.as_str() {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/jpg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/svg+xml" => Some("svg"),
        _ => None,
    }
}

fn escape_html_attr(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn load_browser_tab_manifest(source_path: &Path) -> Option<BrowserTabSourceManifest> {
    if !is_browser_tab_manifest(source_path) {
        return None;
    }
    let raw = fs::read_to_string(source_path).ok()?;
    let manifest = toml::from_str::<BrowserTabSourceManifest>(&raw).ok()?;
    debug!(
        path = %source_path.display(),
        manifest_version = manifest.manifest_version,
        tab_id = manifest.tab_id,
        url = %manifest.url,
        "Loaded browser-tab manifest"
    );
    Some(manifest)
}

pub fn rehydrate_browser_tab_manifest_assets(
    source_path: &Path,
) -> std::result::Result<(), String> {
    let mut manifest = load_browser_tab_manifest(source_path)
        .ok_or_else(|| format!("Not a browser-tab manifest: {}", source_path.display()))?;
    if manifest.manifest_version >= BROWSER_TAB_MANIFEST_VERSION && !manifest.assets.is_empty() {
        return Ok(());
    }
    let raw_html_path = manifest
        .raw_html_path
        .clone()
        .unwrap_or_else(|| manifest.html_path.clone());
    let html = fs::read_to_string(&raw_html_path).map_err(|err| err.to_string())?;
    let asset_dir = manifest.asset_dir.clone().unwrap_or_else(|| {
        manifest
            .html_path
            .parent()
            .unwrap_or(source_path)
            .join(BROWSER_TAB_ASSETS_SUBDIR)
    });
    let prepared = prepare_browser_tab_bundle(&html, &manifest.url, &asset_dir)
        .map_err(|err| err.to_string())?;
    if manifest.raw_html_path.is_none() {
        fs::write(&raw_html_path, &html).map_err(|err| err.to_string())?;
    }
    fs::write(&manifest.html_path, prepared.html).map_err(|err| err.to_string())?;
    fs::write(&manifest.text_path, prepared.text).map_err(|err| err.to_string())?;
    manifest.manifest_version = BROWSER_TAB_MANIFEST_VERSION;
    manifest.raw_html_path = Some(raw_html_path);
    manifest.asset_dir = (!prepared.assets.is_empty()).then_some(asset_dir);
    manifest.assets = prepared.assets;
    let raw = toml::to_string(&manifest).map_err(|err| err.to_string())?;
    fs::write(source_path, raw).map_err(|err| err.to_string())?;
    info!(
        path = %source_path.display(),
        manifest_version = manifest.manifest_version,
        asset_count = manifest.assets.len(),
        url = %manifest.url,
        "Rehydrated browser-tab cache bundle from stored snapshot"
    );
    Ok(())
}

pub fn is_browser_tab_manifest(source_path: &Path) -> bool {
    source_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("lltab"))
        .unwrap_or(false)
}
