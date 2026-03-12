# PDF Rendering Performance Roadmap

## Goal

- [ ] Make the native PDF reader feel as close as possible to a browser-class PDF viewer.
- [ ] Achieve very fast initial open on typical PDFs.
- [ ] Make zoom feel immediate instead of blocking.
- [ ] Keep scrolling smooth on large and complex documents.
- [ ] Preserve stable TTS highlight, jump, and auto-scroll behavior without introducing viewer jank.
- [ ] Eliminate any interaction path that still behaves like a whole-document rerender.

## Problem Statement

- [ ] The current PDF lag is not one isolated bug; it is a stacked hot path.
- [ ] Page rasterization cost is still too high on interaction paths.
- [ ] Text-layer creation and DOM weight are still too expensive.
- [ ] Zoom invalidates too much work at once.
- [ ] Too many page/text artifacts stay live simultaneously.
- [ ] Highlight/sync work still competes with rendering work.
- [ ] The viewer is still more React-driven than a high-performance PDF viewport should be.

## Success Criteria

- [ ] Initial PDF open paints the first visible page quickly on large documents.
- [ ] Zoom gives immediate visual feedback and settles without long stalls.
- [ ] Scrolling through large PDFs does not produce visible blocking or jank.
- [ ] Only visible or near-visible pages are expensive at any given time.
- [ ] TTS highlight and jump behavior do not trigger large rerender cascades.
- [ ] Runtime behavior is measurable with repeatable before/after metrics.

## Performance Contract

- [x] Treat PDF viewing as a dedicated subsystem with explicit scheduling, caching, and lifecycle ownership.
- [ ] Keep React responsible for shell composition and high-level state, not the hot render loop for PDF pages.
- [x] Never eagerly render the entire PDF document at interactive fidelity.
- [x] Treat text-layer work separately from canvas paint work.
- [ ] Ensure TTS sync/highlight consumes cached mapping artifacts where possible instead of rebuilding mappings on hot paths.

## Baseline Metrics To Capture First

- [x] Time from open request to first visible page canvas paint.
- [ ] Time from open request to first visible text layer ready.
- [x] Total pages mounted.
- [x] Total text spans mounted.
- [ ] Visible page render latency during scroll.
- [x] Zoom feedback latency.
- [x] Zoom settle reraster latency.
- [x] Number of canceled renders during active zoom/scroll.
- [ ] Time spent in PDF text-layer postprocessing.
- [ ] Time spent resolving active TTS highlight target.

## Phase 1: Instrumentation and Profiling

- [x] Add `tracing` and frontend perf marks around PDF document load.
- [x] Add `tracing` and frontend perf marks around page shell mount.
- [x] Add `tracing` and frontend perf marks around page canvas render start/end.
- [x] Add `tracing` and frontend perf marks around text-layer extraction start/end.
- [x] Add `tracing` and frontend perf marks around text-layer DOM mount.
- [x] Add `tracing` and frontend perf marks around visible-page scheduling.
- [x] Add `tracing` and frontend perf marks around zoom start/settle.
- [ ] Add `tracing` and frontend perf marks around highlight target resolution.
- [ ] Add `tracing` and frontend perf marks around autoscroll/jump requests.
- [x] Add development-only counters for live rendered pages.
- [x] Add development-only counters for live text layers.
- [x] Add development-only counters for live highlight overlays.
- [x] Add development-only counters for canceled page renders.
- [x] Add development-only counters for page cache hits/misses.
- [ ] Capture a baseline on a small text PDF.
- [ ] Capture a baseline on a large academic PDF.
- [ ] Capture a baseline on an image-heavy PDF.
- [ ] Capture a baseline on a two-column PDF.
- [ ] Capture a baseline on TTS playback over a long PDF.

## Phase 2: Viewer Ownership Refactor

- [ ] Split the current PDF pane into an explicit document model.
- [ ] Split the current PDF pane into an explicit render scheduler.
- [x] Split the current PDF pane into an explicit page registry.
- [x] Split the current PDF pane into an explicit text-layer cache.
- [ ] Split the current PDF pane into an explicit highlight/sync overlay controller.
- [ ] Make the render scheduler imperative and ref-driven rather than React-state-driven for hot paths.
- [ ] Keep React responsible for shell composition, settings, and user intent only.
- [x] Define strict page lifecycle states.
- [x] Add a `placeholder` lifecycle state.
- [x] Add a `scheduled` lifecycle state.
- [x] Add a `rendering_canvas` lifecycle state.
- [x] Add a `canvas_ready` lifecycle state.
- [x] Add a `text_ready` lifecycle state.
- [x] Add an `evicted` lifecycle state.

