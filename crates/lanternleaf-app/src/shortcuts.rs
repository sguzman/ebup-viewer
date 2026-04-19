use crate::contracts::BootstrapConfig;
use crate::pipeline::{AppCommand, ReaderCommand};
use lanternleaf_core::session::SessionCommand;
use std::sync::{Arc, Mutex};

/// Identifier for a registered shortcut binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShortcutId(u64);

/// Indicates the UI scope where a shortcut is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutScope {
    Global,
    Starter,
    Reader,
    Panel,
}

/// UI-only actions that are not AppCommands but still flow through the shortcut registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiShortcutAction {
    FocusSearch,
}

/// The action executed when a shortcut fires.
#[derive(Debug, Clone)]
pub enum ShortcutAction {
    Command(AppCommand),
    Ui(UiShortcutAction),
}

/// A single registered shortcut binding.
#[derive(Debug, Clone)]
pub struct ShortcutBinding {
    pub id: ShortcutId,
    pub scope: ShortcutScope,
    pub combo: String,
    pub display: String,
    pub action: ShortcutAction,
}

struct ShortcutRegistryInner {
    entries: Vec<ShortcutBinding>,
    next_id: u64,
}

impl Default for ShortcutRegistryInner {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            next_id: 0,
        }
    }
}

/// Registry of keyboard shortcuts backed by normalized key combinations.
#[derive(Clone)]
pub struct ShortcutRegistry {
    inner: Arc<Mutex<ShortcutRegistryInner>>,
}

impl Default for ShortcutRegistry {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ShortcutRegistryInner::default())),
        }
    }
}

impl ShortcutRegistry {
    /// Create a registry preloaded with user-configured shortcuts from the bootstrap config.
    pub fn with_bootstrap_config(config: &BootstrapConfig) -> Self {
        let registry = Self::default();
        let register = |scope, combo: &str, action, registry: &ShortcutRegistry| {
            let combo = combo.trim();
            if combo.is_empty() {
                return;
            }
            registry.register(scope, combo, action);
        };

        register(
            ShortcutScope::Reader,
            &config.key_toggle_settings,
            ShortcutAction::Command(AppCommand::ToggleSettingsPanel),
            &registry,
        );
        register(
            ShortcutScope::Reader,
            &config.key_toggle_stats,
            ShortcutAction::Command(AppCommand::ToggleStatsPanel),
            &registry,
        );
        register(
            ShortcutScope::Reader,
            &config.key_toggle_tts,
            ShortcutAction::Command(AppCommand::ToggleTtsPanel),
            &registry,
        );
        register(
            ShortcutScope::Reader,
            &config.key_toggle_play_pause,
            ShortcutAction::Command(AppCommand::Reader(ReaderCommand::Session(
                SessionCommand::TtsTogglePlayPause,
            ))),
            &registry,
        );
        register(
            ShortcutScope::Reader,
            &config.key_next_sentence,
            ShortcutAction::Command(AppCommand::Reader(ReaderCommand::Session(
                SessionCommand::TtsSeekNext,
            ))),
            &registry,
        );
        register(
            ShortcutScope::Reader,
            &config.key_prev_sentence,
            ShortcutAction::Command(AppCommand::Reader(ReaderCommand::Session(
                SessionCommand::TtsSeekPrev,
            ))),
            &registry,
        );
        register(
            ShortcutScope::Reader,
            &config.key_repeat_sentence,
            ShortcutAction::Command(AppCommand::Reader(ReaderCommand::Session(
                SessionCommand::TtsRepeatSentence,
            ))),
            &registry,
        );
        register(
            ShortcutScope::Reader,
            &config.key_toggle_search,
            ShortcutAction::Ui(UiShortcutAction::FocusSearch),
            &registry,
        );
        register(
            ShortcutScope::Global,
            &config.key_safe_quit,
            ShortcutAction::Command(AppCommand::SafeQuit),
            &registry,
        );

        registry
    }

