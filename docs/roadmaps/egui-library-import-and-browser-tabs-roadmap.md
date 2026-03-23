# Egui Library Import And Browser Tabs Roadmap

## Objective
- [x] Rebuild starter, recent books, local open, Calibre browsing, and browser-tab import in egui.
- [x] Preserve current source acquisition, refresh, recents, and deletion semantics.
- [x] Keep browser-tab import and Calibre as first-class migration scope items.

## Current-State Grounding In This Repo
- Current starter/library/import UI ownership lives in:
- `crates/lanternleaf-egui/src/app/ui/starter.rs`
- `crates/lanternleaf-egui/src/app/ui/browser_tabs.rs`
- `crates/lanternleaf-egui/src/app/ui/calibre.rs`
- browser-tab state/services in `crates/lanternleaf-app/src/services/`
- Runtime/service ownership already exists in Rust for:
- local source opening
- Calibre catalog/cache/thumbnails
- browser-tab cache/artifacts and Rust service integration
- Current import surfaces include:
- local file open
- clipboard/open-text flows where supported
- recent books
- Calibre catalog browsing
- browser-tab import through `browsr`

## Target End State Under Egui
- Starter mode is an egui-native desktop surface with dedicated widgets for:
- local open
- recent books
- Calibre browser
- browser-tab import
- import health/diagnostic status
- All import flows dispatch into Rust-native services and feed results back into Rust app state.
- Browser-tab and Calibre integrations keep their current persistence and diagnostics semantics.

## Key Architectural Decisions Already Chosen
- Browser-tab import remains in scope and first-class.
- Calibre remains in scope and first-class.
- Starter/library/import flows are part of parity and not deferred until after reader migration is “done.”
- External integrations become Rust-native services surfaced directly in egui.
- Recents and deletion behavior continue to be backed by Rust persistence/cache ownership.

## Phase 1: Starter Surface Contract
- [x] Define starter-mode widget structure:
- [x] open source controls
- [x] recents list
- [x] Calibre browser panel
- [x] browser-tab panel
- [x] connection/health/status surfaces
- [x] Define state and command boundaries for starter mode independently from reader mode.
- Phase exit:
- [x] starter-mode layout and responsibilities are explicit for egui implementation.

## Phase 2: Local Open And Recents
- [x] Rebuild local file open flow with native desktop affordances.
- [x] Preserve recent entry metadata, delete behavior, and reopen behavior.
- [x] Preserve cache-aware recent deletion semantics.
- Phase exit:
- [x] local open and recents are fully specified in the native shell.

## Phase 3: Calibre Browser
- [x] Rebuild Calibre list browsing, sorting, search, and open actions in egui.
- [x] Preserve thumbnail behavior and background hydration rules.
- [x] Define status/error UI for large-catalog loads and cache refresh conditions.
- Phase exit:
- [x] Calibre parity requirements are explicit enough for implementation and QA.

## Phase 4: Browser-Tab Import
- [x] Rebuild the browser-tab import flow in egui:
- [x] service health
- [x] window selection
- [x] tab selection
- [x] tab search/filter
- [x] import action
- [x] manual refresh/reimport if retained
- [x] Preserve current metadata, truncation diagnostics, and cache ownership semantics.
- [x] Keep browser-tab reopen/delete behavior explicit in the egui app.
- Phase exit:
- [x] browser-tab import remains first-class and implementation-ready in the migration plan.

## Phase 5: Error States And Diagnostics
- [x] Define native error/empty states for:
- [x] no recent books
- [x] Calibre unavailable or empty
- [x] browser-tab service offline
- [x] extension disconnected
- [x] import blocked or snapshot unavailable
- [x] source open failures
- Phase exit:
- [x] starter/import surfaces have consistent native diagnostics behavior.

## Risks / Failure Modes
- Browser-tab import may be neglected because it spans service integration, cache, and starter UI.
- Large Calibre lists can make egui feel sluggish if pagination/filtering and redraw policy are not specified.
- Recent-delete and reopen behavior can regress if cache/persistence work is treated separately from starter UI work.

## Test / Parity Requirements
- [x] Rust integration tests for source opening, recent deletion, and import-service behavior.
- [ ] Manual QA for starter-mode transitions, Calibre browsing, and browser-tab import/refresh/delete flows.
- [x] Parity checks against the existing browser-tab and starter roadmaps/checklists.
- [x] Full implementation-phase build verification excluding AppImage/RPM/DEB packaging outputs.

## Acceptance Criteria
- [x] Starter/library/import features are fully in scope and explicitly owned in the egui migration.
- [x] Calibre and browser-tab flows have native-egui UI and service contracts.
- [x] Recents/local-open/delete/reopen behavior is specified without reliance on legacy UI code.
- [x] No major starter/import parity decision remains open.
