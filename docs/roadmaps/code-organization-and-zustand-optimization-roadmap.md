# Code Organization and Zustand Optimization Roadmap

## Objective

- Reduce UI lag and render churn caused by broad Zustand subscriptions and whole-object store replacement.
- Break oversized frontend and Rust modules into clearer ownership boundaries.
- Remove duplicated rendering and state-transition logic that makes regressions easier to introduce.
- Improve maintainability, testability, and profiling clarity without changing the core product model:
  - pretty-text HTML/EPUB/imported-tab view
  - text-only extraction and TTS ownership
  - sentence/highlight sync between text-only and pretty-text

## Audit Summary

The codebase has working feature breadth, but several structural issues are now large enough to slow development and increase regression risk.

### Primary frontend findings

- [ui/src/components/ReaderShell.tsx](/win/linux/Code/projects/lantern-leaf/ui/src/components/ReaderShell.tsx) is effectively a god component.
  - It is currently about 2,000+ lines and owns rendering, HTML/markdown conversion, sync mapping, scrolling, highlighting, settings UI, stats UI, TTS UI, and reader interaction state.
- [ui/src/components/StarterShell.tsx](/win/linux/Code/projects/lantern-leaf/ui/src/components/StarterShell.tsx) is also oversized.
  - It mixes browser-tab import UI, calibre list UI, virtualization, thumbnail hydration, filtering, sorting, and operational status handling.
- [ui/src/App.tsx](/win/linux/Code/projects/lantern-leaf/ui/src/App.tsx) still acts as a large integration hub with many action props forwarded into `ReaderShell`.
- [ui/src/components/prettyHtml.ts](/win/linux/Code/projects/lantern-leaf/ui/src/components/prettyHtml.ts) and [ui/src/components/ReaderShell.tsx](/win/linux/Code/projects/lantern-leaf/ui/src/components/ReaderShell.tsx) contain overlapping rendering concerns.
  - Markdown rendering helpers currently live in `ReaderShell`.
  - Native HTML sanitization/rewrite logic lives in `prettyHtml.ts`.
  - The split is not by stable responsibility; it is by historical accumulation.
- High-frequency reader/TTS updates are still flowing through a broad `reader` object in the store.
  - This encourages full object replacement and broad selector invalidation.

### Primary Zustand findings

- The store keeps high-frequency, structural, and operational state in a single store object in [ui/src/store/appStore.ts](/win/linux/Code/projects/lantern-leaf/ui/src/store/appStore.ts).
- `reader` is treated as one large mutable payload across many actions.
  - [ui/src/store/slices/readerSlice.ts](/win/linux/Code/projects/lantern-leaf/ui/src/store/slices/readerSlice.ts) replaces the full `reader` snapshot for many operations.
  - [ui/src/store/slices/jobsSlice.ts](/win/linux/Code/projects/lantern-leaf/ui/src/store/slices/jobsSlice.ts) also replaces the full `reader` object on reader events.
- Selectors are still too broad for hot paths.
  - [ui/src/store/selectors.ts](/win/linux/Code/projects/lantern-leaf/ui/src/store/selectors.ts) returns large object bundles, especially `useReaderScreenState` and `useStarterScreenState`.
  - Even with `useShallow`, returning broad objects means any changed member still invalidates the subscriber.
- Operational flags are global when many should be request- or domain-scoped.
  - `busy` is app-global in [ui/src/store/appStore.ts](/win/linux/Code/projects/lantern-leaf/ui/src/store/appStore.ts), so unrelated UI can inherit loading states.
- The event ingestion layer writes directly into UI-facing store shape.
  - `ttsStateEvent`, `sourceOpenEvent`, `calibreLoadEvent`, `pdfTranscriptionEvent`, `session`, and `reader` all live in the same reactive surface.
- The store is action-centric rather than domain-model-centric.
  - Actions are split into slices, but the state model is not deeply normalized.
  - That reduces file count, but it does not isolate hot data from cold data.

### Primary Rust/backend findings

- Several backend files are too large to sustain safe iteration:
  - [src-tauri/src/lib.rs](/win/linux/Code/projects/lantern-leaf/src-tauri/src/lib.rs)
  - [src/cache.rs](/win/linux/Code/projects/lantern-leaf/src/cache.rs)
  - [crates/lanternleaf-core/src/session.rs](/win/linux/Code/projects/lantern-leaf/crates/lanternleaf-core/src/session.rs)
  - [src/normalizer.rs](/win/linux/Code/projects/lantern-leaf/src/normalizer.rs)
  - [src/epub_loader.rs](/win/linux/Code/projects/lantern-leaf/src/epub_loader.rs)
  - [src/calibre.rs](/win/linux/Code/projects/lantern-leaf/src/calibre.rs)
- The Tauri bridge layer and session orchestration layer carry too much policy and transformation logic.
- Cache, source loading, sync metadata, and runtime orchestration are strongly coupled.
- Tracing coverage exists and is generally good, but important domains still lack stable per-feature spans and event-shape metrics for performance work.

## Root Causes