## Phase 3: True Page Virtualization

- [x] Keep lightweight page shells for the full document.
- [x] Render only visible pages.
- [x] Render only a small overscan band.
- [x] Render the active TTS target page even when offscreen.
- [x] Render explicit jump target pages at high priority.
- [x] Aggressively evict offscreen canvases.
- [x] Aggressively evict offscreen text layers.
- [ ] Keep overscan configurable and measurable.
- [x] Eliminate any remaining “render every page” behavior during open, scroll, or zoom.

## Phase 4: Two-Phase Zoom

- [x] Implement immediate zoom feedback using CSS transforms on visible pages.
- [x] Debounce rerasterization after zoom settles.
- [x] Reraster only visible and near-visible pages at final zoom.
- [x] Avoid rebuilding document/page registries on every zoom step.
- [x] Keep text-layer refresh separate from immediate canvas feedback.
- [x] Prioritize visual smoothness first and exact reraster second.

## Phase 5: Separate Canvas and Text-Layer Strategies

- [x] Render canvas first for visible pages.
- [x] Delay text-layer creation for non-active pages.
- [x] Keep text layers mounted only for visible pages.
- [x] Keep text layers mounted only for the active TTS page.
- [x] Keep text layers mounted only for the jump target page.
- [ ] Keep nearby page text layers mounted only when selection/search requires it.
- [x] Avoid building text layers for far-off pages simply because their shells exist.
- [x] Treat text-layer DOM size as a first-class performance budget.

## Phase 6: Page Artifact Caching

- [x] Cache native page size per page.
- [x] Cache extracted page text per page.
- [ ] Cache ordered normalized text spans per page.
- [ ] Cache sentence-to-page mapping metadata per page.
- [ ] Cache bounded raster results by zoom bucket when memory budget allows.
- [ ] Use explicit LRU eviction for page bitmaps.
- [ ] Use explicit LRU eviction for text-layer artifacts.
- [ ] Use explicit LRU eviction for normalized span metadata.
- [ ] Separate persistent caches from ephemeral in-memory caches.

## Phase 7: Render Prioritization and Cancellation

- [x] Prioritize the page in viewport center above all others.
- [x] Prioritize pages being jumped to.
- [x] Prioritize the active TTS highlight page.
- [ ] Schedule adjacent visible pages at medium priority.
- [ ] Schedule overscan neighbors at medium priority.
- [ ] Keep speculative prefetch at low priority.
- [x] Cancel stale render jobs immediately when zoom changes.
- [ ] Cancel stale render jobs immediately when a page leaves viewport.
- [x] Cancel stale render jobs immediately when jump target changes.
- [x] Cancel stale render jobs immediately when the document closes.

## Phase 8: Text-Layer Cost Reduction

- [ ] Stop rebuilding text-order/normalization data after every viewer update.
- [ ] Normalize page text once per page and reuse it.
- [ ] Minimize expensive DOM measurement and ordering work.
- [ ] Prefer deriving stable ordered text metadata from pdf.js text content before or alongside DOM creation.
- [ ] Avoid repeated `querySelectorAll` or full-page span rescans on playback updates.

## Phase 9: TTS Sync Performance Isolation

- [ ] Move TTS highlight resolution onto cached sentence-to-page artifacts wherever possible.
- [ ] Keep the runtime highlight path limited to sentence index lookup.
- [ ] Keep the runtime highlight path limited to cached page lookup.
- [ ] Keep the runtime highlight path limited to cached rect/span lookup.
- [ ] Ensure the runtime highlight path only forces render of the target page when necessary.
- [ ] Ensure the runtime highlight path paints overlay without rescanning the document.
- [ ] Prevent playback ticks from triggering document-wide matching passes.
- [ ] Keep jump-to-sentence on the same cached fast path.
- [ ] Keep auto-scroll on the same cached fast path.

