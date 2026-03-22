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
- [x] plans commands via `plan_command` when toolbar buttons or panels dispatch actions.
- [x] applies events through `apply_event` when long-running effects complete or when modals close.
- [x] exposes `state_snapshot` so widgets can read `AppState` (AppShell, Reader, Playback, etc.) without serialization.
- [x] surfaces a `ShortcutRegistry` so the keyboard/shortcut layer can remain data-driven.
- The shell should document which `AppCommand`/`ReaderCommand` each UI action targets (open source, toggle panel, send shortcut) and track the effect owners so tracing spans correlate with `AppRuntime` commands.
- Modal confirmations, navigation transitions, and panel switches should never mutate `AppState` directly; they should emit `AppCommand`s so the runtime enforces operation scopes and instrumentation.
## Module And Widget Mapping
- [x] `App.tsx` maps to a Rust app entry module that owns top-level frame composition and startup.
- [x] `StarterShell.tsx` maps to starter-mode widgets:
- [x] local open controls
- [x] recents panel
- [x] Calibre panel
- [x] browser-tab import panel
- [x] `ReaderShell.tsx` maps to reader-mode shell widgets:
- [x] toolbar
- [x] panel regions
- [x] content pane selection
- [x] quick actions dock
- [x] `readerPanels.tsx` maps to distinct egui panel widgets:
- [x] settings
- [x] stats
- [x] search
- [x] TTS controls
- [x] status diagnostics
- [x] layout policy utilities map to Rust shell layout helpers rather than CSS/media queries.
- Each toolbar button, quick action, or panel toggle must declare which `AppCommand`/`ReaderCommand` it plans (e.g., `ToggleSettingsPanel`, `Reader(TtsSeekNext)`), enabling the AppRuntime command planner to emit telemetry and effect ownership.
- Shortcut bindings should derive from `ShortcutRegistry` so any new binding automatically flows through the same command pipeline described above, keeping telemetry, cancellation, and logging consistent across mouse/keyboard triggers.

## Phase 1: Shell Contract And Frame Layout
- [x] Capture the current starter-vs-reader split, modal surfaces, and panel stack before any UI coding begins.
- [x] Define top-level shell state:
- [x] `ActiveMode` (`Starter`, `Reader`, `SourceLoading`, `SourceError`, `Calibre`, `BrowserTabImport`)
- [x] panel visibility/exclusivity state machine (settings, search, stats, TTS controls)
- [x] modal/dialog state machine including blocking confirmation flows
- [x] global notifications queue (info/warn/error) and session transition indicators
- [x] runaway screen lock or safe-quit gating flags
- [x] Define frame regions with memoized layout policies:
- [x] top toolbar/command bar with grouped actions and status indicators
- [x] optional navigation/status row for compact mode feedback
- [x] left/right side panels for library controls and reader-specific widgets
- [x] central content pane for starter or reader content
- [x] overlay modal layer that can host blocking or lightweight dialogs
- [x] Document desktop size-class strategy, minimum supported width (e.g., 900px), and density assumptions (panel collapse thresholds).
- [x] Define API surfaces for layout helpers that currently live in CSS/media queries (e.g., `isNarrow`, `shouldShowPanels`). These Rust helpers will be called from the runtime to decide redraw scopes.
### Architectural Artifacts To Deliver
- Shell state enums/structs representing the existing App.tsx runtime decisions.
- Layout helpers describing how panel state affects reader width/padding, derived from current CSS breakpoints, to prevent guesswork when implementing UI.
- A modal overlay contract describing focus and escape semantics so future egui widgets can reuse a single modal manager.
### Phase Exit
- [x] Shell composition and panel ownership are explicit enough for implementation without revisiting layout strategy.

