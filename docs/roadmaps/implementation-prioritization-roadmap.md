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
| Tranche | Focus | Key Deliverables | Dependencies | Completion Signals |
|---------|-------|------------------|--------------|--------------------|
| 1 | Runtime & Instrumentation Foundation | Implement the Rust-native app state/effect engine (State roadmap Phases 1-6) + centralized tracing bootstrap + command/effect pipeline skeleton + shortcut registry | None (foundation) | Typed command models, tracing spans defined, `AppState` accessible |
| 2 | Shell Frame & Navigation | Build egui app shell frame, mode management, modal/shortcut contracts (Shell roadmap Phases 1-4) tying into runtime commands | Runtime tranches, tracing instrumentation | `eframe` shell renders starter/reader skeleton + navigation commands wired |
| 3 | Reader Rendering Core | Implement text-only reader, content-block models, anchor mapping (Reader roadmap Phases 2–5) leveraging shell redraw budgets | Runtime + Shell tranches | canonical sentence renderer, highlight/jump service, shell instrumentation logs |
| 4 | PDF Subsystem | Build Rust-owned PDF renderer, overlays, viewport scheduler, OCR fallback (PDF roadmap Phases 1–6) | Runtime + Shell + Reader (for highlight sync) | Native PDF pipeline outputs textures, overlays, commands logged |
| 5 | Audio & TTS Integration | Wire Piper/rodio playback, sentence timing, caching, and command integration in egui (covered indirectly via existing roadmaps) | Runtime + Reader + Shell for highlight sync | Playback controls drive highlights with tracing |
| 6 | Settings/Cache Persistence & QA | Migrate persistence, cache, bookmarks, plus testing harness replacements (Config + Testing roadmaps) | Runtime + Shell + Reader + PDF | Native persistence API, egui settings surfaces, Rust test suites pass |

## Recommended Work Per Interaction
- Each chat turn should cover one tranche or a discrete chunk within a tranche that can finish within the turn (State foundation, Shell navigation, Reader rendering, etc.).
- After each tranche commit, produce a follow-up roadmap update enumerating the next tranche’s work breakdown.
- Always link commits to the overall phase exit needed for tracing/performance compliance.

## Testing & Validation Strategy
- `cargo check` after each tranche to ensure the workspace stays healthy.
- For UI-related tranches, add targeted Rust unit/integration tests that exercise the command/effect model or instrumentation spans described in the roadmaps.
- Maintain a record of which tranche tests were run for release verification.

## Traceability Notes
- Keep referencing the shared tracing schema (`highlight.anchor`, `pdf.*`, `shell.*`) when describing work in every tranche so instrumentation remains consistent.
- Track fallback hierarchies (exact → fuzzy → block → render-only) and log transitions explicitly as part of the runtime telemetry story.
