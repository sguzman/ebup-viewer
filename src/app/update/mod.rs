use crate::calibre::{CalibreBook, CalibreConfig};
use iced::widget::scrollable::RelativeOffset;

mod appearance;
mod core;
mod navigation;
mod scroll;
mod tts;

/// Describes work that must be performed outside the pure reducer.
pub(super) enum Effect {
    SaveConfig,
    SaveBookmark,
    StartTts {
        page: usize,
        sentence_idx: usize,
    },
    StopTts,
    ScrollTo(RelativeOffset),
    AutoScrollToCurrent,
    LoadCalibreBooks {
        force_refresh: bool,
    },
    ResolveCalibreBook {
        book: CalibreBook,
        config: CalibreConfig,
    },
    LoadBook(std::path::PathBuf),
    QuitSafely,
}
