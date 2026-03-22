# Egui Migration Master Roadmap

## Objective
- [x] Replace the shipped Tauri + React/TypeScript desktop app with a native Rust `eframe` + `egui` desktop app.
- [x] Preserve full current product scope through migration:
- [x] starter/library flow
- [x] reader shell/layout
- [x] EPUB/HTML/Markdown rendering
- [x] native PDF rendering and sync
- [x] TTS runtime and playback controls
- [x] settings/stats/search
- [x] browser-tab import
- [x] Calibre integration
- [x] cache/config/bookmarks/recents
- [x] End with a pure-Rust shipped desktop application and remove Tauri, TypeScript, React, Zustand, Vite, Vitest, and Playwright from production ownership.
- [x] Keep the migration staged and parity-driven rather than performing a greenfield feature reset.

## Current-State Grounding In This Repo
- The workspace currently has three primary roots:
- `src-tauri/` owns the Tauri runtime, bridge commands, logging integration, and desktop shell entrypoint.
- `ui/` owns the React/TypeScript app, Zustand store, PDF/HTML rendering logic, and browser-driven tests.
- `crates/lanternleaf-core/` already contains reusable Rust session/domain logic and is the correct seed for further extraction.
- The current frontend/backend contract is shaped around:
- generated TS bindings from Rust (`src-tauri/src/bin/export_ts_bindings.rs`)
- a Tauri command layer (`src-tauri/src/*_commands.rs`)
- UI-facing state/event ingestion in `ui/src/store/` and `ui/src/api/tauri.ts`
- The current architecture already distinguishes canonical text ownership from presentation ownership:
- `tts_text` and sentence indices are canonical
- pretty HTML/PDF are presentation layers
- `sentence_anchor_map` and PDF sync artifacts are hint surfaces, not ownership roots
- Existing roadmap docs already define mature contracts for:
- native HTML dual-view rendering
- PDF rendering and sync quality modes
- browser-tab import
- migration parity acceptance and QA checklists
- Existing Rust runtime strengths that should be preserved:
- Piper-backed TTS and worker orchestration
- cache/config/bookmark persistence
- source ingestion and normalization
- tracing-based observability

## Target End State Under Egui
- One native Rust desktop executable owns shell, UI, state, and runtime orchestration.
- `eframe` + `egui` becomes the only UI stack for the shipped desktop app.
- The target workspace shape is:
- `crates/lanternleaf-core/` for stable document/session domain state
- additional Rust crates extracted as needed for rendering/runtime/persistence boundaries
- a new egui application crate as the only desktop app entry point
- `src-tauri/` retired after cutover
- `ui/` retired after cutover
- The end-state ownership model is:
- Rust domain/session state remains canonical
- egui widgets render native Rust view models
- async runtime services push typed events into Rust-native app state
- no TypeScript bindings, no browser bridge, no WebView rendering in the shipped app

## Key Architectural Decisions Already Chosen
- Primary UI stack is `eframe` + `egui`.
- Migration target is desktop-only native app, not a hybrid web/native split.
- Migration style is staged parity migration with explicit cutover gates.
- Existing Rust domain/TTS/cache logic is preserved and expanded instead of rewritten from scratch.
- Tauri command boundaries are transitional and should be replaced by Rust trait/module boundaries.
- PDF is a first-class migration track with its own roadmap and separate parity gates.
- Browser-tab import and Calibre stay in scope; they are not optional post-migration extras.
- Tracing remains mandatory in the target architecture for runtime, UI, rendering, and persistence telemetry.
- Build verification for implementation phases must continue to include full app build validation while excluding AppImage/RPM/DEB packaging targets.

