# Egui Native PDF Objectives

This document captures the core objectives for the native PDF subsystem migration.

## Objectives
- Replace the current pdf.js/WebView-based PDF reader with a Rust-native PDF subsystem integrated into egui.
- Preserve existing PDF quality contracts, degraded modes, text ownership rules, and playback/search/highlight semantics.
- Deliver native PDF rendering, zoom, viewport management, text extraction, and overlay sync without relying on Tauri or browser technology.
