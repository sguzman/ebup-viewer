# Egui Migration Implementation Prioritization Roadmap

## Objective
- Provide a stage-by-stage implementation prioritization that translates the existing roadmap decisions into implementable tranches.
- Group work into sizeable chunks that can be completed per interaction, keeping tracing, runtime, shell, reader, and PDF subsystems aligned.
- Ensure the priority order respects dependencies, instrumentation rules, and the “no UI widgets until architecture & runtime foundations exist” constraint established earlier.

## Current-State Alignment
- All subsystem roadmaps (shell, runtime, reader, PDF) already document tracing/instrumentation contracts, fallback hierarchies, and phase exits.
- No tranche may start until its prerequisites (architecture extraction, runtime wiring, viewport contracts) are satisfied.
- Work remains: port shell/routing, runtime state/effects, egui rendering surfaces, PDF renderer, and QA/testing infrastructure.

## Tranche Prioritization
| Status | Tranche | Focus | Key Deliverables | Dependencies | Completion Signals |
|--------|---------|-------|------------------|--------------|--------------------|
| [x] | 1 | Runtime & Instrumentation Foundation | Implement the Rust-native app state/effect engine (State roadmap Phases 1-6) + centralized tracing bootstrap + command/effect pipeline skeleton + shortcut registry | None (foundation) | Typed command models, tracing spans defined, `AppState` accessible |
| [x] | 2 | Shell Frame & Navigation | Build egui app shell frame, mode management, modal/shortcut contracts (Shell roadmap Phases 1-4) tying into runtime commands | Runtime tranches, tracing instrumentation | `eframe` shell renders starter/reader skeleton + navigation commands wired |
| [x] | 3 | Reader Rendering Core | Implement text-only reader, content-block models, anchor mapping (Reader roadmap Phases 2–5) leveraging shell redraw budgets | Runtime + Shell tranches | canonical sentence renderer, highlight/jump service, shell instrumentation logs |
| [x] | 4 | PDF Subsystem | Build Rust-owned PDF renderer, overlays, viewport scheduler, OCR fallback (PDF roadmap Phases 1–6) | Runtime + Shell + Reader (for highlight sync) | Native PDF pipeline outputs textures, overlays, commands logged |
| [x] | 5 | Audio & TTS Integration | Wire Piper/rodio playback, sentence timing, caching, and command integration in egui (covered indirectly via existing roadmaps) | Runtime + Reader + Shell for highlight sync | Playback controls drive highlights with tracing |
| [x] | 6 | Settings/Cache Persistence & QA | Migrate persistence, cache, bookmarks, plus testing harness replacements (Config + Testing roadmaps) | Runtime + Shell + Reader + PDF | Native persistence API, egui settings surfaces, Rust test suites pass |

## Scope Coverage Note
- [x] The roadmap prioritizes the highest-risk, largest subsystems (runtime, shell, reader, PDF, audio, persistence) with instrumentation and tracing baked in.
- [x] It does not enumerate every small refactor, localization tweak, or UX polish; treat those as follow-on tasks once the tranches close.

## Recommended Work Per Interaction
- Each chat turn should cover one tranche or a discrete chunk within a tranche that can finish within the turn (State foundation, Shell navigation, Reader rendering, etc.).
- After each tranche commit, produce a follow-up roadmap update enumerating the next tranche’s work breakdown.
- Always link commits to the overall phase exit needed for tracing/performance compliance.

## Testing & Validation Strategy
- `cargo check` after each tranche to ensure the workspace stays healthy.
- For UI-related tranches, add targeted Rust unit/integration tests that exercise the command/effect model or instrumentation spans described in the roadmaps.
- Maintain a record of which tranche tests were run for release verification.

## Recent Tranche Progress
- **Reader Rendering Core (Tranche 3)**: `[x]` Tied the JumpToSentence/preview renderer flows into the scheduler so canvas/text budgets now emit `shell.performance_budget` spans, QA can replay throttle decisions via the diagnostics surface, and the UI now badges overlay budget pressure signals that jump into the diagnostics panel for replay.
- **PDF Subsystem (Tranche 4)**: `[x]` Hooked `PdfViewportRenderPlan` plus eviction decisions into the actual preview surfaces, honored overlay/text-layer budgets, streamed throttle spans, and surfaced badge-style budget rejections for shell perf-budget auditing while also surfacing native render/eviction pressure spans into the diagnostics panel for QA replay.

## Traceability Notes
- Keep referencing the shared tracing schema (`highlight.anchor`, `pdf.*`, `shell.*`) when describing work in every tranche so instrumentation remains consistent.
- Track fallback hierarchies (exact → fuzzy → block → render-only) and log transitions explicitly as part of the runtime telemetry story.

## Completed Tranche Focus

### Tranche 5: Audio & TTS Integration
- [x] Tie `Piper`/Rodio playback interfaces into the egui shortcut registry + command contracts so the keyboard-driven TTS controls behave exactly like the iced shell.
- [x] Surface `reader.tts` state in the UI with responsive play/pause, seek, and `JumpToSentence` instrumentation that stamps `budget_plan=shell.performance_budget` plus `audio_command` metadata (e.g., `tts.play_next`) so QA can link audio actions with perf traces.
- [x] Cache playback timing and sentence duration metadata, emit `tts.timeline` spans for both auto-play and manual navigation, and provide a diagnostics entry that exposes the latest audio budget decisions and anchor fallback counts.
- [x] Document manual vs. automatic TTS scroll/anchor semantics in the roadmap so follow-up tranches know exactly when the `shell.performance_budget` plan should throttle audio-induced jumps.

### Tranche 6: Settings, Cache Persistence & QA Hardening
- [x] Rebuild the settings panel as an egui sidebar that emits deterministic commands for typography/reader options, ties into the runtime persistence layer, and recounts the tracing fields (`settings.command`, `settings.rebuild`) so telemetry can correlate UI changes with layout spikes.
- [x] Validate cache/ bookmark persistence APIs under the new shell, add regression tests mirroring the existing ones flagged in the QA checklist, and emit bridging spans (e.g., `persistence.flush`, `persistence.evict`) that feed into the implementation prioritization plan.
- [x] Expand the QA diagnostics panel with `trace replay` helpers that can open the relevant roadmap/checklist URLs per span, and keep logging/performance budgets in sync with overlay/panel budgets so overlay heuristics remain auditable.
- [x] Capture and document the remaining regression scenarios (TTS closing, bookmarks, overlay backlog) so the final cutover has a precise QA gate tied to the roadmap checklists.
