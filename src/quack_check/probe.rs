use crate::quack_check::{config::Config, engine::Engine};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbePageStats {
    pub page_index: u32,
    pub char_count: u32,
    pub token_count: u32,
    pub line_count: u32,
    pub whitespace_ratio: f32,
    pub garbage_ratio: f32,
    pub punctuation_ratio: f32,
    pub digit_ratio: f32,
    pub non_latin_ratio: f32,
    pub alpha_char_ratio: f32,
    pub uppercase_char_ratio: f32,
    pub alpha_token_ratio: f32,
    pub avg_token_length: f32,
    pub short_line_ratio: f32,
    pub repeated_line_ratio: f32,
    pub hyphenated_line_ratio: f32,
    pub image_object_count: u32,
    pub image_coverage_ratio: f32,
    pub duplicate_text_ratio: f32,
    pub block_coherence: f32,
    pub coordinate_sanity: f32,
    pub reading_order_stability: f32,
    pub hidden_text_layer_suspected: bool,
    pub duplicate_text_suspected: bool,
    pub mixed_text_image_suspected: bool,
    pub full_page_raster_suspected: bool,
    pub first_line: String,
    pub last_line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub input: ProbeInput,
    pub sample: ProbeSampleStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeInput {
    pub path: String,
    pub file_bytes: u64,
    pub page_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeSampleStats {
    pub sampled_pages: u32,
    pub avg_chars_per_page: u32,
    pub garbage_ratio: f32,
    pub whitespace_ratio: f32,
    pub text_page_ratio: f32,
    pub empty_text_page_ratio: f32,
    pub sparse_text_page_ratio: f32,
    pub noisy_text_page_ratio: f32,
    pub repeated_header_ratio: f32,
    pub repeated_footer_ratio: f32,
    pub image_page_ratio: f32,
    pub mixed_text_image_page_ratio: f32,
    pub full_page_raster_page_ratio: f32,
    pub hidden_text_layer_page_ratio: f32,
    pub duplicate_text_page_ratio: f32,
    pub pages: Vec<ProbePageStats>,
}

pub fn probe_pdf(cfg: &Config, engine: &dyn Engine, input: &Path) -> Result<ProbeResult> {
    let meta = std::fs::metadata(input).with_context(|| "stat input")?;
    let file_bytes = meta.len();
    if file_bytes > cfg.limits.max_input_file_bytes {
        anyhow::bail!("input exceeds max_input_file_bytes: {}", file_bytes);
    }

    let probe = engine
        .probe_pdf(input, cfg.classification.sample_pages)
        .with_context(|| "engine probe_pdf failed")?;

    if probe.page_count > cfg.limits.max_input_pages {
        anyhow::bail!("input exceeds max_input_pages: {}", probe.page_count);
    }
    if probe.page_count == 0 {
        anyhow::bail!("input has zero pages");
    }

    info!(
        path = %input.display(),
        page_count = probe.page_count,
        sampled_pages = probe.sampled_pages,
        avg_chars_per_page = probe.avg_chars_per_page,
        garbage_ratio = probe.garbage_ratio,
        whitespace_ratio = probe.whitespace_ratio,
        text_page_ratio = probe.text_page_ratio,
        empty_text_page_ratio = probe.empty_text_page_ratio,
        sparse_text_page_ratio = probe.sparse_text_page_ratio,
        noisy_text_page_ratio = probe.noisy_text_page_ratio,
        repeated_header_ratio = probe.repeated_header_ratio,
        repeated_footer_ratio = probe.repeated_footer_ratio,
        image_page_ratio = probe.image_page_ratio,
        mixed_text_image_page_ratio = probe.mixed_text_image_page_ratio,
        full_page_raster_page_ratio = probe.full_page_raster_page_ratio,
        hidden_text_layer_page_ratio = probe.hidden_text_layer_page_ratio,
        duplicate_text_page_ratio = probe.duplicate_text_page_ratio,
        "Collected PDF probe features"
    );
    for page in &probe.pages {
        debug!(
            page_index = page.page_index,
            char_count = page.char_count,
            token_count = page.token_count,
            line_count = page.line_count,
            whitespace_ratio = page.whitespace_ratio,
            garbage_ratio = page.garbage_ratio,
            punctuation_ratio = page.punctuation_ratio,
            digit_ratio = page.digit_ratio,
            non_latin_ratio = page.non_latin_ratio,
            alpha_char_ratio = page.alpha_char_ratio,
            uppercase_char_ratio = page.uppercase_char_ratio,
            alpha_token_ratio = page.alpha_token_ratio,
            avg_token_length = page.avg_token_length,
            short_line_ratio = page.short_line_ratio,
            repeated_line_ratio = page.repeated_line_ratio,
            hyphenated_line_ratio = page.hyphenated_line_ratio,
            image_object_count = page.image_object_count,
            image_coverage_ratio = page.image_coverage_ratio,
            duplicate_text_ratio = page.duplicate_text_ratio,
            block_coherence = page.block_coherence,
            coordinate_sanity = page.coordinate_sanity,
            reading_order_stability = page.reading_order_stability,
            hidden_text_layer_suspected = page.hidden_text_layer_suspected,
            duplicate_text_suspected = page.duplicate_text_suspected,
            mixed_text_image_suspected = page.mixed_text_image_suspected,
            full_page_raster_suspected = page.full_page_raster_suspected,
            first_line = %page.first_line,
            last_line = %page.last_line,
            "Collected sampled PDF page probe features"
        );
    }

    Ok(ProbeResult {
        input: ProbeInput {
            path: input.display().to_string(),
            file_bytes,
            page_count: probe.page_count,
        },
        sample: ProbeSampleStats {
            sampled_pages: probe.sampled_pages,
            avg_chars_per_page: probe.avg_chars_per_page,
            garbage_ratio: probe.garbage_ratio,
            whitespace_ratio: probe.whitespace_ratio,
            text_page_ratio: probe.text_page_ratio,
            empty_text_page_ratio: probe.empty_text_page_ratio,
            sparse_text_page_ratio: probe.sparse_text_page_ratio,
            noisy_text_page_ratio: probe.noisy_text_page_ratio,
            repeated_header_ratio: probe.repeated_header_ratio,
            repeated_footer_ratio: probe.repeated_footer_ratio,
            image_page_ratio: probe.image_page_ratio,
            mixed_text_image_page_ratio: probe.mixed_text_image_page_ratio,
            full_page_raster_page_ratio: probe.full_page_raster_page_ratio,
            hidden_text_layer_page_ratio: probe.hidden_text_layer_page_ratio,
            duplicate_text_page_ratio: probe.duplicate_text_page_ratio,
            pages: probe
                .pages
                .into_iter()
                .map(|page| ProbePageStats {
                    page_index: page.page_index,
                    char_count: page.char_count,
                    token_count: page.token_count,
                    line_count: page.line_count,
                    whitespace_ratio: page.whitespace_ratio,
                    garbage_ratio: page.garbage_ratio,
                    punctuation_ratio: page.punctuation_ratio,
                    digit_ratio: page.digit_ratio,
                    non_latin_ratio: page.non_latin_ratio,
                    alpha_char_ratio: page.alpha_char_ratio,
                    uppercase_char_ratio: page.uppercase_char_ratio,
                    alpha_token_ratio: page.alpha_token_ratio,
                    avg_token_length: page.avg_token_length,
                    short_line_ratio: page.short_line_ratio,
                    repeated_line_ratio: page.repeated_line_ratio,
                    hyphenated_line_ratio: page.hyphenated_line_ratio,
                    image_object_count: page.image_object_count,
                    image_coverage_ratio: page.image_coverage_ratio,
                    duplicate_text_ratio: page.duplicate_text_ratio,
                    block_coherence: page.block_coherence,
                    coordinate_sanity: page.coordinate_sanity,
                    reading_order_stability: page.reading_order_stability,
                    hidden_text_layer_suspected: page.hidden_text_layer_suspected,
                    duplicate_text_suspected: page.duplicate_text_suspected,
                    mixed_text_image_suspected: page.mixed_text_image_suspected,
                    full_page_raster_suspected: page.full_page_raster_suspected,
                    first_line: page.first_line,
                    last_line: page.last_line,
                })
                .collect(),
        },
    })
}
