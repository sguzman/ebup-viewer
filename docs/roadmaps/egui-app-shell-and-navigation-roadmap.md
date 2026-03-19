# Egui App Shell And Navigation Roadmap

## Objective
- [ ] Replace the current Tauri window shell and React component composition with a native `eframe` + `egui` application shell.
- [ ] Rebuild starter-mode and reader-mode navigation, top bars, side panels, dialogs, and keyboard shortcuts in Rust.
- [ ] Preserve existing interaction semantics while moving all UI ownership into egui widgets and Rust-native app state.

## Current-State Grounding In This Repo
- Current top-level shell ownership lives in `ui/src/App.tsx`, `ui/src/components/StarterShell.tsx`, `ui/src/components/ReaderShell.tsx`, and `ui/src/components/readerPanels.tsx`.
- The frontend currently relies on:
- route-like shell switching between starter and reader views
- Zustand slices for UI/session state
- MUI component composition for toolbars, panels, and modal-like surfaces
- Tauri window/runtime integration rather than in-process native app ownership
- Existing shell behavior that must survive migration:
- starter vs reader mode transitions
- panel exclusivity and quick actions
- top bar/TTS controls layout stability under width pressure
- keyboard shortcuts and focus-aware command dispatch
- search/settings/stats panel visibility rules

## Target End State Under Egui
- A single native egui application owns:
- startup shell
- starter/library shell
- reader shell
- modal surfaces and confirmations
- global keyboard shortcuts
- view transitions and command routing
- The application structure is:
- top app frame with global command bar and status strip
- optional left/right side panels
- central content area for starter or reader content
- modal/dialog layer for confirmations, diagnostics, and import flows
- egui widgets consume Rust-native view models; no browser layout/runtime assumptions remain.

## Key Architectural Decisions Already Chosen
- The shell stack is `eframe` + `egui`.
- There is no browser routing layer in the target app.
- Shell and navigation state stay inside Rust app state rather than a JS store.
- The shell should preserve current UX contracts before any visual redesign work.
- Shortcut handling is centralized in Rust and routed through typed commands/effects.
- The egui shell must be structured for narrow redraw scopes and avoid whole-app invalidation for local panel changes.

## Module And Widget Mapping
- [ ] `App.tsx` maps to a Rust app entry module that owns top-level frame composition and startup.
- [ ] `StarterShell.tsx` maps to starter-mode widgets:
- [ ] local open controls
- [ ] recents panel
- [ ] Calibre panel
- [ ] browser-tab import panel
- [ ] `ReaderShell.tsx` maps to reader-mode shell widgets:
- [ ] toolbar
- [ ] panel regions
- [ ] content pane selection
- [ ] quick actions dock
- [ ] `readerPanels.tsx` maps to distinct egui panel widgets:
- [ ] settings
- [ ] stats
- [ ] search
- [ ] TTS controls
- [ ] status diagnostics
- [ ] layout policy utilities map to Rust shell layout helpers rather than CSS/media queries.

## Phase 1: Shell Contract And Frame Layout
- [x] Capture the current starter-vs-reader split, modal surfaces, and panel stack before any UI coding begins.
- [ ] Define top-level shell state:
- [ ] `ActiveMode` (`Starter`, `Reader`, `SourceLoading`, `SourceError`, `Calibre`, `BrowserTabImport`)
- [ ] panel visibility/exclusivity state machine (settings, search, stats, TTS controls)
- [ ] modal/dialog state machine including blocking confirmation flows
- [ ] global notifications queue (info/warn/error) and session transition indicators
- [ ] runaway screen lock or safe-quit gating flags
- [ ] Define frame regions with memoized layout policies:
- [ ] top toolbar/command bar with grouped actions and status indicators
- [ ] optional navigation/status row for compact mode feedback
- [ ] left/right side panels for library controls and reader-specific widgets
- [ ] central content pane for starter or reader content
- [ ] overlay modal layer that can host blocking or lightweight dialogs
- [ ] Document desktop size-class strategy, minimum supported width (e.g., 900px), and density assumptions (panel collapse thresholds).
- [ ] Define API surfaces for layout helpers that currently live in CSS/media queries (e.g., `isNarrow`, `shouldShowPanels`). These Rust helpers will be called from the runtime to decide redraw scopes.
### Architectural Artifacts To Deliver
- Shell state enums/structs representing the existing App.tsx runtime decisions.
- Layout helpers describing how panel state affects reader width/padding, derived from current CSS breakpoints, to prevent guesswork when implementing UI.
- A modal overlay contract describing focus and escape semantics so future egui widgets can reuse a single modal manager.
### Phase Exit
- [ ] Shell composition and panel ownership are explicit enough for implementation without revisiting layout strategy.

## Phase 2: Navigation And Mode Switching
- [ ] Map every transition that currently lives in React routing or conditional rendering into explicit Rust commands/fsm transitions.
- [ ] Document how the starter shell pivots to reader mode and back, covering:
- [ ] source open success, failure, and cancellation
- [ ] session close/restart
- [ ] return-to-starter from reader mode shortcuts or menu actions
- [ ] Calibre browser conclusions and browser-tab import completion
- [ ] Preserve transient loading/error surfaces during source open, PDF transcription, and Calibre/browser-tab import by defining modal/notification expectations triggered by these states.
- [ ] Define command routing and transition events for:
- [ ] `OpenSource` (local file, EPUB, Markdown, Calibre, browser tab)
- [ ] `CloseSource`
- [ ] `TogglePanel` (settings, stats, search, console) with exclusivity rules
- [ ] `ActivatePlaybackShortcut`/`JumpToHighlight`
- [ ] `PersistState`/`FlushPersistence`
- [ ] `QuitRequest` and safe shutdown gating
- [ ] Explicitly define transition guards that rely on runtime readiness flags (e.g., disallow reader navigation until source metadata loads).
- [ ] Document fallback navigation for PDF/text vs viewer mismatch so the reader shell can recover from partial loads without UI deadlock.

