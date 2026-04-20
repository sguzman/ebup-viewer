use eframe::egui::{Color32, FontFamily, FontId};
use lanternleaf_core::config;
use lanternleaf_core::session::ReaderImageRef;
use pulldown_cmark as cmark;
use scraper::Html;
use std::path::PathBuf;
use std::time::Instant;
use tracing::{debug, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrettySourceKind {
    Markdown,
    Html,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PrettyStyle {
    pub bold: bool,
    pub italics: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub code: bool,
    pub sup: bool,
    pub sub: bool,
    pub font_scale: Option<f32>,
    pub color: Option<Color32>,
    pub is_link: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrettySpan {
    pub text: String,
    pub style: PrettyStyle,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrettyCell {
    pub spans: Vec<PrettySpan>,
    pub header: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrettyBlockKind {
    Heading {
        level: u8,
    },
    Paragraph,
    ListItem {
        depth: u8,
        ordered: bool,
        index: Option<usize>,
    },
    HorizontalRule,
    CodeBlock,
    BlockQuote,
    Image,
    Table,
}

#[derive(Debug, Clone)]
pub struct PrettyBlock {
    pub kind: PrettyBlockKind,
    pub spans: Vec<PrettySpan>,
    pub code: Option<String>,
    pub image: Option<PrettyImage>,
    pub table: Option<Vec<Vec<PrettyCell>>>,
    pub anchor_idx: usize,
    pub source_kind: PrettySourceKind,
}

#[derive(Debug, Clone)]
pub struct PrettyImage {
    pub src_raw: String,
    pub local_path: PathBuf,
    pub alt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrettyPageCacheKey {
    pub source_path: String,
    pub page: usize,
    pub pretty_kind: lanternleaf_app::contracts::PrettyKind,
    pub text_only: bool,
}

#[derive(Debug, Clone)]
enum FlowItem {
    Span(PrettySpan),
    Image(PrettyImage),
}

pub fn resolve_image_path(src_raw: &str, images: &[ReaderImageRef]) -> Option<PathBuf> {
    if let Some(found) = images
        .iter()
        .find(|img| img.raw_path.as_str() == src_raw)
        .or_else(|| images.iter().find(|img| img.local_path.as_str() == src_raw))
    {
        return Some(PathBuf::from(&found.local_path));
    }

    let candidate = PathBuf::from(src_raw);
    if candidate.is_absolute() && candidate.exists() {
        return Some(candidate);
    }
    if candidate.exists() {
        return Some(candidate);
    }
    None
}

pub fn link_color32(link: config::HighlightColor) -> Color32 {
    Color32::from_rgba_premultiplied(
        (link.r.clamp(0.0, 1.0) * 255.0) as u8,
        (link.g.clamp(0.0, 1.0) * 255.0) as u8,
        (link.b.clamp(0.0, 1.0) * 255.0) as u8,
        (link.a.clamp(0.0, 1.0) * 255.0) as u8,
    )
}

pub fn clamp_image_size(
    available_width: f32,
    original: [usize; 2],
    max_width_pct: f32,
    max_height_px: f32,
) -> [f32; 2] {
    let max_w = (available_width * (max_width_pct.clamp(10.0, 100.0) / 100.0)).max(32.0);
    let max_h = max_height_px.clamp(64.0, 4096.0).max(32.0);
    let ow = original[0].max(1) as f32;
    let oh = original[1].max(1) as f32;
    let scale = (max_w / ow).min(max_h / oh).min(1.0);
    [ow * scale, oh * scale]
}

pub fn font_id_for(
    base_px: f32,
    style: PrettyStyle,
    regular_family: FontFamily,
    bold_family: FontFamily,
    mono_regular: FontFamily,
    mono_bold: FontFamily,
) -> FontId {
    let mut size = base_px.max(6.0);
    if let Some(scale) = style.font_scale {
        size *= scale.clamp(0.25, 6.0);
    }
    if style.sup || style.sub {
        size *= 0.75;
    }
    let is_code = style.code;
    let wants_bold = style.bold;
    let family = if is_code {
        if wants_bold { mono_bold } else { mono_regular }
    } else if wants_bold {
        bold_family
    } else {
        regular_family
    };
    FontId::new(size, family)
}

pub fn anchor_idx_for_block(block_index: usize) -> usize {
    // Keep this stable and simple for now: anchor map indices are block indices.
    block_index
}

pub fn markdown_to_blocks(
    markdown: &str,
    images: &[ReaderImageRef],
    pretty_cfg: config::PrettyUiConfig,
) -> Vec<PrettyBlock> {
    let start = Instant::now();
    let link_color = link_color32(pretty_cfg.link_color);
    let mut blocks = Vec::new();

    let mut block_index = 0usize;
    let mut current_spans: Vec<PrettySpan> = Vec::new();
    let mut current_code: Option<String> = None;
    let mut current_kind: Option<PrettyBlockKind> = None;
    let mut style_stack: Vec<PrettyStyle> = vec![PrettyStyle::default()];

    let mut list_stack: Vec<(bool, usize)> = Vec::new(); // (ordered, next_index)

    // Inline image state (pulldown-cmark reports image alt as Text between Start/End).
    let mut pending_image_src: Option<String> = None;
    let mut pending_image_title: Option<String> = None;
    let mut pending_image_alt: String = String::new();

    // Table state
    let mut table_rows: Vec<Vec<PrettyCell>> = Vec::new();
    let mut table_row: Vec<PrettyCell> = Vec::new();
    let mut table_cell_spans: Vec<PrettySpan> = Vec::new();
    let mut in_table_head = false;
    let mut in_table = false;

    let mut spans_count = 0usize;
    let mut images_count = 0usize;
    let mut tables_count = 0usize;

    let mut options = cmark::Options::empty();
    options.insert(cmark::Options::ENABLE_TABLES);
    options.insert(cmark::Options::ENABLE_STRIKETHROUGH);
    let parser = cmark::Parser::new_ext(markdown, options);

    let mut empty_spans: Vec<PrettySpan> = Vec::new();

    for event in parser {
        match event {
            cmark::Event::Start(tag) => match tag {
                cmark::Tag::Paragraph => {
                    current_kind = Some(PrettyBlockKind::Paragraph);
                }
                cmark::Tag::Heading { level, .. } => {
                    current_kind = Some(PrettyBlockKind::Heading { level: level as u8 });
                }
                cmark::Tag::BlockQuote(_) => {
                    current_kind = Some(PrettyBlockKind::BlockQuote);
                }
                cmark::Tag::List(start) => {
                    let ordered = start.is_some();
                    let next = start.unwrap_or(1) as usize;
                    list_stack.push((ordered, next));
                }
                cmark::Tag::Item => {
                    let depth = list_stack.len().min(u8::MAX as usize) as u8;
                    if let Some((ordered, next)) = list_stack.last_mut() {
                        let ordered_flag = *ordered;
                        let idx = if ordered_flag {
                            let current = *next;
                            *next = next.saturating_add(1);
                            Some(current)
                        } else {
                            None
                        };
                        current_kind = Some(PrettyBlockKind::ListItem {
                            depth,
                            ordered: ordered_flag,
                            index: idx,
                        });
                    } else {
                        current_kind = Some(PrettyBlockKind::ListItem {
                            depth: 1,
                            ordered: false,
                            index: None,
                        });
                    }
                }
                cmark::Tag::Emphasis => {
                    let mut s = *style_stack.last().unwrap_or(&PrettyStyle::default());
                    s.italics = true;
                    style_stack.push(s);
                }
                cmark::Tag::Strong => {
                    let mut s = *style_stack.last().unwrap_or(&PrettyStyle::default());
                    s.bold = true;
                    style_stack.push(s);
                }
                cmark::Tag::Strikethrough => {
                    let mut s = *style_stack.last().unwrap_or(&PrettyStyle::default());
                    s.strikethrough = true;
                    style_stack.push(s);
                }
                cmark::Tag::Superscript => {
                    let mut s = *style_stack.last().unwrap_or(&PrettyStyle::default());
                    s.sup = true;
                    style_stack.push(s);
                }
                cmark::Tag::Subscript => {
                    let mut s = *style_stack.last().unwrap_or(&PrettyStyle::default());
                    s.sub = true;
                    style_stack.push(s);
                }
                cmark::Tag::CodeBlock(_) => {
                    current_kind = Some(PrettyBlockKind::CodeBlock);
                    current_code = Some(String::new());
                }
                cmark::Tag::Link { .. } => {
                    let mut s = *style_stack.last().unwrap_or(&PrettyStyle::default());
                    s.is_link = true;
                    style_stack.push(s);
                }
                cmark::Tag::Image {
                    dest_url, title, ..
                } => {
                    pending_image_src = Some(dest_url.to_string());
                    pending_image_title = if title.is_empty() {
                        None
                    } else {
                        Some(title.to_string())
                    };
                    pending_image_alt.clear();
                }
                cmark::Tag::Table(_) => {
                    in_table = true;
                    tables_count += 1;
                    table_rows.clear();
                }
                cmark::Tag::TableHead => {
                    in_table_head = true;
                }
                cmark::Tag::TableRow => {
                    table_row = Vec::new();
                }
                cmark::Tag::TableCell => {
                    table_cell_spans = Vec::new();
                }
                _ => {}
            },
            cmark::Event::End(tag) => match tag {
                cmark::TagEnd::Paragraph => {
                    finish_block_markdown(
                        &mut blocks,
                        &mut block_index,
                        PrettyBlockKind::Paragraph,
                        &mut current_spans,
                        &mut current_code,
                        None,
                        None,
                    );
                    current_kind = None;
                }
                cmark::TagEnd::Heading(_) => {
                    let kind = current_kind
                        .take()
                        .unwrap_or(PrettyBlockKind::Heading { level: 1 });
                    finish_block_markdown(
                        &mut blocks,
                        &mut block_index,
                        kind,
                        &mut current_spans,
                        &mut current_code,
                        None,
                        None,
                    );
                }
                cmark::TagEnd::BlockQuote(_) => {
                    finish_block_markdown(
                        &mut blocks,
                        &mut block_index,
                        PrettyBlockKind::BlockQuote,
                        &mut current_spans,
                        &mut current_code,
                        None,
                        None,
                    );
                    current_kind = None;
                }
                cmark::TagEnd::Item => {
                    let kind = current_kind.take().unwrap_or(PrettyBlockKind::ListItem {
                        depth: 1,
                        ordered: false,
                        index: None,
                    });
                    finish_block_markdown(
                        &mut blocks,
                        &mut block_index,
                        kind,
                        &mut current_spans,
                        &mut current_code,
                        None,
                        None,
                    );
                }
                cmark::TagEnd::List(_) => {
                    let _ = list_stack.pop();
                }
                cmark::TagEnd::Emphasis
                | cmark::TagEnd::Strong
                | cmark::TagEnd::Strikethrough
                | cmark::TagEnd::Superscript
                | cmark::TagEnd::Subscript
                | cmark::TagEnd::Link => {
                    if style_stack.len() > 1 {
                        style_stack.pop();
                    }
                }
                cmark::TagEnd::Image => {
                    // Flush any preceding spans so inline images don't discard surrounding text.
                    let Some(src) = pending_image_src.take() else {
                        continue;
                    };
                    let alt_text = pending_image_alt.trim();
                    let alt = if !alt_text.is_empty() {
                        Some(alt_text.to_string())
                    } else {
                        pending_image_title.take()
                    };
                    pending_image_title = None;

                    if current_spans.iter().any(|s| !s.text.trim().is_empty()) {
                        if let Some(kind) = current_kind.clone() {
                            finish_block_markdown(
                                &mut blocks,
                                &mut block_index,
                                kind.clone(),
                                &mut current_spans,
                                &mut current_code,
                                None,
                                None,
                            );
                            // Re-enter the same block kind after the inline flush.
                            current_kind = Some(kind);
                        }
                    } else {
                        current_spans.clear();
                    }

                    if let Some(local_path) = resolve_image_path(&src, images) {
                        images_count += 1;
                        finish_block_markdown(
                            &mut blocks,
                            &mut block_index,
                            PrettyBlockKind::Image,
                            &mut empty_spans,
                            &mut current_code,
                            Some(PrettyImage {
                                src_raw: src,
                                local_path,
                                alt,
                            }),
                            None,
                        );
                    } else {
                        warn!(src, "Markdown image could not be resolved");
                    }
                }
                cmark::TagEnd::CodeBlock => {
                    finish_block_markdown(
                        &mut blocks,
                        &mut block_index,
                        PrettyBlockKind::CodeBlock,
                        &mut empty_spans,
                        &mut current_code,
                        None,
                        None,
                    );
                    current_kind = None;
                }
                cmark::TagEnd::Table => {
                    in_table = false;
                    finish_block_markdown(
                        &mut blocks,
                        &mut block_index,
                        PrettyBlockKind::Table,
                        &mut empty_spans,
                        &mut current_code,
                        None,
                        Some(std::mem::take(&mut table_rows)),
                    );
                }
                cmark::TagEnd::TableHead => {
                    in_table_head = false;
                }
                cmark::TagEnd::TableRow => {
                    if in_table {
                        table_rows.push(std::mem::take(&mut table_row));
                    }
                }
                cmark::TagEnd::TableCell => {
                    let cell = PrettyCell {
                        spans: std::mem::take(&mut table_cell_spans),
                        header: in_table_head,
                    };
                    table_row.push(cell);
                }
                _ => {}
            },
            cmark::Event::Text(text) => {
                if pending_image_src.is_some() {
                    pending_image_alt.push_str(&text);
                    continue;
                }
                if let Some(code) = current_code.as_mut() {
                    code.push_str(&text);
                } else if in_table {
                    let mut style = *style_stack.last().unwrap_or(&PrettyStyle::default());
                    if style.is_link {
                        style.color = Some(link_color);
                        style.underline = true;
                    }
                    table_cell_spans.push(PrettySpan {
                        text: text.to_string(),
                        style,
                    });
                    spans_count += 1;
                } else {
                    push_span_text(
                        &mut current_spans,
                        &style_stack,
                        link_color,
                        &mut spans_count,
                        &text,
                    );
                }
            }
            cmark::Event::Code(code) => {
                let mut style = *style_stack.last().unwrap_or(&PrettyStyle::default());
                style.code = true;
                current_spans.push(PrettySpan {
                    text: code.to_string(),
                    style,
                });
                spans_count += 1;
            }
            cmark::Event::SoftBreak => {
                if pending_image_src.is_some() {
                    pending_image_alt.push(' ');
                    continue;
                }
                push_span_text(
                    &mut current_spans,
                    &style_stack,
                    link_color,
                    &mut spans_count,
                    " ",
                );
            }
            cmark::Event::HardBreak => {
                if pending_image_src.is_some() {
                    pending_image_alt.push('\n');
                    continue;
                }
                push_span_text(
                    &mut current_spans,
                    &style_stack,
                    link_color,
                    &mut spans_count,
                    "\n",
                );
            }
            cmark::Event::Rule => {
                finish_block_markdown(
                    &mut blocks,
                    &mut block_index,
                    PrettyBlockKind::HorizontalRule,
                    &mut empty_spans,
                    &mut current_code,
                    None,
                    None,
                );
            }
            _ => {}
        }
    }

    let elapsed_ms = start.elapsed().as_millis();
    debug!(
        blocks = blocks.len(),
        spans = spans_count,
        images = images_count,
        tables = tables_count,
        elapsed_ms,
        "Converted markdown to pretty blocks"
    );
    blocks
}

fn push_span_text(
    spans: &mut Vec<PrettySpan>,
    style_stack: &[PrettyStyle],
    link_color: Color32,
    spans_count: &mut usize,
    text: &str,
) {
    if text.is_empty() {
        return;
    }
    let mut style = *style_stack.last().unwrap_or(&PrettyStyle::default());
    if style.is_link {
        style.color = Some(link_color);
        style.underline = true;
    }
    spans.push(PrettySpan {
        text: text.to_string(),
        style,
    });
    *spans_count = spans_count.saturating_add(1);
}

fn finish_block_markdown(
    blocks: &mut Vec<PrettyBlock>,
    block_index: &mut usize,
    kind: PrettyBlockKind,
    spans: &mut Vec<PrettySpan>,
    code: &mut Option<String>,
    image: Option<PrettyImage>,
    table: Option<Vec<Vec<PrettyCell>>>,
) {
    if matches!(
        kind,
        PrettyBlockKind::Paragraph | PrettyBlockKind::Heading { .. }
    ) && spans.iter().all(|s| s.text.trim().is_empty())
    {
        spans.clear();
        *code = None;
        return;
    }
    blocks.push(PrettyBlock {
        kind,
        spans: std::mem::take(spans),
        code: code.take(),
        image,
        table,
        anchor_idx: anchor_idx_for_block(*block_index),
        source_kind: PrettySourceKind::Markdown,
    });
    *block_index = block_index.saturating_add(1);
}

pub fn html_to_blocks(
    html: &str,
    images: &[ReaderImageRef],
    pretty_cfg: config::PrettyUiConfig,
) -> Vec<PrettyBlock> {
    let start = Instant::now();
    let link_color = link_color32(pretty_cfg.link_color);
    let fragment = Html::parse_fragment(html);
    let mut blocks: Vec<PrettyBlock> = Vec::new();
    let mut block_index = 0usize;
    let mut table_count = 0usize;
    let mut image_count = 0usize;
    let mut span_count = 0usize;
    let mut unsupported: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    let root = fragment.tree.root();
    walk_scraper_children(
        root,
        images,
        link_color,
        0,
        &mut blocks,
        &mut block_index,
        &mut span_count,
        &mut image_count,
        &mut table_count,
        &mut unsupported,
    );

    if !unsupported.is_empty() {
        let mut pairs: Vec<(String, usize)> = unsupported.into_iter().collect();
        pairs.sort_by(|a, b| b.1.cmp(&a.1));
        let top: Vec<String> = pairs
            .into_iter()
            .take(6)
            .map(|(tag, count)| format!("{tag}:{count}"))
            .collect();
        warn!(
            tags = top.join(","),
            "HTML pretty parser encountered unsupported tags"
        );
    }

    debug!(
        blocks = blocks.len(),
        spans = span_count,
        images = image_count,
        tables = table_count,
        elapsed_ms = start.elapsed().as_millis(),
        "Converted html to pretty blocks"
    );
    blocks
}

fn walk_scraper_children(
    node: ego_tree::NodeRef<'_, scraper::Node>,
    images: &[ReaderImageRef],
    link_color: Color32,
    list_depth: u8,
    out: &mut Vec<PrettyBlock>,
    block_index: &mut usize,
    span_count: &mut usize,
    image_count: &mut usize,
    table_count: &mut usize,
    unsupported: &mut std::collections::HashMap<String, usize>,
) {
    for child in node.children() {
        if let Some(element) = child.value().as_element() {
            let tag = element.name();
            match tag {
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    let level = tag[1..].parse::<u8>().unwrap_or(1);
                    let items = collect_scraper_flow_items(
                        child,
                        PrettyStyle::default(),
                        images,
                        link_color,
                        span_count,
                        image_count,
                        false,
                    );
                    push_flow_items_as_blocks_with_kind(
                        items,
                        PrettyBlockKind::Heading { level },
                        out,
                        block_index,
                    );
                    continue;
                }
                "p" => {
                    let items = collect_scraper_flow_items(
                        child,
                        PrettyStyle::default(),
                        images,
                        link_color,
                        span_count,
                        image_count,
                        false,
                    );
                    push_flow_items_as_blocks_with_kind(
                        items,
                        PrettyBlockKind::Paragraph,
                        out,
                        block_index,
                    );
                    continue;
                }
                "hr" => {
                    out.push(PrettyBlock {
                        kind: PrettyBlockKind::HorizontalRule,
                        spans: Vec::new(),
                        code: None,
                        image: None,
                        table: None,
                        anchor_idx: anchor_idx_for_block(*block_index),
                        source_kind: PrettySourceKind::Html,
                    });
                    *block_index = block_index.saturating_add(1);
                    continue;
                }
                "blockquote" => {
                    let items = collect_scraper_flow_items(
                        child,
                        PrettyStyle::default(),
                        images,
                        link_color,
                        span_count,
                        image_count,
                        false,
                    );
                    push_flow_items_as_blocks_with_kind(
                        items,
                        PrettyBlockKind::BlockQuote,
                        out,
                        block_index,
                    );
                    continue;
                }
                "pre" => {
                    let code = collect_scraper_text(child).trim().to_string();
                    out.push(PrettyBlock {
                        kind: PrettyBlockKind::CodeBlock,
                        spans: Vec::new(),
                        code: Some(code),
                        image: None,
                        table: None,
                        anchor_idx: anchor_idx_for_block(*block_index),
                        source_kind: PrettySourceKind::Html,
                    });
                    *block_index = block_index.saturating_add(1);
                    continue;
                }
                "img" => {
                    if let Some(src) = element.attr("src") {
                        if let Some(local_path) = resolve_image_path(src, images) {
                            let alt = element.attr("alt").map(|s| s.to_string());
                            *image_count = image_count.saturating_add(1);
                            out.push(PrettyBlock {
                                kind: PrettyBlockKind::Image,
                                spans: Vec::new(),
                                code: None,
                                image: Some(PrettyImage {
                                    src_raw: src.to_string(),
                                    local_path,
                                    alt,
                                }),
                                table: None,
                                anchor_idx: anchor_idx_for_block(*block_index),
                                source_kind: PrettySourceKind::Html,
                            });
                            *block_index = block_index.saturating_add(1);
                        }
                    }
                    continue;
                }
                "ul" | "ol" => {
                    let ordered = tag == "ol";
                    let start_at = if ordered {
                        element
                            .attr("start")
                            .and_then(|s| s.parse::<usize>().ok())
                            .unwrap_or(1)
                    } else {
                        1
                    };
                    parse_scraper_list(
                        child,
                        images,
                        link_color,
                        list_depth.saturating_add(1),
                        ordered,
                        start_at,
                        out,
                        block_index,
                        span_count,
                        image_count,
                    );
                    continue;
                }
                "table" => {
                    let table = parse_scraper_table(child, images, link_color, span_count);
                    *table_count = table_count.saturating_add(1);
                    out.push(PrettyBlock {
                        kind: PrettyBlockKind::Table,
                        spans: Vec::new(),
                        code: None,
                        image: None,
                        table: Some(table),
                        anchor_idx: anchor_idx_for_block(*block_index),
                        source_kind: PrettySourceKind::Html,
                    });
                    *block_index = block_index.saturating_add(1);
                    continue;
                }
                _ => {}
            }
            *unsupported.entry(tag.to_string()).or_insert(0) += 1;
        }
        walk_scraper_children(
            child,
            images,
            link_color,
            list_depth,
            out,
            block_index,
            span_count,
            image_count,
            table_count,
            unsupported,
        );
    }
}

fn parse_scraper_table(
    table: ego_tree::NodeRef<'_, scraper::Node>,
    images: &[ReaderImageRef],
    link_color: Color32,
    span_count: &mut usize,
) -> Vec<Vec<PrettyCell>> {
    let mut rows = Vec::new();
    for tr in table.descendants() {
        let Some(el) = tr.value().as_element() else {
            continue;
        };
        if el.name() != "tr" {
            continue;
        }
        let mut row = Vec::new();
        for cell in tr.children() {
            let Some(cell_el) = cell.value().as_element() else {
                continue;
            };
            let name = cell_el.name();
            if name != "td" && name != "th" {
                continue;
            }
            let header = name == "th";
            let spans =
                collect_scraper_spans(cell, PrettyStyle::default(), images, link_color, span_count);
            row.push(PrettyCell { spans, header });
        }
        if !row.is_empty() {
            rows.push(row);
        }
    }
    rows
}

fn parse_scraper_list(
    list: ego_tree::NodeRef<'_, scraper::Node>,
    images: &[ReaderImageRef],
    link_color: Color32,
    depth: u8,
    ordered: bool,
    mut next_index: usize,
    out: &mut Vec<PrettyBlock>,
    block_index: &mut usize,
    span_count: &mut usize,
    image_count: &mut usize,
) {
    for li in list.children() {
        let Some(el) = li.value().as_element() else {
            continue;
        };
        if el.name() != "li" {
            continue;
        }
        let idx = if ordered {
            let current = next_index.max(1);
            next_index = next_index.saturating_add(1);
            Some(current)
        } else {
            None
        };
        let items = collect_scraper_flow_items(
            li,
            PrettyStyle::default(),
            images,
            link_color,
            span_count,
            image_count,
            true,
        );
        push_flow_items_as_blocks_with_kind(
            items,
            PrettyBlockKind::ListItem {
                depth,
                ordered,
                index: idx,
            },
            out,
            block_index,
        );

        // Nested lists inside this list item.
        for nested in li.children() {
            let Some(nested_el) = nested.value().as_element() else {
                continue;
            };
            let nested_tag = nested_el.name();
            if nested_tag == "ul" || nested_tag == "ol" {
                let nested_ordered = nested_tag == "ol";
                let nested_start = if nested_ordered {
                    nested_el
                        .attr("start")
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(1)
                } else {
                    1
                };
                parse_scraper_list(
                    nested,
                    images,
                    link_color,
                    depth.saturating_add(1),
                    nested_ordered,
                    nested_start,
                    out,
                    block_index,
                    span_count,
                    image_count,
                );
            }
        }
    }
}

fn collect_scraper_text(node: ego_tree::NodeRef<'_, scraper::Node>) -> String {
    let mut out = String::new();
    collect_scraper_text_inner(node, &mut out);
    out
}

fn collect_scraper_text_inner(node: ego_tree::NodeRef<'_, scraper::Node>, out: &mut String) {
    if let Some(text) = node.value().as_text() {
        out.push_str(text);
    }
    if let Some(el) = node.value().as_element() {
        if el.name() == "br" {
            out.push('\n');
        }
    }
    for child in node.children() {
        collect_scraper_text_inner(child, out);
    }
}

fn collect_scraper_spans(
    node: ego_tree::NodeRef<'_, scraper::Node>,
    base: PrettyStyle,
    images: &[ReaderImageRef],
    link_color: Color32,
    span_count: &mut usize,
) -> Vec<PrettySpan> {
    let mut out = Vec::new();
    collect_scraper_spans_inner(node, base, images, link_color, span_count, &mut out);
    out
}

fn collect_scraper_flow_items(
    node: ego_tree::NodeRef<'_, scraper::Node>,
    base: PrettyStyle,
    images: &[ReaderImageRef],
    link_color: Color32,
    span_count: &mut usize,
    image_count: &mut usize,
    skip_lists: bool,
) -> Vec<FlowItem> {
    let mut out = Vec::new();
    collect_scraper_flow_items_inner(
        node,
        base,
        images,
        link_color,
        span_count,
        image_count,
        skip_lists,
        &mut out,
    );
    out
}

fn collect_scraper_flow_items_inner(
    node: ego_tree::NodeRef<'_, scraper::Node>,
    mut style: PrettyStyle,
    images: &[ReaderImageRef],
    link_color: Color32,
    span_count: &mut usize,
    image_count: &mut usize,
    skip_lists: bool,
    out: &mut Vec<FlowItem>,
) {
    if let Some(text) = node.value().as_text() {
        let text = text.to_string();
        if !text.is_empty() {
            if style.is_link {
                style.color = Some(link_color);
                style.underline = true;
            }
            out.push(FlowItem::Span(PrettySpan { text, style }));
            *span_count = span_count.saturating_add(1);
        }
        return;
    }

    if let Some(el) = node.value().as_element() {
        let tag = el.name();
        match tag {
            "strong" | "b" => style.bold = true,
            "em" | "i" => style.italics = true,
            "u" => style.underline = true,
            "s" | "del" => style.strikethrough = true,
            "code" => style.code = true,
            "sup" => style.sup = true,
            "sub" => style.sub = true,
            "a" => style.is_link = true,
            "span" | "font" => {
                if let Some(style_attr) = el.attr("style") {
                    apply_inline_css(style_attr, &mut style);
                }
                if tag == "font" {
                    if let Some(color_attr) = el.attr("color") {
                        if let Some(color) = parse_color_value(color_attr) {
                            style.color = Some(color);
                        }
                    }
                }
            }
            "br" => {
                out.push(FlowItem::Span(PrettySpan {
                    text: "\n".to_string(),
                    style,
                }));
                *span_count = span_count.saturating_add(1);
                return;
            }
            "img" => {
                let Some(src) = el.attr("src") else { return };
                if let Some(local_path) = resolve_image_path(src, images) {
                    let alt = el.attr("alt").map(|s| s.to_string());
                    *image_count = image_count.saturating_add(1);
                    out.push(FlowItem::Image(PrettyImage {
                        src_raw: src.to_string(),
                        local_path,
                        alt,
                    }));
                } else {
                    warn!(src, "HTML image could not be resolved");
                }
                return;
            }
            "ul" | "ol" if skip_lists => {
                return;
            }
            _ => {}
        }
    }

    for child in node.children() {
        collect_scraper_flow_items_inner(
            child,
            style,
            images,
            link_color,
            span_count,
            image_count,
            skip_lists,
            out,
        );
    }
}

fn collect_scraper_spans_inner(
    node: ego_tree::NodeRef<'_, scraper::Node>,
    mut style: PrettyStyle,
    images: &[ReaderImageRef],
    link_color: Color32,
    span_count: &mut usize,
    out: &mut Vec<PrettySpan>,
) {
    if let Some(text) = node.value().as_text() {
        let text = text.to_string();
        if !text.is_empty() {
            if style.is_link {
                style.color = Some(link_color);
                style.underline = true;
            }
            out.push(PrettySpan { text, style });
            *span_count = span_count.saturating_add(1);
        }
        return;
    }
    if let Some(el) = node.value().as_element() {
        let tag = el.name();
        match tag {
            "strong" | "b" => style.bold = true,
            "em" | "i" => style.italics = true,
            "u" => style.underline = true,
            "s" | "del" => style.strikethrough = true,
            "code" => style.code = true,
            "sup" => style.sup = true,
            "sub" => style.sub = true,
            "a" => style.is_link = true,
            "span" | "font" => {
                if let Some(style_attr) = el.attr("style") {
                    apply_inline_css(style_attr, &mut style);
                }
                if tag == "font" {
                    if let Some(color_attr) = el.attr("color") {
                        if let Some(color) = parse_color_value(color_attr) {
                            style.color = Some(color);
                        }
                    }
                }
            }
            "br" => {
                out.push(PrettySpan {
                    text: "\n".to_string(),
                    style,
                });
                *span_count = span_count.saturating_add(1);
                return;
            }
            "img" => {
                let _ = images;
                return;
            }
            _ => {}
        }
    }
    for child in node.children() {
        collect_scraper_spans_inner(child, style, images, link_color, span_count, out);
    }
}

fn apply_inline_css(style_attr: &str, style: &mut PrettyStyle) {
    for decl in style_attr.split(';') {
        let Some((key, value)) = decl.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value_raw = value.trim();
        let value_lower = value_raw.to_ascii_lowercase();
        match key.as_str() {
            "color" => {
                if let Some(color) = parse_color_value(value_raw) {
                    style.color = Some(color);
                }
            }
            "font-weight" => {
                if value_lower.contains("bold") {
                    style.bold = true;
                } else if let Ok(weight) = value_lower.parse::<u16>() {
                    if weight >= 600 {
                        style.bold = true;
                    }
                }
            }
            "font-style" => {
                if value_lower.contains("italic") || value_lower.contains("oblique") {
                    style.italics = true;
                }
            }
            "text-decoration" => {
                if value_lower.contains("underline") {
                    style.underline = true;
                }
                if value_lower.contains("line-through") {
                    style.strikethrough = true;
                }
            }
            "vertical-align" => {
                if value_lower.contains("super") {
                    style.sup = true;
                }
                if value_lower.contains("sub") {
                    style.sub = true;
                }
            }
            "font-size" => {
                if let Some(scale) = parse_font_scale(value_raw) {
                    style.font_scale = Some(scale);
                }
            }
            _ => {}
        }
    }
}

fn parse_font_scale(value: &str) -> Option<f32> {
    let v = value.trim().to_ascii_lowercase();
    if let Some(pct) = v.strip_suffix('%') {
        let pct: f32 = pct.trim().parse().ok()?;
        return Some((pct / 100.0).clamp(0.25, 6.0));
    }
    if let Some(em) = v.strip_suffix("em") {
        let em: f32 = em.trim().parse().ok()?;
        return Some(em.clamp(0.25, 6.0));
    }
    if let Some(rem) = v.strip_suffix("rem") {
        let rem: f32 = rem.trim().parse().ok()?;
        return Some(rem.clamp(0.25, 6.0));
    }
    if let Some(px) = v.strip_suffix("px") {
        let px: f32 = px.trim().parse().ok()?;
        // Approximate CSS px size relative to 16px baseline.
        return Some((px / 16.0).clamp(0.25, 6.0));
    }
    None
}

fn parse_color_value(value: &str) -> Option<Color32> {
    let v = value.trim();
    if let Some(hex) = v.strip_prefix('#') {
        let hex = hex.trim();
        if hex.len() == 3 {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            return Some(Color32::from_rgb(r, g, b));
        }
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color32::from_rgb(r, g, b));
        }
    }
    let lower = v.to_ascii_lowercase();
    if let Some(args) = lower.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = args.split(',').map(|s| s.trim()).collect();
        if parts.len() == 3 {
            let r: u8 = parts[0].parse().ok()?;
            let g: u8 = parts[1].parse().ok()?;
            let b: u8 = parts[2].parse().ok()?;
            return Some(Color32::from_rgb(r, g, b));
        }
    }
    None
}

fn push_flow_items_as_blocks_with_kind(
    items: Vec<FlowItem>,
    kind_for_text: PrettyBlockKind,
    out: &mut Vec<PrettyBlock>,
    block_index: &mut usize,
) {
    let mut spans: Vec<PrettySpan> = Vec::new();
    for item in items {
        match item {
            FlowItem::Span(span) => spans.push(span),
            FlowItem::Image(image) => {
                if spans.iter().any(|s| !s.text.trim().is_empty()) {
                    out.push(PrettyBlock {
                        kind: kind_for_text.clone(),
                        spans: std::mem::take(&mut spans),
                        code: None,
                        image: None,
                        table: None,
                        anchor_idx: anchor_idx_for_block(*block_index),
                        source_kind: PrettySourceKind::Html,
                    });
                    *block_index = block_index.saturating_add(1);
                } else {
                    spans.clear();
                }

                out.push(PrettyBlock {
                    kind: PrettyBlockKind::Image,
                    spans: Vec::new(),
                    code: None,
                    image: Some(image),
                    table: None,
                    anchor_idx: anchor_idx_for_block(*block_index),
                    source_kind: PrettySourceKind::Html,
                });
                *block_index = block_index.saturating_add(1);
            }
        }
    }

    if spans.iter().any(|s| !s.text.trim().is_empty()) {
        out.push(PrettyBlock {
            kind: kind_for_text,
            spans,
            code: None,
            image: None,
            table: None,
            anchor_idx: anchor_idx_for_block(*block_index),
            source_kind: PrettySourceKind::Html,
        });
        *block_index = block_index.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_parses_inline_styles_hr_list_and_table() {
        let md = r#"
# Title

Paragraph with *italics* and **bold**, `code`, ~~strike~~.

- Item 1
- Item 2

---

| H1 | H2 |
|----|----|
| a  | b  |
"#;
        let blocks = markdown_to_blocks(md, &[], config::PrettyUiConfig::default());
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b.kind, PrettyBlockKind::Heading { level: 1 }))
        );
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b.kind, PrettyBlockKind::HorizontalRule))
        );
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b.kind, PrettyBlockKind::ListItem { .. }))
        );
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b.kind, PrettyBlockKind::Table))
        );

        let para = blocks
            .iter()
            .find(|b| matches!(b.kind, PrettyBlockKind::Paragraph))
            .expect("paragraph block missing");
        assert!(para.spans.iter().any(|s| s.style.italics));
        assert!(para.spans.iter().any(|s| s.style.bold));
        assert!(para.spans.iter().any(|s| s.style.code));
        assert!(para.spans.iter().any(|s| s.style.strikethrough));
    }

    #[test]
    fn markdown_inline_image_splits_paragraph() {
        let md = "Before ![alt](img.png) after";
        let images = vec![ReaderImageRef {
            raw_path: "img.png".to_string(),
            local_path: "/tmp/img.png".to_string(),
        }];
        let blocks = markdown_to_blocks(md, &images, config::PrettyUiConfig::default());
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b.kind, PrettyBlockKind::Image))
        );
        let para_count = blocks
            .iter()
            .filter(|b| matches!(b.kind, PrettyBlockKind::Paragraph))
            .count();
        assert!(para_count >= 1);
    }

    #[test]
    fn html_parses_lists_tables_images_and_css_styles() {
        let html = r#"
<h2>Header</h2>
<p>Hello <span style="font-weight:bold; text-decoration: underline; color: #ff0000;">World</span></p>
<ol start="2"><li>First</li><li>Second<ul><li>Nested</li></ul></li></ol>
<hr>
<table><thead><tr><th>A</th><th>B</th></tr></thead><tbody><tr><td>1</td><td>2</td></tr></tbody></table>
<p><img src="img.png" alt="pic"> after</p>
"#;
        let images = vec![ReaderImageRef {
            raw_path: "img.png".to_string(),
            local_path: "/tmp/img.png".to_string(),
        }];
        let blocks = html_to_blocks(html, &images, config::PrettyUiConfig::default());
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b.kind, PrettyBlockKind::Heading { .. }))
        );
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b.kind, PrettyBlockKind::ListItem { ordered: true, .. }))
        );
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b.kind, PrettyBlockKind::HorizontalRule))
        );
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b.kind, PrettyBlockKind::Table))
        );
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b.kind, PrettyBlockKind::Image))
        );

        let para = blocks
            .iter()
            .find(|b| {
                matches!(b.kind, PrettyBlockKind::Paragraph)
                    && b.spans.iter().any(|s| s.text.contains("World"))
            })
            .expect("expected paragraph with styled span");
        let styled = para
            .spans
            .iter()
            .find(|s| s.text.contains("World"))
            .unwrap();
        assert!(styled.style.bold);
        assert!(styled.style.underline);
        assert_eq!(styled.style.color, Some(Color32::from_rgb(255, 0, 0)));
    }

    #[test]
    fn css_font_scale_parses_percent_em_and_px() {
        assert_eq!(parse_font_scale("150%"), Some(1.5));
        assert_eq!(parse_font_scale("1.25em"), Some(1.25));
        assert!(parse_font_scale("32px").unwrap() > 1.0);
    }
}
