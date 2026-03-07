use super::*;

impl ReaderSession {
    pub fn sentence_click(&mut self, sentence_idx: usize, normalizer: &normalizer::TextNormalizer) {
        if self.text_only_mode {
            let plan = self.ensure_current_plan(normalizer);
            if sentence_idx >= plan.audio_sentences.len() {
                return;
            }
            self.highlighted_audio_idx = Some(sentence_idx);
            self.highlighted_display_idx = self.map_audio_to_display_idx(normalizer, sentence_idx);
            return;
        }

        if sentence_idx >= self.current_display_len() {
            return;
        }
        self.highlighted_display_idx = Some(sentence_idx);
        self.highlighted_audio_idx = self.map_display_to_audio_idx(normalizer, sentence_idx);
    }

    pub fn select_next_sentence(&mut self, normalizer: &normalizer::TextNormalizer) {
        let count = self.current_sentences(normalizer).len();
        if count == 0 {
            return;
        }
        let current = self
            .current_highlight_idx()
            .unwrap_or(0)
            .min(count.saturating_sub(1));
        let next = (current + 1).min(count.saturating_sub(1));
        self.sentence_click(next, normalizer);
    }

    pub fn select_prev_sentence(&mut self, normalizer: &normalizer::TextNormalizer) {
        let count = self.current_sentences(normalizer).len();
        if count == 0 {
            return;
        }
        let current = self
            .current_highlight_idx()
            .unwrap_or(0)
            .min(count.saturating_sub(1));
        let prev = current.saturating_sub(1);
        self.sentence_click(prev, normalizer);
    }

    pub fn toggle_text_only(&mut self, normalizer: &normalizer::TextNormalizer) {
        self.text_only_mode = !self.text_only_mode;
        if self.text_only_mode {
            let display_idx = self.highlighted_display_idx.unwrap_or(0);
            self.highlighted_audio_idx = self.map_display_to_audio_idx(normalizer, display_idx);
        } else if let Some(audio_idx) = self.highlighted_audio_idx {
            self.highlighted_display_idx = self.map_audio_to_display_idx(normalizer, audio_idx);
        }
        self.update_search_matches(normalizer);
    }

    pub fn tts_play(&mut self, normalizer: &normalizer::TextNormalizer) {
        let count = self.current_audio_sentences(normalizer).len();
        if count == 0 {
            self.tts_state = TtsPlaybackState::Idle;
            return;
        }
        if self.current_audio_highlight_idx(normalizer).is_none() {
            let _ = self.set_audio_highlight_idx(normalizer, 0);
        }
        self.tts_state = TtsPlaybackState::Playing;
    }

    pub fn tts_pause(&mut self) {
        if self.tts_state == TtsPlaybackState::Playing {
            self.tts_state = TtsPlaybackState::Paused;
        }
    }

    pub fn tts_toggle_play_pause(&mut self, normalizer: &normalizer::TextNormalizer) {
        if self.tts_state == TtsPlaybackState::Playing {
            self.tts_pause();
        } else {
            self.tts_play(normalizer);
        }
    }

    pub fn tts_play_from_page_start(&mut self, normalizer: &normalizer::TextNormalizer) {
        let count = self.current_audio_sentences(normalizer).len();
        if count == 0 {
            self.tts_state = TtsPlaybackState::Idle;
            return;
        }
        let _ = self.set_audio_highlight_idx(normalizer, 0);
        self.tts_state = TtsPlaybackState::Playing;
    }

    pub fn tts_play_from_highlight(&mut self, normalizer: &normalizer::TextNormalizer) {
        if self.current_audio_highlight_idx(normalizer).is_none() {
            self.tts_play_from_page_start(normalizer);
            return;
        }
        self.tts_state = TtsPlaybackState::Playing;
    }

    pub fn tts_seek_next(&mut self, normalizer: &normalizer::TextNormalizer) {
        if self.move_highlight_relative(1, normalizer) {
            return;
        }
        if self.tts_state == TtsPlaybackState::Playing {
            self.tts_state = TtsPlaybackState::Paused;
        }
    }

    pub fn tts_seek_prev(&mut self, normalizer: &normalizer::TextNormalizer) {
        let _ = self.move_highlight_relative(-1, normalizer);
    }

