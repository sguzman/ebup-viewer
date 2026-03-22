# Egui Native PDF Interaction Model

This document defines the zoom, pan/scroll, and selection interaction rules for the egui-native PDF viewer. It complements the subsystem plan and provides the tracing hooks expected by the shell performance plan.

## Zoom
- Zoom uses a discrete `PdfZoomPolicy` ladder and emits `pdf.zoom.request` spans with `previous_zoom`, `requested_zoom`, and throttle metadata.
- Zoom changes invalidate page textures but preserve overlay geometry; highlights are re-applied on the next render pass via `pdf.highlight.apply`.
- Zoom requests are throttled to avoid repeated reflows that would violate the shell redraw/coalescing budget.

## Scroll and pan
- Scroll is handled by `egui::ScrollArea` and only updates the viewport scheduler when the target page moves beyond the configured threshold.
- Repeat viewport targets are ignored unless a forced jump is issued, with `pdf.viewport.ignore` spans recording the suppression reason.
- Highlight-driven scroll requests emit `pdf.highlight.scroll` spans with `target_page`, `sentence_idx`, and `threshold_pages`.

## Selection and copy (optional)
- Selection and copy are optional commands in the scroll stack to avoid interfering with highlight jumps.
- When implemented, expected commands are `SelectTextRange`, `ClearSelection`, and `CopySelection`.
- Telemetry should use `pdf.selection.*` spans for selection updates and `pdf.copy.selection` for copy events, carrying the same `confidence_tier` fields as highlight spans.

## Confidence and degraded modes
- The UI surfaces confidence tiers (`trustworthy_text`, `mixed_fuzzy`, `ocr_required`, `render_only`) directly, rather than implying exact geometry.
- OCR-required and render-only states should always show a warning badge and link to the same tracing fields used by highlight/jump spans.
