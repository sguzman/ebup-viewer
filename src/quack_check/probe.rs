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
                    first_line: page.first_line,
                    last_line: page.last_line,
                })
                .collect(),
        },
    })
}
