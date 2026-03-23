**Status**

This roadmap documents the completed Tauri/React migration effort. The repository now ships a Rust-only egui app, and the Tauri/TypeScript stack has been removed. Treat the items below as historical context rather than active work.

**Voltlane Findings To Reuse**

- [x] Reuse Voltlane’s split architecture pattern: Rust core crate + Tauri bridge + React/TS UI (`tmp/voltlane/Cargo.toml`, `tmp/voltlane/src-tauri/src/lib.rs`, `tmp/voltlane/ui/src/App.tsx`).
- [x] Reuse Voltlane’s typed command boundary pattern: one Rust `#[tauri::command]` per operation and a TS wrapper layer (`tmp/voltlane/src-tauri/src/lib.rs`, `tmp/voltlane/ui/src/api/tauri.ts`).
- [x] Reuse Voltlane’s single shared app state in backend guarded by `Mutex` (`tmp/voltlane/src-tauri/src/lib.rs`).
- [x] Reuse Voltlane’s state orchestration model in UI with Zustand (`tmp/voltlane/ui/src/store/projectStore.ts`).
- [x] Reuse Voltlane’s Tauri build orchestration: Vite dev server + packaged frontend dist (`tmp/voltlane/src-tauri/tauri.conf.json`).
- [x] Reuse Voltlane’s logging/bootstrap strategy in backend setup (`tmp/voltlane/src-tauri/src/lib.rs`).
- [x] Improve on Voltlane by adding generated API typings (optional `tauri-specta`) to avoid manual Rust/TS drift.
- [x] Improve on Voltlane by adding Tailwind + Material UI integration rules up front (Voltlane currently uses plain CSS only).

**Target Architecture For LanternLeaf**

- [x] Target stack: Tauri 2 shell + Rust core domain crate + React/TypeScript frontend.
- [x] Keep Rust responsible for domain/data/IO/TTS/PDF pipeline.
- [x] Keep frontend responsible for rendering, layout, input handling, DOM-based scrolling/highlight positioning.
- [x] Use Material UI for component primitives and interactions.
- [x] Use Tailwind for layout and spacing utilities.
- [x] Use a single UI state store (Zustand) that calls typed backend commands.
- [x] Preserve all existing behavior before introducing UX changes.
- [x] Preserve Piper-based TTS as a non-negotiable final-product capability (must not be removed during GUI decommission work).

**Roadmap**

**Phase 0: Baseline And Contract Freeze**

- [x] P0-01 Create a full feature inventory from current iced app flows (starter, reader, TTS, calibre, PDF, settings, stats, search, shortcuts, safe quit).
- [x] P0-02 Capture baseline behavior docs for all known “must not regress” items (highlight alignment, non-vertical button policy, pause semantics, close-session behavior).
- [x] P0-03 Capture baseline performance metrics: startup time, page switch latency, TTS start latency, window resize responsiveness.
- [x] P0-04 Capture baseline logs for key scenarios: EPUB open, PDF open, TTS batch generate, close while generating, resume from bookmark.
- [x] P0-05 Define parity acceptance checklist with explicit pass/fail criteria for each feature.
- [x] P0-06 Freeze feature additions during migration except migration-blocking fixes.

**Phase 1: Repository Restructure**

- [x] P1-01 Convert project to a workspace shape similar to Voltlane while preserving existing Rust modules.
- [x] P1-02 Add `src-tauri` crate for desktop shell/command bridge.
- [x] P1-03 Add `ui` app with Vite + React + TypeScript.
- [x] P1-04 Keep current iced entrypoint temporarily as fallback build target until parity gate passes.
- [x] P1-05 Add root scripts for `ui:dev`, `ui:build`, `tauri:dev`, `tauri:build`.
- [x] P1-06 Define CI matrix for Rust core checks, Tauri compile, frontend build, lint, tests.
- [x] P1-07 Add migration branch policy and rollback plan.

**Phase 2: Rust Core Extraction (Headless Domain)**

