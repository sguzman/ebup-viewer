# Egui Migration PDF Parity

This document captures Phase 4 PDF parity expectations.

## Parity requirements
- Implement Rust-native PDF rendering, viewport management, text mapping, and overlay/highlight behavior in egui.
- Preserve the current PDF quality-class and degraded-mode contracts.
- Reach parity for jump-to-highlight, search navigation, and playback sync.

## Phase 4 exit criteria
- PDF flows satisfy the same manual QA and parity acceptance gates used for the Tauri reader.
