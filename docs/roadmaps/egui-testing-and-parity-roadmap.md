# Egui Testing And Parity Roadmap

## Objective
- [ ] Replace the current browser/Tauri-heavy test stack with a Rust-native testing and parity strategy suitable for an egui desktop app.
- [ ] Preserve explicit parity gating against current reader, PDF, TTS, starter, Calibre, and browser-tab behavior.
- [ ] Define a durable post-migration quality model for a native Rust desktop product.

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
- [ ] Rust unit tests
- [ ] pure domain logic
- [ ] parsing/conversion
- [ ] reducers/transitions
- [ ] sync and mapping algorithms
- [ ] Rust integration tests
- [ ] runtime services
- [ ] persistence flows
- [ ] source opening/import flows
- [ ] TTS/runtime orchestration
- [ ] PDF lifecycle and mapping behaviors
- [ ] Native UI behavior checks where feasible
- [ ] panel and command state transitions
- [ ] selected rendering layout invariants
- [ ] screenshot/golden comparisons for stable cases
- [ ] Manual QA
- [ ] PDF fidelity/sync
- [ ] EPUB/HTML rich rendering
- [ ] starter/library/import ergonomics
- [ ] end-to-end reading flows on real content

## Phase 1: Coverage Inventory
- [ ] Inventory current test responsibilities by feature area.
- [ ] For each current JS/Tauri suite, define the future Rust-native owner or manual QA replacement.
- [ ] Build a parity matrix linking features to new test classes.
- Phase exit:
- [ ] no current major test responsibility is orphaned.

## Phase 2: Rust-Native Test Architecture
- [ ] Define locations and patterns for:
- [ ] crate-local unit tests
- [ ] workspace integration tests
- [ ] service harnesses
- [ ] deterministic fixture handling for reader/PDF/browser-tab inputs
- [ ] Define whether any egui-specific UI harness or screenshot tooling is adopted.
- Phase exit:
- [ ] implementers have a clear Rust-native testing structure for the migration.

## Phase 3: Parity Gates By Subsystem
- [ ] Define explicit subsystem parity gates for:
- [ ] starter/local-open/recents
- [ ] Calibre
- [ ] browser-tab import
- [ ] text-only reader
- [ ] HTML/EPUB pretty reader
- [ ] PDF reader
- [ ] TTS controls/runtime
- [ ] persistence and safe quit
- [ ] Each gate must specify automated coverage, manual QA, and build verification expectations.
- Phase exit:
- [ ] parity is measurable across the migration, not just asserted qualitatively.

## Phase 4: Manual QA Strategy
- [ ] Rewrite existing QA checklists for the egui app where needed.
- [ ] Preserve representative source fixtures and document human validation scenarios.
- [ ] Define when screenshot captures or log traces are required as evidence for parity signoff.
- Phase exit:
- [ ] manual QA is structured enough to support cutover decisions.

## Phase 5: Build And CI Strategy
- [ ] Define implementation-phase validation commands for the new app.
- [ ] Define when old JS/Tauri validation still runs during dual-stack migration.
- [ ] Define the final post-cutover CI/build suite once old stacks are removed.
- Phase exit:
- [ ] testing/build gates are explicit for both migration and end state.

## Risks / Failure Modes
- Rich reader and PDF coverage may weaken if browser tests are removed before native replacements exist.
- Egui UI automation may be limited; without strong manual QA gates, regressions can hide.
- Fixtures can become inconsistent across old and new stacks if not centralized.
- Migration may appear “done” prematurely if parity gates are not explicit and subsystem-specific.

## Test / Parity Requirements
- [ ] Maintain a live parity matrix through the migration.
- [ ] Require subsystem signoff before deleting old tests.
- [ ] Preserve representative PDF/EPUB/browser-tab fixtures for regression use.
- [ ] Require full implementation-phase build verification excluding AppImage/RPM/DEB packaging outputs.

## Acceptance Criteria
- [ ] Every current JS/Tauri-era testing responsibility has a Rust-native or manual QA replacement.
- [ ] The egui migration has explicit automated/manual parity gates by subsystem.
- [ ] Build and CI expectations are clear during dual-stack migration and after cutover.
- [ ] The roadmap is sufficient to retire the browser/Tauri test stack without losing quality coverage.
