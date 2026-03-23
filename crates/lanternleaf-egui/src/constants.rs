#![allow(dead_code)]

use std::time::Duration;

pub const PDF_CANVAS_BUDGET_PAGES: usize = 2;
pub const PDF_TEXT_LAYER_BUDGET_PAGES: usize = 1;
pub const PDF_CANVAS_TEXTURE_SIZE: [usize; 2] = [320, 450];
pub const PDF_TEXT_TEXTURE_SIZE: [usize; 2] = [300, 420];
pub const PDF_VIEWPORT_UPDATE_THROTTLE: Duration = Duration::from_millis(150);
pub const PDF_ZOOM_REQUEST_THROTTLE: Duration = Duration::from_millis(180);
pub const PDF_VIEWPORT_SCROLL_THRESHOLD: usize = 1;
pub const PDF_HIGHLIGHT_SCROLL_THRESHOLD: usize = 1;
pub const REGRESSION_EVENT_WINDOW: Duration = Duration::from_secs(3);
pub const READER_RENDR_ROADMAP_URL: &str =
    "https://github.com/sguzman/lantern-leaf/blob/main/docs/roadmaps/egui-reader-rendering-roadmap.md";
pub const PDF_SUBSYSTEM_ROADMAP_URL: &str =
    "https://github.com/sguzman/lantern-leaf/blob/main/docs/roadmaps/egui-native-pdf-roadmap.md";
pub const PRIORITIZATION_ROADMAP_URL: &str =
    "https://github.com/sguzman/lantern-leaf/blob/main/docs/roadmaps/implementation-prioritization-roadmap.md";
pub const TTS_ROADMAP_URL: &str =
    "https://github.com/sguzman/lantern-leaf/blob/main/docs/roadmaps/egui-tts-audio-and-playback-roadmap.md";
pub const SETTINGS_ROADMAP_URL: &str =
    "https://github.com/sguzman/lantern-leaf/blob/main/docs/roadmaps/egui-config-cache-and-persistence-roadmap.md";
pub const PERSISTENCE_ROADMAP_URL: &str = SETTINGS_ROADMAP_URL;
pub const QA_REGRESSION_URL: &str =
    "https://github.com/sguzman/lantern-leaf/blob/main/docs/roadmaps/egui-testing-and-parity-roadmap.md";
pub const TIMELINE_ARCHIVE_DIR: &str = "logs/qa-timeline";
pub const MAX_PINNED_TIMELINE_ENTRIES: usize = 8;
pub const PINNED_TIMELINE_FILE: &str = "pinned-timeline.json";
