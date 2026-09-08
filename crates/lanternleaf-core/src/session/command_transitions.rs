use super::*;

impl SessionCommand {
    pub fn action(&self) -> &'static str {
        match self {
            Self::GetSnapshot => "reader_get_snapshot",
            Self::NextPage => "reader_next_page",
            Self::PrevPage => "reader_prev_page",
            Self::SetPage { .. } => "reader_set_page",
            Self::SentenceClick { .. } => "reader_sentence_click",
            Self::NextSentence => "reader_next_sentence",
            Self::PrevSentence => "reader_prev_sentence",
            Self::ToggleTextOnly => "reader_toggle_text_only",
            Self::ApplySettings { .. } => "reader_apply_settings",
            Self::SearchSetQuery { .. } => "reader_search_set_query",
            Self::SearchNext => "reader_search_next",
            Self::SearchPrev => "reader_search_prev",
            Self::TtsPlay => "reader_tts_play",
            Self::TtsPause => "reader_tts_pause",
            Self::TtsTogglePlayPause => "reader_tts_toggle_play_pause",
            Self::TtsPlayFromPageStart => "reader_tts_play_from_page_start",
            Self::TtsPlayFromHighlight => "reader_tts_play_from_highlight",
            Self::TtsSeekNext => "reader_tts_seek_next",
            Self::TtsSeekPrev => "reader_tts_seek_prev",
            Self::TtsRepeatSentence => "reader_tts_repeat_sentence",
            Self::TtsStop => "reader_tts_stop",
        }
    }
}

impl ReaderSession {
    pub fn apply_command_lightweight(
        &mut self,
        command: SessionCommand,
        normalizer: &normalizer::TextNormalizer,
    ) -> ReaderSessionDelta {
        let action = command.action();
        match command {
            SessionCommand::GetSnapshot => {}
            SessionCommand::NextPage => self.next_page(normalizer),
            SessionCommand::PrevPage => self.prev_page(normalizer),
            SessionCommand::SetPage { page } => self.set_page(page, normalizer),
            SessionCommand::SentenceClick { sentence_idx } => {
                self.sentence_click(sentence_idx, normalizer)
            }
            SessionCommand::NextSentence => self.select_next_sentence(normalizer),
            SessionCommand::PrevSentence => self.select_prev_sentence(normalizer),
            SessionCommand::ToggleTextOnly => self.toggle_text_only(normalizer),
            SessionCommand::ApplySettings { patch } => self.apply_settings_patch(patch, normalizer),
            SessionCommand::SearchSetQuery { query } => self.set_search_query(query, normalizer),
            SessionCommand::SearchNext => self.search_next(normalizer),
            SessionCommand::SearchPrev => self.search_prev(normalizer),
            SessionCommand::TtsPlay => self.tts_play(normalizer),
            SessionCommand::TtsPause => self.tts_pause(),
            SessionCommand::TtsTogglePlayPause => self.tts_toggle_play_pause(normalizer),
            SessionCommand::TtsPlayFromPageStart => self.tts_play_from_page_start(normalizer),
            SessionCommand::TtsPlayFromHighlight => self.tts_play_from_highlight(normalizer),
            SessionCommand::TtsSeekNext => self.tts_seek_next(normalizer),
            SessionCommand::TtsSeekPrev => self.tts_seek_prev(normalizer),
            SessionCommand::TtsRepeatSentence => self.tts_repeat_current_sentence(normalizer),
            SessionCommand::TtsStop => self.tts_stop(),
        }
        ReaderSessionDelta {
            action,
            playback: self.playback_view(normalizer),
        }
    }

