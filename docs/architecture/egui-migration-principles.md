# Egui Migration Principles

This document captures the guiding principles for the migration plan.

## Principles
- Keep canonical text/session ownership in Rust from the start of the migration.
- Extract reusable Rust crates before large UI rewrites whenever a dependency boundary is unclear.
- Prefer replacing bridge protocols with in-process Rust traits and typed events rather than 1:1 command shims.
- Do not delete Tauri/UI paths until parity and observability gates pass.
- Keep dual-run compatibility long enough to compare Tauri and egui behavior on the same sources.
- Treat PDF and browser-tab features as explicit risk domains, not hidden “later” work.
- Keep state boundaries explicit: document/session/playback/UI/persistence/runtime services.
