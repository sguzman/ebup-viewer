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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfZoomDirection {
    In,
    Out,
}

#[derive(Debug, Clone)]
pub struct PdfZoomPolicy {
    levels: Vec<f32>,
}

impl PdfZoomPolicy {
    pub fn new(levels: &[f32]) -> Self {
        let mut levels = levels.to_vec();
        levels.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        levels.dedup();
        Self { levels }
    }

    pub fn step_zoom(&self, current: f32, direction: PdfZoomDirection) -> f32 {
        let mut candidates = self.levels.iter().copied().collect::<Vec<_>>();
        if candidates.is_empty() {
            return current;
        }
        candidates.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        match direction {
            PdfZoomDirection::In => candidates
                .into_iter()
                .find(|level| *level > current)
                .unwrap_or(current),
            PdfZoomDirection::Out => candidates
                .into_iter()
                .rev()
                .find(|level| *level < current)
                .unwrap_or(current),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PdfScrollPolicy {
    threshold_pages: usize,
    last_target_page: Option<usize>,
}

impl PdfScrollPolicy {
    pub fn new(threshold_pages: usize) -> Self {
        Self {
            threshold_pages,
            last_target_page: None,
        }
    }

    pub fn threshold_pages(&self) -> usize {
        self.threshold_pages
    }

    pub fn should_scroll(&mut self, target_page: usize) -> bool {
        let allowed = match self.last_target_page {
            None => true,
            Some(last) => last.abs_diff(target_page) >= self.threshold_pages.max(1),
        };
        if allowed {
            self.last_target_page = Some(target_page);
        }
        allowed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_policy_steps_to_next_level() {
        let policy = PdfZoomPolicy::new(&PDF_ZOOM_LEVELS);
        assert_eq!(policy.step_zoom(1.0, PdfZoomDirection::In), 1.1);
        assert_eq!(policy.step_zoom(1.1, PdfZoomDirection::Out), 1.0);
    }

    #[test]
    fn scroll_policy_throttles_within_threshold() {
        let mut policy = PdfScrollPolicy::new(2);
        assert!(policy.should_scroll(3));
        assert!(!policy.should_scroll(4));
        assert!(policy.should_scroll(5));
    }
}
