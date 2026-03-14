# Egui TTS Audio And Playback Roadmap

## Objective
- [ ] Move TTS controls, playback coordination, audio progress updates, and worker/runtime orchestration into the egui app runtime.
- [ ] Preserve Piper-backed speech synthesis, caching, timing accuracy, and sentence cursor semantics.
- [ ] Ensure the UI consumes typed Rust playback state rather than frontend bridge events.

## Current-State Grounding In This Repo
- Current TTS/runtime ownership is split across:
- `src/tts.rs`
- `src/tts_worker.rs`
- `src-tauri/src/tts_runtime.rs`
- `crates/lanternleaf-core/src/session/playback.rs`
- React/Zustand playback UI currently lives in:
- `ui/src/components/TtsPlayerWidget.tsx`
- reader quick actions
- store playback slices/selectors
- Current product behavior that must survive:
- play/pause/toggle
- play-from-page / play-from-highlight
- prev/next sentence
- highlight cursor updates
- progress display and ETA/stats
- cache reuse and prefetch
- auto-scroll and center-tracking behavior

## Target End State Under Egui
- The egui app runtime owns:
- playback controls
- background synthesis orchestration
- audio queue management
- playback progress events
- cursor/highlight state propagation
- egui widgets render playback controls and status directly from Rust-native playback state.
- Piper, rodio, waveform/cache artifacts, and worker orchestration remain Rust-native with no bridge layer.

## Key Architectural Decisions Already Chosen
- Piper remains in scope and is not removed during migration.
- Canonical playback ownership remains sentence/cursor driven from Rust domain state.
- Worker/runtime orchestration remains asynchronous and off the UI thread.
- Playback progress drives highlight state; UI does not invent independent timers.
- Tracing is mandatory for synthesis, cache hits/misses, append, progress, and failure modes.

## Phase 1: Runtime Ownership Boundary
- [ ] Define which runtime responsibilities live in:
- [ ] core playback/session crate
- [ ] TTS engine/service layer
- [ ] egui app runtime coordinator
- [ ] UI widgets
- [ ] Replace any Tauri event assumptions with typed Rust-native playback events/messages.
- Phase exit:
- [ ] playback ownership is fully assigned inside the Rust workspace.

## Phase 2: Command And Event Surface
- [ ] Define typed playback commands:
- [ ] play
- [ ] pause
- [ ] toggle
- [ ] play from page start
- [ ] play from highlight
- [ ] previous/next sentence
- [ ] speed/volume/settings changes
- [ ] Define typed runtime events:
- [ ] progress changed
- [ ] highlight changed
- [ ] queued/prefetched
- [ ] completed
- [ ] failed/cancelled
- Phase exit:
- [ ] egui widgets can bind to a stable playback control surface without command bridging.

## Phase 3: Worker And Audio Pipeline
- [ ] Preserve or refactor worker orchestration for:
- [ ] synthesis subprocess/work queue
- [ ] cache lookup
- [ ] prefetch
- [ ] append to output
- [ ] cancellation
- [ ] Define thread boundaries so audio and synthesis events enter the UI through coalesced message channels.
- Phase exit:
- [ ] TTS runtime architecture is specified for the egui app with no UI-thread blocking risk.

## Phase 4: Playback Cursor And Highlight Flow
- [ ] Preserve the current canonical flow:
- [ ] text ownership -> playback plan -> progress updates -> highlighted sentence -> UI scroll/highlight
- [ ] Specify how playback progress enters reader document and PDF/non-PDF renderers in Rust-native state.
- [ ] Preserve multi-chunk sentence mapping and audio/display index mapping semantics.
- Phase exit:
- [ ] playback cursor and highlight propagation are explicit and implementation-ready.

## Phase 5: Widget And Status Surface
- [ ] Rebuild playback controls in egui:
- [ ] transport controls
- [ ] speed/volume controls
- [ ] sentence navigation
- [ ] time remaining/progress labels
- [ ] runtime diagnostics where needed
- [ ] Preserve current user-visible semantics and stats behavior.
- Phase exit:
- [ ] playback UI behavior is fully defined for native widgets.

## Phase 6: Performance, Caching, And Telemetry
- [ ] Define cache ownership for synthesized audio and playback artifacts.
- [ ] Keep prefetch and cache metrics visible in tracing.
- [ ] Define acceptable latency targets for:
- [ ] play from page
- [ ] play from highlighted sentence
- [ ] next sentence transitions
- [ ] cold vs warm cache behavior
- Phase exit:
- [ ] implementation has concrete playback performance and observability goals.

## Risks / Failure Modes
- UI progress may become noisy or cause redraw storms if event batching/coalescing is not explicit.
- Regressions in mapping between audio chunks and display sentences can break highlight stability.
- Playback commands may race if old Tauri-era lifecycle assumptions are copied into the native shell.
- Prefetch/caching complexity can be lost if the migration focuses only on visible controls.

## Test / Parity Requirements
- [ ] Rust unit tests for playback transitions and mapping behavior.
- [ ] Rust integration tests for worker queueing, cancellation, and progress propagation.
- [ ] Manual parity checks for all TTS controls, cursor movement, and progress/stat displays.
- [ ] Full implementation-phase build verification excluding AppImage/RPM/DEB packaging outputs.

## Acceptance Criteria
- [ ] The egui migration has a complete Rust-native plan for TTS controls, runtime orchestration, and playback events.
- [ ] Piper, rodio, cache, and progress semantics remain explicit and in scope.
- [ ] Highlight/cursor propagation from playback into readers is fully specified.
- [ ] No TTS/runtime ownership question remains open for the UI migration.
