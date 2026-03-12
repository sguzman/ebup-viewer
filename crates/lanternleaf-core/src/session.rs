use crate::{
    cancellation::CancellationToken, config, epub_loader, normalizer, pagination, text_utils,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;
use ts_rs::TS;

mod command_transitions;
mod document_loading;
mod page_navigation;
mod playback;
mod settings;

pub use document_loading::{
    load_session_for_source, load_session_for_source_with_cancel, persist_session_housekeeping,
};

const BASE_WPM: f64 = 170.0;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, TS)]
#[ts(export)]
pub struct PanelState {
    pub show_settings: bool,
    pub show_stats: bool,
    pub show_tts: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TtsPlaybackState {
    #[default]
    Idle,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ReaderSettingsView {
    pub theme: config::ThemeMode,
    pub font_family: config::FontFamily,
    pub font_weight: config::FontWeight,
    pub day_highlight: config::HighlightColor,
    pub night_highlight: config::HighlightColor,
    pub font_size: u32,
    pub line_spacing: f32,
    pub word_spacing: u32,
    pub letter_spacing: u32,
    pub margin_horizontal: u16,
    pub margin_vertical: u16,
    pub lines_per_page: usize,
    pub pause_after_sentence: f32,
    pub auto_scroll_tts: bool,
    pub center_spoken_sentence: bool,
    pub text_only_show_original_text: bool,
    pub time_remaining_display: config::TimeRemainingDisplay,
    pub tts_speed: f32,
    pub tts_volume: f32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ReaderTtsView {
    pub state: TtsPlaybackState,
    pub current_sentence_idx: Option<usize>,
    pub sentence_count: usize,
    pub can_seek_prev: bool,
    pub can_seek_next: bool,
    pub progress_pct: f64,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct ReaderSettingsPatch {
    #[ts(optional)]
    pub theme: Option<config::ThemeMode>,
    #[ts(optional)]
    pub day_highlight: Option<config::HighlightColor>,
    #[ts(optional)]
    pub night_highlight: Option<config::HighlightColor>,
    #[ts(optional)]
    pub font_family: Option<config::FontFamily>,
    #[ts(optional)]
    pub font_weight: Option<config::FontWeight>,
    #[ts(optional)]
    pub font_size: Option<u32>,
    #[ts(optional)]
    pub line_spacing: Option<f32>,
    #[ts(optional)]
    pub word_spacing: Option<u32>,
    #[ts(optional)]
    pub letter_spacing: Option<u32>,
    #[ts(optional)]
    pub margin_horizontal: Option<u16>,
    #[ts(optional)]
    pub margin_vertical: Option<u16>,
    #[ts(optional)]
    pub lines_per_page: Option<usize>,
    #[ts(optional)]
    pub pause_after_sentence: Option<f32>,
    #[ts(optional)]
    pub auto_scroll_tts: Option<bool>,
    #[ts(optional)]
    pub center_spoken_sentence: Option<bool>,
    #[ts(optional)]
    pub text_only_show_original_text: Option<bool>,
    #[ts(optional)]
    pub tts_speed: Option<f32>,
    #[ts(optional)]
    pub tts_volume: Option<f32>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ReaderStats {
    pub page_index: usize,
    pub total_pages: usize,
    pub tts_progress_pct: f64,
    pub global_progress_pct: f64,
    pub page_time_remaining_secs: f64,
    pub book_time_remaining_secs: f64,
    pub page_word_count: usize,
    pub page_sentence_count: usize,
    pub page_start_percent: f64,
    pub page_end_percent: f64,
    pub words_read_up_to_page_start: usize,
    pub sentences_read_up_to_page_start: usize,
    pub words_read_up_to_page_end: usize,
    pub sentences_read_up_to_page_end: usize,
    pub words_read_up_to_current_position: usize,
    pub sentences_read_up_to_current_position: usize,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ReaderSnapshot {
    pub source_path: String,
    pub source_name: String,
    pub current_page: usize,
    pub total_pages: usize,
    pub text_only_mode: bool,
    pub has_structured_markdown: bool,
    pub pretty_kind: PrettyKind,
    #[ts(
        type = "\"high_text_trust\" | \"mixed_text_trust\" | \"ocr_required\" | \"render_only_no_sync\" | null"
    )]
    pub pdf_geometry_mode: Option<crate::epub_loader::PdfGeometryMode>,
    #[ts(type = "\"sentence_spans\" | \"paragraph_fallback\" | \"render_only\" | null")]
    pub pdf_sync_strategy: Option<crate::epub_loader::PdfSyncStrategy>,
    pub pdf_classification: Option<crate::epub_loader::PdfClassificationSummary>,
    pub pdf_runtime_policy: Option<crate::epub_loader::PdfRuntimePolicySummary>,
    pub pdf_ocr_alignment: Option<crate::epub_loader::PdfOcrAlignmentSummary>,
    pub pdf_ocr_pipeline: Option<crate::epub_loader::PdfOcrPipelineSummary>,
    pub images: Vec<ReaderImageRef>,
    pub tts_text_page: String,
    pub reading_markdown_page: Option<String>,
    pub reading_html_page: Option<String>,
    pub page_text: String,
    pub sentences: Vec<String>,
    pub canonical_sentences: Vec<String>,
    pub page_sentence_counts: Vec<usize>,
    pub sentence_anchor_map: Vec<Option<usize>>,
    pub highlighted_sentence_idx: Option<usize>,
    pub search_query: String,
    pub search_matches: Vec<usize>,
    pub selected_search_match: Option<usize>,
    pub settings: ReaderSettingsView,
    pub tts: ReaderTtsView,
    pub stats: ReaderStats,
    pub panels: PanelState,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ReaderImageRef {
    pub raw_path: String,
    pub local_path: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PrettyKind {
    None,
    Markdown,
    Html,
    Pdf,
}

#[derive(Debug, Clone)]
pub enum SessionCommand {
    GetSnapshot,
    NextPage,
    PrevPage,
    SetPage { page: usize },
    SentenceClick { sentence_idx: usize },
    NextSentence,
    PrevSentence,
    ToggleTextOnly,
    ApplySettings { patch: ReaderSettingsPatch },
    SearchSetQuery { query: String },
    SearchNext,
    SearchPrev,
    TtsPlay,
    TtsPause,
    TtsTogglePlayPause,
    TtsPlayFromPageStart,
    TtsPlayFromHighlight,
    TtsSeekNext,
    TtsSeekPrev,
    TtsRepeatSentence,
    TtsStop,
}

#[derive(Debug, Clone)]
pub struct SessionEvent {
    pub action: &'static str,
    pub snapshot: ReaderSnapshot,
}

#[derive(Debug, Clone)]
pub struct ReaderSession {
    pub source_path: PathBuf,
    source_name: String,
    tts_text: String,
    reading_markdown: Option<String>,
    reading_html: Option<String>,
    has_structured_markdown: bool,
    pdf_geometry_mode: Option<crate::epub_loader::PdfGeometryMode>,
    pdf_sync_strategy: Option<crate::epub_loader::PdfSyncStrategy>,
    pdf_classification: Option<crate::epub_loader::PdfClassificationSummary>,
    pdf_runtime_policy: Option<crate::epub_loader::PdfRuntimePolicySummary>,
    pdf_ocr_alignment: Option<crate::epub_loader::PdfOcrAlignmentSummary>,
    pdf_ocr_pipeline: Option<crate::epub_loader::PdfOcrPipelineSummary>,
    images: Vec<SessionImage>,
    pub config: config::AppConfig,
    pages: Vec<String>,
    markdown_pages: Vec<String>,
    raw_page_sentences: Vec<Vec<String>>,
    sentence_anchor_maps: Vec<Vec<Option<usize>>>,
    page_word_counts: Vec<usize>,
    page_sentence_counts: Vec<usize>,
    pub current_page: usize,
    highlighted_display_idx: Option<usize>,
    highlighted_audio_idx: Option<usize>,
    pub text_only_mode: bool,
    search_query: String,
    search_matches: Vec<usize>,
    selected_search_match: Option<usize>,
    tts_state: TtsPlaybackState,
    current_plan_page: Option<usize>,
    current_plan: Option<normalizer::PageNormalization>,
}

#[derive(Debug, Clone)]
struct SessionImage {
    raw_path: String,
    path: String,
}

struct MappingTelemetry {
    lookups: AtomicUsize,
    hits: AtomicUsize,
    fallbacks: AtomicUsize,
    missing: AtomicUsize,
    summaries: AtomicUsize,
}

static MAPPING_TELEMETRY: OnceLock<MappingTelemetry> = OnceLock::new();

fn mapping_telemetry() -> &'static MappingTelemetry {
    MAPPING_TELEMETRY.get_or_init(|| MappingTelemetry {
        lookups: AtomicUsize::new(0),
        hits: AtomicUsize::new(0),
        fallbacks: AtomicUsize::new(0),
        missing: AtomicUsize::new(0),
        summaries: AtomicUsize::new(0),
    })
}

fn maybe_log_mapping_summary(path: &Path) {
    const SUMMARY_EVERY: usize = 128;
    let telemetry = mapping_telemetry();
    let lookups = telemetry.lookups.load(Ordering::Relaxed);
    if lookups == 0 || !lookups.is_multiple_of(SUMMARY_EVERY) {
        return;
    }
    let summary_idx = lookups / SUMMARY_EVERY;
    let last = telemetry.summaries.load(Ordering::Relaxed);
    if summary_idx <= last {
        return;
    }
    if telemetry
        .summaries
        .compare_exchange(last, summary_idx, Ordering::Relaxed, Ordering::Relaxed)
        .is_err()
    {
        return;
    }
    let hits = telemetry.hits.load(Ordering::Relaxed);
    let fallbacks = telemetry.fallbacks.load(Ordering::Relaxed);
    let missing = telemetry.missing.load(Ordering::Relaxed);
    let fallback_rate = if lookups == 0 {
        0.0
    } else {
        (fallbacks as f64 / lookups as f64) * 100.0
    };
    tracing::info!(
        path = %path.display(),
        lookups,
        hits,
        fallbacks,
        missing,
        fallback_rate_pct = (fallback_rate * 100.0).round() / 100.0,
        "Sentence mapping telemetry summary"
    );
}

fn tokenize_sentence_for_ocr_lineage(sentence: &str) -> Vec<String> {
    sentence
        .split_whitespace()
        .map(|token| token.trim_matches(|ch: char| !ch.is_alphanumeric()))
        .filter(|token| !token.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn build_pdf_ocr_token_lineage(
    sentence_idx: usize,
    sentence_text: &str,
    page_idx: Option<usize>,
) -> Vec<String> {
    let page = page_idx.unwrap_or(usize::MAX);
    tokenize_sentence_for_ocr_lineage(sentence_text)
        .into_iter()
        .enumerate()
        .map(|(token_idx, token)| {
            format!(
                "p{page}:s{sentence_idx}:t{token_idx}:{}",
                crate::cache::stable_sentence_text_hash(&token)
            )
        })
        .collect()
}

fn union_pdf_rects(rects: &[crate::cache::PdfRect]) -> Option<crate::cache::PdfRect> {
    let first = rects.first()?;
    let mut left = first.left;
    let mut top = first.top;
    let mut right = first.left + first.width;
    let mut bottom = first.top + first.height;
    for rect in rects.iter().skip(1) {
        left = left.min(rect.left);
        top = top.min(rect.top);
        right = right.max(rect.left + rect.width);
        bottom = bottom.max(rect.top + rect.height);
    }
    Some(crate::cache::PdfRect {
        left,
        top,
        width: (right - left).max(0.0),
        height: (bottom - top).max(0.0),
    })
}

fn detect_cross_column_alignment(
    alignment: &crate::cache::PdfOcrSentenceAlignment,
) -> (bool, bool) {
    let geometry = if alignment.rects.len() >= 2 {
        &alignment.rects
    } else if alignment.line_rects.len() >= 2 {
        &alignment.line_rects
    } else {
        &alignment.block_rects
    };
    if geometry.len() < 2 {
        return (false, false);
    }
    let min_left = geometry
        .iter()
        .map(|rect| rect.left)
        .fold(f32::INFINITY, f32::min);
    let max_left = geometry
        .iter()
        .map(|rect| rect.left)
        .fold(f32::NEG_INFINITY, f32::max);
    let avg_width = geometry.iter().map(|rect| rect.width).sum::<f32>() / geometry.len() as f32;
    let crosses = (max_left - min_left) >= 0.32 && avg_width <= 0.48;
    let confident =
        crosses && alignment.score >= 0.82 && alignment.fallback_reason != "page_location_only";
    (crosses, confident)
}

fn build_pdf_ocr_geometry_contract(
    sentences: &[String],
    alignments: &mut [crate::cache::PdfOcrSentenceAlignment],
    source_kind: crate::epub_loader::PdfOcrSourceKind,
) -> (
    Vec<crate::cache::PdfOcrBlockGeometry>,
    Vec<crate::cache::PdfOcrLineGeometry>,
    Vec<crate::cache::PdfOcrTokenGeometry>,
    Vec<crate::cache::PdfOcrPageGeometry>,
    Vec<u32>,
    Vec<u32>,
    usize,
    usize,
) {
    let mut blocks = Vec::new();
    let mut lines = Vec::new();
    let mut tokens = Vec::new();
    let mut page_geometry_map: HashMap<usize, crate::cache::PdfOcrPageGeometry> = HashMap::new();
    let mut page_build_ms = Vec::new();
    let mut cross_column_alignment_count = 0usize;
    let mut cross_column_confident_alignment_count = 0usize;

    for alignment in alignments.iter_mut() {
        let page_idx = match alignment.page_idx {
            Some(value) => value,
            None => continue,
        };
        let page_started = Instant::now();
        if alignment.token_lineage.is_empty() {
            alignment.token_lineage = build_pdf_ocr_token_lineage(
                alignment.sentence_idx,
                &sentences[alignment.sentence_idx],
                Some(page_idx),
            );
        }
        let (crosses_column_boundaries, cross_column_confident) =
            detect_cross_column_alignment(alignment);
        alignment.crosses_column_boundaries = crosses_column_boundaries;
        alignment.cross_column_confident = cross_column_confident;
        if crosses_column_boundaries {
            cross_column_alignment_count += 1;
        }
        if cross_column_confident {
            cross_column_confident_alignment_count += 1;
        }

        let block_rects = if alignment.block_rects.is_empty() {
            union_pdf_rects(if !alignment.line_rects.is_empty() {
                &alignment.line_rects
            } else {
                &alignment.rects
            })
            .into_iter()
            .collect::<Vec<_>>()
        } else {
            alignment.block_rects.clone()
        };
        let line_rects = if alignment.line_rects.is_empty() {
            if !alignment.rects.is_empty() {
                alignment.rects.clone()
            } else {
                block_rects.clone()
            }
        } else {
            alignment.line_rects.clone()
        };
        let token_rects = if alignment.rects.is_empty() {
            if !line_rects.is_empty() {
                line_rects.clone()
            } else {
                block_rects.clone()
            }
        } else {
            alignment.rects.clone()
        };
        if block_rects.is_empty() && line_rects.is_empty() && token_rects.is_empty() {
            continue;
        }

        let sentence_text = sentences[alignment.sentence_idx].clone();
        let sentence_tokens = tokenize_sentence_for_ocr_lineage(&sentence_text);

        let block_ids: Vec<String> = block_rects
            .iter()
            .enumerate()
            .map(|(block_offset, block_rect)| {
                let block_id = format!("p{page_idx}:s{}:b{block_offset}", alignment.sentence_idx);
                blocks.push(crate::cache::PdfOcrBlockGeometry {
                    block_id: block_id.clone(),
                    page_idx,
                    reading_order_idx: blocks.len(),
                    text: sentence_text.clone(),
                    rect: block_rect.clone(),
                    confidence: alignment.score,
                    line_ids: Vec::new(),
                });
                block_id
            })
            .collect();

        let line_ids: Vec<String> = line_rects
            .iter()
            .enumerate()
            .map(|(line_offset, line_rect)| {
                let block_idx = if block_ids.is_empty() {
                    0
                } else {
                    line_offset.min(block_ids.len() - 1)
                };
                let line_id = format!("p{page_idx}:s{}:l{line_offset}", alignment.sentence_idx);
                lines.push(crate::cache::PdfOcrLineGeometry {
                    line_id: line_id.clone(),
                    page_idx,
                    block_idx,
                    reading_order_idx: lines.len(),
                    text: sentence_text.clone(),
                    rect: line_rect.clone(),
                    confidence: alignment.score,
                    token_ids: Vec::new(),
                });
                if let Some(block) = block_ids.get(block_idx).and_then(|block_id| {
                    blocks
                        .iter_mut()
                        .find(|candidate| candidate.block_id == *block_id)
                }) {
                    block.line_ids.push(line_id.clone());
                }
                line_id
            })
            .collect();

        let token_ids: Vec<String> = alignment
            .token_lineage
            .iter()
            .enumerate()
            .map(|(token_offset, token_id)| {
                let rect_idx = token_offset.min(token_rects.len().saturating_sub(1));
                let line_idx = token_offset.min(line_ids.len().saturating_sub(1));
                let block_idx = token_offset.min(block_ids.len().saturating_sub(1));
                let token_text = sentence_tokens
                    .get(token_offset)
                    .cloned()
                    .unwrap_or_default();
                tokens.push(crate::cache::PdfOcrTokenGeometry {
                    token_id: token_id.clone(),
                    page_idx,
                    block_idx,
                    line_idx,
                    reading_order_idx: tokens.len(),
                    text: token_text,
                    rect: token_rects
                        .get(rect_idx)
                        .cloned()
                        .or_else(|| union_pdf_rects(&token_rects))
                        .unwrap_or(crate::cache::PdfRect {
                            left: 0.0,
                            top: 0.0,
                            width: 1.0,
                            height: 0.05,
                        }),
                    confidence: alignment.score,
                    source_kind,
                });
                if let Some(line) = line_ids.get(line_idx).and_then(|line_id| {
                    lines
                        .iter_mut()
                        .find(|candidate| candidate.line_id == *line_id)
                }) {
                    line.token_ids.push(token_id.clone());
                }
                token_id.clone()
            })
            .collect();

        let elapsed_ms = page_started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
        page_build_ms.push(elapsed_ms);
        let page_entry =
            page_geometry_map
                .entry(page_idx)
                .or_insert_with(|| crate::cache::PdfOcrPageGeometry {
                    page_idx,
                    confidence: alignment.score,
                    build_ms: 0,
                    reading_order_mode: if cross_column_confident {
                        "cross_column_confident".to_string()
                    } else if crosses_column_boundaries {
                        "cross_column_low_confidence".to_string()
                    } else {
                        "single_stream".to_string()
                    },
                    block_ids: Vec::new(),
                    line_ids: Vec::new(),
                    token_ids: Vec::new(),
                });
        page_entry.confidence = page_entry.confidence.max(alignment.score);
        page_entry.build_ms = page_entry.build_ms.saturating_add(elapsed_ms);
        page_entry.block_ids.extend(block_ids);
        page_entry.line_ids.extend(line_ids);
        page_entry.token_ids.extend(token_ids);
    }

    let mut page_geometry: Vec<crate::cache::PdfOcrPageGeometry> =
        page_geometry_map.into_values().collect();
    page_geometry.sort_by_key(|page| page.page_idx);
    let chunk_build_ms = page_build_ms
        .chunks(8)
        .map(|chunk| chunk.iter().copied().sum())
        .collect();

    (
        blocks,
        lines,
        tokens,
        page_geometry,
        page_build_ms,
        chunk_build_ms,
        cross_column_alignment_count,
        cross_column_confident_alignment_count,
    )
}

fn build_pdf_ocr_alignment_artifact(
    source_path: &Path,
    sentences: &[String],
    classification: Option<&crate::epub_loader::PdfClassificationSummary>,
    runtime_policy: Option<&crate::epub_loader::PdfRuntimePolicySummary>,
) -> crate::cache::PdfOcrAlignmentArtifact {
    let build_started = Instant::now();
    let locations = crate::cache::load_pdf_sentence_map(source_path).unwrap_or_default();
    let previous_artifact = crate::cache::load_pdf_ocr_alignment_artifact(source_path);
    let location_map: HashMap<usize, crate::cache::PdfSentenceLocation> = locations
        .into_iter()
        .map(|location| (location.sentence_idx, location))
        .collect();
    let previous_alignment_map: HashMap<(usize, String), crate::cache::PdfOcrSentenceAlignment> =
        previous_artifact
            .as_ref()
            .map(|artifact| {
                artifact
                    .alignments
                    .iter()
                    .cloned()
                    .map(|alignment| {
                        (
                            (alignment.sentence_idx, alignment.sentence_text_hash.clone()),
                            alignment,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
    let source_kind = derive_pdf_ocr_source_kind(classification);
    let mut alignments = Vec::with_capacity(sentences.len());
    let mut rect_mapped = 0usize;
    let mut line_mapped = 0usize;
    let mut block_mapped = 0usize;
    let mut page_only = 0usize;
    let mut unmappable = 0usize;
    let mut reused_alignment_count = 0usize;
    let mut rebuilt_alignment_count = 0usize;
    let mut degraded_reasons = Vec::new();

    for (sentence_idx, sentence_text) in sentences.iter().enumerate() {
        let sentence_hash = crate::cache::stable_sentence_text_hash(sentence_text);
        let alignment = if let Some(existing) = previous_alignment_map
            .get(&(sentence_idx, sentence_hash.clone()))
            .cloned()
        {
            reused_alignment_count += 1;
            existing
        } else {
            rebuilt_alignment_count += 1;
            let location = location_map.get(&sentence_idx);
            let (page_idx, rects, line_rects, block_rects, score, fallback_reason) = location
                .map(|value| {
                    (
                        value.page_idx,
                        value.rects.clone(),
                        value.line_rects.clone(),
                        value.block_rects.clone(),
                        value.score,
                        value.reason.clone(),
                    )
                })
                .unwrap_or_else(|| {
                    (
                        None,
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        0.0,
                        "missing".to_string(),
                    )
                });
            let confidence_tier = if !rects.is_empty() {
                "sentence_rects".to_string()
            } else if !line_rects.is_empty() {
                "line_fallback".to_string()
            } else if !block_rects.is_empty() {
                "block_fallback".to_string()
            } else if page_idx.is_some() {
                "page_only".to_string()
            } else {
                "missing".to_string()
            };
            crate::cache::PdfOcrSentenceAlignment {
                sentence_idx,
                sentence_text_hash: sentence_hash,
                page_idx,
                rects,
                line_rects,
                block_rects,
                confidence_tier,
                fallback_reason,
                token_lineage: build_pdf_ocr_token_lineage(sentence_idx, sentence_text, page_idx),
                score,
                crosses_column_boundaries: false,
                cross_column_confident: false,
            }
        };
        if !alignment.rects.is_empty() {
            rect_mapped += 1;
        } else if !alignment.line_rects.is_empty() {
            line_mapped += 1;
        } else if !alignment.block_rects.is_empty() {
            block_mapped += 1;
        } else if alignment.page_idx.is_some() {
            page_only += 1;
        } else {
            unmappable += 1;
        }
        if alignment.fallback_reason != "exact_geometry"
            && alignment.fallback_reason != "missing"
            && !alignment.fallback_reason.is_empty()
            && !degraded_reasons
                .iter()
                .any(|reason| reason == &alignment.fallback_reason)
        {
            degraded_reasons.push(alignment.fallback_reason.clone());
        }
        alignments.push(alignment);
    }

    let (
        blocks,
        lines,
        tokens,
        page_geometry,
        page_build_ms,
        chunk_build_ms,
        cross_column_alignment_count,
        cross_column_confident_alignment_count,
    ) = build_pdf_ocr_geometry_contract(sentences, &mut alignments, source_kind);

    let mapped = rect_mapped + line_mapped + block_mapped + page_only;
    let highlightable = rect_mapped + line_mapped + block_mapped;
    let mut page_bucket_map: HashMap<usize, Vec<usize>> = HashMap::new();
    for alignment in &alignments {
        if let Some(page_idx) = alignment.page_idx {
            page_bucket_map
                .entry(page_idx)
                .or_default()
                .push(alignment.sentence_idx);
        }
    }
    let mut page_buckets: Vec<crate::cache::PdfOcrPageAlignmentBucket> = page_bucket_map
        .into_iter()
        .map(|(page_idx, mut sentence_indexes)| {
            sentence_indexes.sort_unstable();
            let highlightable_sentence_count = alignments
                .iter()
                .filter(|alignment| {
                    alignment.page_idx == Some(page_idx)
                        && (!alignment.rects.is_empty()
                            || !alignment.line_rects.is_empty()
                            || !alignment.block_rects.is_empty())
                })
                .count();
            crate::cache::PdfOcrPageAlignmentBucket {
                page_idx,
                sentence_indexes,
                highlightable_sentence_count,
            }
        })
        .collect();
    page_buckets.sort_by_key(|bucket| bucket.page_idx);
    let quality_class = derive_pdf_ocr_quality_class(
        sentences.len(),
        rect_mapped,
        line_mapped,
        block_mapped,
        page_only,
        classification,
        runtime_policy,
    );
    let explanation = match quality_class {
        crate::epub_loader::PdfOcrGeometryQualityClass::OcrHighTrust => {
            "OCR geometry supports sentence-level or near-sentence overlay recovery".to_string()
        }
        crate::epub_loader::PdfOcrGeometryQualityClass::OcrMixedTrust => {
            "OCR geometry is usable, but line/block fallback should be treated as the stable overlay surface".to_string()
        }
        crate::epub_loader::PdfOcrGeometryQualityClass::OcrTextOnly => {
            "OCR text is usable, but geometry remains weak enough that text ownership should stay canonical and native highlights should degrade honestly".to_string()
        }
        crate::epub_loader::PdfOcrGeometryQualityClass::OcrFailedOrUnusable => {
            "OCR geometry is not trustworthy enough for synced native PDF highlights".to_string()
        }
    };

    crate::cache::PdfOcrAlignmentArtifact {
        version: 0,
        quality_class,
        source_kind,
        sentence_count: sentences.len(),
        mapped_sentence_count: mapped,
        rect_mapped_sentence_count: rect_mapped,
        line_mapped_sentence_count: line_mapped,
        block_mapped_sentence_count: block_mapped,
        page_only_sentence_count: page_only,
        unmappable_sentence_count: unmappable,
        highlightable_sentence_count: highlightable,
        token_lineage_available: !tokens.is_empty(),
        deterministic: true,
        reused_alignment_count,
        rebuilt_alignment_count,
        alignment_build_ms: build_started
            .elapsed()
            .as_millis()
            .min(u128::from(u32::MAX)) as u32,
        page_build_ms,
        chunk_build_ms,
        cross_column_alignment_count,
        cross_column_confident_alignment_count,
        degraded_reasons,
        explanation,
        page_buckets,
        blocks,
        lines,
        tokens,
        page_geometry,
        alignments,
    }
}

fn derive_pdf_ocr_source_kind(
    classification: Option<&crate::epub_loader::PdfClassificationSummary>,
) -> crate::epub_loader::PdfOcrSourceKind {
    match classification.map(|value| value.document_class) {
        Some(crate::epub_loader::PdfDocumentClass::EmbeddedClean)
        | Some(crate::epub_loader::PdfDocumentClass::EmbeddedNoisy)
        | Some(crate::epub_loader::PdfDocumentClass::EmbeddedSparse) => {
            crate::epub_loader::PdfOcrSourceKind::EmbeddedText
        }
        Some(crate::epub_loader::PdfDocumentClass::HybridMixedDocument)
        | Some(crate::epub_loader::PdfDocumentClass::LayoutHostileDocument) => {
            crate::epub_loader::PdfOcrSourceKind::MixedMergedText
        }
        _ => crate::epub_loader::PdfOcrSourceKind::OcrText,
    }
}

fn derive_pdf_ocr_quality_class(
    sentence_count: usize,
    rect_mapped: usize,
    line_mapped: usize,
    block_mapped: usize,
    page_only: usize,
    classification: Option<&crate::epub_loader::PdfClassificationSummary>,
    runtime_policy: Option<&crate::epub_loader::PdfRuntimePolicySummary>,
) -> crate::epub_loader::PdfOcrGeometryQualityClass {
    if sentence_count == 0 {
        return crate::epub_loader::PdfOcrGeometryQualityClass::OcrFailedOrUnusable;
    }
    let sentence_count_f = sentence_count as f32;
    let rect_ratio = rect_mapped as f32 / sentence_count_f;
    let highlightable_ratio = (rect_mapped + line_mapped + block_mapped) as f32 / sentence_count_f;
    let mapped_ratio =
        (rect_mapped + line_mapped + block_mapped + page_only) as f32 / sentence_count_f;
    if runtime_policy
        .map(|value| !value.tts_allowed)
        .unwrap_or(false)
    {
        return crate::epub_loader::PdfOcrGeometryQualityClass::OcrFailedOrUnusable;
    }
    if rect_ratio >= 0.65
        && runtime_policy
            .map(|value| value.exact_sentence_sync)
            .unwrap_or(false)
    {
        return crate::epub_loader::PdfOcrGeometryQualityClass::OcrHighTrust;
    }
    if highlightable_ratio >= 0.45
        || matches!(
            classification.map(|value| value.ocr_recommendation),
            Some(crate::epub_loader::PdfOcrRecommendation::GeometryOnly)
        )
    {
        return crate::epub_loader::PdfOcrGeometryQualityClass::OcrMixedTrust;
    }
    if mapped_ratio >= 0.2 {
        return crate::epub_loader::PdfOcrGeometryQualityClass::OcrTextOnly;
    }
    crate::epub_loader::PdfOcrGeometryQualityClass::OcrFailedOrUnusable
}

fn pdf_ocr_alignment_summary_from_artifact(
    artifact: &crate::cache::PdfOcrAlignmentArtifact,
) -> crate::epub_loader::PdfOcrAlignmentSummary {
    let sentence_count = artifact.sentence_count.max(1) as f32;
    crate::epub_loader::PdfOcrAlignmentSummary {
        quality_class: artifact.quality_class,
        source_kind: artifact.source_kind,
        sentence_count: artifact.sentence_count as u32,
        mapped_sentence_count: artifact.mapped_sentence_count as u32,
        rect_mapped_sentence_count: artifact.rect_mapped_sentence_count as u32,
        line_mapped_sentence_count: artifact.line_mapped_sentence_count as u32,
        block_mapped_sentence_count: artifact.block_mapped_sentence_count as u32,
        page_only_sentence_count: artifact.page_only_sentence_count as u32,
        unmappable_sentence_count: artifact.unmappable_sentence_count as u32,
        highlightable_sentence_count: artifact.highlightable_sentence_count as u32,
        token_lineage_available: artifact.token_lineage_available,
        deterministic: artifact.deterministic,
        coverage_ratio: if artifact.sentence_count == 0 {
            0.0
        } else {
            artifact.mapped_sentence_count as f32 / artifact.sentence_count as f32
        },
        reused_alignment_count: artifact.reused_alignment_count as u32,
        rebuilt_alignment_count: artifact.rebuilt_alignment_count as u32,
        cached_page_bucket_count: artifact.page_buckets.len() as u32,
        alignment_build_ms: artifact.alignment_build_ms,
        geometry_block_count: artifact.blocks.len() as u32,
        geometry_line_count: artifact.lines.len() as u32,
        geometry_token_count: artifact.tokens.len() as u32,
        page_timing_count: artifact.page_build_ms.len() as u32,
        chunk_timing_count: artifact.chunk_build_ms.len() as u32,
        max_page_build_ms: artifact.page_build_ms.iter().copied().max().unwrap_or(0),
        max_chunk_build_ms: artifact.chunk_build_ms.iter().copied().max().unwrap_or(0),
        cross_column_alignment_count: artifact.cross_column_alignment_count as u32,
        cross_column_confident_alignment_count: artifact.cross_column_confident_alignment_count
            as u32,
        exact_sentence_rate: artifact.rect_mapped_sentence_count as f32 / sentence_count,
        degraded_fallback_rate: (artifact.line_mapped_sentence_count
            + artifact.block_mapped_sentence_count) as f32
            / sentence_count,
        page_only_rate: artifact.page_only_sentence_count as f32 / sentence_count,
        unmappable_rate: artifact.unmappable_sentence_count as f32 / sentence_count,
        degraded_reasons: artifact.degraded_reasons.clone(),
        explanation: artifact.explanation.clone(),
    }
}

impl ReaderSession {
    fn is_pdf_source(&self) -> bool {
        self.source_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("pdf"))
            .unwrap_or(false)
    }

    fn pdf_runtime_policy_ref(&self) -> Option<&crate::epub_loader::PdfRuntimePolicySummary> {
        self.pdf_runtime_policy
            .as_ref()
            .filter(|_| self.is_pdf_source())
    }

    fn pdf_text_only_allowed(&self) -> bool {
        !matches!(
            self.pdf_runtime_policy_ref()
                .map(|value| value.text_only_policy),
            Some(crate::epub_loader::PdfTextOnlyPolicy::Disabled)
                | Some(crate::epub_loader::PdfTextOnlyPolicy::OcrRequired)
        )
    }

    fn pdf_search_allowed(&self) -> bool {
        !matches!(
            self.pdf_runtime_policy_ref()
                .map(|value| value.search_policy),
            Some(crate::epub_loader::PdfSearchPolicy::Disabled)
        )
    }

    fn pdf_tts_allowed(&self) -> bool {
        self.pdf_runtime_policy_ref()
            .map(|value| value.tts_allowed)
            .unwrap_or(true)
    }

    fn refresh_pdf_ocr_alignment_artifact(&mut self) {
        if !self.is_pdf_source() {
            self.pdf_ocr_alignment = None;
            return;
        }
        let sentences: Vec<String> = self
            .raw_page_sentences
            .iter()
            .flat_map(|page| page.iter().cloned())
            .collect();
        let artifact = build_pdf_ocr_alignment_artifact(
            &self.source_path,
            &sentences,
            self.pdf_classification.as_ref(),
            self.pdf_runtime_policy.as_ref(),
        );
        let summary = pdf_ocr_alignment_summary_from_artifact(&artifact);
        tracing::info!(
            path = %self.source_path.display(),
            sentence_count = artifact.sentence_count,
            mapped_sentence_count = artifact.mapped_sentence_count,
            rect_mapped_sentence_count = artifact.rect_mapped_sentence_count,
            line_mapped_sentence_count = artifact.line_mapped_sentence_count,
            block_mapped_sentence_count = artifact.block_mapped_sentence_count,
            page_only_sentence_count = artifact.page_only_sentence_count,
            unmappable_sentence_count = artifact.unmappable_sentence_count,
            reused_alignment_count = artifact.reused_alignment_count,
            rebuilt_alignment_count = artifact.rebuilt_alignment_count,
            cached_page_bucket_count = artifact.page_buckets.len(),
            alignment_build_ms = artifact.alignment_build_ms,
            geometry_block_count = artifact.blocks.len(),
            geometry_line_count = artifact.lines.len(),
            geometry_token_count = artifact.tokens.len(),
            page_timing_count = artifact.page_build_ms.len(),
            chunk_timing_count = artifact.chunk_build_ms.len(),
            max_page_build_ms = artifact.page_build_ms.iter().copied().max().unwrap_or(0),
            max_chunk_build_ms = artifact.chunk_build_ms.iter().copied().max().unwrap_or(0),
            cross_column_alignment_count = artifact.cross_column_alignment_count,
            cross_column_confident_alignment_count = artifact.cross_column_confident_alignment_count,
            quality_class = ?artifact.quality_class,
            source_kind = ?artifact.source_kind,
            coverage_ratio = ((summary.coverage_ratio as f64) * 100.0).round() / 100.0,
            exact_sentence_rate_pct = ((summary.exact_sentence_rate as f64) * 10000.0).round() / 100.0,
            degraded_fallback_rate_pct = ((summary.degraded_fallback_rate as f64) * 10000.0).round() / 100.0,
            page_only_rate_pct = ((summary.page_only_rate as f64) * 10000.0).round() / 100.0,
            unmappable_rate_pct = ((summary.unmappable_rate as f64) * 10000.0).round() / 100.0,
            "Refreshed PDF OCR alignment artifact from canonical sentence stream"
        );
        crate::cache::persist_pdf_ocr_alignment_artifact(&self.source_path, &artifact);
        self.pdf_ocr_alignment = Some(summary);
    }

    pub fn snapshot(
        &mut self,
        panels: PanelState,
        normalizer: &normalizer::TextNormalizer,
    ) -> ReaderSnapshot {
        let sentences = self.current_sentences(normalizer);
        let canonical_sentences = self
            .raw_page_sentences
            .iter()
            .flat_map(|page| page.iter().cloned())
            .collect();
        let sentence_anchor_map = self.current_sentence_anchor_map();
        let anchor_hits = sentence_anchor_map
            .iter()
            .filter(|value| value.is_some())
            .count();
        let anchor_missing = sentence_anchor_map.len().saturating_sub(anchor_hits);
        let highlighted_sentence_idx = self.current_highlight_idx();
        let stats = self.stats(normalizer);
        let tts = self.tts_view(normalizer, stats.tts_progress_pct);
        let tts_text_page = self
            .pages
            .get(self.current_page)
            .cloned()
            .unwrap_or_else(String::new);
        let reading_markdown_page = self
            .markdown_pages
            .get(self.current_page)
            .cloned()
            .filter(|value| !value.trim().is_empty());
        let reading_html_page = if self.config.native_html_pretty_enabled {
            self.reading_html
                .as_ref()
                .cloned()
                .filter(|value| !value.trim().is_empty())
        } else {
            None
        };
        let source_is_pdf = self
            .source_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("pdf"))
            .unwrap_or(false);
        let pretty_kind = if source_is_pdf {
            PrettyKind::Pdf
        } else if reading_html_page.is_some() {
            PrettyKind::Html
        } else if reading_markdown_page.is_some() {
            PrettyKind::Markdown
        } else {
            PrettyKind::None
        };
        tracing::debug!(
            path = %self.source_path.display(),
            page = self.current_page + 1,
            total_pages = self.pages.len(),
            text_only = self.text_only_mode,
            pretty_kind = ?pretty_kind,
            native_html_pretty_enabled = self.config.native_html_pretty_enabled,
            native_html_pagination_mode = ?self.config.native_html_pagination_mode,
            anchor_hits,
            anchor_missing,
            has_markdown = self.reading_markdown.is_some(),
            has_html = self.reading_html.is_some(),
            "Prepared reader snapshot payload"
        );
        ReaderSnapshot {
            source_path: self.source_path_str(),
            source_name: self.source_name.clone(),
            current_page: self.current_page,
            total_pages: self.pages.len(),
            text_only_mode: self.text_only_mode,
            has_structured_markdown: self.has_structured_markdown,
            pretty_kind,
            pdf_geometry_mode: self.pdf_geometry_mode,
            pdf_sync_strategy: self.pdf_sync_strategy,
            pdf_classification: self.pdf_classification.clone(),
            pdf_runtime_policy: self.pdf_runtime_policy.clone(),
            pdf_ocr_alignment: self.pdf_ocr_alignment.clone(),
            pdf_ocr_pipeline: self.pdf_ocr_pipeline.clone(),
            images: self.current_page_images(),
            tts_text_page: tts_text_page.clone(),
            reading_markdown_page,
            reading_html_page,
            page_text: tts_text_page,
            sentences,
            canonical_sentences,
            page_sentence_counts: self.page_sentence_counts.clone(),
            sentence_anchor_map,
            highlighted_sentence_idx,
            search_query: self.search_query.clone(),
            search_matches: self.search_matches.clone(),
            selected_search_match: self.selected_search_match,
            settings: self.settings_view(),
            tts,
            stats,
            panels,
        }
    }

    pub fn set_search_query(&mut self, query: String, normalizer: &normalizer::TextNormalizer) {
        self.search_query = query;
        self.update_search_matches(normalizer);
        self.apply_selected_match_as_highlight(normalizer);
    }

    pub fn search_next(&mut self, normalizer: &normalizer::TextNormalizer) {
        if self.search_matches.is_empty() {
            self.selected_search_match = None;
            return;
        }
        self.selected_search_match = Some(match self.selected_search_match {
            Some(current) => (current + 1) % self.search_matches.len(),
            None => 0,
        });
        self.apply_selected_match_as_highlight(normalizer);
    }

    pub fn search_prev(&mut self, normalizer: &normalizer::TextNormalizer) {
        if self.search_matches.is_empty() {
            self.selected_search_match = None;
            return;
        }
        self.selected_search_match = Some(match self.selected_search_match {
            Some(0) | None => self.search_matches.len().saturating_sub(1),
            Some(current) => current.saturating_sub(1),
        });
        self.apply_selected_match_as_highlight(normalizer);
    }

    fn current_sentences(&mut self, normalizer: &normalizer::TextNormalizer) -> Vec<String> {
        if self.text_only_mode {
            if self.config.text_only_show_original_text {
                return self
                    .raw_page_sentences
                    .get(self.current_page)
                    .cloned()
                    .unwrap_or_default();
            }
            return self.ensure_current_plan(normalizer).audio_sentences;
        }
        self.raw_page_sentences
            .get(self.current_page)
            .cloned()
            .unwrap_or_default()
    }

    fn current_sentence_anchor_map(&self) -> Vec<Option<usize>> {
        if self.text_only_mode {
            let count = self
                .raw_page_sentences
                .get(self.current_page)
                .map(|v| v.len())
                .unwrap_or(0);
            return (0..count).map(Some).collect();
        }
        self.sentence_anchor_maps
            .get(self.current_page)
            .cloned()
            .or_else(|| {
                crate::cache::load_sentence_anchor_map(&self.source_path, self.current_page)
            })
            .unwrap_or_else(|| {
                let count = self
                    .raw_page_sentences
                    .get(self.current_page)
                    .map(|v| v.len())
                    .unwrap_or(0);
                (0..count).map(Some).collect()
            })
    }

    fn build_sentence_anchor_map_for_page(
        &self,
        page_idx: usize,
        sentence_count: usize,
    ) -> Vec<Option<usize>> {
        if sentence_count == 0 {
            return Vec::new();
        }
        let has_native_html = self.config.native_html_pretty_enabled && self.reading_html.is_some();
        if !has_native_html
            && let Some(cached) =
                crate::cache::load_sentence_anchor_map(&self.source_path, page_idx)
            && cached.len() == sentence_count
        {
            return cached;
        }
        let Some(markdown_page) = self.markdown_pages.get(page_idx) else {
            if self.config.native_html_pretty_enabled
                && let Some(reading_html) = self.reading_html.as_ref()
            {
                let anchor_count = count_html_anchors(reading_html);
                if anchor_count == 0 {
                    tracing::debug!(
                        path = %self.source_path.display(),
                        page = page_idx + 1,
                        sentence_count,
                        "Sentence-anchor map fallback: no HTML anchors detected"
                    );
                    return (0..sentence_count).map(Some).collect();
                }
                tracing::debug!(
                    path = %self.source_path.display(),
                    page = page_idx + 1,
                    sentence_count,
                    anchor_count,
                    source = "html",
                    "Built sentence-anchor map from HTML anchors"
                );
                return proportional_html_anchor_map(
                    &self.page_sentence_counts,
                    page_idx,
                    sentence_count,
                    anchor_count,
                );
            }
            tracing::debug!(
                path = %self.source_path.display(),
                page = page_idx + 1,
                sentence_count,
                "Sentence-anchor map fallback: no markdown/html pretty payload"
            );
            return (0..sentence_count).map(Some).collect();
        };
        let anchor_count = count_markdown_anchors(markdown_page);
        if anchor_count == 0 {
            tracing::debug!(
                path = %self.source_path.display(),
                page = page_idx + 1,
                sentence_count,
                "Sentence-anchor map fallback: no markdown anchors detected"
            );
            return (0..sentence_count).map(Some).collect();
        }
        tracing::debug!(
            path = %self.source_path.display(),
            page = page_idx + 1,
            sentence_count,
            anchor_count,
            source = "markdown",
            "Built sentence-anchor map from markdown anchors"
        );
        proportional_anchor_map(sentence_count, anchor_count)
    }

    fn current_highlight_idx(&self) -> Option<usize> {
        if self.text_only_mode {
            if self.config.text_only_show_original_text {
                return self.highlighted_display_idx;
            }
            self.highlighted_audio_idx
        } else {
            self.highlighted_display_idx
        }
    }

    fn global_display_idx(&self) -> Option<usize> {
        let page_base: usize = self
            .page_sentence_counts
            .iter()
            .take(self.current_page)
            .sum();
        self.highlighted_display_idx.map(|idx| page_base + idx)
    }

    fn current_page_images(&self) -> Vec<ReaderImageRef> {
        if self.images.is_empty() {
            return Vec::new();
        }
        // Expose all extracted assets so pretty view can resolve image/css
        // references even when page char-offset mapping is imperfect.
        self.images
            .iter()
            .map(|image| ReaderImageRef {
                raw_path: image.raw_path.clone(),
                local_path: image.path.clone(),
            })
            .collect()
    }

    fn tts_view(
        &mut self,
        normalizer: &normalizer::TextNormalizer,
        progress_pct: f64,
    ) -> ReaderTtsView {
        let sentence_count = self.current_audio_sentences(normalizer).len();
        let current_sentence_idx = self.current_audio_highlight_idx(normalizer);
        let can_seek_prev = if let Some(idx) = current_sentence_idx {
            idx > 0 || self.has_sentence_before_current_page()
        } else {
            self.has_sentence_before_current_page()
        };
        let can_seek_next = if let Some(idx) = current_sentence_idx {
            idx + 1 < sentence_count || self.has_sentence_after_current_page()
        } else {
            sentence_count > 0 || self.has_sentence_after_current_page()
        };
        ReaderTtsView {
            state: self.tts_state,
            current_sentence_idx,
            sentence_count,
            can_seek_prev,
            can_seek_next,
            progress_pct: (progress_pct * 1000.0).round() / 1000.0,
        }
    }

    fn has_sentence_before_current_page(&self) -> bool {
        self.page_sentence_counts
            .iter()
            .take(self.current_page)
            .any(|count| *count > 0)
    }

    fn has_sentence_after_current_page(&self) -> bool {
        self.page_sentence_counts
            .iter()
            .skip(self.current_page.saturating_add(1))
            .any(|count| *count > 0)
    }

    fn update_search_matches(&mut self, normalizer: &normalizer::TextNormalizer) {
        self.search_matches.clear();
        self.selected_search_match = None;
        let query = self.search_query.trim().to_string();
        if query.is_empty() {
            return;
        }
        if !self.pdf_search_allowed() {
            tracing::info!(
                path = %self.source_path.display(),
                policy = ?self.pdf_runtime_policy_ref().map(|value| value.search_policy),
                "Ignoring search because PDF runtime policy disables it"
            );
            return;
        }

        let sentences = self.current_sentences(normalizer);
        let regex = Regex::new(&query).ok();
        let query_lower = query.to_ascii_lowercase();
        for (idx, sentence) in sentences.iter().enumerate() {
            let matched = if let Some(regex) = &regex {
                regex.is_match(sentence)
            } else {
                sentence.to_ascii_lowercase().contains(&query_lower)
            };
            if matched {
                self.search_matches.push(idx);
            }
        }
        if !self.search_matches.is_empty() {
            self.selected_search_match = Some(0);
        }
    }

    fn reselect_search_match_for_current_highlight(&mut self) {
        let Some(highlight_idx) = self.current_highlight_idx() else {
            return;
        };
        let Some(position) = self
            .search_matches
            .iter()
            .position(|candidate| *candidate == highlight_idx)
        else {
            return;
        };
        self.selected_search_match = Some(position);
    }

    fn apply_selected_match_as_highlight(&mut self, normalizer: &normalizer::TextNormalizer) {
        let Some(selected_idx) = self.selected_search_match else {
            return;
        };
        let Some(sentence_idx) = self.search_matches.get(selected_idx).copied() else {
            return;
        };
        self.sentence_click(sentence_idx, normalizer);
    }

    fn stats(&mut self, normalizer: &normalizer::TextNormalizer) -> ReaderStats {
        let page_word_count = self
            .page_word_counts
            .get(self.current_page)
            .copied()
            .unwrap_or_default();
        let page_sentence_count = self
            .page_sentence_counts
            .get(self.current_page)
            .copied()
            .unwrap_or_default();
        let words_before_page: usize = self.page_word_counts.iter().take(self.current_page).sum();
        let sentences_before_page: usize = self
            .page_sentence_counts
            .iter()
            .take(self.current_page)
            .sum();
        let words_up_to_page_end = words_before_page + page_word_count;
        let sentences_up_to_page_end = sentences_before_page + page_sentence_count;
        let total_words = self.page_word_counts.iter().sum::<usize>().max(1);

        let (progress_fraction, sentence_progress_count, sentence_progress_total) =
            if self.text_only_mode {
                let plan = self.ensure_current_plan(normalizer);
                let count = plan.audio_sentences.len();
                let idx = self.highlighted_audio_idx.unwrap_or(0);
                let clamped_idx = idx.min(count.saturating_sub(1));
                let fraction = if count == 0 {
                    0.0
                } else {
                    (clamped_idx + 1) as f64 / count as f64
                };
                (fraction, clamped_idx + 1, count)
            } else {
                let count = page_sentence_count;
                let idx = self.highlighted_display_idx.unwrap_or(0);
                let clamped_idx = idx.min(count.saturating_sub(1));
                let fraction = if count == 0 {
                    0.0
                } else {
                    (clamped_idx + 1) as f64 / count as f64
                };
                (fraction, clamped_idx + 1, count)
            };

        let tts_progress_pct = progress_fraction * 100.0;
        let words_up_to_current_position =
            words_before_page + ((page_word_count as f64) * progress_fraction).round() as usize;
        let sentences_up_to_current_position = sentences_before_page
            + (((page_sentence_count as f64) * progress_fraction).round() as usize).min(
                page_sentence_count
                    .max(sentence_progress_count)
                    .min(sentence_progress_total.max(1)),
            );

        let effective_wpm = (BASE_WPM * self.config.tts_speed as f64).max(40.0);
        let page_total_secs = (page_word_count as f64 / effective_wpm) * 60.0;
        let page_time_remaining_secs = page_total_secs * (1.0 - progress_fraction);
        let book_total_secs = (total_words as f64 / effective_wpm) * 60.0;
        let global_word_progress =
            (words_up_to_current_position as f64 / total_words as f64).clamp(0.0, 1.0);
        let book_time_remaining_secs = book_total_secs * (1.0 - global_word_progress);

        let page_start_percent = (words_before_page as f64 / total_words as f64) * 100.0;
        let page_end_percent = (words_up_to_page_end as f64 / total_words as f64) * 100.0;

        ReaderStats {
            page_index: self.current_page + 1,
            total_pages: self.pages.len(),
            tts_progress_pct,
            global_progress_pct: global_word_progress * 100.0,
            page_time_remaining_secs,
            book_time_remaining_secs,
            page_word_count,
            page_sentence_count,
            page_start_percent,
            page_end_percent,
            words_read_up_to_page_start: words_before_page,
            sentences_read_up_to_page_start: sentences_before_page,
            words_read_up_to_page_end: words_up_to_page_end,
            sentences_read_up_to_page_end: sentences_up_to_page_end,
            words_read_up_to_current_position: words_up_to_current_position,
            sentences_read_up_to_current_position: sentences_up_to_current_position,
        }
    }
}

fn count_markdown_anchors(markdown: &str) -> usize {
    markdown
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| {
            line.starts_with('#')
                || line.starts_with("- ")
                || line.starts_with("* ")
                || line
                    .chars()
                    .next()
                    .map(|ch| ch.is_alphanumeric())
                    .unwrap_or(false)
        })
        .count()
}

fn count_html_anchors(html: &str) -> usize {
    const TAGS: [&str; 13] = [
        "<section",
        "<article",
        "<h1",
        "<h2",
        "<h3",
        "<h4",
        "<h5",
        "<h6",
        "<p",
        "<li",
        "<blockquote",
        "<pre",
        "<img",
    ];
    let lower = html.to_ascii_lowercase();
    TAGS.iter()
        .map(|tag| lower.match_indices(tag).count())
        .sum()
}

fn proportional_anchor_map(sentence_count: usize, anchor_count: usize) -> Vec<Option<usize>> {
    if sentence_count == 0 {
        return Vec::new();
    }
    if anchor_count == 0 {
        return (0..sentence_count).map(Some).collect();
    }
    if sentence_count == 1 {
        return vec![Some(0)];
    }
    (0..sentence_count)
        .map(|idx| {
            let mapped = (idx.saturating_mul(anchor_count)) / sentence_count;
            Some(mapped.min(anchor_count.saturating_sub(1)))
        })
        .collect()
}

fn proportional_html_anchor_map(
    page_sentence_counts: &[usize],
    page_idx: usize,
    sentence_count: usize,
    anchor_count: usize,
) -> Vec<Option<usize>> {
    if sentence_count == 0 {
        return Vec::new();
    }
    if anchor_count == 0 {
        return (0..sentence_count).map(Some).collect();
    }
    let total_sentences: usize = page_sentence_counts.iter().sum();
    if total_sentences == 0 {
        return proportional_anchor_map(sentence_count, anchor_count);
    }
    let global_page_start: usize = page_sentence_counts.iter().take(page_idx).sum();
    (0..sentence_count)
        .map(|local_idx| {
            let global_idx = global_page_start.saturating_add(local_idx);
            let mapped = (global_idx.saturating_mul(anchor_count)) / total_sentences;
            Some(mapped.min(anchor_count.saturating_sub(1)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn build_test_session(page_sentences: &[&[&str]]) -> ReaderSession {
        let pages: Vec<String> = page_sentences
            .iter()
            .map(|sentences| sentences.join(" "))
            .collect();
        let raw_page_sentences: Vec<Vec<String>> = page_sentences
            .iter()
            .map(|sentences| {
                sentences
                    .iter()
                    .map(|sentence| sentence.to_string())
                    .collect()
            })
            .collect();
        let page_word_counts: Vec<usize> = pages
            .iter()
            .map(|page| page.split_whitespace().count())
            .collect();
        let page_sentence_counts: Vec<usize> = raw_page_sentences.iter().map(Vec::len).collect();

        ReaderSession {
            source_path: PathBuf::from("/tmp/test.epub"),
            source_name: "test.epub".to_string(),
            tts_text: pages.join("\n\n"),
            reading_markdown: None,
            reading_html: None,
            has_structured_markdown: false,
            pdf_geometry_mode: None,
            pdf_sync_strategy: None,
            pdf_classification: None,
            pdf_runtime_policy: None,
            pdf_ocr_alignment: None,
            pdf_ocr_pipeline: None,
            images: Vec::new(),
            config: config::AppConfig::default(),
            pages,
            markdown_pages: Vec::new(),
            raw_page_sentences,
            sentence_anchor_maps: Vec::new(),
            page_word_counts,
            page_sentence_counts,
            current_page: 0,
            highlighted_display_idx: Some(0),
            highlighted_audio_idx: None,
            text_only_mode: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            selected_search_match: None,
            tts_state: TtsPlaybackState::Paused,
            current_plan_page: None,
            current_plan: None,
        }
    }

    fn apply_pdf_runtime_policy(
        session: &mut ReaderSession,
        policy: crate::epub_loader::PdfRuntimePolicySummary,
    ) {
        session.source_path = PathBuf::from("/tmp/test.pdf");
        session.pdf_runtime_policy = Some(policy);
    }

    fn unique_pdf_source_path() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("lanternleaf_pdf_sync_session_{nanos}.pdf"))
    }

    #[test]
    fn paused_state_is_preserved_when_changing_pages() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B."], &["C.", "D."]]);
        session.current_page = 0;
        session.highlighted_display_idx = Some(1);
        session.tts_state = TtsPlaybackState::Paused;

        session.next_page(&normalizer);

        assert_eq!(session.current_page, 1);
        assert_eq!(session.current_highlight_idx(), Some(0));
        assert_eq!(session.tts_state, TtsPlaybackState::Paused);
    }

    #[test]
    fn paused_state_is_preserved_when_seeking_next_sentence() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B.", "C."]]);
        session.highlighted_display_idx = Some(0);
        session.tts_state = TtsPlaybackState::Paused;

        session.tts_seek_next(&normalizer);

        assert_eq!(session.current_highlight_idx(), Some(1));
        assert_eq!(session.tts_state, TtsPlaybackState::Paused);
    }

    #[test]
    fn paused_state_is_preserved_when_seeking_prev_across_page_boundary() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A."], &["B."]]);
        session.current_page = 1;
        session.highlighted_display_idx = Some(0);
        session.tts_state = TtsPlaybackState::Paused;

        session.tts_seek_prev(&normalizer);

        assert_eq!(session.current_page, 0);
        assert_eq!(session.current_highlight_idx(), Some(0));
        assert_eq!(session.tts_state, TtsPlaybackState::Paused);
    }

    #[test]
    fn sentence_click_keeps_paused_state() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B.", "C."]]);
        session.highlighted_display_idx = Some(0);
        session.tts_state = TtsPlaybackState::Paused;

        session.sentence_click(2, &normalizer);

        assert_eq!(session.current_highlight_idx(), Some(2));
        assert_eq!(session.tts_state, TtsPlaybackState::Paused);
    }

    #[test]
    fn text_only_sentence_click_uses_audio_index_mapping() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&[
            r#"In the word lists of Cheshire, Derbyshire, Lancashire and Yorkshire we find the following terms, all of which took root in the Delaware Valley: abide as in cannot abide it, all out for entirely, apple-pie order to mean very good order, bamboozle for deceive, black and white for writing, blather for empty talk, boggle for take fright, brat for child, budge for move, burying for funeral, by golly as an expletive, by gum for another expletive."#,
        ]]);
        session.tts_state = TtsPlaybackState::Paused;
        session.toggle_text_only(&normalizer);
        let audio_count = session.current_sentences(&normalizer).len();
        assert!(
            audio_count > 1,
            "expected long sentence to split into multiple audio chunks"
        );

        let target_audio_idx = audio_count - 1;
        session.sentence_click(target_audio_idx, &normalizer);

        assert_eq!(session.highlighted_audio_idx, Some(target_audio_idx));
        assert_eq!(session.highlighted_display_idx, Some(0));
        assert_eq!(session.current_highlight_idx(), Some(target_audio_idx));
        assert_eq!(session.tts_state, TtsPlaybackState::Paused);
    }

    #[test]
    fn apply_settings_patch_clamps_pause_speed_and_volume() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B."]]);

        session.apply_settings_patch(
            ReaderSettingsPatch {
                theme: None,
                day_highlight: None,
                night_highlight: None,
                font_family: None,
                font_weight: None,
                font_size: None,
                line_spacing: None,
                word_spacing: None,
                letter_spacing: None,
                margin_horizontal: None,
                margin_vertical: None,
                lines_per_page: None,
                pause_after_sentence: Some(0.056),
                auto_scroll_tts: None,
                center_spoken_sentence: None,
                text_only_show_original_text: None,
                tts_speed: Some(4.9),
                tts_volume: Some(-1.0),
            },
            &normalizer,
        );

        assert!((session.config.pause_after_sentence - 0.06).abs() < f32::EPSILON);
        assert!((session.config.tts_speed - 4.0).abs() < f32::EPSILON);
        assert!((session.config.tts_volume - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn tts_stop_forces_idle_state() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B."]]);
        session.tts_play(&normalizer);
        assert_eq!(session.tts_state, TtsPlaybackState::Playing);

        session.tts_stop();

        assert_eq!(session.tts_state, TtsPlaybackState::Idle);
    }

    #[test]
    fn restore_bookmark_position_preserves_page_and_sentence() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B."], &["C.", "D.", "E."], &["F."]]);

        let bookmark = crate::cache::Bookmark {
            page: 1,
            sentence_idx: Some(2),
            sentence_text: None,
            scroll_y: 0.0,
            pdf_page_idx: None,
            pdf_rects: Vec::new(),
            pdf_line_rects: Vec::new(),
            pdf_block_rects: Vec::new(),
            pdf_confidence: None,
            pdf_reason: None,
            pdf_quality_class: None,
            pdf_sentence_text_hash: None,
            pdf_token_lineage: Vec::new(),
        };
        session.restore_bookmark_position(&bookmark, &normalizer);

        assert_eq!(session.current_page, 1);
        assert_eq!(session.highlighted_display_idx, Some(2));
    }

    #[test]
    fn session_command_dispatch_emits_expected_action_and_snapshot() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B."], &["C.", "D."]]);

        let event =
            session.apply_command(SessionCommand::NextPage, PanelState::default(), &normalizer);

        assert_eq!(event.action, "reader_next_page");
        assert_eq!(event.snapshot.current_page, 1);
        assert_eq!(event.snapshot.highlighted_sentence_idx, Some(0));
    }

    #[test]
    fn session_command_dispatch_preserves_paused_tts_state() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B."], &["C.", "D."]]);
        session.tts_state = TtsPlaybackState::Paused;

        let event =
            session.apply_command(SessionCommand::NextPage, PanelState::default(), &normalizer);

        assert_eq!(session.tts_state, TtsPlaybackState::Paused);
        assert_eq!(event.snapshot.tts.state, TtsPlaybackState::Paused);
    }

    #[test]
    fn session_command_dispatch_applies_settings_patch_with_rounding() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B."]]);

        let event = session.apply_command(
            SessionCommand::ApplySettings {
                patch: ReaderSettingsPatch {
                    theme: None,
                    day_highlight: None,
                    night_highlight: None,
                    font_family: None,
                    font_weight: None,
                    font_size: None,
                    line_spacing: None,
                    word_spacing: None,
                    letter_spacing: None,
                    margin_horizontal: None,
                    margin_vertical: None,
                    lines_per_page: None,
                    pause_after_sentence: Some(0.056),
                    auto_scroll_tts: None,
                    center_spoken_sentence: None,
                    text_only_show_original_text: None,
                    tts_speed: Some(2.5),
                    tts_volume: Some(1.3),
                },
            },
            PanelState::default(),
            &normalizer,
        );

        assert_eq!(event.action, "reader_apply_settings");
        assert!((event.snapshot.settings.pause_after_sentence - 0.06).abs() < f32::EPSILON);
        assert!((event.snapshot.settings.tts_speed - 2.5).abs() < f32::EPSILON);
        assert!((event.snapshot.settings.tts_volume - 1.3).abs() < f32::EPSILON);
    }

    #[test]
    fn tts_highlight_continuity_is_preserved_across_view_toggles() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B.", "C."]]);
        session.highlighted_display_idx = Some(2);

        session.toggle_text_only(&normalizer);
        assert_eq!(session.current_highlight_idx(), Some(2));

        session.toggle_text_only(&normalizer);
        assert_eq!(session.current_highlight_idx(), Some(2));
    }