## Phase 10: DOM Weight and Overlay Simplification

- [ ] Keep highlight overlays lightweight and page-local.
- [ ] Avoid wrapping or remutating large portions of text-layer DOM during playback.
- [ ] Prefer dedicated overlay layers over repeated span-tree mutation where possible.
- [ ] Cap mounted canvases.
- [ ] Cap mounted text spans.
- [ ] Cap overlay node count.

## Phase 11: Open Flow Optimization

- [ ] On open, load document metadata first.
- [ ] On open, mount page shells second.
- [ ] On open, render the current page first.
- [ ] On open, render adjacent pages second.
- [ ] Defer everything else.
- [ ] Do not block first usability on whole-document text extraction.
- [ ] Do not block first usability on whole-document highlight mapping.
- [ ] Do not block first usability on non-visible text layers.
- [ ] Define and measure time to first useful page.

## Phase 12: Jump and Navigation Optimization

- [ ] Resolve page jump targets from cached mapping.
- [ ] Resolve TTS jump targets from cached mapping.
- [ ] Schedule jump target pages at highest priority.
- [ ] Scroll only once per jump action.
- [ ] Paint highlight only once per jump action.
- [ ] Eliminate multi-step heuristic scroll/reload cycles.
- [ ] Avoid jump behavior that forces neighboring page rebuilds unless necessary.

## Phase 13: Memory Budgets

- [ ] Introduce a hard budget for live canvases.
- [ ] Introduce a hard budget for live text layers.
- [ ] Introduce a hard budget for bitmap cache size.
- [ ] Introduce a hard budget for normalized page metadata cache.
- [ ] Track and log eviction causes.
- [ ] Tune budgets for low-memory laptop usage.
- [ ] Tune budgets for high-memory desktop usage.

## Phase 14: Backend and Precompute Support

- [ ] Use the Rust/backend layer for page text extraction where it helps.
- [ ] Use the Rust/backend layer for sentence-to-page metadata where it helps.
- [ ] Use the Rust/backend layer for reusable PDF sync artifacts where it helps.
- [ ] Avoid assuming Rust-side rasterization alone solves interaction lag.
- [ ] Keep the interactive viewer optimized on the frontend even if preprocessing moves backend-side.

## Phase 15: Alignment With Stock PDF.js Viewer Behavior

- [ ] Audit the current custom viewer against stock PDF.js viewer viewport virtualization behavior.
- [ ] Audit the current custom viewer against stock PDF.js viewer render queue prioritization behavior.
- [ ] Audit the current custom viewer against stock PDF.js viewer zoom behavior.
- [ ] Audit the current custom viewer against stock PDF.js viewer text-layer lifecycle behavior.
- [ ] Audit the current custom viewer against stock PDF.js viewer cancellation semantics.
- [ ] Where the custom path is materially worse, adopt the proven PDF.js lifecycle pattern instead of inventing new behavior.
- [ ] Increase direct reuse of stock PDF.js viewer concepts if the bespoke path remains slower.

## Recommended Implementation Order

- [x] Step 1: Add instrumentation and define hard baseline metrics.
- [ ] Step 2: Refactor PDF pane ownership into scheduler/cache/page-registry layers.
- [x] Step 3: Enforce strict page virtualization and eviction.
- [x] Step 4: Implement immediate CSS zoom plus debounced reraster.
- [x] Step 5: Restrict text layers to visible/active pages only.
- [ ] Step 6: Add render cancellation and page-priority scheduling.
- [ ] Step 7: Cache page text/span artifacts and isolate TTS highlight from hot render work.
- [ ] Step 8: Tune open/jump flow and memory budgets.
- [ ] Step 9: Compare against stock PDF.js viewer behavior and adopt missing lifecycle patterns.

## Acceptance Criteria

- [ ] The first visible PDF page becomes usable quickly on large documents.
- [ ] Zoom feels immediate instead of blocking.
- [ ] Scrolling through long PDFs does not stall on page creation.
- [ ] TTS highlight and jump behavior remain stable without degrading render responsiveness.
- [ ] No interaction path causes whole-document rerendering.
- [ ] Build verification passes after implementation, excluding `deb`, `rpm`, and AppImage packaging targets.