    pub fn apply_command(
        &mut self,
        command: SessionCommand,
        panels: PanelState,
        normalizer: &normalizer::TextNormalizer,
    ) -> SessionEvent {
        let action = command.action();
        match command {
            SessionCommand::GetSnapshot => {}
            SessionCommand::NextPage => self.next_page(normalizer),
            SessionCommand::PrevPage => self.prev_page(normalizer),
            SessionCommand::SetPage { page } => self.set_page(page, normalizer),
            SessionCommand::SentenceClick { sentence_idx } => {
                self.sentence_click(sentence_idx, normalizer)
            }
            SessionCommand::NextSentence => self.select_next_sentence(normalizer),
            SessionCommand::PrevSentence => self.select_prev_sentence(normalizer),
            SessionCommand::ToggleTextOnly => self.toggle_text_only(normalizer),
            SessionCommand::ApplySettings { patch } => self.apply_settings_patch(patch, normalizer),
            SessionCommand::SearchSetQuery { query } => self.set_search_query(query, normalizer),
            SessionCommand::SearchNext => self.search_next(normalizer),
            SessionCommand::SearchPrev => self.search_prev(normalizer),
            SessionCommand::TtsPlay => self.tts_play(normalizer),
            SessionCommand::TtsPause => self.tts_pause(),
            SessionCommand::TtsTogglePlayPause => self.tts_toggle_play_pause(normalizer),
            SessionCommand::TtsPlayFromPageStart => self.tts_play_from_page_start(normalizer),
            SessionCommand::TtsPlayFromHighlight => self.tts_play_from_highlight(normalizer),
            SessionCommand::TtsSeekNext => self.tts_seek_next(normalizer),
            SessionCommand::TtsSeekPrev => self.tts_seek_prev(normalizer),
            SessionCommand::TtsRepeatSentence => self.tts_repeat_current_sentence(normalizer),
            SessionCommand::TtsStop => self.tts_stop(),
        }
        SessionEvent {
            action,
            snapshot: self.snapshot(panels, normalizer),
        }
    }

    pub fn source_path_str(&self) -> String {
        self.source_path.to_string_lossy().to_string()
    }

    pub fn to_bookmark(&self) -> crate::cache::Bookmark {
        let sentence_text = self.highlighted_display_idx.and_then(|idx| {
            self.raw_page_sentences
                .get(self.current_page)
                .and_then(|sentences| sentences.get(idx))
                .cloned()
        });
        let pdf_location = self.global_display_idx().and_then(|global_idx| {
            crate::cache::load_pdf_ocr_alignment_artifact(&self.source_path).and_then(|artifact| {
                artifact
                    .alignments
                    .into_iter()
                    .find(|location| location.sentence_idx == global_idx)
            })
        });
        let pdf_quality_class = crate::cache::load_pdf_ocr_alignment_artifact(&self.source_path)
            .map(|artifact| artifact.quality_class);
        crate::cache::Bookmark {
            page: self.current_page,
            sentence_idx: self.current_highlight_idx(),
            sentence_text,
            scroll_y: 0.0,
            pdf_page_idx: pdf_location.as_ref().and_then(|location| location.page_idx),
            pdf_rects: pdf_location
                .as_ref()
                .map(|location| location.rects.clone())
                .unwrap_or_default(),
            pdf_line_rects: pdf_location
                .as_ref()
                .map(|location| location.line_rects.clone())
                .unwrap_or_default(),
            pdf_block_rects: pdf_location
                .as_ref()
                .map(|location| location.block_rects.clone())
                .unwrap_or_default(),
            pdf_confidence: pdf_location
                .as_ref()
                .map(|location| location.confidence_tier.clone()),
            pdf_reason: pdf_location
                .as_ref()
                .map(|location| location.fallback_reason.clone()),
            pdf_quality_class,
            pdf_sentence_text_hash: self
                .highlighted_display_idx
                .and_then(|idx| {
                    self.raw_page_sentences
                        .get(self.current_page)
                        .and_then(|page| page.get(idx))
                })
                .map(|sentence| crate::cache::stable_sentence_text_hash(sentence)),
            pdf_token_lineage: pdf_location
                .as_ref()
                .map(|location| location.token_lineage.clone())
                .unwrap_or_default(),
        }
    }
}