- Feature delivery has been concentrated into a few high-leverage files instead of sustained module boundaries.
- The frontend store models backend payloads too directly.
- Reader data has not been separated into:
  - stable document/page structure
  - transient playback cursor state
  - UI panel/view state
  - async operation state
- Components are memoized in places, but their prop surfaces are still too large to benefit consistently.
- Multiple rendering concerns are colocated with state orchestration and effect-heavy DOM work.

## Success Criteria

- [ ] Reader playback updates do not force rerender of controls that do not depend on the active cursor.
- [ ] Zustand selectors subscribe to narrow, domain-specific state with stable equality semantics.
- [ ] `ReaderShell` and `StarterShell` are split into small, testable components and hooks with clear ownership.
- [ ] Reader state distinguishes stable document data from transient playback state.
- [ ] Event ingestion is isolated from UI presentation state.
- [ ] Large Rust files are decomposed by capability rather than by historical growth.
- [ ] Profiling can attribute lag to a specific module, selector, or event path instead of a giant integration surface.

## Phase 1: Establish Audit Baseline and Ownership Boundaries

- [x] Record current line-count hotspots and keep them visible in the roadmap issue/PR stream.
- [ ] Define target module boundaries for:
  - [x] reader rendering
  - [x] reader sync/highlight logic
  - [x] reader controls and panel UI
  - [x] starter browser-tab import UI
  - [x] calibre list and thumbnail flow
  - [x] store event ingestion
  - [ ] store reader domain state
- [ ] Add a short architecture note documenting which layer owns:
  - [x] pretty HTML rendering
  - [x] markdown rendering
  - [x] text-only sentence ownership
  - [x] TTS playback cursor
  - [x] HTML sync mapping

## Phase 2: Reshape Zustand State by Domain and Update Frequency

- [ ] Split the current monolithic app store state into explicit domains:
  - [ ] `appShell`
  - [ ] `session`
  - [ ] `readerDocument`
  - [ ] `readerPlayback`
  - [ ] `readerUi`
  - [ ] `starter`
  - [ ] `jobs`
  - [ ] `notifications`
- [ ] Stop treating `reader` as a single always-replaced snapshot for all frontend concerns.
- [ ] Introduce a normalized reader state shape:
  - [ ] stable document/page payload
  - [ ] transient playback cursor and progress
  - [ ] search state
  - [ ] panel state
  - [ ] view-mode flags
- [x] Move `ttsStateEvent` out of broad UI selector surfaces unless a component truly renders it.
- [x] Replace app-global `busy` with scoped operation flags:
  - [x] source opening
  - [x] panel/settings mutation
  - [x] reader navigation
  - [x] calibre load
  - [x] browser-tab refresh
- [ ] Ensure each store write updates only the minimum necessary domain.

## Phase 3: Narrow Selectors and Subscription Surfaces

- [x] Replace broad object selectors in [ui/src/store/selectors.ts](/win/linux/Code/projects/lantern-leaf/ui/src/store/selectors.ts) with smaller dedicated hooks.
- [x] Eliminate large selectors such as `useReaderScreenState` and `useStarterScreenState`.
- [ ] Prefer selectors that return:
  - [x] a single primitive
  - [ ] a tiny tuple
  - [x] a very small object with stable member count and clear equality
- [ ] Audit every component that currently subscribes to `reader`.
- [x] Ensure `ReaderQuickActionsDock` remains isolated from playback churn.
- [ ] Add explicit tests proving that:
  - [x] playback ticks do not rerender starter UI
  - [x] non-TTS controls do not rerender on pure TTS metadata changes
  - [ ] panel toggles do not rerender the entire reader tree

## Phase 4: Separate Event Ingestion from UI State

- [x] Refactor [ui/src/store/slices/jobsSlice.ts](/win/linux/Code/projects/lantern-leaf/ui/src/store/slices/jobsSlice.ts) into a dedicated event-ingestion module.
- [x] Convert backend listeners into domain-specific adapters that translate bridge events into minimal store mutations.
- [ ] Coalesce or discard redundant events at the ingestion boundary rather than after they hit the UI store.
- [ ] Track event rates and payload sizes for:
  - [x] `reader-state`
  - [x] `tts-state`
  - [x] `session-state`
- [x] Introduce explicit “no-op if no visible change” guards for hot events.

## Phase 5: Decompose ReaderShell into Focused Modules

- [ ] Extract from [ui/src/components/ReaderShell.tsx](/win/linux/Code/projects/lantern-leaf/ui/src/components/ReaderShell.tsx):
  - [x] `ReaderTopBar`
  - [x] `ReaderSearchBar`
  - [x] `ReaderPrettyHtmlPane`
  - [x] `ReaderPrettyMarkdownPane`
  - [x] `ReaderTextOnlyPane`
  - [x] `ReaderStatsPanel`
  - [x] `ReaderSettingsPanel`
  - [x] `ReaderTtsPanel`
  - [x] `useReaderScrollSync`
  - [x] `useReaderHighlightSync`
  - [x] `useHtmlSentenceAnchorMap`
- [x] Move markdown rendering helpers out of `ReaderShell` into their own module.
- [x] Ensure each extracted component receives only the props it actually needs.
- [x] Reduce inline object creation and large effect dependency sets where those dependencies only exist because too much logic is colocated.

