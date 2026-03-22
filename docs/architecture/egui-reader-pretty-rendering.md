# Egui Reader Pretty Rendering Plan

This document specifies how the content-block model renders in egui for Markdown/HTML/EPUB. It is the implementation plan for Phase 3 of the reader rendering roadmap.

## Rendering widgets and redraw scope
- Each content block renders as an egui widget with a stable `block_id`.
- Widgets request minimal repaint scopes by only invalidating the block whose state changed.
- Layout caches store block measurements keyed by `(block_id, width, font settings)` to avoid full recomposition.

## Supported block types
- **Paragraphs/Headings:** lazy layout with cached measurements; reflow only when width or typography changes.
- **Inline emphasis:** inline runs render independently so span-level updates do not invalidate entire paragraphs.
- **Links/anchors:** inline link runs map to canonical anchors; link clicks emit runtime commands without blocking the UI thread.
- **Images/assets:** placeholders use known aspect ratios; actual textures are loaded lazily on scroll visibility.
- **Block spacing/margins:** spacing derives from the layout helper policy so panel resizes recompute widths deterministically.
- **Footnotes/captions:** secondary content renders as collapsible summaries or detachable overlays to prevent repaint storms.

## Degraded behavior for unsupported HTML/Markdown
- Complex tables or grids degrade to a simplified row/column block with warning diagnostics.
- Unsupported inline styles fall back to plain text while preserving anchors.
- Degraded states must log a reader warning span for QA.

## Tracing integration
- Each block emits trace spans on layout recalculation and highlight updates.
- Spans carry `block_id`, `anchor_id`, and `sentence_refs` to align with the command/effect pipeline.

## Off-UI-thread conversion
- Markdown/HTML conversion runs in the runtime async pipeline before data reaches the egui renderer.
- The UI thread only consumes the prepared content-block tree.

## Phase 3 exit condition
Pretty rendering is specified without reliance on DOM/CSS execution and is aligned with shell performance constraints.
