use super::super::state::App;
use super::Effect;
use crate::cache::{Bookmark, save_bookmark};
use crate::config::FontFamily;
use crate::text_utils::split_sentences;
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
        self.bookmark.viewport_fraction = if viewport_height.is_finite()
            && content_height.is_finite()
            && content_height > 0.0
        {
            (viewport_height / content_height).clamp(0.05, 1.0)
        } else {
            0.25
        };

        if sanitized != self.bookmark.last_scroll_offset {
            self.bookmark.last_scroll_offset = sanitized;
            effects.push(Effect::SaveBookmark);
        }
    }

    pub(super) fn handle_jump_to_current_audio(&mut self, effects: &mut Vec<Effect>) {
        if let Some(idx) = self.tts.current_sentence_idx {
            let total = self.tts.last_sentences.len();
            if let Some(offset) = self.scroll_offset_for_sentence(idx, total) {
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
        split_sentences(
            self.reader
                .pages
                .get(self.reader.current_page)
                .map(String::as_str)
                .unwrap_or("")
                .to_string(),
        )
    }

    pub(crate) fn scroll_offset_for_sentence(
        &self,
        sentence_idx: usize,
        total_sentences: usize,
    ) -> Option<RelativeOffset> {
        if total_sentences == 0 {
            return None;
        }

        let base = self
            .sentence_progress_for_page(sentence_idx, total_sentences)
            .unwrap_or_else(|| {
                let clamped_idx = sentence_idx.min(total_sentences.saturating_sub(1)) as f32;
                let denom = total_sentences.saturating_sub(1).max(1) as f32;
                (clamped_idx / denom).clamp(0.0, 1.0)
            });

        let viewport_fraction = self.estimated_viewport_fraction();
        if viewport_fraction >= 0.999 {
            return Some(RelativeOffset::START);
        }

        let desired_top = if self.config.center_spoken_sentence {
            // Center mode keeps the active sentence near middle of the viewport.
            base - 0.5 * viewport_fraction
        } else {
            // Track mode follows the sentence while keeping it comfortably in view.
            base - 0.2 * viewport_fraction
        };

        // `snap_to` expects offset over the scrollable range (content - viewport),
        // not over full content height.
        let scrollable_fraction = (1.0 - viewport_fraction).max(0.000_1);
        let y = (desired_top / scrollable_fraction).clamp(0.0, 1.0);

        Some(RelativeOffset { x: 0.0, y })
    }

    fn sentence_progress_for_page(
        &self,
        sentence_idx: usize,
        total_sentences: usize,
    ) -> Option<f32> {
        let page = self
            .reader
            .pages
            .get(self.reader.current_page)
            .map(String::as_str)?;
        let sentences = if self.tts.last_sentences.len() == total_sentences
            && !self.tts.last_sentences.is_empty()
        {
            self.tts.last_sentences.clone()
        } else {
            split_sentences(page.to_string())
        };
        if sentences.is_empty() {
            return None;
        }

        let idx = sentence_idx.min(sentences.len().saturating_sub(1));
        let sentence_weights = self.estimate_sentence_line_weights(&sentences);
        let total_weight: f32 = sentence_weights.iter().sum();
        if total_weight <= f32::EPSILON {
            return None;
        }

        let before_weight: f32 = sentence_weights.iter().take(idx).sum();
        let anchor_weight = before_weight + sentence_weights[idx] * 0.5;
        Some((anchor_weight / total_weight).clamp(0.0, 1.0))
    }

    fn estimate_sentence_line_weights(&self, sentences: &[String]) -> Vec<f32> {
        let available_width = self.estimated_text_width();
        if available_width <= f32::EPSILON {
            return sentences.iter().map(|_| 1.0).collect();
        }

        let glyph_width = self.estimated_glyph_width_px().max(1.0);
        let max_units_per_line = (available_width / glyph_width).max(8.0);

        sentences
            .iter()
            .map(|sentence| {
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

                lines * self.config.line_spacing.max(0.8)
            })
            .collect()
    }

    fn estimated_viewport_fraction(&self) -> f32 {
        if self.bookmark.viewport_height > 0.0
            && self.bookmark.content_height > self.bookmark.viewport_height
        {
            return (self.bookmark.viewport_height / self.bookmark.content_height).clamp(0.05, 0.95);
        }
        if self.bookmark.viewport_fraction.is_finite() && self.bookmark.viewport_fraction > 0.0 {
            return self.bookmark.viewport_fraction.clamp(0.05, 0.95);
        }
        0.25
    }

    fn estimated_text_width(&self) -> f32 {
        let base_width = match (
            self.bookmark.viewport_width > 0.0,
            self.bookmark.content_width > 0.0,
        ) {
            (true, true) => self.bookmark.viewport_width.min(self.bookmark.content_width),
            (true, false) => self.bookmark.viewport_width,
            (false, true) => self.bookmark.content_width,
            (false, false) => 1.0,
        };

        let margin_total = (self.config.margin_horizontal as f32 * 2.0).min(base_width * 0.9);
        (base_width - margin_total).max(1.0)
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
}
