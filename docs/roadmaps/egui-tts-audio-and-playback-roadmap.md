# Egui TTS Audio And Playback Roadmap

## Objective
- [x] Move TTS controls, playback coordination, audio progress updates, and worker/runtime orchestration into the egui app runtime.
- [x] Preserve Piper-backed speech synthesis, caching, timing accuracy, and sentence cursor semantics.
- [x] Ensure the UI consumes typed Rust playback state rather than frontend bridge events.

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
- [x] Define which runtime responsibilities live in:
- [x] core playback/session crate
- [x] TTS engine/service layer
- [x] egui app runtime coordinator
- [x] UI widgets
- [x] Replace any Tauri event assumptions with typed Rust-native playback events/messages.
- Phase exit:
- [x] playback ownership is fully assigned inside the Rust workspace.

## Phase 2: Command And Event Surface
- [x] Define typed playback commands:
- [x] play
- [x] pause
- [x] toggle
- [x] play from page start
- [x] play from highlight
- [x] previous/next sentence
- [x] speed/volume/settings changes
- [x] Define typed runtime events:
- [x] progress changed
- [x] highlight changed
- [x] queued/prefetched
- [x] completed
- [x] failed/cancelled
- Phase exit:
- [x] egui widgets can bind to a stable playback control surface without command bridging.

## Phase 3: Worker And Audio Pipeline
- [x] Preserve or refactor worker orchestration for:
- [x] synthesis subprocess/work queue
- [x] cache lookup
- [x] prefetch
- [x] append to output
- [x] cancellation
- [x] Define thread boundaries so audio and synthesis events enter the UI through coalesced message channels.
- Phase exit:
- [x] TTS runtime architecture is specified for the egui app with no UI-thread blocking risk.

## Phase 4: Playback Cursor And Highlight Flow
- [x] Preserve the current canonical flow:
- [x] text ownership -> playback plan -> progress updates -> highlighted sentence -> UI scroll/highlight
- [x] Specify how playback progress enters reader document and PDF/non-PDF renderers in Rust-native state.
- [x] Preserve multi-chunk sentence mapping and audio/display index mapping semantics.
- Phase exit:
- [x] playback cursor and highlight propagation are explicit and implementation-ready.

## Phase 5: Widget And Status Surface
- [x] Rebuild playback controls in egui:
- [x] transport controls
- [x] speed/volume controls
- [x] sentence navigation
- [x] time remaining/progress labels
- [x] runtime diagnostics where needed
- [x] Preserve current user-visible semantics and stats behavior.
- Phase exit:
- [x] playback UI behavior is fully defined for native widgets.

## Phase 6: Performance, Caching, And Telemetry
- [x] Define cache ownership for synthesized audio and playback artifacts.
- [x] Keep prefetch and cache metrics visible in tracing.
- [x] Define acceptable latency targets for:
- [x] play from page
- [x] play from highlighted sentence
- [x] next sentence transitions
- [x] cold vs warm cache behavior
- Phase exit:
- [x] implementation has concrete playback performance and observability goals.

## Risks / Failure Modes
- UI progress may become noisy or cause redraw storms if event batching/coalescing is not explicit.
- Regressions in mapping between audio chunks and display sentences can break highlight stability.
- Playback commands may race if old Tauri-era lifecycle assumptions are copied into the native shell.
- Prefetch/caching complexity can be lost if the migration focuses only on visible controls.

## Test / Parity Requirements
- [x] Rust unit tests for playback transitions and mapping behavior.
- [x] Rust integration tests for worker queueing, cancellation, and progress propagation.
- [ ] Manual parity checks for all TTS controls, cursor movement, and progress/stat displays.
- [x] Full implementation-phase build verification excluding AppImage/RPM/DEB packaging outputs.

### Manual Parity Checklist (Run In Egui)
- [ ] Play / pause / toggle behave as expected
- [ ] Play from page start begins at sentence 1
- [ ] Play from highlight begins at selected sentence
- [ ] Prev / next sentence move the highlight and audio cursor
- [ ] Repeat sentence replays current sentence
- [ ] Speed / volume changes take effect immediately
- [ ] Progress + ETA labels update during playback
- [ ] Auto-scroll + center-spoken-sentence behaviors match expectations
- [ ] PDF text-only policies still gate TTS when disallowed
- [ ] Cancel/close session stops playback and resets UI state

## Acceptance Criteria
- [x] The egui migration has a complete Rust-native plan for TTS controls, runtime orchestration, and playback events.
- [x] Piper, rodio, cache, and progress semantics remain explicit and in scope.
- [x] Highlight/cursor propagation from playback into readers is fully specified.
- [x] No TTS/runtime ownership question remains open for the UI migration.
