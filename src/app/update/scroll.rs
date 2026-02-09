use super::super::state::{
    App, IMAGE_BLOCK_SPACING_PX, IMAGE_FOOTER_FONT_SIZE_PX, IMAGE_FOOTER_LINE_HEIGHT,
    IMAGE_LABEL_FONT_SIZE_PX, IMAGE_LABEL_LINE_HEIGHT, IMAGE_PREVIEW_HEIGHT_PX,
    PAGE_FLOW_SPACING_PX,
};
use super::Effect;
use crate::cache::{Bookmark, save_bookmark};
use crate::config::FontFamily;
use iced::widget::scrollable::RelativeOffset;
use tracing::info;

impl App {
    pub(super) fn handle_scrolled(
        &mut self,
        offset: RelativeOffset,
        viewport_width: f32,
        viewport_height: f32,
        content_width: f32,
        content_height: f32,
        effects: &mut Vec<Effect>,
    ) {
        let sanitized = Self::sanitize_offset(offset);
        self.bookmark.viewport_width = if viewport_width.is_finite() {
            viewport_width.max(0.0)
        } else {
            0.0
        };
        self.bookmark.viewport_height = if viewport_height.is_finite() {
            viewport_height.max(0.0)
        } else {
            0.0
        };
        self.bookmark.content_width = if content_width.is_finite() {
            content_width.max(0.0)
        } else {
            0.0
        };
        self.bookmark.content_height = if content_height.is_finite() {
            content_height.max(0.0)
        } else {
            0.0
        };
        self.bookmark.viewport_fraction =
            if viewport_height.is_finite() && content_height.is_finite() && content_height > 0.0 {
                (viewport_height / content_height).clamp(0.05, 1.0)
            } else {
                0.25
            };

        if sanitized != self.bookmark.last_scroll_offset {
            self.bookmark.last_scroll_offset = sanitized;
            effects.push(Effect::SaveBookmark);
        }

        if let Some(idx) = self.bookmark.pending_sentence_snap.take() {
            if let Some(offset) = self.scroll_offset_for_sentence(idx) {
                let offset = Self::sanitize_offset(offset);
                if offset != self.bookmark.last_scroll_offset {
                    self.bookmark.last_scroll_offset = offset;
                    effects.push(Effect::ScrollTo(offset));
                    effects.push(Effect::SaveBookmark);
                }
            }
        }
    }

    pub(super) fn handle_jump_to_current_audio(&mut self, effects: &mut Vec<Effect>) {
        if let Some(idx) = self.tts.current_sentence_idx {
            if let Some(offset) = self.scroll_offset_for_sentence(idx) {
                info!(
                    idx,
                    fraction = offset.y,
                    "Jumping to current audio sentence (scroll only)"
                );
                effects.push(Effect::ScrollTo(offset));
                effects.push(Effect::SaveBookmark);
            }
        }
    }

    pub(super) fn persist_bookmark(&self) {
        let sentences = self.current_sentences();

        let sentence_idx = self
            .tts
            .current_sentence_idx
            .filter(|idx| *idx < sentences.len())
            .or_else(|| {
                if sentences.is_empty() {
                    None
                } else {
                    let frac = Self::sanitize_offset(self.bookmark.last_scroll_offset).y;
                    let idx = (frac * (sentences.len().saturating_sub(1) as f32)).round() as usize;
                    Some(idx.min(sentences.len().saturating_sub(1)))
                }
            });
        let sentence_text = sentence_idx.and_then(|idx| sentences.get(idx).cloned());
        let scroll_y = Self::sanitize_offset(self.bookmark.last_scroll_offset).y;

        let bookmark = Bookmark {
            page: self.reader.current_page,
            sentence_idx,
            sentence_text,
            scroll_y,
        };

        save_bookmark(&self.epub_path, &bookmark);
    }

