use super::super::super::messages::Message;
use super::super::super::state::App;
use iced::keyboard::{Key, Modifiers, key};

impl App {
    pub(super) fn shortcut_message_for_key(
        &self,
        key: Key,
        modifiers: Modifiers,
    ) -> Option<Message> {
        let pressed = match key.as_ref() {
            Key::Named(key::Named::Space) => "space".to_string(),
            Key::Character(ch) => ch.to_ascii_lowercase(),
            _ => return None,
        };

        if Self::shortcut_matches(
            &self.config.key_toggle_play_pause,
            "space",
            &pressed,
            modifiers,
        ) {
            Some(Message::TogglePlayPause)
        } else if Self::shortcut_matches(&self.config.key_safe_quit, "q", &pressed, modifiers) {
            Some(Message::SafeQuit)
        } else if Self::shortcut_matches(&self.config.key_next_sentence, "f", &pressed, modifiers) {
            Some(Message::SeekForward)
        } else if Self::shortcut_matches(&self.config.key_prev_sentence, "s", &pressed, modifiers) {
            Some(Message::SeekBackward)
        } else if Self::shortcut_matches(&self.config.key_repeat_sentence, "r", &pressed, modifiers)
        {
            Some(Message::RepeatCurrentSentence)
        } else if Self::shortcut_matches(
            &self.config.key_toggle_search,
            "ctrl+f",
            &pressed,
            modifiers,
        ) {
            Some(Message::ToggleSearch)
        } else if Self::shortcut_matches(
            &self.config.key_toggle_settings,
            "ctrl+t",
            &pressed,
            modifiers,
        ) {
            Some(Message::ToggleSettings)
        } else if Self::shortcut_matches(
            &self.config.key_toggle_stats,
            "ctrl+g",
            &pressed,
            modifiers,
        ) {
            Some(Message::ToggleStats)
        } else if Self::shortcut_matches(&self.config.key_toggle_tts, "ctrl+y", &pressed, modifiers)
        {
            Some(Message::ToggleTtsControls)
        } else {
            None
        }
    }

    pub(super) fn shortcut_matches(
        raw: &str,
        fallback: &str,
        pressed: &str,
        modifiers: Modifiers,
    ) -> bool {
        let normalized = Self::normalize_shortcut_token(raw, fallback);

        let mut required_ctrl = false;
        let mut required_alt = false;
        let mut required_logo = false;
        let mut required_shift = false;
        let mut required_key: Option<&str> = None;

        for token in normalized
            .split('+')
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            match token {
                "ctrl" | "control" => required_ctrl = true,
                "alt" => required_alt = true,
                "logo" | "meta" | "super" | "cmd" | "command" => required_logo = true,
                "shift" => required_shift = true,
                key => required_key = Some(key),
            }
        }

        let required_key = required_key.unwrap_or(fallback);
        if pressed != required_key {
            return false;
        }

        modifiers.control() == required_ctrl
            && modifiers.alt() == required_alt
            && modifiers.logo() == required_logo
            && modifiers.shift() == required_shift
    }

    pub(super) fn normalize_shortcut_token(raw: &str, fallback: &str) -> String {
        let normalized = raw.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            fallback.to_string()
        } else {
            normalized.replace("spacebar", "space")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::Modifiers;

    #[test]
    fn normalizes_spacebar_alias() {
        assert_eq!(App::normalize_shortcut_token(" SpaceBar ", "x"), "space");
    }

    #[test]
    fn matches_ctrl_f_shortcut() {
        assert!(App::shortcut_matches("ctrl+f", "x", "f", Modifiers::CTRL));
    }

    #[test]
    fn rejects_unexpected_extra_modifier() {
        assert!(!App::shortcut_matches(
            "ctrl+f",
            "x",
            "f",
            Modifiers::CTRL | Modifiers::SHIFT,
        ));
    }
}
