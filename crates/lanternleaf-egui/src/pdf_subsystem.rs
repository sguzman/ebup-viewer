#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PdfRenderPriority {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PdfViewportUpdateTrigger {
    Init,
    Scroll,
    Jump,
    Tts,
    Refresh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PdfViewportRange {
    pub start: usize,
    pub end: usize,
}

impl PdfViewportRange {
    pub fn from_pages(pages: &[usize]) -> Option<Self> {
        if pages.is_empty() {
            return None;
        }
        let mut min = usize::MAX;
        let mut max = 0usize;
        for &page in pages {
            min = min.min(page);
            max = max.max(page);
        }
        Some(Self { start: min, end: max })
    }

    pub fn as_inclusive_range(&self) -> RangeInclusive<usize> {
        self.start..=self.end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PdfRenderRequest {
    pub page_index: usize,
    pub zoom_level: f32,
    pub priority: PdfRenderPriority,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PdfRenderCompletion {
    pub page_index: usize,
    pub zoom_level: f32,
    pub priority: PdfRenderPriority,
    pub duration_ms: f32,
    pub cache_hit: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PdfTextureUploadHandle {
    pub page_index: usize,
    pub zoom_level: f32,
    pub priority: PdfRenderPriority,
}

pub trait PageRenderService: Send + Sync {
    fn request_render(&mut self, request: PdfRenderRequest);
}

pub trait TextExtractionService: Send + Sync {
    fn request_page_text(&mut self, page_index: usize);
}

pub trait SyncMapBuilder: Send + Sync {
    fn request_sync_map(&mut self, page_index: usize);
}

pub trait OcrArtifactLoader: Send + Sync {
    fn request_ocr_artifacts(&mut self, page_index: usize);
}

pub trait ViewportLifecycleManager: Send + Sync {
    fn update_viewport(
        &mut self,
        visible_range: Option<PdfViewportRange>,
        overscan_range: Option<PdfViewportRange>,
        trigger: PdfViewportUpdateTrigger,
    );
}

pub trait OverlayAndHighlightManager: Send + Sync {
    fn refresh_overlays(&mut self);
}

pub const PDF_ZOOM_LEVELS: [f32; 7] = [0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75];
pub const PDF_DEFAULT_ZOOM_LEVEL: f32 = 1.0;
