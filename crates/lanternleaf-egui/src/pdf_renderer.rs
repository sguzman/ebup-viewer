use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    convert::TryFrom,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crate::{PDF_CANVAS_TEXTURE_SIZE, PDF_TEXT_TEXTURE_SIZE};
use eframe::egui::ColorImage;
use pdfium_auto::bind_bundled;
use pdfium_render::prelude::*;

const CACHE_CAPACITY: usize = 32;

#[allow(dead_code)]
#[derive(Debug)]
pub enum NativePdfRendererError {
    Auto(pdfium_auto::PdfiumAutoError),
    Pdfium(PdfiumError),
    PageIndexOutOfBounds(usize),
}

impl From<pdfium_auto::PdfiumAutoError> for NativePdfRendererError {
    fn from(value: pdfium_auto::PdfiumAutoError) -> Self {
        NativePdfRendererError::Auto(value)
    }
}

impl From<PdfiumError> for NativePdfRendererError {
    fn from(value: PdfiumError) -> Self {
        NativePdfRendererError::Pdfium(value)
    }
}

pub struct NativePdfRenderer {
    pdfium: Pdfium,
    cache: HashMap<RenderCacheKey, ColorImage>,
    order: VecDeque<RenderCacheKey>,
    capacity: usize,
    eviction_events: Vec<NativeRenderEviction>,
}

impl NativePdfRenderer {
    pub fn new() -> Result<Self, NativePdfRendererError> {
        let pdfium = bind_bundled()?;
        Ok(Self {
            pdfium,
            cache: HashMap::new(),
            order: VecDeque::new(),
            capacity: CACHE_CAPACITY,
            eviction_events: Vec::new(),
        })
    }

    pub fn render_canvas(
        &mut self,
        source_path: &Path,
        page_index: usize,
    ) -> Result<RenderOutcome, NativePdfRendererError> {
        self.render_for_target(source_path, page_index, RenderTarget::Canvas)
    }

    pub fn render_text_layer(
        &mut self,
        source_path: &Path,
        page_index: usize,
    ) -> Result<RenderOutcome, NativePdfRendererError> {
        self.render_for_target(source_path, page_index, RenderTarget::TextLayer)
    }

    fn render_for_target(
        &mut self,
        source_path: &Path,
        page_index: usize,
        target: RenderTarget,
    ) -> Result<RenderOutcome, NativePdfRendererError> {
        let key = RenderCacheKey {
            source: source_path.to_path_buf(),
            page_index,
            target,
        };
        if let Some(image) = self.cache.get(&key) {
            return Ok(RenderOutcome {
                image: image.clone(),
                duration: Duration::from_micros(0),
                cache_hit: true,
            });
        }

        let start = Instant::now();
        let (image, duration) = {
            let document = self.pdfium.load_pdf_from_file(source_path, None)?;
            let page_index = PdfPageIndex::try_from(page_index)
                .map_err(|_| NativePdfRendererError::PageIndexOutOfBounds(page_index))?;
            let page = document.pages().get(page_index)?;
            let dims = target.render_dimensions(&page);
            let config = PdfRenderConfig::new()
                .set_target_width(dims.width)
                .set_target_height(dims.height);

            let bitmap = page.render_with_config(&config)?;
            (Self::bitmap_to_color_image(bitmap)?, start.elapsed())
        };
        self.insert_cache(key.clone(), image.clone());
        Ok(RenderOutcome {
            image,
            duration,
            cache_hit: false,
        })
    }

    fn insert_cache(&mut self, key: RenderCacheKey, image: ColorImage) {
        if self.cache.contains_key(&key) {
            self.order.retain(|existing| existing != &key);
        }
        while self.cache.len() >= self.capacity {
            if let Some(old_key) = self.order.pop_front() {
                self.cache.remove(&old_key);
                self.eviction_events.push(NativeRenderEviction {
                    timestamp: Instant::now(),
                    target: old_key.target,
                    page_index: old_key.page_index,
                    reason: "capacity_evicted".to_string(),
                });
            }
        }
        self.order.push_back(key.clone());
        self.cache.insert(key, image);
    }

    fn bitmap_to_color_image(bitmap: PdfBitmap<'_>) -> Result<ColorImage, NativePdfRendererError> {
        let rgba = bitmap.as_image().into_rgba8();
        let width = rgba.width() as usize;
        let height = rgba.height() as usize;
        let raw = rgba.into_raw();
        Ok(ColorImage::from_rgba_unmultiplied([width, height], &raw))
    }

    pub fn drain_eviction_events(&mut self) -> Vec<NativeRenderEviction> {
        std::mem::take(&mut self.eviction_events)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum RenderTarget {
    Canvas,
    TextLayer,
}

impl RenderTarget {
    fn render_dimensions(&self, page: &PdfPage) -> RenderDimensions {
        let max_size = match self {
            RenderTarget::Canvas => PDF_CANVAS_TEXTURE_SIZE,
            RenderTarget::TextLayer => PDF_TEXT_TEXTURE_SIZE,
        };
        let max_width = max_size[0] as f32;
        let max_height = max_size[1] as f32;
        let page_width = page.width().value.abs().max(1.0);
        let page_height = page.height().value.abs().max(1.0);
        let mut width = max_width;
        let mut height = (width * (page_height / page_width)).round();
        if height > max_height {
            height = max_height;
            width = (height * (page_width / page_height)).round();
        }
        let width = width.max(1.0).round() as Pixels;
        let height = height.max(1.0).round() as Pixels;
        RenderDimensions { width, height }
    }

    pub(crate) fn label(&self) -> &'static str {
        match self {
            RenderTarget::Canvas => "canvas",
            RenderTarget::TextLayer => "text-layer",
        }
    }
}

pub struct RenderOutcome {
    pub image: ColorImage,
    pub duration: Duration,
    pub cache_hit: bool,
}

#[derive(Clone, Debug)]
pub struct NativeRenderSpan {
    pub timestamp: Instant,
    pub target: RenderTarget,
    pub page_index: usize,
    pub duration: Duration,
    pub cache_hit: bool,
}

impl NativeRenderSpan {
    pub fn describe(&self) -> String {
        format!(
            "Native render: page {} {} (cache hit: {}) {:.2?}",
            self.page_index + 1,
            self.target.label(),
            self.cache_hit,
            self.duration,
        )
    }
}

#[derive(Clone, Debug)]
pub struct NativeRenderEviction {
    pub timestamp: Instant,
    pub target: RenderTarget,
    pub page_index: usize,
    pub reason: String,
}

impl NativeRenderEviction {
    pub fn describe(&self) -> String {
        format!(
            "Evicted page {} {} ({})",
            self.page_index + 1,
            self.target.label(),
            self.reason
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RenderCacheKey {
    source: PathBuf,
    page_index: usize,
    target: RenderTarget,
}

impl Hash for RenderCacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.source.hash(state);
        self.page_index.hash(state);
        self.target.hash(state);
    }
}

#[derive(Copy, Clone, Debug)]
struct RenderDimensions {
    width: Pixels,
    height: Pixels,
}
