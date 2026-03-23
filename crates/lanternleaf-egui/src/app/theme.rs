use lanternleaf_core::config;
use lanternleaf_app::state::AppState;
use lanternleaf_app::contracts::ReaderSnapshot;

use super::LanternLeafApp;

impl LanternLeafApp {
    pub(crate) fn theme_from_state(
        &self,
        state: &AppState,
        reader_snapshot: Option<&ReaderSnapshot>,
    ) -> config::ThemeMode {
        reader_snapshot
            .map(|snapshot| snapshot.settings.theme)
            .or_else(|| state.app_shell.bootstrap.as_ref().map(|bootstrap| bootstrap.config.theme))
            .or_else(|| state.app_shell.app_config_snapshot.as_ref().map(|config| config.theme))
            .unwrap_or(config::ThemeMode::Night)
    }

    pub(crate) fn resolve_theme(
        &self,
        state: &AppState,
        reader_snapshot: Option<&ReaderSnapshot>,
    ) -> config::ThemeMode {
        self.theme_override
            .unwrap_or_else(|| self.theme_from_state(state, reader_snapshot))
    }
}
