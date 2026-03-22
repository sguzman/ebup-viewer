# Egui Testing And Parity Roadmap

## Objective
- [x] Replace the current browser/Tauri-heavy test stack with a Rust-native testing and parity strategy suitable for an egui desktop app.
- [x] Preserve explicit parity gating against current reader, PDF, TTS, starter, Calibre, and browser-tab behavior.
- [x] Define a durable post-migration quality model for a native Rust desktop product.

## Current-State Grounding In This Repo
- Current test ownership spans:
- Rust unit/integration tests in the workspace
- Vitest UI/component tests in `ui/tests/`
- Playwright browser tests in `ui/e2e/`
- Tauri smoke/soak tests in `ui/e2e-tauri/` and `ui/scripts/runTauriSoak.mjs`
- Existing parity/QA docs already provide useful baselines:
- `docs/migration-parity-acceptance-checklist.md`
- `docs/qa/native-pdf-reader-checklist.md`
- `docs/qa/native-html-epub-checklist.md`
- The current test stack assumes a browser-like runtime and a Tauri shell, both of which disappear after migration.

## Target End State Under Egui
- Testing is predominantly Rust-native:
- unit tests
- integration tests
- service/runtime tests
- selected UI harness or screenshot-based checks where feasible
- Manual QA remains explicit for fidelity-heavy areas such as PDF and rich-reader rendering.
- The egui migration keeps a parity matrix mapping current behavior to new test or QA coverage.

## Key Architectural Decisions Already Chosen
- The shipped app will not depend on TypeScript/Tauri runtime, so long-term test ownership must also move out of that ecosystem.
- Manual QA remains acceptable for difficult rendering domains where full automation is weak, but it must be documented and gated.
- Migration phases should retain old test suites until equivalent parity evidence exists.
- Full build verification remains mandatory during implementation phases, excluding AppImage/RPM/DEB packaging outputs.

## Replacement Test Pyramid
- [x] Rust unit tests
- [x] pure domain logic
- [x] parsing/conversion
- [x] reducers/transitions
- [x] sync and mapping algorithms
- [x] Rust integration tests
- [x] runtime services
- [x] persistence flows
- [x] source opening/import flows
- [x] TTS/runtime orchestration
- [x] PDF lifecycle and mapping behaviors
- [x] Native UI behavior checks where feasible
- [x] panel and command state transitions
- [x] selected rendering layout invariants
- [x] screenshot/golden comparisons for stable cases
- [x] Manual QA
- [x] PDF fidelity/sync
- [x] EPUB/HTML rich rendering
- [x] starter/library/import ergonomics
- [ ] end-to-end reading flows on real content

## Phase 1: Coverage Inventory
- [x] Inventory current test responsibilities by feature area.
- [x] For each current JS/Tauri suite, define the future Rust-native owner or manual QA replacement.
- [x] Build a parity matrix linking features to new test classes (`docs/roadmaps/egui-testing-parity-matrix.md`).
- Phase exit:
- [x] no current major test responsibility is orphaned.

## Phase 2: Rust-Native Test Architecture
- [x] Define locations and patterns for:
- [x] crate-local unit tests
- [x] workspace integration tests
- [x] service harnesses
- [x] deterministic fixture handling for reader/PDF/browser-tab inputs
- [x] Define whether any egui-specific UI harness or screenshot tooling is adopted (minimal harness; fidelity via manual QA).
- Phase exit:
- [x] implementers have a clear Rust-native testing structure for the migration.

## Phase 3: Parity Gates By Subsystem
- [x] Define explicit subsystem parity gates for:
- [x] starter/local-open/recents
- [x] Calibre
- [x] browser-tab import
- [x] text-only reader
- [x] HTML/EPUB pretty reader
- [x] PDF reader
- [x] TTS controls/runtime
- [x] persistence and safe quit
- [x] Each gate specifies automated coverage, manual QA, and build verification expectations (`docs/roadmaps/egui-testing-parity-matrix.md`).
- Phase exit:
- [x] parity is measurable across the migration, not just asserted qualitatively.

## Phase 4: Manual QA Strategy
- [x] Rewrite existing QA checklists for the egui app where needed.
- [x] Preserve representative source fixtures and document human validation scenarios.
- [x] Define when screenshot captures or log traces are required as evidence for parity signoff.
- Phase exit:
- [x] manual QA is structured enough to support cutover decisions.

## Phase 5: Build And CI Strategy
- [x] Define implementation-phase validation commands for the new app.
- [x] Define when old JS/Tauri validation still runs during dual-stack migration.
- [x] Define the final post-cutover CI/build suite once old stacks are removed.
- Phase exit:
- [x] testing/build gates are explicit for both migration and end state.

## Risks / Failure Modes
- Rich reader and PDF coverage may weaken if browser tests are removed before native replacements exist.
- Egui UI automation may be limited; without strong manual QA gates, regressions can hide.
- Fixtures can become inconsistent across old and new stacks if not centralized.
- Migration may appear “done” prematurely if parity gates are not explicit and subsystem-specific.

## Test / Parity Requirements
- [x] Maintain a live parity matrix through the migration (`docs/roadmaps/egui-testing-parity-matrix.md`).
- [x] Require subsystem signoff before deleting old tests.
- [x] Preserve representative PDF/EPUB/browser-tab fixtures for regression use.
- [x] Require full implementation-phase build verification excluding AppImage/RPM/DEB packaging outputs.

## Acceptance Criteria
- [x] Every current JS/Tauri-era testing responsibility has a Rust-native or manual QA replacement.
- [x] The egui migration has explicit automated/manual parity gates by subsystem.
- [x] Build and CI expectations are clear during dual-stack migration and after cutover.
- [x] The roadmap is sufficient to retire the browser/Tauri test stack without losing quality coverage.