    pub(super) fn sanitize_offset(offset: RelativeOffset) -> RelativeOffset {
        let clamp = |v: f32| {
            if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        RelativeOffset {
            x: clamp(offset.x),
            y: clamp(offset.y),
        }
    }

    fn current_sentences(&self) -> Vec<String> {
        self.raw_sentences_for_page(self.reader.current_page)
    }

    pub(crate) fn scroll_offset_for_sentence(&self, sentence_idx: usize) -> Option<RelativeOffset> {
        let model = self.scroll_target_model(sentence_idx)?;
        let progress = self.sentence_progress_for_model(&model).unwrap_or_else(|| {
            let clamped_idx = model
                .target_idx
                .min(model.sentences.len().saturating_sub(1)) as f32;
            let denom = model.sentences.len().saturating_sub(1).max(1) as f32;
            let ratio = (clamped_idx / denom).clamp(0.0, 1.0);
            SentenceProgress {
                start: ratio,
                middle: ratio,
            }
        });

        let viewport_fraction = self.estimated_viewport_fraction();
        if viewport_fraction >= 0.999 {
            return Some(RelativeOffset::START);
        }
        let content_height = self.bookmark.content_height.max(1.0);
        let viewport_height = self.estimated_viewport_height_px(viewport_fraction);
        let (text_top_px, text_height_px) = self.estimated_text_geometry_px(content_height);

        let sentence_start_px = text_top_px + progress.start * text_height_px;
        let sentence_middle_px = text_top_px + progress.middle * text_height_px;

        let desired_top_px = if self.config.center_spoken_sentence {
            sentence_middle_px - 0.50 * viewport_height
        } else {
            sentence_start_px - 0.25 * viewport_height
        };

        // `snap_to` expects offset over the scrollable range (content - viewport).
        let scrollable_px = (content_height - viewport_height).max(1.0);
        let y = (desired_top_px / scrollable_px).clamp(0.0, 1.0);

        Some(RelativeOffset { x: 0.0, y })
    }

    fn sentence_progress_for_model(&self, model: &ScrollTargetModel) -> Option<SentenceProgress> {
        if model.sentences.is_empty() {
            return None;
        }

        let sentence_weights =
            self.estimate_sentence_line_weights(&model.sentences, model.extra_gap_lines);
        let idx = model
            .target_idx
            .min(sentence_weights.len().saturating_sub(1));
        let total_weight: f32 = sentence_weights.iter().sum();
        if total_weight <= f32::EPSILON {
            return None;
        }

        let before_weight: f32 = sentence_weights.iter().take(idx).sum();
        let sentence_weight = sentence_weights[idx].max(f32::EPSILON);
        let start = (before_weight / total_weight).clamp(0.0, 1.0);
        let middle = ((before_weight + sentence_weight * 0.5) / total_weight).clamp(0.0, 1.0);

        Some(SentenceProgress { start, middle })
    }

    fn scroll_target_model(&self, display_sentence_idx: usize) -> Option<ScrollTargetModel> {
        if self.text_only_mode {
            let preview = self.text_only_preview_for_current_page()?;
            if preview.audio_sentences.is_empty() {
                return None;
            }
            let target_idx = Self::map_display_to_audio_index(
                display_sentence_idx,
                &preview.display_to_audio,
                preview.audio_sentences.len(),
            )?;
            return Some(ScrollTargetModel {
                sentences: preview.audio_sentences.clone(),
                target_idx,
                // Text-only renders each sentence separated by a blank line.
                extra_gap_lines: 1.0,
            });
        }

        let sentences = self.display_sentences_for_current_page();
        if sentences.is_empty() {
            return None;
        }
        let target_idx = display_sentence_idx.min(sentences.len().saturating_sub(1));
        Some(ScrollTargetModel {
            sentences,
            target_idx,
            extra_gap_lines: 0.0,
        })
    }

    fn map_display_to_audio_index(
        display_sentence_idx: usize,
        display_to_audio: &[Option<usize>],
        audio_len: usize,
    ) -> Option<usize> {
        if audio_len == 0 {
            return None;
        }
        if display_to_audio.is_empty() {
            return Some(display_sentence_idx.min(audio_len.saturating_sub(1)));
        }

        let clamped = display_sentence_idx.min(display_to_audio.len().saturating_sub(1));
        display_to_audio
            .iter()
            .skip(clamped)
            .find_map(|mapped| *mapped)
            .or_else(|| {
                display_to_audio
                    .iter()
                    .take(clamped + 1)
                    .rev()
                    .find_map(|mapped| *mapped)
            })
            .or(Some(0))
            .map(|idx| idx.min(audio_len.saturating_sub(1)))
    }

    fn estimate_sentence_line_weights(
        &self,
        sentences: &[String],
        extra_gap_lines: f32,
    ) -> Vec<f32> {
        let available_width = self.estimated_text_width();
        if available_width <= f32::EPSILON {
            return sentences.iter().map(|_| 1.0).collect();
        }

        let glyph_width = self.estimated_glyph_width_px().max(1.0);
        let max_units_per_line = (available_width / glyph_width).max(8.0);
        let line_height_weight = self.config.line_spacing.max(0.8);

        sentences
            .iter()
            .enumerate()
            .map(|(idx, sentence)| {
                let mut lines = 1.0f32;
                let mut line_units = 0.0f32;

                for ch in sentence.chars() {
                    if ch == '\n' {
                        lines += 1.0;
                        line_units = 0.0;
                        continue;
                    }

                    let units = if ch.is_whitespace() {
                        0.45 + self.config.word_spacing as f32 * 0.45
                    } else if ch.is_ascii_punctuation() {
                        0.55 + self.config.letter_spacing as f32 * 0.10
                    } else if ch.is_ascii() {
                        1.0 + self.config.letter_spacing as f32 * 0.35
                    } else {
                        1.8 + self.config.letter_spacing as f32 * 0.20
                    };

                    if line_units + units > max_units_per_line {
                        lines += 1.0;
                        line_units = units;
                    } else {
                        line_units += units;
                    }
                }

                let mut weight = lines * line_height_weight;
                if extra_gap_lines > 0.0 && idx + 1 < sentences.len() {
                    weight += extra_gap_lines * line_height_weight;
                }
                weight
            })
            .collect()
    }

    fn estimated_viewport_fraction(&self) -> f32 {
        if self.bookmark.viewport_height > 0.0
            && self.bookmark.content_height > self.bookmark.viewport_height
        {
            return (self.bookmark.viewport_height / self.bookmark.content_height)
                .clamp(0.05, 0.95);
        }
        if self.bookmark.viewport_fraction.is_finite() && self.bookmark.viewport_fraction > 0.0 {
            return self.bookmark.viewport_fraction.clamp(0.05, 0.95);
        }
        0.25
    }

    fn estimated_viewport_height_px(&self, viewport_fraction: f32) -> f32 {
        if self.bookmark.viewport_height > 0.0 {
            return self.bookmark.viewport_height.max(1.0);
        }
        if self.bookmark.content_height > 0.0 {
            return (self.bookmark.content_height * viewport_fraction).max(1.0);
        }
        1.0
    }

    fn estimated_text_geometry_px(&self, content_height: f32) -> (f32, f32) {
        let text_container_height = if self.text_only_mode {
            content_height
        } else {
            (content_height - self.estimated_non_text_tail_px()).max(1.0)
        };
        let top_padding = (self.config.margin_vertical as f32).min(text_container_height);
        let text_height = (text_container_height - top_padding * 2.0).max(1.0);
        (top_padding, text_height)
    }

    fn estimated_text_width(&self) -> f32 {
        let mut width = if self.bookmark.viewport_width > 0.0 {
            self.bookmark.viewport_width
        } else {
            let mut fallback = self.config.window_width.max(1.0);
            if self.config.show_settings {
                fallback = (fallback - 320.0).max(1.0);
            }
            fallback
        };
        let margin_total = (self.config.margin_horizontal as f32 * 2.0).min(width * 0.9);
        width = (width - margin_total).max(1.0);
        width
    }

    fn estimated_glyph_width_px(&self) -> f32 {
        let font_size = self.config.font_size.max(1) as f32;
        let family_scale = match self.config.font_family {
            FontFamily::Monospace | FontFamily::Courier | FontFamily::FiraCode => 0.64,
            FontFamily::Serif => 0.56,
            FontFamily::Lexend | FontFamily::NotoSans => 0.54,
            FontFamily::AtkinsonHyperlegible
            | FontFamily::AtkinsonHyperlegibleNext
            | FontFamily::LexicaUltralegible => 0.57,
            _ => 0.55,
        };
        let weight_scale = match self.config.font_weight {
            crate::config::FontWeight::Light => 0.98,
            crate::config::FontWeight::Normal => 1.0,
            crate::config::FontWeight::Bold => 1.03,
        };

        font_size * family_scale * weight_scale
    }

    fn current_page_image_count(&self) -> usize {
        self.reader
            .images
            .iter()
            .enumerate()
            .filter(|(idx, _)| self.image_assigned_page(*idx) == self.reader.current_page)
            .count()
    }

    fn estimated_non_text_tail_px(&self) -> f32 {
        let image_count = self.current_page_image_count() as f32;
        if image_count <= 0.0 {
            return 0.0;
        }

        // Keep these values in sync with `view.rs` image layout.
        let label_height = IMAGE_LABEL_FONT_SIZE_PX * IMAGE_LABEL_LINE_HEIGHT;
        let footer_height = IMAGE_FOOTER_FONT_SIZE_PX * IMAGE_FOOTER_LINE_HEIGHT;
        let image_block_height = label_height + IMAGE_BLOCK_SPACING_PX + IMAGE_PREVIEW_HEIGHT_PX;
        let flow_spacing = PAGE_FLOW_SPACING_PX * (image_count + 1.0);

        image_count * image_block_height + flow_spacing + footer_height
    }
}

struct ScrollTargetModel {
    sentences: Vec<String>,
    target_idx: usize,
    extra_gap_lines: f32,
}

#[derive(Clone, Copy)]
struct SentenceProgress {
    start: f32,
    middle: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::epub_loader::{BookImage, LoadedBook};
    use std::path::PathBuf;

