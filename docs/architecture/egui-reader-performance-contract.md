# Egui Reader Performance Contract

This document captures the shell redraw/coalescing constraints and how the reader renderer must respond to them.

## Shell redraw/coalescing constraints
- Reader widgets must honor the shell’s redraw budget and coalescing rules defined in `egui-app-shell-and-navigation-roadmap.md` Phase 6.
- Reader-level invalidations should be scoped to the smallest region that changed.

## Invalidation scopes
- **Sentence-level highlights:** only the affected sentence rows/spans request repaint when the canonical highlight changes.
- **Panel-induced layout changes:** panel width changes recompute reader layout using Phase 1 layout helpers without forcing a full scroll reflow.
- **In-progress runtime events:** TTS heartbeats and ingestion progress are throttled before they reach reader widgets to avoid repaint storms.

## Tracing and metrics hooks
- Reader render passes emit frame-level spans aligned with the shell tracing plan.
- Scroll/jump updates include command source, target sentence, and auto/manual markers.
- Highlight updates emit spans tied to the command/effect pipeline so diagnostics can correlate UI and runtime state.

## Scroll/jump interplay with performance throttles
- Auto-scroll only runs when the highlight leaves the viewport threshold.
- Repeated scroll targets are ignored unless forced by a user command.
- Throttle decisions are logged so QA can replay scroll suppression.

## Phase 2.5 exit condition
The reader rendering plan explicitly documents how it obeys shell performance constraints before interactive widgets are implemented.