- [x] P2-01 Extract UI-agnostic modules into a dedicated core crate (config/cache/loader/normalizer/pagination/calibre/quack_check/tts orchestration). (`crates/lanternleaf-core` now owns these modules and Tauri consumes the crate instead of `#[path]` imports.)
- [x] P2-02 Remove `iced` types from domain layer (`RelativeOffset`, UI font/color concerns) and replace with neutral DTOs. (Extracted core modules are iced-free; UI-only iced types remain isolated to legacy iced codepath.)
- [x] P2-03 Introduce session-centric core API (`SessionState`, `SessionCommand`, `SessionEvent`). (`lanternleaf_core::session` now exposes command/event dispatch used by Tauri reader commands.)
- [x] P2-04 Isolate async jobs behind explicit handles and cancellation tokens (TTS prep, calibre load, PDF extraction). (Source-open, calibre load, quack-check PDF extraction, and TTS playback/prep now run behind explicit request IDs + cancellation tokens in `src-tauri/src/lib.rs`, `src/calibre.rs`, `src/epub_loader.rs`, and `src/quack_check/*`.)
- [x] P2-05 Preserve deterministic state transitions currently implemented in reducer/effects. (Reader command handlers now dispatch through one deterministic core command path.)
- [x] P2-06 Add unit tests for extracted command/state transitions before connecting frontend. (Core session command-dispatch tests added under `crates/lanternleaf-core/src/session.rs`.)

**Phase 3: Backend Bridge (Tauri)**

- [x] P3-01 Implement backend `AppState` with mutexed core session manager (Voltlane-style).
- [x] P3-02 Define command groups: session, source-open, navigation, appearance/settings, search, TTS, calibre, PDF, diagnostics.
- [x] P3-03 Implement one Tauri command per operation; return typed DTOs only.
- [x] P3-04 Add backend event emitters for long-running progress and state changes (TTS planning/prep, calibre load, PDF transcription).
- [x] P3-05 Add command-level error taxonomy (user-safe errors vs internal errors).
- [x] P3-06 Add operation-guard rules (single active book load; no duplicate PDF processing; reject conflicting requests).
- [x] P3-07 Add shutdown hooks to cancel all in-flight tasks on close/return-to-starter.
- [x] P3-08 Add structured tracing in bridge and correlate with session/request IDs.

**Phase 4: Command Contract And Types**

- [x] P4-01 Define canonical Rust DTO schema for frontend consumption.
- [x] P4-02 Generate or mirror TS types from Rust DTOs.
- [x] P4-03 Create stable command naming and versioning convention.
- [x] P4-04 Add contract tests that validate serialization/deserialization across bridge.
- [x] P4-05 Add compatibility policy for future command evolution.

**Phase 5: Frontend Foundation (React + TS + MUI + Tailwind)**

- [x] P5-01 Initialize strict TS config and lint/format tooling.
- [x] P5-02 Install Material UI and theme infrastructure.
- [x] P5-03 Install Tailwind and PostCSS pipeline.
- [x] P5-04 Decide style precedence policy: MUI theme + Tailwind utilities without conflict.
- [x] P5-05 Configure `CssBaseline` and decide on Tailwind preflight strategy to avoid reset collisions.
- [x] P5-06 Map existing day/night theme values to MUI theme tokens + CSS variables.
- [x] P5-07 Build minimal app shell layout with responsive split panes.
- [x] P5-08 Add global error boundary and command failure toast system.

**Phase 6: UI Data Layer**

- [x] P6-01 Implement `ui/src/api/tauri.ts` style typed wrappers for all backend commands.
- [x] P6-02 Implement runtime adapter: real Tauri invoke + optional mock adapter for browser-only UI dev.
- [x] P6-03 Build Zustand store slices: session, reader, tts, calibre, settings, stats, jobs, notifications.
- [x] P6-04 Centralize optimistic update policy and rollback logic.
- [x] P6-05 Implement event subscription handlers to update store from backend progress/events.
- [x] P6-06 Add telemetry fields in store actions for reproducible debugging.

