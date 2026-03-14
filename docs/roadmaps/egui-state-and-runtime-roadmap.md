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
- [ ] Port current Tauri logging/tracing bootstrap to the egui app crate.
- [ ] Define tracing for:
- [ ] state transitions
- [ ] command dispatch
- [ ] runtime effects
- [ ] redraw/perf hotspots
- [ ] import/transcription/PDF/TTS service calls
- [ ] Keep logs structured enough for migration-side parity debugging.
- Phase exit:
- [ ] tracing requirements are specified for every major runtime surface.

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