## Phase 6: Decompose StarterShell and Async Thumbnail Flow

- [ ] Split [ui/src/components/StarterShell.tsx](/win/linux/Code/projects/lantern-leaf/ui/src/components/StarterShell.tsx) into:
  - [x] `StarterOpenPanel`
  - [x] `StarterBrowserTabsPanel`
  - [x] `StarterRecentsPanel`
  - [x] `StarterCalibrePanel`
  - [x] `useBrowserTabs`
  - [x] `useCalibreThumbnails`
  - [ ] virtualization helpers colocated with their owning list
- [x] Move browser health and browser-tab loading into focused hooks with explicit loading/error state.
- [x] Batch thumbnail updates and keep them local to the calibre list domain.
- [x] Avoid list-wide rerender when one row thumbnail changes.

## Phase 7: Remove Duplicated Rendering and Sync Logic

- [ ] Consolidate content rendering responsibilities:
  - [ ] native HTML sanitization/rewrite
  - [ ] markdown-to-HTML rendering
  - [x] HTML anchor extraction
  - [x] sentence-to-anchor mapping
- [x] Remove rendering helpers from [ui/src/components/ReaderShell.tsx](/win/linux/Code/projects/lantern-leaf/ui/src/components/ReaderShell.tsx) that belong in dedicated utility modules.
- [x] Define one module that owns HTML sync contracts end to end.
- [x] Add tests that assert sync logic independently of the full reader component tree.

## Phase 8: Backend Contract and Payload Optimization

- [ ] Review the bridge contract so it does not require replacing the full reader snapshot for transient playback movement.
- [ ] Split backend payloads into:
  - [ ] document/page structure
  - [ ] playback cursor/progress
  - [ ] settings/panels
  - [ ] operational events
- [ ] Reduce frequency of heavy reader snapshot emission from the backend when only playback cursor changed.
- [ ] Add tracing fields for:
  - [x] snapshot size
  - [x] snapshot emission rate
  - [x] playback update rate
  - [x] page-load vs cursor-move distinction

## Phase 9: Rust Module Decomposition

- [ ] Split [src-tauri/src/lib.rs](/win/linux/Code/projects/lantern-leaf/src-tauri/src/lib.rs) by runtime capability:
  - [ ] bootstrap/config
  - [ ] source open commands
  - [ ] reader commands
  - [ ] TTS runtime orchestration
  - [ ] browser-tab commands
  - [ ] shutdown/persistence
- [ ] Split [crates/lanternleaf-core/src/session.rs](/win/linux/Code/projects/lantern-leaf/crates/lanternleaf-core/src/session.rs) by session domain:
  - [ ] document loading
  - [ ] pagination and page changes
  - [ ] playback/cursor movement
  - [ ] settings mutations
  - [ ] session transitions
- [ ] Split [src/cache.rs](/win/linux/Code/projects/lantern-leaf/src/cache.rs) by artifact type:
  - [x] bookmarks/config
  - [x] dual-view artifacts
  - [ ] browser-tab assets
  - [x] TTS cache
  - [x] normalized artifacts
- [ ] Split [src/epub_loader.rs](/win/linux/Code/projects/lantern-leaf/src/epub_loader.rs) by source type and artifact pipeline.
- [ ] Split [src/calibre.rs](/win/linux/Code/projects/lantern-leaf/src/calibre.rs) into:
  - [x] catalogue loading
  - [x] cache handling
  - [x] thumbnail pipeline
  - [x] EPUB cover extraction

## Phase 10: Performance Instrumentation and Regression Protection

- [x] Add dev-only counters for selector invalidation frequency by component.
- [x] Add tests around store write granularity.
- [x] Add a profiling script for long-running playback on:
  - [x] large EPUB
  - [x] imported browser tab
  - [x] image-heavy HTML
- [ ] Add guardrail tests for reader state decomposition:
  - [x] document state unchanged on pure playback move
  - [x] panel toggle does not invalidate document payload
  - [x] TTS event does not force starter-screen rerender

## Recommended Order

- [x] Step 1: Reshape Zustand state by domain and frequency.
- [x] Step 2: Replace broad selectors with narrow domain hooks.
- [x] Step 3: Extract `ReaderShell` hooks/components so selector work has somewhere smaller to land.
- [x] Step 4: Decompose `StarterShell` and thumbnail/browser-tab flows.
- [x] Step 5: Clean up duplicated rendering/sync utilities.
- [ ] Step 6: Adjust backend event/payload shapes to match the new frontend state model.
- [ ] Step 7: Split the largest Rust modules along capability boundaries.

## Expected Payoff

- Lower rerender frequency during playback.
- Smaller blast radius for reader and HTML sync regressions.
- Easier diagnosis of highlight, scroll, and TTS bugs.
- Cleaner module ownership across frontend and backend.
- Better long-term velocity because fewer features require editing the same giant files.

## Suggested Non-Semver Epic Title

- `Audit and restructure reader, store, and runtime hot paths for maintainability and responsiveness`
