use super::*;

impl ReaderSession {
    pub fn settings_view(&self) -> ReaderSettingsView {
        ReaderSettingsView {
            theme: self.config.theme,
            font_family: self.config.font_family,
            font_weight: self.config.font_weight,
            day_highlight: self.config.day_highlight,
            night_highlight: self.config.night_highlight,
            font_size: self.config.font_size,
            line_spacing: self.config.line_spacing,
            word_spacing: self.config.word_spacing,
            letter_spacing: self.config.letter_spacing,
            margin_horizontal: self.config.margin_horizontal,
            margin_vertical: self.config.margin_vertical,
            lines_per_page: self.config.lines_per_page,
            pause_after_sentence: self.config.pause_after_sentence,
            auto_scroll_tts: self.config.auto_scroll_tts,
            center_spoken_sentence: self.config.center_spoken_sentence,
            text_only_show_original_text: self.config.text_only_show_original_text,
            time_remaining_display: self.config.time_remaining_display,
            tts_speed: self.config.tts_speed,
            tts_volume: self.config.tts_volume,
        }
    }

    pub fn apply_settings_patch(
        &mut self,
        patch: ReaderSettingsPatch,
        normalizer: &normalizer::TextNormalizer,
    ) {
        let preserve = self.global_display_idx();
        let mut repaginate = false;

        if let Some(theme) = patch.theme {
            self.config.theme = theme;
        }
        if let Some(day_highlight) = patch.day_highlight {
            self.config.day_highlight = config::HighlightColor {
                r: day_highlight.r.clamp(0.0, 1.0),
                g: day_highlight.g.clamp(0.0, 1.0),
                b: day_highlight.b.clamp(0.0, 1.0),
                a: day_highlight.a.clamp(0.0, 1.0),
            };
        }
        if let Some(night_highlight) = patch.night_highlight {
            self.config.night_highlight = config::HighlightColor {
                r: night_highlight.r.clamp(0.0, 1.0),
                g: night_highlight.g.clamp(0.0, 1.0),
                b: night_highlight.b.clamp(0.0, 1.0),
                a: night_highlight.a.clamp(0.0, 1.0),
            };
        }
        if let Some(font_family) = patch.font_family {
            self.config.font_family = font_family;
        }
        if let Some(font_weight) = patch.font_weight {
            self.config.font_weight = font_weight;
        }
        if let Some(font_size) = patch.font_size {
            let clamped = font_size.clamp(pagination::MIN_FONT_SIZE, pagination::MAX_FONT_SIZE);
            if clamped != self.config.font_size {
                self.config.font_size = clamped;
                repaginate = true;
            }
        }
        if let Some(lines) = patch.lines_per_page {
            let clamped = lines.clamp(
                pagination::MIN_LINES_PER_PAGE,
                pagination::MAX_LINES_PER_PAGE,
            );
            if clamped != self.config.lines_per_page {
                self.config.lines_per_page = clamped;
                repaginate = true;
            }
        }
        if let Some(margin_horizontal) = patch.margin_horizontal {
            self.config.margin_horizontal = margin_horizontal.clamp(0, 600);
        }
        if let Some(margin_vertical) = patch.margin_vertical {
            self.config.margin_vertical = margin_vertical.clamp(0, 240);
        }
        if let Some(line_spacing) = patch.line_spacing {
            self.config.line_spacing = line_spacing.clamp(0.8, 3.0);
        }
        if let Some(word_spacing) = patch.word_spacing {
            self.config.word_spacing = word_spacing.clamp(0, 24);
        }
        if let Some(letter_spacing) = patch.letter_spacing {
            self.config.letter_spacing = letter_spacing.clamp(0, 24);
        }
        if let Some(pause) = patch.pause_after_sentence {
            let rounded = ((pause.clamp(0.0, 3.0) * 100.0).round()) / 100.0;
            self.config.pause_after_sentence = rounded;
        }
        if let Some(auto_scroll_tts) = patch.auto_scroll_tts {
            self.config.auto_scroll_tts = auto_scroll_tts;
        }
        if let Some(center_spoken_sentence) = patch.center_spoken_sentence {
            self.config.center_spoken_sentence = center_spoken_sentence;
        }
        let mut text_only_display_mode_changed = false;
        if let Some(text_only_show_original_text) = patch.text_only_show_original_text {
            if self.config.text_only_show_original_text != text_only_show_original_text {
                self.config.text_only_show_original_text = text_only_show_original_text;
                text_only_display_mode_changed = true;
            }
        }
        if let Some(tts_speed) = patch.tts_speed {
            self.config.tts_speed = tts_speed.clamp(0.25, 4.0);
        }
        if let Some(tts_volume) = patch.tts_volume {
            self.config.tts_volume = tts_volume.clamp(0.0, 2.0);
        }

        if repaginate {
            self.repaginate(normalizer, preserve);
        }
        if text_only_display_mode_changed {
            let highlighted_audio_idx = self.current_audio_highlight_idx(normalizer);
            self.highlighted_audio_idx = highlighted_audio_idx;
            self.highlighted_display_idx = highlighted_audio_idx
                .and_then(|audio_idx| self.map_audio_to_display_idx(normalizer, audio_idx));
            self.update_search_matches(normalizer);
            self.reselect_search_match_for_current_highlight();
            tracing::debug!(
                path = %self.source_path.display(),
                text_only_mode = self.text_only_mode,
                text_only_show_original_text = self.config.text_only_show_original_text,
                highlighted_audio_idx = self.highlighted_audio_idx,
                highlighted_display_idx = self.highlighted_display_idx,
                "Updated text-only display mode without changing canonical TTS ownership"
            );
        }
    }
}
