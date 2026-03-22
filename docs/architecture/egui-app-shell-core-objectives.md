# Egui App Shell Core Objectives

This document captures the core objectives and verification needs for the egui app shell.

## Objectives
- Replace the current Tauri window shell and React component composition with a native `eframe` + `egui` application shell.
- Rebuild starter-mode and reader-mode navigation, top bars, side panels, dialogs, and keyboard shortcuts in Rust.
- Preserve existing interaction semantics while moving all UI ownership into egui widgets and Rust-native app state.

## Verification requirements
- Add UI behavior harnesses where feasible for panel toggles and modal lifecycle.
- Verify control bars remain readable and do not collapse vertically under narrow widths.
