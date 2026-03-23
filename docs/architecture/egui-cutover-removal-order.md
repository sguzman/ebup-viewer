# Egui Cutover Dependency Removal Order

This document defines the dependency removal order for cutover (now complete).

## Removal order
- Removed Tauri as the default app entrypoint in developer docs and build scripts.
- Removed TS binding generation and bridge compatibility checks after Rust-native replacements were in place.
- Removed frontend build/test commands from CI once native replacements were authoritative.
- Removed frontend dependencies and scripts after no runtime/test gate depended on them.
