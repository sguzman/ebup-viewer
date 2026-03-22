# Egui Native PDF Subsystem Plan

This document defines the Rust-native PDF subsystem boundaries, instrumentation, and viewport policy used by the egui renderer. It is the decision baseline for the native PDF roadmap.

## Subsystem boundaries and contracts
- **PageRenderService**: owns page rasterization, zoom scaling, cache policy, and GPU texture upload. Emits spans `pdf.render.request` and `pdf.render.complete` including `page`, `zoom_level`, `priority`, `cache_hit`, and `duration_ms`.
- **TextExtractionService**: extracts canonical page text, applies normalization, and persists text caches. Emits `pdf.text.extract` spans with `extraction_mode` and timing plus cache hit/miss counters.
- **SyncMapBuilder**: builds sentence-to-geometry maps with traceable sentence IDs and quality tiers. Emits `pdf.sync.build` spans with `sync_map_quality` and fallback path.
- **OCRArtifactLoader**: loads OCR alignment artifacts and reports confidence/quality via `pdf.ocr.load` spans.
- **ViewportLifecycleManager**: owns viewport range, overscan range, and eviction scheduling; emits `pdf.viewport.update` spans with ranges, trigger, and throttle/coalescing metadata.
- **OverlayAndHighlightManager**: applies overlays, manages cleanup, and emits `pdf.highlight.apply` / `pdf.highlight.cleanup` spans.

## Caching and persistence responsibilities
- Rendered page textures: cached by `PageRenderService` with LRU eviction; evictions emit reason tags (`viewport`, `memory_pressure`, `oob`).
- Text caches and sync maps: persisted under `.cache/lantern-leaf/<source_hash>/content/pdf-*` using the existing cache service; flushes occur on source open, session close, and safe quit.
- Overlay geometry caches: stored in memory per session; regenerated on page/zoom changes.

## Artifact reuse and migration
- Existing PDF sync maps and OCR artifacts are reused when their signatures match the current source hash and runtime policy.
- When mismatched, the subsystem rebuilds artifacts via `TextExtractionService` and `SyncMapBuilder`, logging the fallback path and rebuild reason.

## Instrumentation expectations
- Every request/completion path emits `pdf.*` spans with fields stable enough for QA replay.
- Fallback transitions must annotate the exact tier shift: `exact` → `fuzzy` → `block` → `page` → `render_only`.
- Viewport changes carry a `trigger` field (`init`, `scroll`, `jump`, `tts`, `refresh`) to correlate with user actions.

## Rendering and zoom policy
- Zoom levels are discrete (`0.75`, `0.9`, `1.0`, `1.1`, `1.25`, `1.5`, `1.75`) with inertial damping between levels.
- Zoom change events coalesce redraws and throttle viewport updates to avoid reflows exceeding the shell redraw budget.
- Render priority ordering remains: visible pages, active TTS page, jump target page, then overscan.

## Texture upload policy
- Render requests create upload handles that describe page/zoom/priority. Upload completion emits `pdf.texture.upload` spans.
- The current implementation performs immediate uploads in egui but preserves the `UploadHandle` model for async move-off-main-thread work.

## Viewport scheduling policy
- Visible range is derived from current page and tracked with overscan windows.
- Scheduler emits `pdf.viewport.update` spans including `visible_range`, `overscan_range`, and throttle state.
- Scheduler decisions feed back into the command/effect pipeline by annotating render requests with the originating trigger.

## Overlay geometry and highlight semantics
- Highlights are page-relative rectangles tied to canonical sentence IDs and logged with `highlight.anchor` and `sync_map_quality`.
- Overlay cleanup is mandatory on sentence/page changes and emits `pdf.highlight.cleanup` spans.
- Overlay layering order: page textures → text layer → highlight overlays → modal overlays. Tracing must record the highlight layer as the highest-priority PDF overlay span.
- Render-only fallback is used only when no geometry exists; highlight anchors must be labeled `render_only` so the shell can reduce expectations.
