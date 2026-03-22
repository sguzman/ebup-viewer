# Egui Reader Scroll And Interaction Semantics

This document defines Phase 5 behavior for scroll ownership, JumpToSentence routing, and link handling in the egui reader.

## Native scroll ownership
- The reader owns scroll state via `egui::ScrollArea` and never delegates to browser APIs.
- Scroll actions honor shell redraw/coalescing budgets from the app shell roadmap.

## JumpToSentence command flow
- Navigation commands emit `JumpToSentence` with canonical sentence indices.
- Auto-scroll triggers only when the highlight leaves the viewport threshold.
- Center-tracking vs. top-aligned modes are explicit settings, with tracing fields so telemetry can correlate perf spikes.
- Jitter prevention compares canonical highlight indices before requesting a new scroll frame.

## Link behavior
- Internal anchors reuse the canonical anchor map to produce deterministic scroll targets.
- External links emit shell commands that open the system browser and log telemetry for QA.
- Missing anchor targets degrade to a no-op with warning diagnostics.

## Unified interaction routing
- Selection, search navigation, and reader interactions flow through the same JumpToSentence command path.

## Phase 5 exit condition
All reader interactions have native-egui semantics, obey shell performance instrumentation, and expose parity rules for QA.