**Phase 7: Starter Screen Port**

- [x] P7-01 Port welcome/open path/open clipboard controls.
- [x] P7-02 Port recent list embedded under welcome section (2-column behavior).
- [x] P7-03 Port recent delete action with source+cache deletion semantics.
- [x] P7-04 Port calibre visibility toggle, refresh, search, sorting, open behavior.
- [x] P7-05 Implement list virtualization for large calibre catalogs.
- [x] P7-06 Preserve “book loading lock” behavior that prevents concurrent opens.
- [x] P7-07 Preserve starter-level error/status reporting messages.

**Phase 8: Reader View Port**

- [x] P8-01 Port text rendering modes (pretty/text-only) and sentence click interactions.
- [x] P8-02 Port top bar policies (no vertical compression, hide when too tight).
- [x] P8-03 Port settings panel and stats panel mutual exclusivity.
- [x] P8-04 Port numeric slider/textbox hybrid editing with validation + wheel adjust.
- [x] P8-05 Port search bar and regex navigation behavior.
- [x] P8-06 Port keyboard shortcuts and shortcut configurability.
- [x] P8-07 Port close session behavior back to starter with save housekeeping.

**Phase 9: TTS Control And Highlight Fidelity**

- [x] P9-01 Port all playback commands: play/pause/toggle/play-from-page/play-from-highlight/seek/repeat.
- [x] P9-02 Preserve pause-after-sentence semantics and speed/volume behavior.
- [x] P9-03 Preserve clicked sentence start logic with correct audio/display mapping.
- [x] P9-04 Keep mapping logic in Rust; move visual positioning to DOM measurements in UI.
- [x] P9-05 Replace heuristic scroll math with actual element anchoring where feasible.
- [x] P9-06 Port auto-scroll and auto-center toggles with exact visibility guarantees.
- [x] P9-07 Preserve cancellation semantics for close/quit during preparation.
- [x] P9-08 Emit and render 3-decimal TTS progress consistently.

**Phase 10: EPUB/PDF/Clipboard Ingestion Paths**

- [x] P10-01 Port EPUB open and image extraction flow unchanged.
- [x] P10-02 Port PDF flow with quack-check pipeline invocation and cache signature behavior.
- [x] P10-03 Port clipboard-source persistence flow to cached `.txt` source and normal open path.
- [x] P10-04 Port normalization pipeline and sentence chunking behavior exactly.
- [x] P10-05 Port source cache, normalized cache, bookmark cache compatibility.
- [x] P10-06 Ensure per-source config override loading parity.

**Phase 11: Persistence, Config, And Runtime**

- [x] P11-01 Preserve config schema and defaults (`conf/config.toml`, `conf/normalizer.toml`, `conf/calibre.toml`).
- [x] P11-02 Preserve bookmark save/load semantics and resume fidelity.
- [x] P11-03 Preserve recent-book indexing and thumbnail handling.
- [x] P11-04 Preserve safe quit behavior including Ctrl+C semantics.
- [x] P11-05 Preserve logging configuration and dynamic level updates.
- [x] P11-06 Add Tauri capability permissions for file access, logging, and subprocess usage required by quack-check.

**Phase 12: Tailwind + MUI Production Hardening**

- [x] P12-01 Define component usage policy: MUI for controls/dialogs/sliders/tables, Tailwind for layout containers.
- [x] P12-02 Build reusable design tokens that map your current app settings into MUI theme and Tailwind classes.
- [x] P12-03 Validate dark/day mode parity against existing visuals.
- [x] P12-04 Ensure typography/rendering remains stable at current default font size and spacing settings.
- [x] P12-05 Validate responsive breakpoints to preserve no-vertical-collapse policies.
- [x] P12-06 Audit final CSS bundle size and remove dead styles.

**Phase 13: Testing And Parity Gates**