    #[test]
    fn search_selection_continuity_is_preserved_across_view_toggles() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["Alpha.", "Needle sentence.", "Omega."]]);

        session.set_search_query("Needle".to_string(), &normalizer);
        assert_eq!(session.highlighted_display_idx, Some(1));
        assert_eq!(session.selected_search_match, Some(0));
        assert_eq!(session.search_matches, vec![1]);

        session.toggle_text_only(&normalizer);
        assert_eq!(session.current_highlight_idx(), Some(1));
        assert_eq!(session.selected_search_match, Some(0));
        assert_eq!(session.search_matches, vec![1]);

        session.toggle_text_only(&normalizer);
        assert_eq!(session.highlighted_display_idx, Some(1));
        assert_eq!(session.selected_search_match, Some(0));
        assert_eq!(session.search_matches, vec![1]);
    }

    #[test]
    fn text_only_original_text_changes_display_but_not_tts_audio_plan() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&[
            "This claim remains disputed1.",
            "The next sentence still aligns.",
        ]]);

        session.toggle_text_only(&normalizer);
        let normalized_sentences = session.current_sentences(&normalizer);
        let audio_sentences = session.current_audio_sentences(&normalizer);

        session.apply_settings_patch(
            ReaderSettingsPatch {
                theme: None,
                day_highlight: None,
                night_highlight: None,
                font_family: None,
                font_weight: None,
                font_size: None,
                line_spacing: None,
                word_spacing: None,
                letter_spacing: None,
                margin_horizontal: None,
                margin_vertical: None,
                lines_per_page: None,
                pause_after_sentence: None,
                auto_scroll_tts: None,
                center_spoken_sentence: None,
                text_only_show_original_text: Some(true),
                tts_speed: None,
                tts_volume: None,
            },
            &normalizer,
        );

        let original_display_sentences = session.current_sentences(&normalizer);
        let audio_sentences_after = session.current_audio_sentences(&normalizer);

        assert_ne!(normalized_sentences, original_display_sentences);
        assert_eq!(audio_sentences, audio_sentences_after);
        assert_eq!(
            original_display_sentences,
            vec![
                "This claim remains disputed1.".to_string(),
                "The next sentence still aligns.".to_string()
            ]
        );
    }

    #[test]
    fn pdf_runtime_policy_blocks_text_only_toggle_when_ocr_is_required() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["Alpha.", "Beta."]]);
        apply_pdf_runtime_policy(
            &mut session,
            crate::epub_loader::PdfRuntimePolicySummary {
                text_only_policy: crate::epub_loader::PdfTextOnlyPolicy::OcrRequired,
                sentence_highlight_policy: crate::epub_loader::PdfSentenceHighlightPolicy::Disabled,
                search_policy: crate::epub_loader::PdfSearchPolicy::LimitedText,
                bookmark_policy: crate::epub_loader::PdfBookmarkPolicy::PageOnly,
                tts_allowed: false,
                pretty_sync_enabled: false,
                exact_sentence_sync: false,
                explanation: "OCR required".to_string(),
                degraded_reasons: vec!["ocr_needed_for_text_ownership".to_string()],
            },
        );

        session.toggle_text_only(&normalizer);

        assert!(!session.text_only_mode);
    }

    #[test]
    fn pdf_runtime_policy_blocks_search_when_search_is_disabled() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["Alpha needle.", "Beta."]]);
        apply_pdf_runtime_policy(
            &mut session,
            crate::epub_loader::PdfRuntimePolicySummary {
                text_only_policy: crate::epub_loader::PdfTextOnlyPolicy::LimitedText,
                sentence_highlight_policy:
                    crate::epub_loader::PdfSentenceHighlightPolicy::ParagraphFallback,
                search_policy: crate::epub_loader::PdfSearchPolicy::Disabled,
                bookmark_policy: crate::epub_loader::PdfBookmarkPolicy::PageOnly,
                tts_allowed: true,
                pretty_sync_enabled: true,
                exact_sentence_sync: false,
                explanation: "Search disabled".to_string(),
                degraded_reasons: vec!["render_only_mode".to_string()],
            },
        );

        session.set_search_query("needle".to_string(), &normalizer);

        assert_eq!(session.search_query, "needle");
        assert!(session.search_matches.is_empty());
        assert_eq!(session.selected_search_match, None);
    }

    #[test]
    fn pdf_runtime_policy_blocks_tts_play_when_tts_is_disallowed() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["Alpha.", "Beta."]]);
        apply_pdf_runtime_policy(
            &mut session,
            crate::epub_loader::PdfRuntimePolicySummary {
                text_only_policy: crate::epub_loader::PdfTextOnlyPolicy::Disabled,
                sentence_highlight_policy: crate::epub_loader::PdfSentenceHighlightPolicy::Disabled,
                search_policy: crate::epub_loader::PdfSearchPolicy::Disabled,
                bookmark_policy: crate::epub_loader::PdfBookmarkPolicy::PageOnly,
                tts_allowed: false,
                pretty_sync_enabled: false,
                exact_sentence_sync: false,
                explanation: "TTS disabled".to_string(),
                degraded_reasons: vec!["no_usable_text_available".to_string()],
            },
        );

        session.tts_play(&normalizer);

        assert_eq!(session.tts_state, TtsPlaybackState::Idle);
    }

    #[test]
    fn page_navigation_and_seek_state_stay_aligned_across_view_toggles() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session =
            build_test_session(&[&["Page one alpha.", "Page one beta."], &["Page two gamma."]]);

        session.toggle_text_only(&normalizer);
        session.tts_seek_next(&normalizer);
        assert_eq!(session.current_page, 0);
        assert_eq!(session.current_highlight_idx(), Some(1));
        assert_eq!(session.highlighted_display_idx, Some(1));

        session.set_page(1, &normalizer);
        assert_eq!(session.current_page, 1);
        assert_eq!(session.current_highlight_idx(), Some(0));

        session.toggle_text_only(&normalizer);
        assert_eq!(session.current_page, 1);
        assert_eq!(session.highlighted_display_idx, Some(0));
        assert_eq!(session.current_highlight_idx(), Some(0));
    }

    #[test]
    fn text_only_search_uses_canonical_tts_sentences() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&[
            "This intentionally long sentence contains an uncommon sync token quetzalcoatlus so audio chunking can target it precisely.",
        ]]);

        session.toggle_text_only(&normalizer);
        session.set_search_query("quetzalcoatlus".to_string(), &normalizer);

        assert_eq!(session.selected_search_match, Some(0));
        assert_eq!(session.search_matches.len(), 1);
        assert_eq!(
            session.current_highlight_idx(),
            session.search_matches.first().copied()
        );

        session.toggle_text_only(&normalizer);
        assert_eq!(session.highlighted_display_idx, Some(0));
    }

    #[test]
    fn markdown_anchor_count_detects_blocks() {
        let markdown = "# Title\n\nParagraph one.\n\n- Item one\n- Item two\n\n## Next";
        assert_eq!(count_markdown_anchors(markdown), 5);
    }

    #[test]
    fn html_anchor_count_detects_structural_elements() {
        let html = "<section><h1>A</h1><p>One</p><ul><li>x</li><li>y</li></ul><img src=\"a.png\"/></section>";
        assert_eq!(count_html_anchors(html), 6);
    }

    #[test]
    fn proportional_anchor_map_spreads_sentences_across_anchors() {
        let map = proportional_anchor_map(5, 3);
        assert_eq!(map, vec![Some(0), Some(0), Some(1), Some(1), Some(2)]);
    }

    #[test]
    fn proportional_html_anchor_map_uses_global_sentence_position() {
        let page_sentence_counts = vec![2, 2, 2];
        let page1 = proportional_html_anchor_map(&page_sentence_counts, 0, 2, 6);
        let page2 = proportional_html_anchor_map(&page_sentence_counts, 1, 2, 6);
        let page3 = proportional_html_anchor_map(&page_sentence_counts, 2, 2, 6);
        assert_eq!(page1, vec![Some(0), Some(1)]);
        assert_eq!(page2, vec![Some(2), Some(3)]);
        assert_eq!(page3, vec![Some(4), Some(5)]);
    }

    #[test]
    fn html_sentence_anchor_map_falls_back_to_identity_when_no_pretty_anchors() {
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["A.", "B.", "C."]]);
        session.reading_html = Some("<div>   </div>".to_string());
        session.repaginate(&normalizer, None);
        let sentence_count = session
            .raw_page_sentences
            .get(session.current_page)
            .map(|v| v.len())
            .unwrap_or(0);
        assert_eq!(
            session.current_sentence_anchor_map(),
            (0..sentence_count).map(Some).collect::<Vec<_>>()
        );
    }

    #[test]
    fn build_sentence_anchor_map_for_native_html_uses_current_page_sentence_counts() {
        let mut session = build_test_session(&[&["A1.", "A2."], &["B1.", "B2."], &["C1.", "C2."]]);
        session.config.native_html_pretty_enabled = true;
        session.reading_html =
            Some("<p>A1.</p><p>A2.</p><p>B1.</p><p>B2.</p><p>C1.</p><p>C2.</p>".to_string());

        assert_eq!(
            session.build_sentence_anchor_map_for_page(0, 2),
            vec![Some(0), Some(1)]
        );
        assert_eq!(
            session.build_sentence_anchor_map_for_page(1, 2),
            vec![Some(2), Some(3)]
        );
        assert_eq!(
            session.build_sentence_anchor_map_for_page(2, 2),
            vec![Some(4), Some(5)]
        );
    }

    #[test]
    fn restore_bookmark_position_falls_back_to_sentence_text_match() {
        let mut session = build_test_session(&[&["Alpha.", "Beta."], &["Gamma.", "Delta."]]);
        let bookmark = crate::cache::Bookmark {
            page: 0,
            sentence_idx: Some(99),
            sentence_text: Some("Gamma.".to_string()),
            scroll_y: 0.0,
            pdf_page_idx: None,
            pdf_rects: Vec::new(),
            pdf_line_rects: Vec::new(),
            pdf_block_rects: Vec::new(),
            pdf_confidence: None,
            pdf_reason: None,
            pdf_quality_class: None,
            pdf_sentence_text_hash: None,
            pdf_token_lineage: Vec::new(),
        };
        session.restore_bookmark_position(&bookmark, &normalizer::TextNormalizer::default());
        assert_eq!(session.current_page, 1);
        assert_eq!(session.highlighted_display_idx, Some(0));
    }

    #[test]
    fn restore_bookmark_position_falls_back_to_pdf_ocr_alignment_metadata() {
        let source_path = unique_pdf_source_path();
        fs::write(&source_path, b"pdf").expect("write source");
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["Alpha.", "Beta."], &["Gamma.", "Delta."]]);
        session.source_path = source_path.clone();
        crate::cache::persist_pdf_sentence_map(
            &source_path,
            &[crate::cache::PdfSentenceLocation {
                sentence_idx: 2,
                page_idx: Some(7),
                rects: vec![],
                line_rects: vec![crate::cache::PdfRect {
                    left: 0.1,
                    top: 0.2,
                    width: 0.4,
                    height: 0.05,
                }],
                block_rects: vec![],
                confidence: "fallback".to_string(),
                reason: "line_window_fuzzy_alignment".to_string(),
                score: 0.63,
            }],
        );
        session.refresh_pdf_ocr_alignment_artifact();
        let bookmark = crate::cache::Bookmark {
            page: 0,
            sentence_idx: None,
            sentence_text: None,
            scroll_y: 0.0,
            pdf_page_idx: Some(7),
            pdf_rects: Vec::new(),
            pdf_line_rects: Vec::new(),
            pdf_block_rects: Vec::new(),
            pdf_confidence: Some("line_fallback".to_string()),
            pdf_reason: Some("line_window_fuzzy_alignment".to_string()),
            pdf_quality_class: Some(crate::epub_loader::PdfOcrGeometryQualityClass::OcrMixedTrust),
            pdf_sentence_text_hash: Some(crate::cache::stable_sentence_text_hash("Gamma.")),
            pdf_token_lineage: Vec::new(),
        };

        session.restore_bookmark_position(&bookmark, &normalizer);

        assert_eq!(session.current_page, 1);
        assert_eq!(session.highlighted_display_idx, Some(0));
        let _ = crate::cache::delete_recent_source_and_cache(&source_path);
    }

    #[test]
    fn bookmark_carries_cached_pdf_location_metadata_for_current_sentence() {
        let source_path = unique_pdf_source_path();
        fs::write(&source_path, b"pdf").expect("write source");
        let mut session = build_test_session(&[&["Alpha.", "Beta."], &["Gamma.", "Delta."]]);
        session.source_path = source_path.clone();
        session.current_page = 1;
        session.highlighted_display_idx = Some(0);
        crate::cache::persist_pdf_sentence_map(
            &source_path,
            &[crate::cache::PdfSentenceLocation {
                sentence_idx: 2,
                page_idx: Some(7),
                rects: vec![crate::cache::PdfRect {
                    left: 0.1,
                    top: 0.2,
                    width: 0.4,
                    height: 0.05,
                }],
                line_rects: vec![crate::cache::PdfRect {
                    left: 0.1,
                    top: 0.2,
                    width: 0.4,
                    height: 0.05,
                }],
                block_rects: vec![crate::cache::PdfRect {
                    left: 0.1,
                    top: 0.2,
                    width: 0.4,
                    height: 0.05,
                }],
                confidence: "exact".to_string(),
                reason: "exact_geometry".to_string(),
                score: 1.0,
            }],
        );
        session.refresh_pdf_ocr_alignment_artifact();

        let bookmark = session.to_bookmark();
        assert_eq!(bookmark.page, 1);
        assert_eq!(bookmark.pdf_page_idx, Some(7));
        assert_eq!(bookmark.pdf_confidence.as_deref(), Some("sentence_rects"));
        assert_eq!(bookmark.pdf_reason.as_deref(), Some("exact_geometry"));
        assert_eq!(bookmark.pdf_rects.len(), 1);
        assert_eq!(bookmark.pdf_line_rects.len(), 1);
        assert_eq!(bookmark.pdf_block_rects.len(), 1);
        assert_eq!(
            bookmark.pdf_sentence_text_hash.as_deref(),
            Some(crate::cache::stable_sentence_text_hash("Gamma.").as_str())
        );

        let _ = crate::cache::delete_recent_source_and_cache(&source_path);
    }

    #[test]
    fn refresh_pdf_ocr_alignment_artifact_reuses_unchanged_sentence_alignments() {
        let source_path = unique_pdf_source_path();
        fs::write(&source_path, b"pdf").expect("write source");
        let mut session = build_test_session(&[&["Alpha.", "Beta."], &["Gamma.", "Delta."]]);
        session.source_path = source_path.clone();
        crate::cache::persist_pdf_sentence_map(
            &source_path,
            &[
                crate::cache::PdfSentenceLocation {
                    sentence_idx: 0,
                    page_idx: Some(1),
                    rects: vec![crate::cache::PdfRect {
                        left: 0.1,
                        top: 0.2,
                        width: 0.3,
                        height: 0.04,
                    }],
                    line_rects: vec![],
                    block_rects: vec![],
                    confidence: "exact".to_string(),
                    reason: "exact_geometry".to_string(),
                    score: 1.0,
                },
                crate::cache::PdfSentenceLocation {
                    sentence_idx: 1,
                    page_idx: Some(1),
                    rects: vec![],
                    line_rects: vec![crate::cache::PdfRect {
                        left: 0.1,
                        top: 0.25,
                        width: 0.3,
                        height: 0.04,
                    }],
                    block_rects: vec![],
                    confidence: "fallback".to_string(),
                    reason: "line_window_fuzzy_alignment".to_string(),
                    score: 0.7,
                },
            ],
        );

        session.refresh_pdf_ocr_alignment_artifact();
        let first = crate::cache::load_pdf_ocr_alignment_artifact(&source_path)
            .expect("alignment artifact should exist");
        session.refresh_pdf_ocr_alignment_artifact();
        let second = crate::cache::load_pdf_ocr_alignment_artifact(&source_path)
            .expect("alignment artifact should still exist");

        assert_eq!(first.alignments, second.alignments);
        assert_eq!(second.reused_alignment_count, 4);
        assert_eq!(second.rebuilt_alignment_count, 0);
        assert_eq!(second.page_buckets.len(), 1);

        let _ = crate::cache::delete_recent_source_and_cache(&source_path);
    }

    #[test]
    fn pdf_ocr_alignment_summary_reports_exact_fallback_and_page_only_rates() {
        let source_path = unique_pdf_source_path();
        fs::write(&source_path, b"pdf").expect("write source");
        let mut session = build_test_session(&[&["Alpha.", "Beta."], &["Gamma.", "Delta."]]);
        session.source_path = source_path.clone();
        crate::cache::persist_pdf_sentence_map(
            &source_path,
            &[
                crate::cache::PdfSentenceLocation {
                    sentence_idx: 0,
                    page_idx: Some(1),
                    rects: vec![crate::cache::PdfRect {
                        left: 0.1,
                        top: 0.2,
                        width: 0.3,
                        height: 0.04,
                    }],
                    line_rects: vec![],
                    block_rects: vec![],
                    confidence: "exact".to_string(),
                    reason: "exact_geometry".to_string(),
                    score: 1.0,
                },
                crate::cache::PdfSentenceLocation {
                    sentence_idx: 1,
                    page_idx: Some(1),
                    rects: vec![],
                    line_rects: vec![crate::cache::PdfRect {
                        left: 0.1,
                        top: 0.25,
                        width: 0.3,
                        height: 0.04,
                    }],
                    block_rects: vec![],
                    confidence: "fallback".to_string(),
                    reason: "line_window_fuzzy_alignment".to_string(),
                    score: 0.7,
                },
                crate::cache::PdfSentenceLocation {
                    sentence_idx: 2,
                    page_idx: Some(2),
                    rects: vec![],
                    line_rects: vec![],
                    block_rects: vec![],
                    confidence: "page".to_string(),
                    reason: "page_location_only".to_string(),
                    score: 0.2,
                },
            ],
        );

        session.refresh_pdf_ocr_alignment_artifact();
        let summary = session
            .pdf_ocr_alignment
            .clone()
            .expect("pdf ocr summary should exist");

        assert!((summary.exact_sentence_rate - 0.25).abs() < f32::EPSILON);
        assert!((summary.degraded_fallback_rate - 0.25).abs() < f32::EPSILON);
        assert!((summary.page_only_rate - 0.25).abs() < f32::EPSILON);
        assert!((summary.unmappable_rate - 0.25).abs() < f32::EPSILON);
        assert_eq!(summary.geometry_block_count, 2);
        assert_eq!(summary.geometry_line_count, 2);
        assert!(summary.geometry_token_count >= 2);
        assert_eq!(summary.page_timing_count, 2);
        assert_eq!(summary.chunk_timing_count, 1);

        let _ = crate::cache::delete_recent_source_and_cache(&source_path);
    }

    #[test]
    fn pdf_ocr_alignment_artifact_populates_token_lineage_and_cross_column_contract() {
        let source_path = unique_pdf_source_path();
        fs::write(&source_path, b"pdf").expect("write source");
        let mut session = build_test_session(&[&["Alpha beta gamma."], &["Delta epsilon."]]);
        session.source_path = source_path.clone();
        crate::cache::persist_pdf_sentence_map(
            &source_path,
            &[crate::cache::PdfSentenceLocation {
                sentence_idx: 0,
                page_idx: Some(3),
                rects: vec![
                    crate::cache::PdfRect {
                        left: 0.08,
                        top: 0.2,
                        width: 0.18,
                        height: 0.04,
                    },
                    crate::cache::PdfRect {
                        left: 0.62,
                        top: 0.2,
                        width: 0.18,
                        height: 0.04,
                    },
                ],
                line_rects: vec![
                    crate::cache::PdfRect {
                        left: 0.08,
                        top: 0.2,
                        width: 0.18,
                        height: 0.04,
                    },
                    crate::cache::PdfRect {
                        left: 0.62,
                        top: 0.2,
                        width: 0.18,
                        height: 0.04,
                    },
                ],
                block_rects: vec![
                    crate::cache::PdfRect {
                        left: 0.08,
                        top: 0.18,
                        width: 0.18,
                        height: 0.08,
                    },
                    crate::cache::PdfRect {
                        left: 0.62,
                        top: 0.18,
                        width: 0.18,
                        height: 0.08,
                    },
                ],
                confidence: "exact".to_string(),
                reason: "exact_geometry".to_string(),
                score: 0.93,
            }],
        );

        session.refresh_pdf_ocr_alignment_artifact();
        let artifact = crate::cache::load_pdf_ocr_alignment_artifact(&source_path)
            .expect("alignment artifact should exist");
        let alignment = artifact
            .alignments
            .iter()
            .find(|alignment| alignment.sentence_idx == 0)
            .expect("sentence alignment should exist");

        assert!(artifact.token_lineage_available);
        assert!(!alignment.token_lineage.is_empty());
        assert!(alignment.crosses_column_boundaries);
        assert!(alignment.cross_column_confident);
        assert_eq!(artifact.cross_column_alignment_count, 1);
        assert_eq!(artifact.cross_column_confident_alignment_count, 1);
        assert_eq!(artifact.blocks.len(), 2);
        assert_eq!(artifact.lines.len(), 2);
        assert_eq!(artifact.page_geometry.len(), 1);
        assert_eq!(
            artifact.page_geometry[0].reading_order_mode,
            "cross_column_confident"
        );
        assert_eq!(artifact.page_build_ms.len(), 1);
        assert_eq!(artifact.chunk_build_ms.len(), 1);

        let _ = crate::cache::delete_recent_source_and_cache(&source_path);
    }

    #[test]
    fn pdf_toggle_text_only_preserves_tts_highlight_and_ocr_summary() {
        let source_path = unique_pdf_source_path();
        fs::write(&source_path, b"pdf").expect("write source");
        let normalizer = normalizer::TextNormalizer::default();
        let mut session = build_test_session(&[&["Alpha.", "Beta."], &["Gamma.", "Delta."]]);
        session.source_path = source_path.clone();
        apply_pdf_runtime_policy(
            &mut session,
            crate::epub_loader::PdfRuntimePolicySummary {
                text_only_policy: crate::epub_loader::PdfTextOnlyPolicy::FullText,
                sentence_highlight_policy:
                    crate::epub_loader::PdfSentenceHighlightPolicy::ParagraphFallback,
                search_policy: crate::epub_loader::PdfSearchPolicy::FullText,
                bookmark_policy: crate::epub_loader::PdfBookmarkPolicy::CanonicalText,
                tts_allowed: true,
                pretty_sync_enabled: true,
                exact_sentence_sync: false,
                explanation: "test".to_string(),
                degraded_reasons: vec!["sentence_sync_not_exact".to_string()],
            },
        );
        crate::cache::persist_pdf_sentence_map(
            &source_path,
            &[crate::cache::PdfSentenceLocation {
                sentence_idx: 2,
                page_idx: Some(7),
                rects: vec![],
                line_rects: vec![crate::cache::PdfRect {
                    left: 0.1,
                    top: 0.2,
                    width: 0.4,
                    height: 0.05,
                }],
                block_rects: vec![],
                confidence: "fallback".to_string(),
                reason: "line_window_fuzzy_alignment".to_string(),
                score: 0.63,
            }],
        );
        session.current_page = 1;
        session.highlighted_display_idx = Some(0);
        session.refresh_pdf_ocr_alignment_artifact();
        let baseline_summary = session
            .pdf_ocr_alignment
            .clone()
            .expect("pdf summary should exist");
        let baseline_highlight = session.current_highlight_idx();

        session.toggle_text_only(&normalizer);
        let text_only_highlight = session.current_highlight_idx();
        let text_only_audio_highlight = session.highlighted_audio_idx;
        let text_only_summary = session
            .pdf_ocr_alignment
            .clone()
            .expect("pdf summary should still exist");

        session.toggle_text_only(&normalizer);
        let pretty_highlight = session.current_highlight_idx();
        let pretty_summary = session
            .pdf_ocr_alignment
            .clone()
            .expect("pdf summary should still exist after toggling back");

        assert_eq!(baseline_highlight, Some(0));
        assert_eq!(text_only_highlight, Some(0));
        assert_eq!(text_only_audio_highlight, baseline_highlight);
        assert_eq!(pretty_highlight, baseline_highlight);
        assert_eq!(
            text_only_summary.quality_class,
            baseline_summary.quality_class
        );
        assert_eq!(text_only_summary.source_kind, baseline_summary.source_kind);
        assert_eq!(
            text_only_summary.coverage_ratio,
            baseline_summary.coverage_ratio
        );
        assert_eq!(
            text_only_summary.degraded_fallback_rate,
            baseline_summary.degraded_fallback_rate
        );
        assert_eq!(
            text_only_summary.page_only_rate,
            baseline_summary.page_only_rate
        );
        assert_eq!(pretty_summary.quality_class, baseline_summary.quality_class);
        assert_eq!(pretty_summary.source_kind, baseline_summary.source_kind);
        assert_eq!(
            pretty_summary.coverage_ratio,
            baseline_summary.coverage_ratio
        );
        assert_eq!(
            pretty_summary.degraded_fallback_rate,
            baseline_summary.degraded_fallback_rate
        );
        assert_eq!(
            pretty_summary.page_only_rate,
            baseline_summary.page_only_rate
        );

        let _ = crate::cache::delete_recent_source_and_cache(&source_path);
    }
}
