# PDF.js Viewer Lifecycle Audit

## Scope

- Compare the native LanternLeaf PDF viewer against stock PDF.js viewer behavior.
- Record where the custom viewer now matches proven PDF.js patterns.
- Record the remaining gaps that still matter for responsiveness.

## Viewport Virtualization

- Stock PDF.js keeps lightweight page views for the document and only renders visible or near-visible pages.
- LanternLeaf now matches this pattern with full-document page shells, visible-page rendering, overscan, and aggressive artifact eviction.
- LanternLeaf also keeps the active TTS page and explicit jump target page in the render queue even when offscreen.

## Render Queue Prioritization

- Stock PDF.js prioritizes the visible center and nearby pages before speculative work.
- LanternLeaf now uses explicit priority, medium-priority, and low-priority buckets in the viewport scheduler.
- Jump targets and active TTS targets are elevated ahead of speculative prefetch.

## Zoom Behavior

- Stock PDF.js gives immediate zoom feedback, then rerenders at the settled scale.
- LanternLeaf now mirrors that with CSS preview scaling plus deferred reraster through `renderZoom`.
- Bitmap reuse is now layered on top so evicted canvases can be restored without forcing immediate rerender.

## Text-Layer Lifecycle

- Stock PDF.js does not treat every page text layer as permanently live.
- LanternLeaf now keeps text layers scoped to visible, active, jump-target, or selection-relevant pages.
- Ordered text metadata is now derived from pdf.js text content and span style geometry before falling back to DOM measurement.

## Cancellation Semantics

- Stock PDF.js invalidates stale work as viewport intent changes.
- LanternLeaf now invalidates page-local canvas and text-layer requests on zoom changes, document close, jump-target changes, and page-leave transitions.
- Low-priority prefetch is also isolated from the active generation so stale speculative work does not keep running forever.

## Backend Precompute

- Stock PDF.js remains the interactive renderer; preprocessing should complement it rather than replace it.
- LanternLeaf now uses the Rust/Tauri layer to cache backend-derived PDF page texts and sentence-page hints.
- This improves page targeting and reduces UI hot-path rebuilding without pretending backend rasterization alone solves viewer lag.

## Adopted Stock Patterns

- Explicit page shell virtualization.
- Explicit render queue prioritization.
- Immediate visual zoom followed by settled reraster.
- Separate canvas and text-layer lifecycles.
- Cancellation of stale render work.
- Reuse of cached artifacts across viewport churn.

## Remaining Gaps

- The custom viewer still does more TTS-specific work than stock PDF.js.
- The viewer still has bespoke overlay and sentence-sync logic that PDF.js does not need.
- Large bundle chunk sizes remain a separate startup concern outside the page renderer itself.
