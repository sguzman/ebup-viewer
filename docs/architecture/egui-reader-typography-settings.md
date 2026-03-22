# Egui Reader Typography And Settings Plan

This document captures Phase 6 typography and reader settings requirements for the egui reader.

## Settings ownership
- Reader settings are native egui controls and respect the shell redraw/coalescing budgets.
- Settings changes emit tracing spans that tie into the runtime command/effect model.

## Typography controls
- Font family and weight selection provide preview spans without forcing full reflow.
- Font size slider triggers throttled renderer requests and logs prior/next size.
- Line spacing, letter spacing, and word spacing update layout metadata without full redraw.
- Horizontal/vertical margin controls reuse Phase 1 layout helpers to recompute widths deterministically.
- Highlight color modes update rendering state incrementally and emit tracing fields for the runtime plan.

## Degraded behavior
- Advanced shaping/ligatures that cannot be matched in egui fall back gracefully and are logged for QA.

## Phase 6 exit condition
Reader settings behavior is explicit, instrumented, and implementable without reopening layout/performance questions.