    fn sample_text(sentence_count: usize) -> String {
        (0..sentence_count)
            .map(|i| {
                format!(
                    "Sentence {i} contains enough words to exercise wrapping and scroll calculations."
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn build_test_app(sentence_count: usize, image_count: usize) -> App {
        let images = (0..image_count)
            .map(|i| BookImage {
                path: PathBuf::from(format!("/tmp/fake-scroll-image-{i}.png")),
                label: format!("Image {i}"),
            })
            .collect::<Vec<_>>();

        let book = LoadedBook {
            text: sample_text(sentence_count),
            images,
        };

        let mut config = AppConfig::default();
        config.show_settings = false;
        config.window_width = 1280.0;
        config.window_height = 900.0;
        config.margin_horizontal = 20;
        config.margin_vertical = 12;
        config.lines_per_page = 200;
        config.font_size = 16;
        config.auto_scroll_tts = true;

        let epub_path = PathBuf::from(format!(
            "/tmp/ebup-scroll-test-{}-{}.epub",
            std::process::id(),
            sentence_count
        ));
        let (mut app, _task) = App::bootstrap(book, config, epub_path, None);

        app.reader.current_page = 0;
        app.bookmark.viewport_width = 920.0;
        app.bookmark.viewport_height = 640.0;
        app.bookmark.content_width = 920.0;
        app.bookmark.content_height = 3600.0 + image_count as f32 * 300.0;
        app.bookmark.viewport_fraction =
            (app.bookmark.viewport_height / app.bookmark.content_height).clamp(0.05, 0.95);
        app
    }

    #[test]
    fn text_only_center_differs_from_auto_scroll() {
        let mut app = build_test_app(140, 0);
        app.text_only_mode = true;

        let sentences = app.raw_sentences_for_page(app.reader.current_page);
        let display_to_audio = (0..sentences.len()).map(Some).collect();
        app.text_only_preview = Some(super::super::super::state::TextOnlyPreview {
            page: app.reader.current_page,
            audio_sentences: sentences,
            display_to_audio,
        });

        let idx = 40usize;
        app.config.center_spoken_sentence = false;
        let y_scroll = app
            .scroll_offset_for_sentence(idx)
            .expect("scroll offset in text-only auto-scroll")
            .y;

        app.config.center_spoken_sentence = true;
        let y_center = app
            .scroll_offset_for_sentence(idx)
            .expect("scroll offset in text-only auto-center")
            .y;

        assert!(
            (y_scroll - y_center).abs() > 0.03,
            "text-only center and auto-scroll should not collapse to the same offset"
        );
    }

    #[test]
    fn pretty_jump_targets_are_monotonic() {
        let app = build_test_app(180, 0);
        let mut previous = -1.0f32;

        for idx in (0..80).step_by(4) {
            let y = app
                .scroll_offset_for_sentence(idx)
                .expect("jump target for sentence")
                .y;
            assert!(
                y + 1e-6 >= previous,
                "scroll target should be monotonic for increasing sentence idx"
            );
            previous = y;
        }
    }

    #[test]
    fn pretty_offsets_remain_stable_when_margin_changes() {
        let mut app = build_test_app(180, 0);
        let idx = 60usize;

        app.config.margin_horizontal = 0;
        let y_min_margin = app
            .scroll_offset_for_sentence(idx)
            .expect("scroll offset with min margin")
            .y;

        app.config.margin_horizontal = 100;
        let y_max_margin = app
            .scroll_offset_for_sentence(idx)
            .expect("scroll offset with max margin")
            .y;

        assert!(
            (y_max_margin - y_min_margin).abs() < 0.35,
            "changing horizontal margin should not produce catastrophic scroll jumps"
        );
    }

    #[test]
    fn pretty_images_reduce_text_target_fraction() {
        let mut app_without_images = build_test_app(180, 0);
        let mut app_with_images = build_test_app(180, 4);
        let idx = 70usize;

        app_without_images.config.center_spoken_sentence = false;
        app_with_images.config.center_spoken_sentence = false;

        let y_without = app_without_images
            .scroll_offset_for_sentence(idx)
            .expect("scroll offset without images")
            .y;
        let y_with = app_with_images
            .scroll_offset_for_sentence(idx)
            .expect("scroll offset with images")
            .y;

        assert!(
            y_with <= y_without,
            "image-tail compensation should avoid pushing text targets further down"
        );
    }
}