### Implementation Artifacts
- A Rust enum (`ShellTransition`) enumerating the navigation intents above and the data they carry.
- A command-to-state transition table describing how each intent mutates `AppState`, `SessionState`, and `ReaderUiState`.
- A “navigation policy” section showing which commands are available in each mode and how they anchor to keyboard shortcuts or UI controls.

### Phase Exit
- [ ] all high-level navigation transitions have a Rust-native owner, typed transition model, and guard conditions documented so implementers can code deterministic mode switches.

## Phase 3: Toolbar, Panels, And Command Surface
- [ ] Define the toolbar layout contract derived from `ReaderShell.tsx` and `readerPanels.tsx`, documenting how button groups, status chips, and TTS controls align across narrow/wide widths.
- [ ] Specify panel exclusivity rules in detail:
- [ ] `Settings` and `Stats` remain mutually exclusive; define the transition rules when either is invoked while the other is open.
- [ ] `Search` and `Quick Actions` panels can co-exist but share layout space; capture the expected width impact on the reader content pane.
- [ ] `TTS` controls live in a dock that must remain accessible while other panels open; describe how it responds to active playback state.
- [ ] Document button-to-command mapping for toolbar actions (open source, import, calibre toggle, search, stats, settings, reader quick actions) so each produces a typed command/event into the runtime.
- [ ] Define the command dispatch path:
- [ ] Canvas-level input (buttons, toggles) emits `ShellCommand` variants into the runtime.
- [ ] Keyboard shortcuts and right-click items share the same command inputs.
- [ ] Command handlers produce effects (e.g., `OpenSource` -> `SourceIngestionService`, `TogglePanel` -> state mutation) with tracing spans.
- [ ] Provide overflow handling for disabled/hidden buttons when modes disallow actions (e.g., `OpenSource` disabled during source load).
- [ ] Add status/diagnostic surfaces that display import/transcription/runtime conditions, capturing the current TTS status, sync health, and browser-tab health.

### Implementation Artifacts
- A toolbar layout descriptor that defines action groups, icon priorities, and responsive breakpoint behavior.
- A panel exclusivity matrix that guides panel toggling effects and ensures reader content width adjustments are predictable.
- A `ShellCommand` enum that wraps toolbar/panel actions, associated metadata (e.g., target panel, playback data), and the routing rules that feed the command/effect pipeline defined in the runtime roadmap.
- Status/diagnostic surface requirements that enumerate the data each panel or chip must display so implementers can hook into the traced runtime state.

### Phase Exit
- [ ] shell command surfaces match current UX behavior without WebView dependencies and feed explicitly into the runtime command/effect model.

## Phase 4: Keyboard Shortcut And Focus Model
- [ ] Define a Rust-native shortcut registry replacing frontend shortcut parsing/matching behavior.
- [ ] Preserve focus-sensitive behavior so text entry and reader hotkeys do not conflict.
- [ ] Document global vs reader-only vs starter-only shortcut scopes.
- [ ] Define modal focus trapping and escape/confirm semantics.
- Phase exit:
- [ ] shortcut routing is deterministic and does not rely on browser focus semantics.

## Phase 5: Modal, Notification, And Error Strategy
- [ ] Replace browser-style modal/dialog flows with egui-native confirmation, alert, and progress surfaces.
- [ ] Define toast/notification strategy for:
- [ ] import failures
- [ ] persistence errors
- [ ] browser-tab health
- [ ] Calibre load failures
- [ ] PDF degraded mode
- [ ] Ensure all blocking flows have a clear cancel/retry path.
- Phase exit:
- [ ] implementers have a single modal/notification contract for the egui shell.

## Phase 6: Performance And Responsiveness Constraints
- [ ] Define redraw policy for shell-level state:
- [ ] panel toggles must not invalidate heavy reader rendering unnecessarily
- [ ] passive status updates should not force full content repaints
- [ ] repeated runtime events must be coalesced where safe
- [ ] Add tracing metrics for shell redraw frequency, command latency, and layout recalculation hotspots.
- Phase exit:
- [ ] shell performance expectations are concrete enough to review during implementation.

## Risks / Failure Modes
- Porting the current shell 1:1 without explicit layout contracts may produce unstable egui panel behavior.
- Immediate-mode UI can regress responsiveness if panel and content state are not separated cleanly.
- Keyboard shortcut behavior can regress if browser focus assumptions are copied instead of redesigned for native input.
- Modal/error flows can become fragmented if each subsystem invents its own overlay pattern.

## Test / Parity Requirements
- [ ] Manual parity checks for starter-to-reader transitions, close/reopen flows, panel exclusivity, and keyboard shortcuts.
- [ ] Rust integration tests for shell state transitions and command routing.
- [ ] UI behavior harnesses where feasible for panel toggles and modal lifecycle.
- [ ] Verification that control bars remain readable and do not collapse vertically under narrow widths.
- [ ] Full implementation-phase build verification excluding AppImage/RPM/DEB packaging outputs.

## Acceptance Criteria
- [ ] The egui app shell can host both starter and reader modes.
- [ ] Current navigation and panel semantics are fully specified for Rust implementation.
- [ ] Command routing, shortcut handling, and modal strategy are decision-complete.
- [ ] Responsiveness constraints are explicit enough to prevent accidental full-shell redraw regressions.
