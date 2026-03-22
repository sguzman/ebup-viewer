# Egui Migration Cutover And Tauri Removal

This document captures Phase 7 cutover and Tauri removal expectations.

## Cutover requirements
- Switch primary developer and user docs to the egui app.
- Remove Tauri runtime ownership from CI/build scripts.
- Remove TS/React/Tauri dependencies and delete obsolete codepaths only after parity signoff.
- Retire generated TS bindings, browser tests, and WebView-specific rendering layers.

## Phase 7 exit criteria
- Shipped product is a pure Rust desktop app.
- No production dependency remains on `src-tauri/` or `ui/`.
