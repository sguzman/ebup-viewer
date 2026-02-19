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
    PrepareTtsBatches {
        page: usize,
        request_id: u64,
        audio_start_idx: usize,
        audio_sentences: Vec<String>,
    },
    PrepareTtsAppend {
        page: usize,
        request_id: u64,
        start_idx: usize,
        audio_sentences: Vec<String>,
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
    ReadClipboard,
    LoadBook(std::path::PathBuf),
    ReturnToStarter,
    QuitSafely,
}
