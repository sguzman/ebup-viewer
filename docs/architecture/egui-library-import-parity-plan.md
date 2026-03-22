# Egui Library Import Parity Plan

This document captures objectives, parity requirements, and acceptance criteria for starter/library/import flows.

## Objectives
- Rebuild starter, recent books, local open, Calibre browsing, and browser-tab import in egui.
- Preserve current source acquisition, refresh, recents, and deletion semantics.
- Keep browser-tab import and Calibre as first-class migration scope items.

## Parity requirements
- Rust integration tests cover source opening, recent deletion, and import-service behavior.
- Parity checks are performed against the existing browser-tab and starter roadmaps/checklists.

## Acceptance criteria
- Starter/library/import features are fully in scope and explicitly owned in the egui migration.
- Calibre and browser-tab flows have native-egui UI and service contracts.
- Recents/local-open/delete/reopen behavior is specified without reliance on Tauri/UI code.
- No major starter/import parity decision remains open.
