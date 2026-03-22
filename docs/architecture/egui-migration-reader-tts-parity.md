# Egui Migration Reader + TTS Parity

This document captures Phase 3 reader and TTS parity expectations.

## Parity requirements
- Rebuild text-only and pretty-text reader flows for EPUB/TXT/Markdown/HTML in egui.
- Restore sentence highlighting, click-to-play, jump-to-highlight, auto-scroll/center, search, stats, and settings.
- Move playback/event ingestion fully into Rust-native app/runtime state.
- Keep canonical sentence and playback ownership unchanged from the current Rust logic.

## Phase 3 exit criteria
- Non-PDF reading and TTS flows reach parity with the Tauri app.
- Bookmark/config/session semantics remain deterministic.
