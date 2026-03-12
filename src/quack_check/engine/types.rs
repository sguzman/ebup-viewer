use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocDiag {
    pub python_exe: String,
    pub python_version: String,
    pub docling_version: Option<String>,
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbePageOut {
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
    #[serde(default)]
    pub image_object_count: u32,
    #[serde(default)]
    pub image_coverage_ratio: f32,
    #[serde(default)]
    pub duplicate_text_ratio: f32,
    #[serde(default)]
    pub block_coherence: f32,
    #[serde(default)]
    pub coordinate_sanity: f32,
    #[serde(default)]
    pub reading_order_stability: f32,
    #[serde(default)]
    pub hidden_text_layer_suspected: bool,
    #[serde(default)]
    pub invisible_text_suspected: bool,
    #[serde(default)]
    pub duplicate_text_suspected: bool,
    #[serde(default)]
    pub stacked_duplicate_text_suspected: bool,
    #[serde(default)]
    pub mixed_text_image_suspected: bool,
    #[serde(default)]
    pub full_page_raster_suspected: bool,
    #[serde(default)]
    pub first_line: String,
    #[serde(default)]
    pub last_line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeOut {
    pub page_count: u32,
    pub sampled_pages: u32,
    pub avg_chars_per_page: u32,
    pub garbage_ratio: f32,
    pub whitespace_ratio: f32,
    #[serde(default)]
    pub text_page_ratio: f32,
    #[serde(default)]
    pub empty_text_page_ratio: f32,
    #[serde(default)]
    pub sparse_text_page_ratio: f32,
    #[serde(default)]
    pub noisy_text_page_ratio: f32,
    #[serde(default)]
    pub repeated_header_ratio: f32,
    #[serde(default)]
    pub repeated_footer_ratio: f32,
    #[serde(default)]
    pub image_page_ratio: f32,
    #[serde(default)]
    pub mixed_text_image_page_ratio: f32,
    #[serde(default)]
    pub full_page_raster_page_ratio: f32,
    #[serde(default)]
    pub hidden_text_layer_page_ratio: f32,
    #[serde(default)]
    pub invisible_text_layer_page_ratio: f32,
    #[serde(default)]
    pub duplicate_text_page_ratio: f32,
    #[serde(default)]
    pub stacked_duplicate_text_page_ratio: f32,
    #[serde(default)]
    pub pages: Vec<ProbePageOut>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertIn {
    pub input_pdf: String,
    pub out_dir: String,
    pub chunk_index: u32,
    pub start_page: u32,
    pub end_page: u32,
    pub do_ocr: bool,
    pub pdf_backend: String,
    pub use_page_range: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertOut {
    pub ok: bool,
    pub markdown: String,
    pub warnings: Vec<String>,
    pub meta: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfPageTextOut {
    pub ok: bool,
    #[serde(default)]
    pub page_texts: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub meta: serde_json::Value,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitChunk {
    pub chunk_index: u32,
    pub start_page: u32,
    pub end_page: u32,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitOut {
    pub ok: bool,
    #[serde(default)]
    pub outputs: Vec<SplitChunk>,
    #[serde(default)]
    pub error: Option<String>,
}
