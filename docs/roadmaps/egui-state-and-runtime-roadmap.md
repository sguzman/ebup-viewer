# Egui State And Runtime Roadmap

## Objective
- [ ] Replace Zustand slices, frontend event ingestion, and Tauri bridge commands with Rust-native application state and runtime orchestration.
- [ ] Preserve the existing domain ownership model while moving command, effect, and event flow entirely into Rust.
- [ ] Establish a stable Rust-native state/runtime architecture that all egui UI surfaces can consume directly.

## Current-State Grounding In This Repo
- Current UI state ownership is spread across:
- `ui/src/store/appStore.ts`
- `ui/src/store/slices/*`
- `ui/src/store/selectors.ts`
- `ui/src/store/slices/eventIngestion.ts`
- The frontend currently consumes runtime behavior through:
- `ui/src/api/tauri.ts`
- generated TS types from Rust bindings
- Tauri command invocations and Tauri event listeners
- Existing architectural docs already point toward the correct domain split:
- document state
- playback state
- UI-local panel state
- session/runtime state
- Rust already owns meaningful runtime logic in:
- `crates/lanternleaf-core/`
- `src-tauri/src/tts_runtime.rs`
- `src-tauri/src/*_commands.rs`
- ingestion, config, cache, and source loading code under `src/`

## Target End State Under Egui
- A Rust-native app runtime owns:
- top-level application state
- reader session state
- playback state
- background task orchestration
- notifications/diagnostics
- persistence coordination
- All UI widgets read from and dispatch into Rust-native types with no JSON bridge boundary.
- External service integrations become internal Rust services rather than command endpoints.

## Key Architectural Decisions Already Chosen
- State stays in Rust, not in any embedded scripting layer.
- Tauri commands are replaced by Rust module/trait boundaries.
- The desired state split is:
- `app_state`
- `session_state`
- `reader_document`
- `reader_playback`
- `reader_ui`
- `runtime_jobs`
- Async work flows through typed commands/effects/events rather than direct widget-side blocking logic.
- Tracing remains first-class for runtime events, task spans, state transitions, and error paths.

## Target State Architecture
- [ ] `AppState`
- [ ] startup mode
- [ ] global shell state
- [ ] notifications
- [ ] app config snapshot
- [ ] service handles
- [ ] `SessionState`
- [ ] current source metadata
- [ ] session lifecycle
- [ ] reader-mode availability
- [ ] persistence status
- [ ] `ReaderDocumentState`
- [ ] canonical page/document payload
- [ ] pretty payload metadata
- [ ] images/assets
- [ ] sentence anchors / PDF sync metadata
- [ ] `ReaderPlaybackState`
- [ ] highlighted sentence
- [ ] TTS state/progress
- [ ] cursor movement
- [ ] playback diagnostics
- [ ] `ReaderUiState`
- [ ] text-only vs pretty mode
- [ ] panel toggles
- [ ] search query and selection
- [ ] transient local controls
- [ ] `RuntimeJobState`
- [ ] background tasks
- [ ] import/transcription progress
- [ ] Calibre loading
- [ ] browser-tab health
- [ ] PDF/OCR work

## Phase 1: Rust Interface Inventory
- [ ] Inventory every command/event crossing the current Tauri boundary.
- [ ] Group them into future Rust service traits/modules:
- [ ] source opening
- [ ] reader commands
- [ ] TTS runtime control
- [ ] browser-tab integration
- [ ] Calibre integration
- [ ] config/cache persistence
- [ ] PDF artifact loading and rebuild
- [ ] Define typed request/response/event models in Rust.
- Phase exit:
- [ ] all current bridge interactions have future in-process Rust owners.

## Phase 2: State Model Extraction
- [ ] Introduce explicit Rust-native state structs that mirror the intended domain split.
- [ ] Remove dependence on “whole snapshot replacement” semantics inherited from frontend ingestion.
- [ ] Document identity/update rules to prevent unnecessary egui redraws:
- [ ] document changes should not invalidate playback-only widgets
- [ ] panel toggles should not invalidate heavy reader content
- [ ] runtime progress updates should be coalesced when safe
- Phase exit:
- [ ] egui implementers can render against stable Rust state without guessing update ownership.

## Phase 3: Command / Effect / Event Pipeline
- [ ] Define typed commands emitted by widgets.
- [ ] Define effect execution ownership in Rust runtime services.
- [ ] Define event ingestion back into app state after background work completes.
- [ ] Preserve explicit transition semantics for:
- [ ] source open/close
- [ ] playback actions
- [ ] search navigation
- [ ] persistence flush
- [ ] import/transcription jobs
- Phase exit:
- [ ] runtime orchestration is explicit and detached from Tauri invoke/listen patterns.

## Phase 4: Async Task And Channel Strategy
- [ ] Define task runtime approach for background work compatible with egui:
- [ ] worker threads
- [ ] channels/message queues
- [ ] cancellation semantics
- [ ] progress event batching
- [ ] Ensure UI thread does not block on:
- [ ] TTS work
- [ ] source ingestion
- [ ] PDF extraction/transcription/OCR
- [ ] browser-tab imports
- [ ] Calibre loading
- Phase exit:
- [ ] all long-running workflows have a native Rust async/task strategy.