## Feature Inventory And Future Ownership
| Current surface | Current owner | Future egui owner |
| --- | --- | --- |
| Starter shell, open flow, quick actions | React app + Tauri shell | egui app shell and command widgets |
| Reader shell/layout, top bars, side panels | `ui/src/components/ReaderShell.tsx`, `readerPanels.tsx` | egui shell crate/widgets |
| EPUB/HTML/Markdown pretty rendering | React renderers + DOM/HTML pipeline | Rust-native reader rendering/view-model pipeline |
| PDF render/sync/highlight | `ReaderPrettyPdfPane.tsx` + Tauri PDF artifacts | Rust-native PDF subsystem integrated into egui |
| TTS controls/runtime coordination | Zustand + Tauri events + Rust runtime | Rust app runtime + egui playback widgets |
| Settings, stats, search | React component tree + Zustand slices | egui panels backed by Rust state |
| Browser-tab import | Tauri command integration + React list flow | Rust-native import service + egui starter/library surfaces |
| Calibre integration | Rust backend + TS UI list rendering | Rust service + egui library browser |
| Cache/config/bookmarks/recents | mixed Rust persistence + TS presentation | pure Rust persistence and app-state orchestration |

## Migration Principles
- [x] Keep canonical text/session ownership in Rust from the start of the migration.
- [x] Extract reusable Rust crates before large UI rewrites whenever a dependency boundary is unclear.
- [x] Prefer replacing bridge protocols with in-process Rust traits and typed events rather than 1:1 command shims.
- [x] Do not delete Tauri/UI paths until parity and observability gates pass.
- [x] Keep dual-run compatibility long enough to compare Tauri and egui behavior on the same sources.
- [x] Treat PDF and browser-tab features as explicit risk domains, not hidden “later” work.
- [x] Keep state boundaries explicit: document/session/playback/UI/persistence/runtime services.
- [x] Preserve existing cache/config/bookmark data or provide deterministic migration/invalidation rules.

## Dependency Order
- [x] Extract stable Rust-side application/domain boundaries from the current Tauri/UI split.
- [x] Create the new egui desktop crate and shell/runtime skeleton.
- [x] Move app state/event/effect ownership into Rust-native services.
- [x] Rebuild shell, panels, and reader navigation in egui.
- [x] Rebuild EPUB/HTML/Markdown reader rendering.
- [x] Rebuild TTS controls/runtime flow on top of Rust-native app state.
- [x] Rebuild native PDF rendering/sync.
- [x] Rebuild starter/library/browser-tab/Calibre flows.
- [x] Replace JS/Tauri test coverage with Rust-native and manual QA equivalents.
- [x] Cut over packaging/CI/build scripts.
- [x] Remove Tauri/UI stacks after final parity gate.

## Phase 1: Architecture Extraction
- [x] Define the target Rust crate map for:
- [x] app shell/runtime
- [x] document/session state
- [x] reader presentation models
- [x] persistence/config/cache
- [x] import/library integrations
- [x] PDF rendering/sync
- [x] Extract or formalize typed Rust interfaces for all current Tauri commands consumed by the UI.
- [x] Collapse generated TS binding ownership into Rust-side DTO/view-model types.
- [x] Add architecture docs for the new in-process runtime boundaries before egui feature work begins.
- Phase exit:
- [x] implementers can build new UI/runtime work without depending on Tauri command semantics.
- [x] each current Tauri/UI integration point has a future Rust-native owner.

## Phase 2: Egui Shell Bootstrap
- [x] Add a new egui desktop crate to the workspace.
- [x] Stand up app window, panel layout, top toolbar, keyboard shortcut capture, modal strategy, and tracing bootstrap.
- [x] Wire the new shell to Rust-native mock or real domain state without feature parity yet.
- [x] Establish redraw/performance discipline and frame-budget telemetry.
- Phase exit:
- [x] the workspace can launch the egui shell independently.
- [x] shell, panel, and command plumbing no longer depends on the web stack.

## Phase 3: Reader And TTS Parity
- [x] Rebuild text-only and pretty-text reader flows for EPUB/TXT/Markdown/HTML in egui.
- [x] Restore sentence highlighting, click-to-play, jump-to-highlight, auto-scroll/center, search, stats, and settings.
- [x] Move playback/event ingestion fully into Rust-native app/runtime state.
- [x] Keep canonical sentence and playback ownership unchanged from the current Rust logic.
- Phase exit:
- [x] non-PDF reading and TTS flows reach parity with the Tauri app.
- [x] bookmark/config/session semantics remain deterministic.

