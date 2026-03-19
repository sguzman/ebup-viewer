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
- The shell is built atop `lanternleaf-app::AppRuntime`, so every navigation action should call `AppRuntime::plan_command`/`apply_event` instead of driving UI-local state.
- The shortcut layer consumes `ShortcutRegistry` so bindings remain consistent with the runtime command graph and can be modified via bootstrap config without changing UI code.

## Runtime & Shortcut Integration
- The future egui shell will own a single `AppRuntime` instance that:
- [ ] plans commands via `plan_command` when toolbar buttons or panels dispatch actions.
- [ ] applies events through `apply_event` when long-running effects complete or when modals close.
- [ ] exposes `state_snapshot` so widgets can read `AppState` (AppShell, Reader, Playback, etc.) without serialization.
- [ ] surfaces a `ShortcutRegistry` so the keyboard/shortcut layer can remain data-driven.
- The shell should document which `AppCommand`/`ReaderCommand` each UI action targets (open source, toggle panel, send shortcut) and track the effect owners so tracing spans correlate with `AppRuntime` commands.
- Modal confirmations, navigation transitions, and panel switches should never mutate `AppState` directly; they should emit `AppCommand`s so the runtime enforces operation scopes and instrumentation.
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
- Each toolbar button, quick action, or panel toggle must declare which `AppCommand`/`ReaderCommand` it plans (e.g., `ToggleSettingsPanel`, `Reader(TtsSeekNext)`), enabling the AppRuntime command planner to emit telemetry and effect ownership.
- Shortcut bindings should derive from `ShortcutRegistry` so any new binding automatically flows through the same command pipeline described above, keeping telemetry, cancellation, and logging consistent across mouse/keyboard triggers.

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

## Tranche 2 Implementation Plan
- **Top bar / command bar**: Define an egui `TopBottomPanel` that lists main shell commands (open source, import, shortcuts). Each button converts to an `AppCommand` (`OpenSourcePath`, `SafeQuit`, `ToggleSettingsPanel`, etc.) and calls `AppRuntime::plan_command`; the returned `DispatchPlan` drives the same `ApplicationEvent`s as the existing runtime so tracing records the effect owner. The top bar should also display telemetry chips from `state_snapshot` (e.g., busy operations) so instrumentation links UI state to runtime events.
- **Starter / reader mode shell**: Implement mode-aware central content using `AppState` snapshots (starter recents or reader view). Mode switches (e.g., `ReturnToStarter`, `CloseReaderSession`) should issue `AppCommand`s and wait for related `AppEvent`s before fully swapping panels; this keeps the runtime operation scope consistent and prevents UI-local race conditions. Use `AppRuntime::apply_event` to acknowledge operation completions.
- **Left/right panels and quick actions**: Panels (settings, stats, search, TTS) must call `AppRuntime::plan_command(AppCommand::TogglePanel { panel: ... })` and rely on `state_snapshot` to determine visibility. Quick actions (play/pause, next/prev sentence) map to `ReaderCommand` variants and share `ShortcutRegistry` bindings so they emit the same commands whether triggered by toolbar buttons or keyboard shortcuts.
- **Modal/dialog layer**: Define an egui `CentralPanel` overlay stack that hosts blocking confirmations (safe quit, close reader), progress dialogs (import), and toasts. When a modal action is confirmed, emit the corresponding `AppCommand` (e.g., `SafeQuit`, `FlushPersistence`) and use `AppRuntime::apply_event` to clear visible states once the runtime reports completion. Cancel/close actions should also translate into commands (e.g., `CommandFailed` or `OperationChanged` resets) so instrumentation sees how modal flows interact with the runtime.
- **Shortcut synchronization**: Bind the keyboard handler to the `ShortcutRegistry` snapshot rather than hardcoding combos; each triggered binding emits the appropriate `ShortcutAction` (command or UI follow-up) and calls `AppRuntime::plan_command`. This ensures that future config changes (per bootstrap) automatically update both toolbar labels and keyboard handling through the same runtime pipeline.
- **Tracing hooks**: Every UI action in this tranche should annotate `tracing::instrument` spans that mention the originating `AppCommand`/`ReaderCommand` so shell metrics can correlate frame interactions with the runtime verbs emitted by `AppRuntime`.

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
- [ ] Define a Rust-native shortcut registry that mirrors the current shortcut map in `ui/src/lib/shortcuts.ts` and derives from keyboard definitions used in `ReaderShell` and `StarterShell`.
- [ ] Catalog the scopes for every shortcut:
- [ ] Global (window-wide, always active unless blocked by modal)
- [ ] Starter-only (allowed before a source opens)
- [ ] Reader-only (playback, navigation, highlight)
- [ ] Panel-only (search, stats, settings focus)
- [ ] Define focus ownership rules:
- [ ] Text entry fields (search, settings, modal inputs) capture keys and suppress reader shortcuts while focused.
- [ ] Reader viewport captures navigation/playback shortcuts when it has focus, but yields to modals and panel inputs.
- [ ] Modal dialogs and toast notifications trap escape/confirm behavior and block global shortcuts when active.
- [ ] Define modal focus trapping and escape/confirm semantics explicitly so future egui modals can reuse a single focus manager.
- [ ] Document fallback behavior for shortcut collisions (e.g., search entry pressing `Ctrl+F` vs reader playback keys) with priority rules.
- [ ] Define shortcut registration semantics:
- [ ] a `ShortcutRegistry` service that accepts `(ShortcutId, Scope, KeyCombo, Handler)` tuples.
- [ ] a `FocusOwner` state that tracks currently active scope, informs command routing, and resets when panels/modal close.
- [ ] Document how `eframe` key events flow into the registry without the DOM event bubble.
### Phase Exit
- [ ] shortcut routing is deterministic, focus-aware, and no longer relies on browser focus semantics so the egui input layer can plug into the Rust runtime command model.

