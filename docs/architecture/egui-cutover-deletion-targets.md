# Egui Cutover Deletion Targets

This document lists deletion targets for the final cutover.

## Deletion targets
- Delete `ui/` after test and doc replacement is complete.
- Delete `src-tauri/` after shell/runtime responsibilities are replaced.
- Remove root/package-manager artifacts no longer needed for the shipped product.
- Remove old browser/Tauri-specific docs after equivalent egui docs are published.
- Remove legacy migration shims and dual-run compatibility code once no longer needed.