    /// Registers a new shortcut binding. Returns the assigned identifier if the combination was valid.
    pub fn register(
        &self,
        scope: ShortcutScope,
        combo: impl Into<String>,
        action: ShortcutAction,
    ) -> Option<ShortcutId> {
        let mut inner = self.inner.lock().unwrap();
        let raw_combo = combo.into();
        let normalized = Self::normalize_combo(&raw_combo);
        if normalized.is_empty() {
            return None;
        }
        let id = ShortcutId(inner.next_id);
        inner.next_id = inner.next_id.wrapping_add(1);
        inner.entries.push(ShortcutBinding {
            id,
            scope,
            combo: normalized,
            display: raw_combo,
            action,
        });
        Some(id)
    }

    /// Finds bindings whose normalized combo matches the input and whose scope matches the
    /// provided scope (or global scope).
    pub fn matches(&self, combo: &str, scope: ShortcutScope) -> Vec<ShortcutBinding> {
        let normalized = Self::normalize_combo(combo);
        let inner = self.inner.lock().unwrap();
        inner
            .entries
            .iter()
            .filter(|binding| {
                binding.combo == normalized
                    && (binding.scope == scope || binding.scope == ShortcutScope::Global)
            })
            .cloned()
            .collect()
    }

    /// Returns a snapshot of all registered bindings.
    pub fn bindings(&self) -> Vec<ShortcutBinding> {
        let inner = self.inner.lock().unwrap();
        inner.entries.clone()
    }

    fn normalize_combo(combo: &str) -> String {
        combo
            .chars()
            .filter(|c| !c.is_whitespace())
            .flat_map(|c| c.to_lowercase())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::BootstrapConfig;
    use crate::pipeline::ReaderCommand;
    use lanternleaf_core::config::{FontFamily, FontWeight, HighlightColor, ThemeMode};

    fn sample_bootstrap_config() -> BootstrapConfig {
        BootstrapConfig {
            theme: ThemeMode::Day,
            font_family: FontFamily::Lexend,
            font_weight: FontWeight::Normal,
            day_highlight: HighlightColor {
                r: 0.1,
                g: 0.2,
                b: 0.3,
                a: 0.4,
            },
            night_highlight: HighlightColor {
                r: 0.5,
                g: 0.6,
                b: 0.7,
                a: 0.8,
            },
            log_level: "info".to_string(),
            default_font_size: 18,
            default_lines_per_page: 30,
            default_tts_speed: 1.0,
            default_pause_after_sentence: 0.0,
            key_toggle_play_pause: "Space".to_string(),
            key_next_sentence: "J".to_string(),
            key_prev_sentence: "K".to_string(),
            key_repeat_sentence: "L".to_string(),
            key_toggle_search: "/".to_string(),
            key_safe_quit: "Q".to_string(),
            key_toggle_settings: "S".to_string(),
            key_toggle_stats: "D".to_string(),
            key_toggle_tts: "T".to_string(),
            browser_tabs_enabled: true,
            close_browser_tab_on_recent_delete: false,
            remote_url: None,
        }
    }

    #[test]
    fn bootstrap_config_registers_shortcuts() {
        let config = sample_bootstrap_config();
        let registry = ShortcutRegistry::with_bootstrap_config(&config);

        let play_pause = registry.matches("space", ShortcutScope::Reader);
        assert!(play_pause.iter().any(|binding| {
            matches!(
                binding.action,
                ShortcutAction::Command(AppCommand::Reader(ReaderCommand::Session(
                    SessionCommand::TtsTogglePlayPause
                )))
            )
        }));

        let safe_quit = registry.matches("q", ShortcutScope::Reader);
        assert!(safe_quit.iter().any(|binding| {
            matches!(
                binding.action,
                ShortcutAction::Command(AppCommand::SafeQuit)
            )
        }));
    }
}
