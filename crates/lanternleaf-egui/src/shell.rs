use lanternleaf_app::contracts::UiMode;
use lanternleaf_app::state::AppState;
use tracing::{debug, trace};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveMode {
    Starter,
    Reader,
    SourceLoading,
    SourceError,
    Calibre,
    BrowserTabImport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PanelVisibility {
    pub show_settings: bool,
    pub show_stats: bool,
    pub show_search: bool,
    pub show_tts: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalState {
    None,
    SafeQuitConfirm,
    CloseReaderConfirm,
}

impl Default for ModalState {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub level: NotificationLevel,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ShellState {
    pub active_mode: ActiveMode,
    pub panels: PanelVisibility,
    pub modal: ModalState,
    pub notifications: Vec<Notification>,
    pub safe_quit_pending: bool,
    pub screen_lock_active: bool,
}

impl Default for ShellState {
    fn default() -> Self {
        Self {
            active_mode: ActiveMode::Starter,
            panels: PanelVisibility::default(),
            modal: ModalState::None,
            notifications: Vec::new(),
            safe_quit_pending: false,
            screen_lock_active: false,
        }
    }
}

impl ShellState {
    pub fn update_from_app_state(
        &mut self,
        state: &AppState,
        show_safe_quit: bool,
        show_reader_confirm: bool,
        pending_search: bool,
    ) {
        let previous_mode = self.active_mode;
        let active_mode = if state.app_shell.operations.source_open {
            ActiveMode::SourceLoading
        } else if let Some(source_event) = state.runtime_jobs.source_open_event.as_ref() {
            if matches!(source_event.phase.as_str(), "failed") {
                ActiveMode::SourceError
            } else if state.app_shell.operations.browser_tab_refresh {
                ActiveMode::BrowserTabImport
            } else if state.app_shell.operations.calibre_load {
                ActiveMode::Calibre
            } else {
                mode_from_session(state)
            }
        } else if state.app_shell.operations.browser_tab_refresh {
            ActiveMode::BrowserTabImport
        } else if state.app_shell.operations.calibre_load {
            ActiveMode::Calibre
        } else {
            mode_from_session(state)
        };
        self.active_mode = active_mode;

        let session_panels = state
            .session
            .session
            .as_ref()
            .map(|session| session.panels)
            .unwrap_or_default();
        self.panels = PanelVisibility {
            show_settings: session_panels.show_settings,
            show_stats: session_panels.show_stats,
            show_tts: session_panels.show_tts,
            show_search: pending_search,
        };

        self.modal = if show_safe_quit {
            ModalState::SafeQuitConfirm
        } else if show_reader_confirm {
            ModalState::CloseReaderConfirm
        } else {
            ModalState::None
        };
        self.safe_quit_pending = show_safe_quit;

        if previous_mode != self.active_mode {
            debug!(?previous_mode, ?self.active_mode, "Shell active mode updated");
        } else {
            trace!(?self.active_mode, "Shell active mode unchanged");
        }
    }

    pub fn record_notification(&mut self, level: NotificationLevel, message: impl Into<String>) {
        let message = message.into();
        self.notifications.push(Notification { level, message });
        if self.notifications.len() > 10 {
            self.notifications.remove(0);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutSizeClass {
    Narrow,
    Standard,
    Wide,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutPolicy {
    pub size_class: LayoutSizeClass,
    pub min_desktop_width: f32,
    pub show_status_row: bool,
    pub show_side_panels: bool,
}

impl Default for LayoutPolicy {
    fn default() -> Self {
        Self {
            size_class: LayoutSizeClass::Standard,
            min_desktop_width: 900.0,
            show_status_row: true,
            show_side_panels: true,
        }
    }
}

impl LayoutPolicy {
    pub fn from_width(width: f32) -> Self {
        let min_desktop_width = 900.0;
        let size_class = if width < 720.0 {
            LayoutSizeClass::Narrow
        } else if width < 1100.0 {
            LayoutSizeClass::Standard
        } else {
            LayoutSizeClass::Wide
        };
        let show_side_panels = width >= min_desktop_width;
        let show_status_row = width >= 640.0;
        Self {
            size_class,
            min_desktop_width,
            show_status_row,
            show_side_panels,
        }
    }

    pub fn is_narrow(&self) -> bool {
        matches!(self.size_class, LayoutSizeClass::Narrow)
    }
}

fn mode_from_session(state: &AppState) -> ActiveMode {
    match state.session.session.as_ref().map(|session| session.mode) {
        Some(UiMode::Reader) => ActiveMode::Reader,
        _ => ActiveMode::Starter,
    }
}
