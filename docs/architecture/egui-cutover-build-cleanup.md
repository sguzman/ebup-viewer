# Egui Cutover Build Cleanup

This document captures the initial package/build cleanup steps for cutover.

## Cleanup steps
- Update workspace manifests to add the egui app crate as the canonical desktop target.
- Remove Tauri packaging/build instructions from root docs and scripts.