## Phase 5: Modal, Notification, And Error Strategy
- [ ] Replace browser-style modal/dialog flows with egui-native confirmation, alert, toast, and progress surfaces.
- [ ] Define the modal stack semantics:
- [ ] blocking confirmations (e.g., close source without save, Calibre import overwrite) that prevent other shell interactions until resolved
- [ ] information dialogs (filters, help) that sit above the shell but permit non-blocking background activity
- [ ] progress overlays tied to long-running jobs (import, transcription, PDF OCR) that also expose cancel actions and show progress metrics
- [ ] Define notification/toast surface rules:
- [ ] Import failures and Calibre/browser-tab errors display persistent toast with retry/copy log actions.
- [ ] Persistence errors and safe-quit issues surface warnings with deep links to settings or diagnostics.
- [ ] Reader health issues (PDF degraded mode, sync drift) highlight in status chips but also post toasts when severity increases.
- [ ] Each modal/notification must emit typed commands/events when acknowledged, canceled, or closed so the runtime can record the outcome and resume the underlying task.
- [ ] Document how modal focus trapping works with the `FocusOwner` defined in Phase 4, ensuring escape/confirm semantics remain consistent across dialogs.
- [ ] Ensure all blocking flows (import/transcription, persistence flush, browser-tab sync) have defined cancel/retry paths and inform the command model accordingly.

### Implementation Artifacts
- A modal contract describing layers (blocking vs passive), required metadata (title, body, actions, default focus), and the lifecycle commands emitted on open/close.
- A notification/toast registry describing severity levels, required action buttons, and how to link to diagnostics or logs.
- A set of sentinel cases (e.g., persistence failure, Calibre sync failure, PDF degraded sync) with explicit UI behavior, data sources, and the resulting command/effect transitions tracing back to Phase 2.

### Phase Exit
- [ ] implementers have a single modal/notification contract for the egui shell that keeps the command layer consistent while replacing Web-style dialogs.

## Phase 6: Performance And Responsiveness Constraints
- [ ] Define redraw policy for shell-level state so panel interactions and status updates do not force full reader recomposition.
- [ ] Panel toggles must only touch the panel regions they own and rely on derived layout helpers (Phase 1) so heavy reader renders remain cached.
- [ ] Passive status updates (notifications, runtime chips) should update minimal widgets instead of repainting the central pane.
- [ ] Repeated runtime events (progress ticks, TTS heartbeats) must be coalesced at the command surface and throttled before they reach the egui frame pipeline.
- [ ] Capture coalescing rules for:
- [ ] playback progress updates
- [ ] import/transcription status ticks
- [ ] browser-tab health pings
- [ ] panel diagnostics refreshes
- [ ] Add tracing/metrics hooks for:
- [ ] redraw frequency per frame (per region if possible)
- [ ] toolbar/panel command latency from click/shortcut to effect spawn
- [ ] layout recalculation hotspots when the window resizes
- [ ] modal stacking depth and notification bursts
- [ ] Document how these metrics plug into the tracing model described in the runtime roadmap so the egui shell and runtime share observability.
- [ ] Define a “redraw budget” contract: specify the maximum allowed frame budget for each panel update and the fallback behavior (e.g., delay heavy status updates until the reader is idle).

### Implementation Artifacts
- A redraw policy doc linking shell state changes from Phases 1–5 to the expected invalidation scopes so implementers know when it is okay to use `ctx.request_repaint` or rely on `egui`’s auto-invalidation.
- A throttling/coalescing matrix that maps event sources (e.g., playback heartbeat) to their destination UI widgets, including the suggested merge interval.
- A tracing/metrics plan referencing the runtime tracing spans so the shell instrumentation can be aligned with the runtime event model.

### Phase Exit
- [ ] shell performance expectations are concrete enough to review during implementation and the tracing hooks are documented so the runtime team can instrument the same metrics.

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