## Tranche 2 Implementation Plan
- **Top bar / command bar**: Define an egui `TopBottomPanel` that lists main shell commands (open source, import, shortcuts). Each button converts to an `AppCommand` (`OpenSourcePath`, `SafeQuit`, `ToggleSettingsPanel`, etc.) and calls `AppRuntime::plan_command`; the returned `DispatchPlan` drives the same `ApplicationEvent`s as the existing runtime so tracing records the effect owner. The top bar should also display telemetry chips from `state_snapshot` (e.g., busy operations) so instrumentation links UI state to runtime events.
- **Starter / reader mode shell**: Implement mode-aware central content using `AppState` snapshots (starter recents or reader view). Mode switches (e.g., `ReturnToStarter`, `CloseReaderSession`) should issue `AppCommand`s and wait for related `AppEvent`s before fully swapping panels; this keeps the runtime operation scope consistent and prevents UI-local race conditions. Use `AppRuntime::apply_event` to acknowledge operation completions.
- **Left/right panels and quick actions**: Panels (settings, stats, search, TTS) must call `AppRuntime::plan_command(AppCommand::TogglePanel { panel: ... })` and rely on `state_snapshot` to determine visibility. Quick actions (play/pause, next/prev sentence) map to `ReaderCommand` variants and share `ShortcutRegistry` bindings so they emit the same commands whether triggered by toolbar buttons or keyboard shortcuts.
- **Modal/dialog layer**: Define an egui `CentralPanel` overlay stack that hosts blocking confirmations (safe quit, close reader), progress dialogs (import), and toasts. When a modal action is confirmed, emit the corresponding `AppCommand` (e.g., `SafeQuit`, `FlushPersistence`) and use `AppRuntime::apply_event` to clear visible states once the runtime reports completion. Cancel/close actions should also translate into commands (e.g., `CommandFailed` or `OperationChanged` resets) so instrumentation sees how modal flows interact with the runtime.
- **Shortcut synchronization**: Bind the keyboard handler to the `ShortcutRegistry` snapshot rather than hardcoding combos; each triggered binding emits the appropriate `ShortcutAction` (command or UI follow-up) and calls `AppRuntime::plan_command`. This ensures that future config changes (per bootstrap) automatically update both toolbar labels and keyboard handling through the same runtime pipeline.
- **Tracing hooks**: Every UI action in this tranche should annotate `tracing::instrument` spans that mention the originating `AppCommand`/`ReaderCommand` so shell metrics can correlate frame interactions with the runtime verbs emitted by `AppRuntime`.

## Phase 2: Navigation And Mode Switching
- [x] Map every transition that currently lives in React routing or conditional rendering into explicit Rust commands/fsm transitions.
- [x] Document how the starter shell pivots to reader mode and back, covering:
- [x] source open success, failure, and cancellation
- [x] session close/restart
- [x] return-to-starter from reader mode shortcuts or menu actions
- [x] Calibre browser conclusions and browser-tab import completion
- [x] Preserve transient loading/error surfaces during source open, PDF transcription, and Calibre/browser-tab import by defining modal/notification expectations triggered by these states.
- [x] Define command routing and transition events for:
- [x] `OpenSource` (local file, EPUB, Markdown, Calibre, browser tab)
- [x] `CloseSource`
- [x] `TogglePanel` (settings, stats, search, console) with exclusivity rules
- [x] `ActivatePlaybackShortcut`/`JumpToHighlight`
- [x] `PersistState`/`FlushPersistence`
- [x] `QuitRequest` and safe shutdown gating
- [x] Explicitly define transition guards that rely on runtime readiness flags (e.g., disallow reader navigation until source metadata loads).
- [x] Document fallback navigation for PDF/text vs viewer mismatch so the reader shell can recover from partial loads without UI deadlock.

