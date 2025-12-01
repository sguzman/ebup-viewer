use crate::config::{FontFamily, FontWeight};
use iced::widget::scrollable::RelativeOffset;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Messages emitted by the UI.
#[derive(Debug, Clone)]
pub enum Message {
    NextPage,
    PreviousPage,
    FontSizeChanged(u32),
    ToggleTheme,
    ToggleSettings,
    FontFamilyChanged(FontFamily),
    FontWeightChanged(FontWeight),
    LineSpacingChanged(f32),
    MarginHorizontalChanged(u16),
    MarginVerticalChanged(u16),
    WordSpacingChanged(u32),
    LetterSpacingChanged(u32),
    LinesPerPageChanged(u32),
    ToggleTtsControls,
    JumpToCurrentAudio,
    PauseAfterSentenceChanged(f32),
    DayHighlightChanged(Component, f32),
    NightHighlightChanged(Component, f32),
    AutoScrollTtsChanged(bool),
    CenterSpokenSentenceChanged(bool),
    Play,
    Pause,
    PlayFromPageStart,
    PlayFromCursor(usize),
    SetTtsSpeed(f32),
    SeekForward,
    SeekBackward,
    Scrolled(RelativeOffset),
    TtsPrepared {
        page: usize,
        start_idx: usize,
        request_id: u64,
        files: Vec<(PathBuf, Duration)>,
    },
    Tick(Instant),
}

#[derive(Debug, Clone, Copy)]
pub enum Component {
    R,
    G,
    B,
    A,
}