## Phase 5: Persistence And Runtime Integration
- [ ] Move config/bookmark/cache writes behind Rust service interfaces.
- [ ] Define save/load lifecycle during:
- [ ] source open
- [ ] session close
- [ ] application quit
- [ ] periodic bookmark/config persistence
- [ ] Preserve current safe-quit guarantees and runtime housekeeping.
- Phase exit:
- [ ] state changes and persistence responsibilities are fully native and deterministic.

## Phase 6: Logging And Tracing Strategy
- [x] Port current Tauri logging/tracing bootstrap to the egui app crate and capture the existing `tracing` config, level filters, and field set.
- [x] Define an instrumentation plan that records transitions for every major state slice, command dispatch path, runtime effect, and service invocation.
- [ ] Keep logs structured enough for migration-side parity debugging and eventual telemetry ingestion.

### Current Tracing Footing
- The Tauri app currently initializes `tracing` via the Rust command runtime and mirrors native logs through Tauri’s `tauri::Builder::plugin(TracingPlugin)` entry points.
- Helper macros in `src-tauri/src/*_commands.rs` and `crates/lanternleaf-core/` emit spans that reference Tauri command names, but the UI is only passively observing them.
- There is no centralized tracing policy for state transitions or UI-facing events yet.

### Target Instrumentation Model
- **Bootstrap continuity**: reuse the existing event level configuration so early startup and config loading remain readable in the new egui entry point.
- **State transitions**: emit spans when `AppState`, `SessionState`, `ReaderDocumentState`, `ReaderPlaybackState`, `ReaderUiState`, or `RuntimeJobState` mutates. Include prior state versions when debuggable and avoid tracing noise by coalescing frequent non-semantic updates.
- **Command dispatch**: attach spans/fields for each typed command emitted by widgets (open source, playback action, search navigation, persistence flush, import job). Ensure subsequent effect execution spans carry the originating command trace.
- **Runtime effects**: trace the lifecycle of async effects (source ingestion, TTS playback, PDF extraction, browser-tab import, Calibre sync). Include service-level metadata so operations can be correlated to persisted artifacts (file IDs, session IDs, job IDs).
- **Redraw/perf hotspots**: instrument reader redraw slices, long reader layouts, and heavy list rendering so regressions in the egui frame rate are visible.
- **Service calls**: log the major domain services (config persistence, cache writes, bookmark updates, recent book management, tracing-specific stats) with structured outcome/result fields.

### Phase Workflow
1. **Bootstrap audit**: catalog current tracing init and ensure the egui app crate replicates `tracing_subscriber` setup. Gate this with the restored `tracing` profile (DEV/RELEASE) to guarantee comparability.
2. **Instrumentation targets**: implement spans for each state slice, command, effect, and service listed above. Explicitly document the spans/fields for implementers so they can hook `tracing` macros uniformly.
3. **Validation and drift detection**: write a lightweight smoke test that exercises the runtime and ensures spans are emitted (via log capture) for the key command/effect pairs.

### Phase Exit
- [x] All major runtime surfaces have tracing requirements spelled out.
- [ ] The future egui runtime crate can wire tracing macros without guessing what needs instrumentation.

### Risks / Failure Modes (specific to Phase 6)
- Missing the current Tauri tracing init would cause startup logs to disappear in the egui build.
- Over-instrumenting high-frequency slices could bloat logs and make the parity tests fail.
- Losing correlation between commands and effects would make debugging async boundaries harder.

### Test / Parity Requirements (specific to Phase 6)
- [ ] Confirm `cargo check` plus any existing unit tests still run with the new tracing bootstrapping.
- [ ] Manual log inspection (or scripted smoke run) shows spans for each command/effect label described above.
- [ ] Ensure the instrumentation plan documents the structured fields expected by the wider logging/obs team.

## Risks / Failure Modes
- Carrying over snapshot-shaped frontend assumptions can cause coarse invalidation and sluggish egui updates.
- Replacing Tauri command/event flow without a formal command/effect model risks hidden coupling.
- Persistence can regress if session/runtime ownership is not explicit during close/quit transitions.
- Runtime tasks may overwhelm the UI thread if channel and coalescing rules are unspecified.

## Test / Parity Requirements
- [ ] Rust unit tests for state reducers/transitions.
- [ ] Rust integration tests for command/effect/event flows.
- [ ] Persistence lifecycle tests for open/close/quit behavior.
- [ ] Runtime cancellation/progress tests for long-running jobs.
- [ ] Full implementation-phase build verification excluding AppImage/RPM/DEB packaging outputs.

## Acceptance Criteria
- [ ] The egui app can be implemented against a Rust-native state model with no Tauri/TS bridge dependency.
- [ ] Command, effect, and event responsibilities are explicit and typed.
- [ ] The target runtime model preserves current ownership boundaries and tracing expectations.
- [ ] No major runtime/state architecture question remains open for implementation.