## Phase 4: Native PDF Parity
- [x] Implement Rust-native PDF rendering, viewport management, text mapping, and overlay/highlight behavior in egui.
- [x] Preserve the current PDF quality-class and degraded-mode contracts.
- [x] Reach parity for jump-to-highlight, search navigation, and playback sync.
- Phase exit:
- [ ] PDF flows satisfy the same manual QA and parity acceptance gates currently used for the Tauri reader.

## Phase 5: Starter/Library/Import Parity
- [ ] Rebuild starter shell, recent books, Calibre browser, local file open flow, and browser-tab import in egui.
- [ ] Preserve deletion, reopen, cache cleanup, and metadata display semantics.
- [ ] Expose browser-tab and Calibre diagnostics in a Rust-native UI.
- Phase exit:
- [ ] the egui app can serve as the main user-facing entrypoint for all current source acquisition paths.

## Phase 6: Testing And Packaging Parity
- [ ] Replace browser-centric tests with Rust-native unit/integration/UI harness coverage where feasible.
- [ ] Add screenshot/manual QA gates where egui automation is weaker.
- [ ] Update workspace build, smoke, and release checks for the new desktop crate.
- [ ] Preserve the rule that full builds are verified excluding AppImage/RPM/DEB artifact generation during normal engineering validation.
- Phase exit:
- [ ] the egui app has documented and automated parity gates sufficient for cutover.

## Phase 7: Cutover And Tauri Removal
- [ ] Switch primary developer and user docs to the egui app.
- [ ] Remove Tauri runtime ownership from CI/build scripts.
- [ ] Remove TS/React/Tauri dependencies and delete obsolete codepaths only after parity signoff.
- [ ] Retire generated TS bindings, browser tests, and WebView-specific rendering layers.
- Phase exit:
- [ ] shipped product is pure Rust desktop app.
- [ ] no production dependency remains on `src-tauri/` or `ui/`.

## Phase Gates
- [ ] Gate A: architecture extraction complete
- [ ] Gate B: egui shell boots with Rust-native state/runtime
- [ ] Gate C: non-PDF reader + TTS parity complete
- [ ] Gate D: PDF parity complete
- [ ] Gate E: starter/library/import parity complete
- [ ] Gate F: testing/build/release parity complete
- [ ] Gate G: cutover checklist passes and Tauri removal is approved

## Rollback Points
- [ ] keep Tauri app runnable until Gate G
- [ ] preserve cache/config compatibility or versioned migration until egui app is stable
- [ ] support side-by-side validation builds during shell, reader, and PDF phases
- [ ] do not remove TypeScript/Tauri tests until equivalent parity evidence exists

## Risks / Failure Modes
- PDF parity may stall migration if renderer/sync choices are under-scoped.
- egui immediate-mode rendering may regress responsiveness if state invalidation is too broad.
- HTML/EPUB fidelity may regress if DOM-based assumptions are not replaced with a deliberate Rust render model.
- Browser-tab import may be deprioritized accidentally because it straddles service integration and UI migration.
- Existing persistence semantics may break if cache/config/bookmark migration is not treated as a first-class track.
- Team velocity may drop if extraction work is skipped and egui UI is built directly on top of current Tauri command shapes.

## Test / Parity Requirements
- [ ] Maintain a parity matrix linking each current feature area to an egui replacement owner and acceptance check.
- [ ] Add migration-specific comparison runs on representative EPUB, PDF, and browser-tab sources.
- [ ] Keep build verification green for Rust workspace and frontend until the old stack is retired.
- [ ] Require full implementation-phase build validation after changes, excluding AppImage/RPM/DEB packaging outputs.
- [ ] Track manual QA for PDF, HTML/EPUB, starter/library, TTS, and persistence separately.

## Acceptance Criteria
- [ ] A new egui desktop crate exists and is the planned final app entrypoint.
- [ ] Every current user-facing capability in README and existing roadmap docs has a mapped egui owner and migration phase.
- [ ] No major architectural decision needed for implementation remains open in this master plan.
- [ ] The dependency order, gates, and rollback points are explicit enough to guide parallel execution.
- [ ] Final cutover criteria are clear enough to remove Tauri and TypeScript without reopening strategy decisions.