    pub fn tts_repeat_current_sentence(&mut self, normalizer: &normalizer::TextNormalizer) {
        if self.current_highlight_idx().is_none() {
            self.tts_play_from_page_start(normalizer);
        }
    }

    pub fn tts_stop(&mut self) {
        self.tts_state = TtsPlaybackState::Idle;
    }

    pub(super) fn ensure_current_plan(
        &mut self,
        normalizer: &normalizer::TextNormalizer,
    ) -> normalizer::PageNormalization {
        let needs_refresh = self.current_plan_page != Some(self.current_page);
        if needs_refresh {
            let page_text_chars = self
                .pages
                .get(self.current_page)
                .map(|value| value.len())
                .unwrap_or(0);
            tracing::trace!(
                path = %self.source_path.display(),
                page = self.current_page + 1,
                page_text_chars,
                source = "tts_text",
                "Building normalization/TTS plan from canonical plain text page"
            );
            let display = self
                .raw_page_sentences
                .get(self.current_page)
                .cloned()
                .unwrap_or_default();
            let plan = normalizer.plan_page_cached(&self.source_path, self.current_page, &display);
            self.current_plan_page = Some(self.current_page);
            self.current_plan = Some(plan);
        }

        self.current_plan
            .clone()
            .unwrap_or(normalizer::PageNormalization {
                audio_sentences: Vec::new(),
                display_to_audio: Vec::new(),
                audio_to_display: Vec::new(),
            })
    }

    pub(super) fn map_display_to_audio_idx(
        &mut self,
        normalizer: &normalizer::TextNormalizer,
        display_idx: usize,
    ) -> Option<usize> {
        let plan = self.ensure_current_plan(normalizer);
        if plan.display_to_audio.is_empty() {
            return None;
        }
        let telemetry = mapping_telemetry();
        telemetry.lookups.fetch_add(1, Ordering::Relaxed);
        let clamped = display_idx.min(plan.display_to_audio.len().saturating_sub(1));
        let mapped = plan
            .display_to_audio
            .iter()
            .skip(clamped)
            .find_map(|mapped| *mapped)
            .or_else(|| {
                plan.display_to_audio
                    .iter()
                    .take(clamped + 1)
                    .rev()
                    .find_map(|mapped| *mapped)
            });
        if mapped.is_none() {
            let fallback = telemetry.fallbacks.fetch_add(1, Ordering::Relaxed) + 1;
            if fallback % 32 == 0 {
                tracing::warn!(
                    fallback_events = fallback,
                    lookups = telemetry.lookups.load(Ordering::Relaxed),
                    "Display->audio mapping fallback frequency is elevated"
                );
            }
            telemetry.missing.fetch_add(1, Ordering::Relaxed);
        } else {
            telemetry.hits.fetch_add(1, Ordering::Relaxed);
        }
        maybe_log_mapping_summary(&self.source_path);
        mapped
    }

    pub(super) fn map_audio_to_display_idx(
        &mut self,
        normalizer: &normalizer::TextNormalizer,
        audio_idx: usize,
    ) -> Option<usize> {
        let plan = self.ensure_current_plan(normalizer);
        let telemetry = mapping_telemetry();
        telemetry.lookups.fetch_add(1, Ordering::Relaxed);
        let mapped = if plan.audio_to_display.is_empty() {
            None
        } else {
            let clamped = audio_idx.min(plan.audio_to_display.len().saturating_sub(1));
            plan.audio_to_display
                .get(clamped)
                .copied()
                .or_else(|| {
                    for offset in 1..plan.audio_to_display.len() {
                        let prev = clamped.saturating_sub(offset);
                        if let Some(display) = plan.audio_to_display.get(prev) {
                            return Some(*display);
                        }
                        let next = clamped.saturating_add(offset);
                        if let Some(display) = plan.audio_to_display.get(next) {
                            return Some(*display);
                        }
                    }
                    None
                })
                .or(self.highlighted_display_idx)
                .or_else(|| (self.current_display_len() > 0).then_some(0))
        };
        if mapped.is_none() {
            let fallback = telemetry.fallbacks.fetch_add(1, Ordering::Relaxed) + 1;
            if fallback % 32 == 0 {
                tracing::warn!(
                    fallback_events = fallback,
                    lookups = telemetry.lookups.load(Ordering::Relaxed),
                    "Audio->display mapping fallback frequency is elevated"
                );
            }
            telemetry.missing.fetch_add(1, Ordering::Relaxed);
        } else {
            telemetry.hits.fetch_add(1, Ordering::Relaxed);
        }
        maybe_log_mapping_summary(&self.source_path);
        mapped
    }

