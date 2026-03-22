use lanternleaf_app::state::AppState;
use lanternleaf_egui::shell::{ActiveMode, FocusOwner, ShellState};

#[test]
fn shell_state_tracks_active_mode_and_focus() {
    let mut state = AppState::default();
    let mut shell = ShellState::default();

    shell.update_from_app_state(&state, false, false, false);
    assert_eq!(shell.active_mode, ActiveMode::Starter);
    assert_eq!(shell.focus_owner, FocusOwner::Starter);

    state.app_shell.operations.source_open = true;
    shell.update_from_app_state(&state, false, false, false);
    assert_eq!(shell.active_mode, ActiveMode::SourceLoading);

    state.app_shell.operations.source_open = false;
    state.app_shell.operations.calibre_load = true;
    shell.update_from_app_state(&state, false, false, false);
    assert_eq!(shell.active_mode, ActiveMode::Calibre);

    state.app_shell.operations.calibre_load = false;
    state.app_shell.operations.browser_tab_refresh = true;
    shell.update_from_app_state(&state, false, false, false);
    assert_eq!(shell.active_mode, ActiveMode::BrowserTabImport);

    state.app_shell.operations.browser_tab_refresh = false;
    shell.update_from_app_state(&state, true, false, false);
    assert_eq!(shell.focus_owner, FocusOwner::Modal);

    shell.update_from_app_state(&state, false, false, true);
    assert_eq!(shell.focus_owner, FocusOwner::PanelInput);
}
