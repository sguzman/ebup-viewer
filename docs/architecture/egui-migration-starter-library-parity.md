# Egui Migration Starter/Library/Import Parity

This document captures Phase 5 parity expectations for starter, library, and import flows.

## Parity requirements
- Rebuild starter shell, recent books, Calibre browser, local file open flow, and browser-tab import in egui.
- Preserve deletion, reopen, cache cleanup, and metadata display semantics.
- Expose browser-tab and Calibre diagnostics in a Rust-native UI.

## Phase 5 exit criteria
- The egui app can serve as the main user-facing entrypoint for all current source acquisition paths.
