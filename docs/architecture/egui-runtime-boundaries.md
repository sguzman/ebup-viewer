# Expected Egui Boundaries
Phase 1: Architecture Extraction

## Target Rust Crate Map

The workspace is structured to move all state and runtime orchestration into native Rust before introducing the Egui shell.

1. **`lanternleaf-core`**: 
   - Pure domain state: parsers (EPUB, PDF, Calibre).
   - Cache, config, and persistence mechanisms.
   - Piper TTS engine runtime bindings.
   - Free from any Tauri dependencies.

2. **`lanternleaf-app`**: 
   - Top-level application state (`AppState`, `SessionDomainState`).
   - UI Data Transfer Objects (DTOs) in `contracts.rs`.
   - Formal typed Rust interfaces for all application commands and events (`bridge.rs`, `pipeline.rs`).
   - Acts as the intermediary layer, holding domain-specific orchestration decoupled from any specific presentation framework.

3. **`lanternleaf-egui`** (Future Phase 2): 
   - The `eframe` + `egui` native desktop executable.
   - Responsible strictly for shell rendering and UI layout.
   - Consumes `lanternleaf-app` directly in-process via typed traits, bypassing IPC or JSON serialization.

## Typed Interfaces

Previously, the application relied heavily on Tauri commands living in `src-tauri`. Going forward:

- Tauri command endpoints are being entirely replaced by the formal Rust traits defined in `lanternleaf-app::bridge`.
- The `Bridge` trait specifies the exact request-response contracts for all async operations (source opening, TTS control, PDF generation, etc.).

## DTO Ownership

- The generated TypeScript bindings (formerly built via `export_ts_bindings` in `src-tauri`) are obsolete in the new architecture. 
- Ownership is transitioning purely to Rust-side view-models within `lanternleaf-app::contracts`. Egui will consume these native generic structs directly without stringifying boundaries.
