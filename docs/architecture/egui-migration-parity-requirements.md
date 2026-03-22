# Egui Migration Parity Requirements

This document captures the test and parity requirements for the egui migration.

## Requirements
- Maintain a parity matrix linking each current feature area to an egui replacement owner and acceptance check.
- Add migration-specific comparison runs on representative EPUB, PDF, and browser-tab sources.
- Keep build verification green for Rust workspace and frontend until the old stack is retired.
- Require full implementation-phase build validation after changes, excluding AppImage/RPM/DEB packaging outputs.
- Track manual QA for PDF, HTML/EPUB, starter/library, TTS, and persistence separately.
