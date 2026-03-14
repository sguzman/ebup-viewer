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
- [ ] Define top-level shell state:
- [ ] active mode (`starter`, `reader`, `loading`, `error`)
- [ ] panel visibility/exclusivity
- [ ] modal/dialog state
- [ ] global notifications
- [ ] session transition state
- [ ] Define frame regions:
- [ ] top toolbar
- [ ] optional navigation/status row
- [ ] left/right side panels
- [ ] central content pane
- [ ] overlay modal layer
- [ ] Document size-class behavior and minimum supported desktop widths.
- Phase exit:
- [ ] shell composition and panel ownership are explicit enough for implementation without revisiting layout strategy.

## Phase 2: Navigation And Mode Switching
- [ ] Recreate starter-to-reader transitions, session close, return-to-starter, and open-source flows.
- [ ] Preserve transient loading/error surfaces during source open, PDF transcription, and Calibre/browser-tab import.
- [ ] Define command routing for:
- [ ] open source
- [ ] close source
- [ ] toggle panels
- [ ] jump/search/playback actions
- [ ] quit and safe shutdown
- Phase exit:
- [ ] all high-level navigation transitions have a Rust-native owner and typed transition model.

## Phase 3: Toolbar, Panels, And Command Surface
- [ ] Rebuild the current top toolbar with deterministic width planning.
- [ ] Preserve panel exclusivity rules:
- [ ] settings vs stats remain mutually exclusive
- [ ] search and quick actions maintain current visibility expectations
- [ ] Implement a native command dispatch path from buttons, hotkeys, and context menus into Rust effects.
- [ ] Add status/diagnostic surfaces for import/transcription/runtime conditions.
- Phase exit:
- [ ] shell command surfaces match current UX behavior without WebView dependencies.

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
