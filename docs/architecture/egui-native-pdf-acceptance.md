# Egui Native PDF Acceptance Mapping

This document maps the egui-native PDF acceptance criteria to concrete artifacts in the repo.

## Acceptance criteria mapping
- **Rust-native PDF rendering/sync strategy fully specified without WebView fallback**
  - `docs/architecture/egui-native-pdf-subsystem.md`
  - `docs/architecture/egui-native-pdf-renderer-decision.md`
- **Existing PDF quality contracts and degraded modes remain intact**
  - `docs/architecture/pdf-renderer-contract.md`
  - `docs/architecture/egui-native-pdf-text-sync.md`
  - `docs/architecture/egui-native-pdf-subsystem.md`
- **Page rastering, text extraction, overlay sync, jump behavior have explicit owners**
  - `docs/architecture/egui-native-pdf-subsystem.md`
  - `docs/architecture/egui-native-pdf-interaction.md`
- **Roadmap is decision-complete enough to start implementation**
  - `docs/roadmaps/egui-native-pdf-roadmap.md`
  - `docs/architecture/egui-native-pdf-interaction.md`