### Navigation Transition Notes (Starter <-> Reader)
- Source open success: `AppCommand::OpenSourcePath|OpenClipboard|OpenCalibreBook|OpenBrowserTab*` -> `AppEvent::SourceOpened` -> session switches to `UiMode::Reader` and `ShellState::active_mode` updates from `Starter` to `Reader`.
- Source open failure: `AppEvent::CommandFailed { scope: Some(OperationScope::SourceOpen) }` or `AppEvent::SourceOpenProgress` with phase `failed` -> `ShellState::active_mode = SourceError` until a new open attempt succeeds.
- Source open cancellation: `AppEvent::SourceOpenProgress` with phase `cancelled` clears `OperationScope::SourceOpen` and returns the shell to `Starter` without altering recents.
- Return to starter: `AppCommand::ReturnToStarter` -> `AppEvent::SessionUpdated` with `UiMode::Starter` -> `ShellState::active_mode = Starter`.
- Session close/restart: `AppCommand::CloseReaderSession` -> `AppEvent::SessionUpdated` (mode `Starter`) + `ReaderUpdated` cleared -> show close confirmation modal first, then return to starter.
- Calibre completion: `AppCommand::OpenCalibreBook` -> `AppEvent::SourceOpened` transitions to reader; `CalibreLoadEvent` phases drive Calibre loading/error toasts.
- Browser-tab import completion: `AppCommand::OpenBrowserTab|OpenBrowserTabBundle|RefreshBrowserTab` -> `AppEvent::SourceOpened` or `CommandFailed` updates shell mode and status diagnostics.

### Command Routing Map
- OpenSource: `OpenSourcePath|OpenClipboard|OpenClipboardText|OpenCalibreBook|OpenBrowserTab*` -> `RuntimeEffect::Open*` -> `AppEvent::SourceOpened` or `CommandFailed`.
- CloseSource: `CloseReaderSession` -> `RuntimeEffect::CloseReaderSession` -> `AppEvent::SessionUpdated`.
- TogglePanel: `ToggleSettingsPanel|ToggleStatsPanel|ToggleTtsPanel` -> `RuntimeEffect::TogglePanel` -> session panel state updates.
- ActivatePlaybackShortcut / JumpToHighlight: `Reader(SessionCommand::Tts*)` + `Reader(SessionCommand::JumpToSentence)` -> `RuntimeEffect::ApplyReaderCommand`.
- Persist/Flush: `FlushPersistence` / `SafeQuit` -> `RuntimeEffect::FlushPersistence` -> persistence lifecycle hooks -> `SafeQuit` effect.
- QuitRequest: `SafeQuit` -> modal confirm -> `RuntimeEffect::SafeQuit` with persistence flush gate.

### Transition Guards And Fallbacks
- Guard: disallow reader navigation commands until `state.reader_document.snapshot` is present and `OperationScope::SourceOpen` is cleared.
- Guard: while `state.app_shell.operations.calibre_load` or `browser_tab_refresh` is active, show loading indicators and avoid mode flips until completion.
- Fallback: if PDF/text viewer mismatch or partial load occurs, keep `UiMode::Reader` but surface an error toast and offer `ReturnToStarter` in modal.

### Implementation Artifacts
- A Rust enum (`ShellTransition`) enumerating the navigation intents above and the data they carry.
- A command-to-state transition table describing how each intent mutates `AppState`, `SessionState`, and `ReaderUiState`.
- A “navigation policy” section showing which commands are available in each mode and how they anchor to keyboard shortcuts or UI controls.

### Phase Exit
- [x] all high-level navigation transitions have a Rust-native owner, typed transition model, and guard conditions documented so implementers can code deterministic mode switches.

## Phase 3: Toolbar, Panels, And Command Surface
- [x] Define the toolbar layout contract derived from `ReaderShell.tsx` and `readerPanels.tsx`, documenting how button groups, status chips, and TTS controls align across narrow/wide widths.
- [x] Specify panel exclusivity rules in detail:
- [x] `Settings` and `Stats` remain mutually exclusive; define the transition rules when either is invoked while the other is open.
- [x] `Search` and `Quick Actions` panels can co-exist but share layout space; capture the expected width impact on the reader content pane.
- [x] `TTS` controls live in a dock that must remain accessible while other panels open; describe how it responds to active playback state.
- [x] Document button-to-command mapping for toolbar actions (open source, import, calibre toggle, search, stats, settings, reader quick actions) so each produces a typed command/event into the runtime.
- [x] Define the command dispatch path:
- [x] Canvas-level input (buttons, toggles) emits `ShellCommand` variants into the runtime.
- [x] Keyboard shortcuts and right-click items share the same command inputs.
- [x] Command handlers produce effects (e.g., `OpenSource` -> `SourceIngestionService`, `TogglePanel` -> state mutation) with tracing spans.
- [x] Provide overflow handling for disabled/hidden buttons when modes disallow actions (e.g., `OpenSource` disabled during source load).
- [x] Add status/diagnostic surfaces that display import/transcription/runtime conditions, capturing the current TTS status, sync health, and browser-tab health.