- [x] P13-01 Keep and run Rust unit/integration tests for core logic at every phase.
- [x] P13-02 Add bridge command tests for all critical command paths.
- [x] P13-03 Add frontend unit tests for reducers/store actions and command adapters.
- [x] P13-04 Add E2E scenarios (Playwright + Tauri runner) for core reading/TTS flows. (Playwright browser scenarios plus dedicated Tauri-runner smoke wiring added in CI, now including starter recent-delete + clipboard-open runtime coverage.)
- [x] P13-05 Add explicit regression scenarios from your prior incidents (sentence click halt, highlight drift, duplicate PDF jobs, close-during-tts).
- [x] P13-06 Add large calibre dataset performance scenario and verify non-blocking UX.
- [x] P13-07 Add PDF edge corpus tests with degraded pages and fallback paths.
- [x] P13-08 Create migration parity report that compares iced vs new shell outputs for key workflows.

**Phase 14: Cutover And Decommission**

- [x] P14-01 Run dual-track period where iced build remains available for fallback. (Both root iced path and Tauri path are continuously validated in CI.)
- [x] P14-02 Complete parity signoff checklist with explicit pass on all must-have behaviors. (Completed with full checklist pass plus 3-iteration Tauri runtime soak and machine-readable soak report output.)
- [x] P14-03 Switch default desktop target to Tauri app. (Root `pnpm dev`/`pnpm build` target Tauri; legacy iced fallback script removed after parity/soak completion.)
- [x] P14-04 Remove iced UI modules only after parity and soak tests pass. (`src/app/*` iced UI tree removed; root binary now decommissioned for GUI use while retaining TTS worker mode; Tauri shell is the only desktop UI path.)
  Constraint: This applies only to iced UI framework code. Piper/TTS domain/runtime functionality must remain intact.
- [x] P14-05 Keep core interfaces stable for future GUI changes. (Bridge command names are now single-source via macro list + stability tests; core reader operations dispatch via `SessionCommand` API.)

**Critical Risks To Track (and Mitigate)**

- [x] R-01 Large text rendering performance in WebView with per-sentence spans. (Mitigated with virtualization and repeated Tauri runtime smoke/soak validation under real reader flows.)
- [x] R-02 Highlight/scroll mismatch from mixed Rust vs DOM coordinate systems. (Mitigated with DOM-anchored reader logic plus Tauri-runner highlight-visibility checks under settings changes.)
- [x] R-03 Long-running TTS/PDF tasks outliving session context. (Mitigated with explicit cancellation tokens/request IDs for TTS, source-open, PDF, and calibre jobs; verified by runtime smoke + soak.)
- [x] R-04 Type drift between Rust DTOs and TS interfaces.
- [x] R-05 Styling conflicts between MUI and Tailwind resets/utilities.
- [x] R-06 Tauri permission/capability restrictions breaking filesystem/subprocess workflows. (Mitigated by capability setup plus root-aware config path resolution for pandoc/quack-check/calibre under Tauri manifest contexts, with cache-root override support and passing runtime smoke.)
- [x] R-07 Calibre table scale issues without virtualization.
- [x] R-08 Behavior drift in config/bookmark compatibility. (Mitigated with cache bookmark/config roundtrip tests, shutdown housekeeping persistence tests, and config persistence helper tests in Tauri bridge; parity signoff complete.)

**Definition Of Done**

- [x] DOD-01 All current user-facing features from iced are present and parity-tested.
- [x] DOD-02 All known regressions you previously flagged have explicit passing tests.
- [x] DOD-03 Close-session cancels in-flight background work reliably.
- [x] DOD-04 PDF, EPUB, and clipboard sources are all first-class and stable.
- [x] DOD-04a Piper TTS remains fully functional in final shipped product (play/pause/seek/repeat/highlight sync).
- [x] DOD-05 Tailwind + MUI coexist without style regressions.
- [x] DOD-06 Performance targets (startup, page changes, TTS response, resize) meet or beat baseline.
- [x] DOD-07 Iced UI codepath can be retired safely.
