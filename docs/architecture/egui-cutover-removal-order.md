# Egui Cutover Dependency Removal Order

This document defines the dependency removal order for cutover.

## Removal order
- Remove Tauri as the default app entrypoint in developer docs and build scripts.
- Remove TS binding generation and bridge compatibility checks after Rust-native replacements are in place.
- Remove frontend build/test commands from required CI once native replacements are authoritative.
- Remove `ui/` package dependencies and scripts only after no production/runtime/test gate depends on them.
- Remove `src-tauri/` dependencies and workspace membership only after the egui app fully replaces its responsibilities.
