# Egui Native PDF Parity Gate

This document defines the parity gate for the egui-native PDF subsystem. It establishes a concrete checklist-to-gate mapping so migration status can be audited without relying on implicit memory.

## Gate inputs
- Primary checklist: `docs/qa/native-pdf-reader-checklist.md`
- Roadmap: `docs/roadmaps/egui-native-pdf-roadmap.md`
- Contract docs:
  - `docs/architecture/egui-native-pdf-subsystem.md`
  - `docs/architecture/egui-native-pdf-text-sync.md`
  - `docs/architecture/egui-native-pdf-interaction.md`
  - `docs/architecture/pdf-renderer-contract.md`

## Gate structure
- **Functional parity:** checklist items that must be satisfied before native PDF replaces the WebView path.
- **Degraded modes parity:** render-only and OCR-required behavior must be traceable via `pdf.*` spans and visible in diagnostics.
- **Performance parity:** scheduler/viewport budget behavior must align with shell tracing, with explicit spans for render/eviction/throttle events.

## Passing criteria
- All non-manual checklist items are satisfied.
- Manual QA is scheduled separately and recorded as a sign-off note.
- Tracing fields are populated for zoom/scroll, overlay apply/cleanup, OCR runs, and confidence tier updates.

## Required artifacts
- A short QA report that references the checklist section and includes a trace log snapshot.
- A commit or changelog entry noting the parity gate completion.
