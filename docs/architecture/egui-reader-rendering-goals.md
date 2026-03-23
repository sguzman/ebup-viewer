# Egui Reader Rendering Goals And Acceptance

This document captures the top-level goals and acceptance criteria for the egui reader rendering roadmap.

## Goals
- Rebuild EPUB/TXT/Markdown/HTML reading in egui while preserving text ownership, sentence highlighting, click-to-play, and reading controls.
- Replace legacy web rendering assumptions with a Rust-native render model suitable for immediate-mode UI.
- Keep canonical sentence/TTS ownership entirely in the Rust domain state.

## Acceptance criteria
- Text-only and pretty-text reader modes are fully specified for a Rust-native egui implementation.
- Canonical sentence/TTS ownership is preserved and explicit.
- HTML/markdown rendering no longer depends on legacy DOM/WebView ownership in the target plan.
- Scroll, jump, and highlight semantics are concrete enough for implementation without reopening design questions.
