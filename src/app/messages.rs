use crate::calibre::{CalibreBook, CalibreColumn};
use crate::config::{FontFamily, FontWeight};
use iced::keyboard::{Key, Modifiers};
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
    ToggleSearch,
    SearchQueryChanged(String),
    SearchSubmit,
    SearchNext,
    SearchPrev,
    ToggleRecentBooks,
    OpenRecentBook(PathBuf),
    ToggleCalibreBrowser,
    OpenPathInputChanged(String),
    OpenPathRequested,
    RefreshCalibreBooks,
    SortCalibreBy(CalibreColumn),
    CalibreBooksLoaded {
        books: Vec<CalibreBook>,
        error: Option<String>,
    },
    OpenCalibreBook(u64),
    CalibreBookResolved {
        book_id: u64,
        path: Option<PathBuf>,
        error: Option<String>,
    },
    ToggleTextOnly,
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
    TogglePlayPause,
    RepeatCurrentSentence,
    SafeQuit,
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
    SetTtsVolume(f32),
    SeekForward,
    SeekBackward,
    SentenceClicked(usize),
    WindowResized {
        width: f32,
        height: f32,
    },
    WindowMoved {
        x: f32,
        y: f32,
    },
    KeyPressed {
        key: Key,
        modifiers: Modifiers,
    },
    Scrolled {
        offset: RelativeOffset,
        viewport_width: f32,
        viewport_height: f32,
        content_width: f32,
        content_height: f32,
    },
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