### Toolbar Layout Contract
- Primary left group: open/import actions (open file, clipboard, browser-tab import), visible in Starter and Reader.
- Center group: reader quick actions (play/pause, prev/next, repeat) only in Reader; hidden in Starter.
- Right group: settings/stats/search/TTS toggles + runtime status chips (busy, source open, Calibre load, browser-tab refresh).
- Narrow width behavior: collapse center group into a compact row; status chips move to the navigation/status row.

### Panel Exclusivity Rules
- Settings vs Stats: mutually exclusive; opening one closes the other.
- Search + Quick Actions: may coexist; search panel should not suppress quick actions dock.
- TTS controls: always visible when `panels.show_tts` is true, regardless of Settings/Stats visibility.

### Button-to-Command Mapping
- Open source: `OpenSourcePath`, `OpenClipboard`, `OpenClipboardText`.
- Calibre open: `OpenCalibreBook`, thumbnail hydration via `EnsureCalibreThumbnail`.
- Browser tabs: `OpenBrowserTab`, `OpenBrowserTabBundle`, `RefreshBrowserTab`, `ListBrowserTabs`.
- Panel toggles: `ToggleSettingsPanel`, `ToggleStatsPanel`, `ToggleTtsPanel`.
- Reader quick actions: `Reader(SessionCommand::TtsTogglePlayPause|TtsSeekPrev|TtsSeekNext|TtsRepeatSentence)`.
- Safe quit: `SafeQuit` (modal confirmation required).

### Command Dispatch Path
- Canvas inputs (buttons, toggles) emit `AppCommand`/`ReaderCommand` via `AppRuntime::plan_command`.
- Shortcut bindings route through `ShortcutRegistry` and reuse the same command path.
- Commands generate `DispatchPlan` effects with tracing spans; effects resolve into `AppEvent` updates.
- Disabled/hidden actions: use operation scopes (e.g., `OperationScope::SourceOpen`) to disable recents/opens while a source is opening.
- Status/diagnostics: expose busy flags, source open, Calibre load, browser-tab refresh, and TTS state in the panel/status surfaces.

### Implementation Artifacts
- A toolbar layout descriptor that defines action groups, icon priorities, and responsive breakpoint behavior.
- A panel exclusivity matrix that guides panel toggling effects and ensures reader content width adjustments are predictable.
- A `ShellCommand` enum that wraps toolbar/panel actions, associated metadata (e.g., target panel, playback data), and the routing rules that feed the command/effect pipeline defined in the runtime roadmap.
- Status/diagnostic surface requirements that enumerate the data each panel or chip must display so implementers can hook into the traced runtime state.

### Phase Exit
- [x] shell command surfaces match current UX behavior without WebView dependencies and feed explicitly into the runtime command/effect model.