    fn move_highlight_relative(
        &mut self,
        delta: isize,
        normalizer: &normalizer::TextNormalizer,
    ) -> bool {
        if delta == 0 {
            return self.current_audio_highlight_idx(normalizer).is_some();
        }

        let count = self.current_audio_sentences(normalizer).len();
        if count == 0 {
            if delta > 0 {
                return self.move_to_adjacent_page_with_sentences(1, normalizer);
            }
            return self.move_to_adjacent_page_with_sentences(-1, normalizer);
        }

        let current = self
            .current_audio_highlight_idx(normalizer)
            .unwrap_or(0)
            .min(count.saturating_sub(1));
        if delta > 0 {
            let next = current.saturating_add(delta as usize);
            if next < count {
                let _ = self.set_audio_highlight_idx(normalizer, next);
                return true;
            }
            if self.move_to_adjacent_page_with_sentences(1, normalizer) {
                return self.set_audio_highlight_idx(normalizer, 0);
            }
            return false;
        }

        let back = delta.unsigned_abs();
        if current >= back {
            return self.set_audio_highlight_idx(normalizer, current - back);
        }
        if self.move_to_adjacent_page_with_sentences(-1, normalizer) {
            let new_count = self.current_audio_sentences(normalizer).len();
            if new_count > 0 {
                return self.set_audio_highlight_idx(normalizer, new_count - 1);
            }
        }
        false
    }

    pub(super) fn current_audio_sentences(
        &mut self,
        normalizer: &normalizer::TextNormalizer,
    ) -> Vec<String> {
        self.ensure_current_plan(normalizer).audio_sentences
    }

    pub(super) fn current_audio_highlight_idx(
        &mut self,
        normalizer: &normalizer::TextNormalizer,
    ) -> Option<usize> {
        let audio_count = self.current_audio_sentences(normalizer).len();
        if audio_count == 0 {
            return None;
        }
        if let Some(idx) = self.highlighted_audio_idx {
            return Some(idx.min(audio_count.saturating_sub(1)));
        }
        self.highlighted_display_idx
            .and_then(|idx| self.map_display_to_audio_idx(normalizer, idx))
            .map(|idx| idx.min(audio_count.saturating_sub(1)))
    }

    fn set_audio_highlight_idx(
        &mut self,
        normalizer: &normalizer::TextNormalizer,
        audio_idx: usize,
    ) -> bool {
        let audio_count = self.current_audio_sentences(normalizer).len();
        if audio_count == 0 {
            self.highlighted_audio_idx = None;
            return false;
        }
        let clamped = audio_idx.min(audio_count.saturating_sub(1));
        self.highlighted_audio_idx = Some(clamped);
        self.highlighted_display_idx = self.map_audio_to_display_idx(normalizer, clamped);
        true
    }

    pub fn current_tts_audio_slice(
        &mut self,
        normalizer: &normalizer::TextNormalizer,
    ) -> (Vec<String>, usize) {
        let audio = self.current_audio_sentences(normalizer);
        if audio.is_empty() {
            return (audio, 0);
        }
        let start = self
            .current_audio_highlight_idx(normalizer)
            .unwrap_or(0)
            .min(audio.len().saturating_sub(1));
        (audio, start)
    }

    fn move_to_adjacent_page_with_sentences(
        &mut self,
        direction: isize,
        normalizer: &normalizer::TextNormalizer,
    ) -> bool {
        if direction == 0 || self.pages.is_empty() {
            return false;
        }
        let mut page = self.current_page as isize + direction;
        while page >= 0 && (page as usize) < self.pages.len() {
            let idx = page as usize;
            if self.page_sentence_counts.get(idx).copied().unwrap_or(0) > 0 {
                self.set_page(idx, normalizer);
                return true;
            }
            page += direction;
        }
        false
    }
}
