# Egui Testing Parity Matrix

## Purpose
- Provide a single inventory of current JS/Tauri-era tests and the Rust-native or manual QA owners that replace them.
- Define the Rust-native test architecture and fixture strategy for the egui migration.
- Make subsystem parity gates explicit so the old test stack can be retired safely.

## Current Test Inventory -> Future Owner Mapping

| Current suite | Current location | Feature area | Target owner (egui) | Target test class | Notes |
| --- | --- | --- | --- | --- | --- |
| Vitest: appStore | `ui/tests/appStore.test.ts` | App state + reducers | `lanternleaf-app` | Rust unit tests | Map store reducers to `AppState` transitions. |
| Vitest: storeSelectors | `ui/tests/storeSelectors.test.ts` | Selector logic | `lanternleaf-app` | Rust unit tests | Keep deterministic selector fixtures in Rust. |
| Vitest: appBrowserTabTransition | `ui/tests/appBrowserTabTransition.test.tsx` | Browser-tab flows | `lanternleaf-app` + `lanternleaf-egui` | Rust integration tests | Exercise command pipeline + panel state. |
| Vitest: starterBrowserTabs | `ui/tests/starterBrowserTabs.test.tsx` | Starter + tabs | `lanternleaf-egui` | UI harness + manual QA | Keep manual QA gate for UX parity. |
| Vitest: tmpBrowserTabLoop | `ui/tests/tmpBrowserTabLoop.test.tsx` | Tab loop regression | `lanternleaf-app` | Rust integration tests | Regression fixture for open/close loops. |
| Vitest: tauriApi | `ui/tests/tauriApi.test.ts` | Tauri bridge | `lanternleaf-app` | Rust unit tests | Replace with runtime bridge tests. |
| Vitest: calibreList | `ui/tests/calibreList.test.ts` | Calibre import | `lanternleaf-app` | Rust integration tests | Use fixture library indexes. |
| Vitest: readerHtmlSync | `ui/tests/readerHtmlSync.test.ts` | Reader sync | `lanternleaf-core` | Rust unit tests | Anchor/offset mapping logic only. |
| Vitest: htmlSync | `ui/tests/htmlSync.test.ts` | HTML sync | `lanternleaf-core` | Rust unit tests | Pure mapping and layout math. |
| Vitest: nativeHtmlSentenceAnchors | `ui/tests/nativeHtmlSentenceAnchors.test.ts` | Sentence anchors | `lanternleaf-core` | Rust unit tests | Anchor parsing + matching rules. |
| Vitest: prettyHtml | `ui/tests/prettyHtml.test.ts` | Pretty reader | `lanternleaf-egui` + QA | Manual QA + UI harness | Visual fidelity and style checks. |
| Vitest: markdownRender | `ui/tests/markdownRender.test.ts` | Markdown render | `lanternleaf-core` | Rust unit tests | Parsing/render plan snapshot tests. |
| Vitest: readerTypography | `ui/tests/readerTypography.test.ts` | Typography settings | `lanternleaf-app` | Rust unit tests | Settings normalization + layout policies. |
| Vitest: layoutPolicies | `ui/tests/layoutPolicies.test.ts` | Layout policies | `lanternleaf-core` | Rust unit tests | Pure layout decisions. |
| Vitest: renderIsolation | `ui/tests/renderIsolation.test.tsx` | Render isolation | `lanternleaf-egui` | UI harness | Coalescing / redraw budget checks. |
| Vitest: themeMapping | `ui/tests/themeMapping.test.ts` | Theme mapping | `lanternleaf-app` | Rust unit tests | Settings to theme mapping. |
| Vitest: readerTtsPlayerIntegration | `ui/tests/readerTtsPlayerIntegration.test.tsx` | Reader + TTS | `lanternleaf-app` + `lanternleaf-egui` | Rust integration tests | Command/effect chain + highlight sync. |
| Vitest: ttsPlayerWidget | `ui/tests/ttsPlayerWidget.test.tsx` | TTS widget | `lanternleaf-egui` | UI harness | Widget states + shortcuts. |
| Vitest: pdfDocumentModel | `ui/tests/pdfDocumentModel.test.ts` | PDF model | `lanternleaf-app` | Rust unit tests | PDF model, metadata mapping. |
| Vitest: pdfArtifactCache | `ui/tests/pdfArtifactCache.test.ts` | PDF cache | `lanternleaf-app` | Rust integration tests | Cache lifecycle + invalidation. |
| Vitest: pdfTextSync | `ui/tests/pdfTextSync.test.ts` | PDF text sync | `lanternleaf-app` | Rust integration tests | OCR + text sync mapping. |
| Vitest: pdfTextLayer | `ui/tests/pdfTextLayer.test.ts` | PDF text layer | `lanternleaf-app` | Rust unit tests | Text layer geometry math. |
| Vitest: pdfOverlayGeometry | `ui/tests/pdfOverlayGeometry.test.ts` | PDF overlay math | `lanternleaf-app` | Rust unit tests | Overlay geometry + selection math. |
| Vitest: pdfOverlayNavigation | `ui/tests/pdfOverlayNavigation.test.ts` | PDF overlay nav | `lanternleaf-app` | Rust integration tests | Overlay -> viewport mapping. |
| Vitest: pdfOverlayDom | `ui/tests/pdfOverlayDom.test.ts` | PDF overlay DOM | Manual QA | Manual QA | Visual fidelity + interaction. |
| Vitest: pdfHighlightDom | `ui/tests/pdfHighlightDom.test.ts` | PDF highlight DOM | Manual QA | Manual QA | Visual highlight fidelity. |
| Vitest: pdfHighlightController | `ui/tests/pdfHighlightController.test.ts` | PDF highlight control | `lanternleaf-app` | Rust integration tests | Highlight model + commands. |
| Vitest: pdfViewportScheduler | `ui/tests/pdfViewportScheduler.test.ts` | PDF viewport scheduling | `lanternleaf-app` | Rust unit tests | Scheduler budget checks. |
| Vitest: pdfPerformanceProfile | `ui/tests/pdfPerformanceProfile.test.ts` | PDF perf budgets | `lanternleaf-app` | Rust integration tests | Performance traces + budget validation. |
| Vitest: pdfPerformanceScenario | `ui/tests/pdfPerformanceScenario.test.ts` | PDF perf scenario | Manual QA | Manual QA | Pair with perf baseline doc. |
| Playwright: readerFlow | `ui/e2e/readerFlow.spec.ts` | End-to-end reader | Manual QA | Manual QA | E2E reading flow on real content. |
| Playwright: perfBaseline | `ui/e2e/perfBaseline.spec.ts` | Performance baseline | Manual QA + tracing | Manual QA | Requires trace capture. |
| Playwright: playbackProfile | `ui/e2e/playbackProfile.spec.ts` | TTS playback profile | `lanternleaf-app` + QA | Rust integration + manual QA | Use TTS trace replay. |
| Tauri smoke | `ui/e2e-tauri/smoke.test.mjs` | Shell + startup | `lanternleaf-egui` | Manual QA + harness | Replace with native smoke script. |
| Tauri soak | `ui/scripts/runTauriSoak.mjs` | Long-running soak | `lanternleaf-egui` | Manual QA | Egui soak script requirement. |

