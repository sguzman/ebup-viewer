# Egui Library Import And Browser Tabs Roadmap

## Objective
- [ ] Rebuild starter, recent books, local open, Calibre browsing, and browser-tab import in egui.
- [ ] Preserve current source acquisition, refresh, recents, and deletion semantics.
- [ ] Keep browser-tab import and Calibre as first-class migration scope items.

## Current-State Grounding In This Repo
- Current starter/library/import UI ownership lives in:
- `ui/src/components/StarterShell.tsx`
- `starterPanels.tsx`
- `calibreList.ts`
- `useBrowserTabs.ts`
- browser-tab state/store slices
- Runtime/service ownership already exists in Rust for:
- local source opening
- Calibre catalog/cache/thumbnails
- browser-tab cache/artifacts and Tauri command integration
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
- [ ] Define starter-mode widget structure:
- [ ] open source controls
- [ ] recents list
- [ ] Calibre browser panel
- [ ] browser-tab panel
- [ ] connection/health/status surfaces
- [ ] Define state and command boundaries for starter mode independently from reader mode.
- Phase exit:
- [ ] starter-mode layout and responsibilities are explicit for egui implementation.

## Phase 2: Local Open And Recents
- [ ] Rebuild local file open flow with native desktop affordances.
- [ ] Preserve recent entry metadata, delete behavior, and reopen behavior.
- [ ] Preserve cache-aware recent deletion semantics.
- Phase exit:
- [ ] local open and recents are fully specified in the native shell.

## Phase 3: Calibre Browser
- [ ] Rebuild Calibre list browsing, sorting, search, and open actions in egui.
- [ ] Preserve thumbnail behavior and background hydration rules.
- [ ] Define status/error UI for large-catalog loads and cache refresh conditions.
- Phase exit:
- [ ] Calibre parity requirements are explicit enough for implementation and QA.

## Phase 4: Browser-Tab Import
- [ ] Rebuild the browser-tab import flow in egui:
- [ ] service health
- [ ] window selection
- [ ] tab selection
- [ ] tab search/filter
- [ ] import action
- [ ] manual refresh/reimport if retained
- [ ] Preserve current metadata, truncation diagnostics, and cache ownership semantics.
- [ ] Keep browser-tab reopen/delete behavior explicit in the egui app.
- Phase exit:
- [ ] browser-tab import remains first-class and implementation-ready in the migration plan.

## Phase 5: Error States And Diagnostics
- [ ] Define native error/empty states for:
- [ ] no recent books
- [ ] Calibre unavailable or empty
- [ ] browser-tab service offline
- [ ] extension disconnected
- [ ] import blocked or snapshot unavailable
- [ ] source open failures
- Phase exit:
- [ ] starter/import surfaces have consistent native diagnostics behavior.

## Risks / Failure Modes
- Browser-tab import may be neglected because it spans service integration, cache, and starter UI.
- Large Calibre lists can make egui feel sluggish if pagination/filtering and redraw policy are not specified.
- Recent-delete and reopen behavior can regress if cache/persistence work is treated separately from starter UI work.

## Test / Parity Requirements
- [ ] Rust integration tests for source opening, recent deletion, and import-service behavior.
- [ ] Manual QA for starter-mode transitions, Calibre browsing, and browser-tab import/refresh/delete flows.
- [ ] Parity checks against the existing browser-tab and starter roadmaps/checklists.
- [ ] Full implementation-phase build verification excluding AppImage/RPM/DEB packaging outputs.

## Acceptance Criteria
- [ ] Starter/library/import features are fully in scope and explicitly owned in the egui migration.
- [ ] Calibre and browser-tab flows have native-egui UI and service contracts.
- [ ] Recents/local-open/delete/reopen behavior is specified without reliance on Tauri/UI code.
- [ ] No major starter/import parity decision remains open.
