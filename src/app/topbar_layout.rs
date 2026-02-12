#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TopBarPlan {
    pub(crate) show_text_mode: bool,
    pub(crate) show_tts: bool,
    pub(crate) show_search: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TopBarLabels<'a> {
    pub(crate) theme: &'a str,
    pub(crate) settings: &'a str,
    pub(crate) stats: &'a str,
    pub(crate) text_mode: &'a str,
    pub(crate) tts: &'a str,
    pub(crate) search: &'a str,
}

const CONTROLS_SPACING_PX: f32 = 10.0;
const CONTROLS_PADDING_BUDGET_PX: f32 = 12.0;

pub(crate) fn estimate_button_width_px(label: &str) -> f32 {
    let chars = label.chars().count() as f32;
    (chars * 8.4) + 36.0
}

pub(crate) fn topbar_plan(available_width: f32, labels: TopBarLabels<'_>) -> TopBarPlan {
    let controls_budget = (available_width - CONTROLS_PADDING_BUDGET_PX).max(0.0);

    let mandatory_labels = [
        "Previous",
        "Next",
        labels.theme,
        "Close Book",
        labels.settings,
        labels.stats,
    ];
    let mandatory_width = mandatory_labels
        .iter()
        .map(|label| estimate_button_width_px(label))
        .sum::<f32>()
        + (CONTROLS_SPACING_PX * (mandatory_labels.len().saturating_sub(1) as f32));

    if mandatory_width >= controls_budget {
        return TopBarPlan {
            show_text_mode: false,
            show_tts: false,
            show_search: false,
        };
    }

    let mut used = mandatory_width;
    let mut show_text_mode = false;
    let mut show_tts = false;
    let mut show_search = false;

    let add_optional = |used: &mut f32, label: &str| -> bool {
        let extra = CONTROLS_SPACING_PX + estimate_button_width_px(label);
        if *used + extra <= controls_budget {
            *used += extra;
            true
        } else {
            false
        }
    };

    // Priority order intentionally preserves mode-switching first.
    if add_optional(&mut used, labels.text_mode) {
        show_text_mode = true;
    }
    if add_optional(&mut used, labels.tts) {
        show_tts = true;
    }
    if add_optional(&mut used, labels.search) {
        show_search = true;
    }

    TopBarPlan {
        show_text_mode,
        show_tts,
        show_search,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels() -> TopBarLabels<'static> {
        TopBarLabels {
            theme: "Day Mode",
            settings: "Show Settings",
            stats: "Show Stats",
            text_mode: "Text Only",
            tts: "Show TTS",
            search: "Search",
        }
    }

    #[test]
    fn shows_all_optional_with_large_width() {
        let plan = topbar_plan(5000.0, labels());
        assert!(plan.show_text_mode);
        assert!(plan.show_tts);
        assert!(plan.show_search);
    }

    #[test]
    fn preserves_priority_when_tight() {
        let l = labels();
        let mandatory = [
            "Previous",
            "Next",
            l.theme,
            "Close Book",
            l.settings,
            l.stats,
        ]
        .iter()
        .map(|label| estimate_button_width_px(label))
        .sum::<f32>()
            + 10.0 * 5.0;
        let width = mandatory + 10.0 + estimate_button_width_px(l.text_mode) + 5.0;
        let plan = topbar_plan(width + 12.0, l);
        assert!(plan.show_text_mode);
        assert!(!plan.show_tts);
        assert!(!plan.show_search);
    }

    #[test]
    fn applies_optional_thresholds_in_order() {
        let l = labels();
        let mandatory = [
            "Previous",
            "Next",
            l.theme,
            "Close Book",
            l.settings,
            l.stats,
        ]
        .iter()
        .map(|label| estimate_button_width_px(label))
        .sum::<f32>()
            + 10.0 * 5.0;

        let text_extra = 10.0 + estimate_button_width_px(l.text_mode);
        let tts_extra = 10.0 + estimate_button_width_px(l.tts);
        let search_extra = 10.0 + estimate_button_width_px(l.search);

        let only_mandatory = topbar_plan(mandatory + 12.0 + 1.0, l);
        assert_eq!(
            only_mandatory,
            TopBarPlan {
                show_text_mode: false,
                show_tts: false,
                show_search: false
            }
        );

        let with_text = topbar_plan(mandatory + text_extra + 12.0 + 1.0, l);
        assert_eq!(
            with_text,
            TopBarPlan {
                show_text_mode: true,
                show_tts: false,
                show_search: false
            }
        );

        let with_tts = topbar_plan(mandatory + text_extra + tts_extra + 12.0 + 1.0, l);
        assert_eq!(
            with_tts,
            TopBarPlan {
                show_text_mode: true,
                show_tts: true,
                show_search: false
            }
        );

        let with_search = topbar_plan(
            mandatory + text_extra + tts_extra + search_extra + 12.0 + 1.0,
            l,
        );
        assert_eq!(
            with_search,
            TopBarPlan {
                show_text_mode: true,
                show_tts: true,
                show_search: true
            }
        );
    }
}