## Phase 4: Keyboard Shortcut And Focus Model
- [x] Define a Rust-native shortcut registry that mirrors the current shortcut map in `ui/src/lib/shortcuts.ts` and derives from keyboard definitions used in `ReaderShell` and `StarterShell`.
- [x] Catalog the scopes for every shortcut:
- [x] Global (window-wide, always active unless blocked by modal)
- [x] Starter-only (allowed before a source opens)
- [x] Reader-only (playback, navigation, highlight)
- [x] Panel-only (search, stats, settings focus)
- [x] Define focus ownership rules:
- [x] Text entry fields (search, settings, modal inputs) capture keys and suppress reader shortcuts while focused.
- [x] Reader viewport captures navigation/playback shortcuts when it has focus, but yields to modals and panel inputs.
- [x] Modal dialogs and toast notifications trap escape/confirm behavior and block global shortcuts when active.
- [x] Define modal focus trapping and escape/confirm semantics explicitly so future egui modals can reuse a single focus manager.
- [x] Document fallback behavior for shortcut collisions (e.g., search entry pressing `Ctrl+F` vs reader playback keys) with priority rules.
- [x] Define shortcut registration semantics:
- [x] a `ShortcutRegistry` service that accepts `(ShortcutId, Scope, KeyCombo, Handler)` tuples.
- [x] a `FocusOwner` state that tracks currently active scope, informs command routing, and resets when panels/modal close.
- [x] Document how `eframe` key events flow into the registry without the DOM event bubble.

### Shortcut Scope Catalog
- Global: safe quit, toggle panels, focus search.
- Starter-only: open source, refresh recents, Calibre/browser-tab refresh.
- Reader-only: playback controls, seek next/prev, repeat, jump to highlight.
- Panel-only: search input navigation and settings field editing.

### Focus Ownership Rules
- `FocusOwner::PanelInput` suppresses reader/global shortcuts while text fields are active.
- `FocusOwner::Modal` traps escape/confirm and blocks other shortcuts until modal closes.
- `FocusOwner::Reader` routes playback/navigation shortcuts when reader is active.
- `FocusOwner::Starter` allows global + starter shortcuts only.

### Shortcut Registration Semantics
- The `ShortcutRegistry` provides `(ShortcutId, Scope, KeyCombo, Handler)` bindings.
- The egui key handler queries the registry by active scope and emits the matching `AppCommand`/`ReaderCommand`.
- Focus owner changes reset shortcut routing to the appropriate scope.

### Eframe Input Flow
- `eframe` delivers key events via `Context::input`.
- The shell reads key events each frame, formats them into combos, and queries `ShortcutRegistry`.
- When a match is found, it emits the same `AppCommand`/`ReaderCommand` path used by UI buttons.
- Focus owners (modal/panel input) short-circuit processing before registry lookup.
### Phase Exit
- [x] shortcut routing is deterministic, focus-aware, and no longer relies on browser focus semantics so the egui input layer can plug into the Rust runtime command model.

## Phase 5: Modal, Notification, And Error Strategy
## Phase 5: Modal, Notification, And Error Strategy
- [x] Replace browser-style modal/dialog flows with egui-native confirmation, alert, toast, and progress surfaces.
- [x] Define the modal stack semantics:
- [x] blocking confirmations (e.g., close source without save, Calibre import overwrite) that prevent other shell interactions until resolved
- [x] information dialogs (filters, help) that sit above the shell but permit non-blocking background activity
- [x] progress overlays tied to long-running jobs (import, transcription, PDF OCR) that also expose cancel actions and show progress metrics
- [x] Define notification/toast surface rules:
- [x] Import failures and Calibre/browser-tab errors display persistent toast with retry/copy log actions.
- [x] Persistence errors and safe-quit issues surface warnings with deep links to settings or diagnostics.
- [x] Reader health issues (PDF degraded mode, sync drift) highlight in status chips but also post toasts when severity increases.
- [x] Each modal/notification must emit typed commands/events when acknowledged, canceled, or closed so the runtime can record the outcome and resume the underlying task.
- [x] Document how modal focus trapping works with the `FocusOwner` defined in Phase 4, ensuring escape/confirm semantics remain consistent across dialogs.
- [x] Ensure all blocking flows (import/transcription, persistence flush, browser-tab sync) have defined cancel/retry paths and inform the command model accordingly.

