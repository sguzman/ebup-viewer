use super::*;

impl ReaderSession {
    pub fn next_page(&mut self, normalizer: &normalizer::TextNormalizer) {
        if self.current_page + 1 >= self.pages.len() {
            return;
        }
        self.current_page += 1;
        self.highlighted_display_idx = Some(0).filter(|_| self.current_display_len() > 0);
        self.highlighted_audio_idx = None;
        self.current_plan_page = None;
        self.current_plan = None;
        if self.text_only_mode {
            self.highlighted_audio_idx = self
                .highlighted_display_idx
                .and_then(|idx| self.map_display_to_audio_idx(normalizer, idx));
        }
        self.update_search_matches(normalizer);
    }

    pub fn prev_page(&mut self, normalizer: &normalizer::TextNormalizer) {
        if self.current_page == 0 {
            return;
        }
        self.current_page = self.current_page.saturating_sub(1);
        self.highlighted_display_idx = Some(0).filter(|_| self.current_display_len() > 0);
        self.highlighted_audio_idx = None;
        self.current_plan_page = None;
        self.current_plan = None;
        if self.text_only_mode {
            self.highlighted_audio_idx = self
                .highlighted_display_idx
                .and_then(|idx| self.map_display_to_audio_idx(normalizer, idx));
        }
        self.update_search_matches(normalizer);
    }

    pub fn set_page(&mut self, page: usize, normalizer: &normalizer::TextNormalizer) {
        if self.pages.is_empty() {
            self.current_page = 0;
            return;
        }
        self.current_page = page.min(self.pages.len().saturating_sub(1));
        self.highlighted_display_idx = Some(0).filter(|_| self.current_display_len() > 0);
        self.highlighted_audio_idx = None;
        self.current_plan_page = None;
        self.current_plan = None;
        if self.text_only_mode {
            self.highlighted_audio_idx = self
                .highlighted_display_idx
                .and_then(|idx| self.map_display_to_audio_idx(normalizer, idx));
        }
        self.update_search_matches(normalizer);
    }

    pub(super) fn repaginate(
        &mut self,
        normalizer: &normalizer::TextNormalizer,
        preserve_global_idx: Option<usize>,
    ) {
        self.pages = pagination::paginate(
            &self.tts_text,
            self.config.font_size,
            self.config.lines_per_page,
        );
        if self.pages.is_empty() {
            self.pages.push(String::new());
        }
        self.markdown_pages = self
            .reading_markdown
            .as_ref()
            .map(|markdown| {
                pagination::paginate(markdown, self.config.font_size, self.config.lines_per_page)
            })
            .unwrap_or_default();
        if !self.markdown_pages.is_empty() && self.markdown_pages.len() < self.pages.len() {
            self.markdown_pages.resize(self.pages.len(), String::new());
        }
        self.raw_page_sentences = self
            .pages
            .iter()
            .map(|page| text_utils::split_sentences(page))
            .collect();
        self.sentence_anchor_maps = self
            .raw_page_sentences
            .iter()
            .enumerate()
            .map(|(page_idx, sentences)| {
                self.build_sentence_anchor_map_for_page(page_idx, sentences.len())
            })
            .collect();
        for (page_idx, map) in self.sentence_anchor_maps.iter().enumerate() {
            crate::cache::persist_sentence_anchor_map(&self.source_path, page_idx, map);
        }
        self.page_word_counts = self
            .pages
            .iter()
            .map(|page| page.split_whitespace().count())
            .collect();
        self.page_sentence_counts = self.raw_page_sentences.iter().map(Vec::len).collect();

        self.current_page = self.current_page.min(self.pages.len().saturating_sub(1));
        self.current_plan_page = None;
        self.current_plan = None;

        if let Some(global_idx) = preserve_global_idx {
            let (page, idx) = self.page_idx_for_global_sentence(global_idx);
            self.current_page = page;
            self.highlighted_display_idx = Some(idx);
        } else {
            self.highlighted_display_idx = Some(0).filter(|_| self.current_display_len() > 0);
        }

        self.highlighted_audio_idx = None;
        if self.text_only_mode {
            self.highlighted_audio_idx = self
                .highlighted_display_idx
                .and_then(|idx| self.map_display_to_audio_idx(normalizer, idx));
        }
        self.update_search_matches(normalizer);
    }

    fn global_idx_for_bookmark(&self, bookmark: &crate::cache::Bookmark) -> Option<usize> {
        let sentence_idx = bookmark.sentence_idx?;
        let page = bookmark
            .page
            .min(self.page_sentence_counts.len().saturating_sub(1));
        let base: usize = self.page_sentence_counts.iter().take(page).sum();
        Some(base + sentence_idx)
    }

    pub(super) fn restore_bookmark_position(
        &mut self,
        bookmark: &crate::cache::Bookmark,
        normalizer: &normalizer::TextNormalizer,
    ) {
        if self.page_sentence_counts.is_empty() {
            self.current_page = 0;
            self.highlighted_display_idx = None;
            self.highlighted_audio_idx = None;
            return;
        }

        let clamped_page = bookmark
            .page
            .min(self.page_sentence_counts.len().saturating_sub(1));
        self.current_page = clamped_page;

        self.highlighted_display_idx =
            if let Some(global_idx) = self.global_idx_for_bookmark(bookmark) {
                let (page, idx) = self.page_idx_for_global_sentence(global_idx);
                self.current_page = page;
                Some(idx)
            } else {
                Some(0).filter(|_| self.current_display_len() > 0)
            };

        self.highlighted_audio_idx = None;
        if self.text_only_mode {
            self.highlighted_audio_idx = self
                .highlighted_display_idx
                .and_then(|idx| self.map_display_to_audio_idx(normalizer, idx));
        }
    }

    fn page_idx_for_global_sentence(&self, global_idx: usize) -> (usize, usize) {
        if self.page_sentence_counts.is_empty() {
            return (0, 0);
        }
        let mut remaining = global_idx;
        for (page_idx, count) in self.page_sentence_counts.iter().copied().enumerate() {
            if count == 0 {
                continue;
            }
            if remaining < count {
                return (page_idx, remaining);
            }
            remaining = remaining.saturating_sub(count);
        }
        let last_page = self.page_sentence_counts.len().saturating_sub(1);
        let last_idx = self.page_sentence_counts[last_page].saturating_sub(1);
        (last_page, last_idx)
    }

    pub(super) fn current_display_len(&self) -> usize {
        self.raw_page_sentences
            .get(self.current_page)
            .map(Vec::len)
            .unwrap_or(0)
    }
}