## Rust-Native Test Architecture

### Test placement rules
- Unit tests live next to the logic they cover (crate-local `mod tests`).
- Integration tests live in `crates/lanternleaf-app/tests/` or per-crate `tests/` directories.
- Runtime/service harness tests should exercise `AppState`, `Runtime`, and `Pipeline` at the command/effect boundary.
- UI harness tests should be limited to state transitions and measurable invariants; full visual fidelity remains manual QA.

### Shared fixture strategy
- Centralize fixtures under `tests/fixtures/` at the repo root (future migration from `ui/tests/fixtures`).
- Prefer `include_bytes!`/`include_str!` for stable fixtures; use temp dirs for mutation.
- Record fixture provenance in the parity matrix to avoid drift.

### Deterministic timing and async control
- All async integration tests should run under a deterministic clock or explicit timeouts.
- Where timing is performance-sensitive (PDF, TTS), capture traces and assert budgets.

## Parity Gates By Subsystem

| Subsystem | Automated coverage (Rust) | Manual QA | Build verification |
| --- | --- | --- | --- |
| Starter / local open / recents | Integration tests for open + recents lifecycle | Manual flow in `docs/migration-parity-acceptance-checklist.md` | `cargo build --workspace` + `pnpm ui:build` |
| Calibre import | Integration tests using fixture library indexes | Manual import sanity pass | Same as above |
| Browser-tab import | Integration tests for URL ingestion + cache | Manual import UX review | Same as above |
| Text-only reader | Unit tests for anchor mapping + reader state | Manual reading flow | Same as above |
| HTML/EPUB pretty reader | Unit tests for parsing + mapping | `docs/qa/native-html-epub-checklist.md` | Same as above |
| PDF reader | Unit + integration for overlays, sync, caching | `docs/qa/native-pdf-reader-checklist.md` + PDF QA docs | Same as above |
| TTS controls/runtime | Unit + integration tests for playback + highlight sync | Playback manual QA + trace replay | Same as above |
| Persistence + safe quit | Integration tests for cache/recents/bookmarks | Manual corruption + reopen flow | Same as above |

## Build / CI Phases

### Dual-stack migration
- Keep JS/Tauri test suites until parity gates for the subsystem are signed off.
- Rust-native tests must run on every implementation-phase change.

### Post-cutover
- Remove JS/Tauri test suites.
- CI runs Rust unit + integration tests plus build verification commands.

## Manual QA Evidence Requirements
- PDF and pretty reader parity require manual QA checklist completion.
- Performance-related QA must include a trace capture with timing budgets.
- TTS playback QA requires a trace of sentence advance and highlight sync.