### Implementation Artifacts
- A modal contract describing layers (blocking vs passive), required metadata (title, body, actions, default focus), and the lifecycle commands emitted on open/close.
- A notification/toast registry describing severity levels, required action buttons, and how to link to diagnostics or logs.
- A set of sentinel cases (e.g., persistence failure, Calibre sync failure, PDF degraded sync) with explicit UI behavior, data sources, and the resulting command/effect transitions tracing back to Phase 2.

### Modal And Notification Contract
- Blocking confirmations: safe quit, close reader session; modal must block shortcuts and route confirm/dismiss into `AppCommand` or `CommandFailed`.
- Info dialogs: read-only help or filter details that do not block background tasks.
- Progress overlays: source open, PDF transcription, Calibre refresh, browser-tab import; include cancel actions tied to the underlying `OperationScope`.
- Toasts: surfaced via shell notifications with severity levels (info/warn/error) and optional action buttons (retry, copy logs, open diagnostics).
- Focus trapping: `FocusOwner::Modal` prevents shortcut handling until modal closes.

### Phase Exit
- [x] implementers have a single modal/notification contract for the egui shell that keeps the command layer consistent while replacing Web-style dialogs.

## Phase 6: Performance And Responsiveness Constraints
- [x] Define redraw policy for shell-level state so panel interactions and status updates do not force full reader recomposition.
- [x] Panel toggles must only touch the panel regions they own and rely on derived layout helpers (Phase 1) so heavy reader renders remain cached.
- [x] Passive status updates (notifications, runtime chips) should update minimal widgets instead of repainting the central pane.
- [x] Repeated runtime events (progress ticks, TTS heartbeats) must be coalesced at the command surface and throttled before they reach the egui frame pipeline.
- [x] Capture coalescing rules for:
- [x] playback progress updates
- [x] import/transcription status ticks
- [x] browser-tab health pings
- [x] panel diagnostics refreshes
- [x] Add tracing/metrics hooks for:
- [x] redraw frequency per frame (per region if possible)
- [x] toolbar/panel command latency from click/shortcut to effect spawn
- [x] layout recalculation hotspots when the window resizes
- [x] modal stacking depth and notification bursts
- [x] Document how these metrics plug into the tracing model described in the runtime roadmap so the egui shell and runtime share observability.
- [x] Define a “redraw budget” contract: specify the maximum allowed frame budget for each panel update and the fallback behavior (e.g., delay heavy status updates until the reader is idle).

### Implementation Artifacts
- A redraw policy doc linking shell state changes from Phases 1–5 to the expected invalidation scopes so implementers know when it is okay to use `ctx.request_repaint` or rely on `egui`’s auto-invalidation.
- A throttling/coalescing matrix that maps event sources (e.g., playback heartbeat) to their destination UI widgets, including the suggested merge interval.
- A tracing/metrics plan referencing the runtime tracing spans so the shell instrumentation can be aligned with the runtime event model.

### Redraw Policy And Coalescing Matrix
- Redraw scope: top bar + status row update on operation/notification changes; central reader content should not repaint on passive status changes.
- Panel toggles: repaint only panel regions (left/right panels) using layout helpers (`LayoutPolicy`); avoid invalidating the central reader unless layout changes.
- Coalescing rules:
- Playback progress: batch to 4–8 Hz; skip frames when the reader is idle.
- Import/transcription status: batch to 1–2 Hz; render only the status row/toast.
- Browser-tab health pings: batch to 0.5–1 Hz; update status diagnostics only.
- Panel diagnostics: update on demand or with explicit user interaction.
- Tracing hooks:
- Track frame redraw count per second; annotate with active mode and panel visibility.
- Track command latency from UI action to `RuntimeEffect` spawn via `app_command` span timings.
- Track layout recalculation events on window resize.
- Track modal stack depth + toast bursts to flag UI overload.
- Redraw budget: keep panel updates under 16ms; defer heavy diagnostics updates until `OperationScope::SourceOpen` is false.

### Phase Exit
- [x] shell performance expectations are concrete enough to review during implementation and the tracing hooks are documented so the runtime team can instrument the same metrics.

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
