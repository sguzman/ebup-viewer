# Egui Migration Crate Map

This document defines the target Rust crate map for the egui migration.

## Target crates and responsibilities
- **App shell/runtime:** the native egui desktop application entry point and shell widget orchestration.
- **Document/session state:** canonical session, playback, and document state (Rust-owned).
- **Reader presentation models:** content-block and reader view models for pretty text and PDF.
- **Persistence/config/cache:** config, cache artifacts, bookmarks, and recents.
- **Import/library integrations:** browser-tab import, Calibre, and local file ingestion services.
- **PDF rendering/sync:** PDF rendering, geometry mapping, and highlight/overlay sync.

## Notes
- Crate boundaries should be owned by Rust traits/modules rather than Tauri command shims.
- Tracing is mandatory across crate boundaries to preserve parity diagnostics.
