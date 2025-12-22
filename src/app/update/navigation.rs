use super::Effect;
use super::super::state::App;
use crate::pagination::{MAX_LINES_PER_PAGE, MIN_LINES_PER_PAGE};
use crate::text_utils::split_sentences;
use iced::widget::scrollable::RelativeOffset;
use tracing::debug;

impl App {
    pub(super) fn handle_next_page(&mut self, effects: &mut Vec<Effect>) {
        effects.extend(self.go_to_page(self.reader.current_page + 1));
    }

    pub(super) fn handle_previous_page(&mut self, effects: &mut Vec<Effect>) {
        if self.reader.current_page > 0 {
            effects.extend(self.go_to_page(self.reader.current_page - 1));
        }
    }

    pub(super) fn handle_lines_per_page_changed(
        &mut self,
        lines: u32,
        effects: &mut Vec<Effect>,
    ) {
        let clamped = lines.clamp(MIN_LINES_PER_PAGE as u32, MAX_LINES_PER_PAGE as u32) as usize;
        if clamped != self.config.lines_per_page {
            let anchor = self
                .reader
                .pages
                .get(self.reader.current_page)
                .and_then(|p| split_sentences(p.clone()).into_iter().next());
            let before = self.reader.current_page;
            self.config.lines_per_page = clamped;
            self.repaginate();
            if let Some(sentence) = anchor {
                if let Some(idx) = self.reader.pages.iter().position(|page| page.contains(&sentence))
                {
                    self.reader.current_page = idx;
                }
            }
            if self.reader.current_page != before {
                self.bookmark.last_scroll_offset = RelativeOffset::START;
                effects.push(Effect::SaveBookmark);
            }
            debug!(
                lines_per_page = self.config.lines_per_page,
                "Lines per page changed"
            );
            effects.push(Effect::SaveConfig);
        }
    }

    fn go_to_page(&mut self, new_page: usize) -> Vec<Effect> {
        let mut effects = Vec::new();
        if new_page < self.reader.pages.len() {
            self.reader.current_page = new_page;
            self.bookmark.last_scroll_offset = RelativeOffset::START;
            tracing::info!(page = self.reader.current_page + 1, "Navigated to page");
            effects.push(Effect::StartTts {
                page: self.reader.current_page,
                sentence_idx: 0,
            });
            effects.push(Effect::AutoScrollToCurrent);
            effects.push(Effect::SaveBookmark);
        }
        effects
    }
}
