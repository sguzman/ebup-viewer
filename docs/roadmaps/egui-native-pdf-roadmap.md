# Egui Native PDF Roadmap

## Objective
- [ ] Replace the current pdf.js/WebView-based PDF reader with a Rust-native PDF subsystem integrated into egui.
- [ ] Preserve existing PDF quality contracts, degraded modes, text ownership rules, and playback/search/highlight semantics.
- [ ] Deliver native PDF rendering, zoom, viewport management, text extraction, and overlay sync without relying on Tauri or browser technology.

## Current-State Grounding In This Repo
- Current PDF behavior is defined across:
- `ui/src/components/ReaderPrettyPdfPane.tsx`
- `pdfTextSync.ts`
- `pdfTextLayer.ts`
- `pdfOverlayGeometry.ts`
- `pdfOverlayDom.ts`
- `pdfHighlightController.ts`
- `pdfViewportScheduler.ts`
- `src-tauri/src/reader_commands.rs`
- Existing docs already define the core contracts:
- `docs/architecture/pdf-renderer-contract.md`
- `docs/roadmaps/native-pdf-rendering-and-text-sync-roadmap.md`
- `docs/qa/native-pdf-reader-checklist.md`
- Current implementation characteristics:
- rendered PDF is browser/pdf.js driven
- text sync depends on page text, text layer spans, cached sync maps, and OCR alignment artifacts
- quality modes already exist (`high_text_trust`, `mixed_text_trust`, `ocr_required`, `render_only_no_sync`)

## Target End State Under Egui
- PDF pages render natively in Rust into textures/images shown in egui.
- Viewport, zoom, and page virtualization are owned by Rust.
- Text extraction, sentence mapping, overlays, and click/jump behavior are Rust-native.
- Existing PDF sync quality classes and degraded behaviors remain intact.
- Canonical text/search/TTS ownership remains in extracted plain text, not PDF visual order.

## Key Architectural Decisions Already Chosen
- Rust-native PDF rendering is the primary target.
- PDF remains a first-class subsystem with separate migration and parity gates.
- The quality-contract hierarchy from current docs is preserved:
- exact sentence geometry
- fuzzy sentence geometry
- block fallback
- page-level location
- render-only with no sync
- The roadmap must recommend a primary Rust-native rendering stack and lock fallback criteria.

## Decisioned Evaluation: Renderer Strategy
- [ ] Primary recommendation: adopt a Rust-native PDF stack that separates:
- [ ] page raster/rendering
- [ ] text extraction/layout metadata
- [ ] optional OCR augmentation
- [ ] egui texture presentation
- [ ] Evaluation target for the primary path:
- [ ] one crate or a pair of crates can provide deterministic page rastering plus reliable enough text extraction for sync ownership
- [ ] page render output can be cached and uploaded into egui textures efficiently
- [ ] licensing and maintenance are acceptable for long-term desktop shipping
- [ ] Explicit fallback criteria for deviating from a single-stack Rust-native approach:
- [ ] text extraction quality is insufficient for current PDF sync contracts
- [ ] rendering fidelity/performance is not competitive on representative PDFs
- [ ] viewport memory behavior is not controllable enough for multi-page documents
- [ ] If no single Rust crate satisfies both rendering and text extraction:
- [ ] split rendering and text extraction into separate Rust-owned components
- [ ] keep all integration native and in-process
- [ ] do not reintroduce browser/WebView ownership as a fallback
- [ ] Align PDF rendering instrumentation with the egui shell performance and tracing contracts (shell roadmap Phase 6):
- [ ] page render requests must respect the same coalescing/back-pressure rules as panel state updates to avoid frame drops.
- [ ] highlight overlay updates must emit tracing spans (command, sentence, anchor) so the shell instrumentation sees the same state transitions as it does for text and reader interactions.
- [ ] Text sync/resync cycles should log the fallback path taken (exact/mixed/block/page) in the shared tracing schema so QA can correlate highlight jumps with PDF metrics.

## Phase 1: PDF Subsystem Boundaries
- [ ] Define the Rust-native services and instrumentation contracts for:
- [ ] `PageRenderService`: page raster generation, zoom scaling, caching, and texture uploading; emits spans for render requests, cache hits/misses, and upload latency aligned with the shell performance tracing plan.
- [ ] `TextExtractionService`: canonical page text extraction, normalization, and persistence/ cache updates; logs fallback decisions (exact vs fuzzy vs block) and surfaces text quality tiers to the instrumentation schema.
- [ ] `SyncMapBuilder`: sentence-to-geometry and overlay metadata creation; keeps traceable references to the originating sentence ID for highlight sync in the overlay manager.
- [ ] `OCRArtifactLoader`: optional OCR supplementation, artifact ingestion, and fallback labeling; reports confidence events to the shared tracing schema (match `highlight.anchor` tags).
- [ ] `ViewportLifecycleManager`: page lifecycle, virtualization, eviction, and overscan scheduling; enforces the shell’s redraw budget by exposing APIs for requesting frames only when visibility warrants it.
- [ ] `OverlayAndHighlightManager`: overlay geometry application, cleanup, and click/jump handling; emits spans tied to the shell command/effect model for diagnostics.
- [ ] Define caching/persistence responsibilities (render images, text caches, sync maps) and how they flush during shell events (source open, close, safe quit) while emitting structured logs for QM.
- [ ] Define how existing PDF artifacts from the current Rust side are reused or rebuilt in the egui app, including migration paths for cached sync maps and highlight geometry.
- [ ] Document instrumentation expectations: each subsystem must raise telemetry for request start/completion, cache behavior, fallback reason, and error path so the tracing plan can correlate PDF behavior with shell performance.
### Phase Exit
- [ ] implementers have an explicit subsystem map, service API definitions, and tracing requirements for each PDF component before implementation begins.

## Phase 2: Rendering And Viewport Model
- [ ] Define page raster/render strategy with instrumentation that ties into the shell’s redraw and coalescing plan:
- [ ] `PageRenderService` exposes `request_render(page_id, zoom_level, priority)` and emits spans `pdf.render.request`/`pdf.render.complete` containing zoom, priority, and cache_hit metadata so shell telemetry can gate frame requests.
- [ ] Zoom scaling policy includes discrete zoom levels, smooth transitions, and inertial zoom damping; zoom change events must throttle repaint requests per the shell coalescing rules to prevent repeated reflows.
- [ ] Texture upload/cache policy handles GPU/egui texture creation with async `UploadHandle` futures; cache hits/misses emit spans and inform the virtualization scheduler whether to reuse existing textures or re-render.
- [ ] Visible-page and overscan scheduling rely on a virtualization scheduler service that tracks viewport extent and emits spans `pdf.viewport.update` with visible_range, overscan_range, and activation triggers (scroll, jump, navigation). Scheduler decisions must respect the shell redraw budget, only requesting renders if viewport changes exceed configurable deltas.
- [ ] Eviction and reuse rules define when textures/textures are released or reused; eviction operations emit structured logs with reason (`viewport`, `memory_pressure`, `oob`) and coordinate with the tracing plan to prevent audit gaps.
- [ ] Preserve current priority ordering (visible, active TTS sentence page, jump target, overscan) but also allow urgent commands (e.g., highlight jumps) to preempt the scheduler with documented guard rails.
- [ ] Document how virtualization decisions feed back into the command/effect pipeline so page renders triggered by auto-scroll or reader navigation are traceable to the originating `ShellCommand`.
- Phase exit:
- [ ] the egui PDF viewer has a virtualization lifecycle contract, with rendering and viewport scheduling tightly instrumented and obeying the shell redraw/coalescing constraints.

## Phase 3: Text Extraction And Sync Artifact Strategy
- [ ] Define Rust-native page text extraction ownership, normalization, cache persistence, and tracing invariants:
- [ ] `TextExtractionService` emits spans `pdf.text.extract` with page_id, extraction_mode (exact/mixed/block), and extraction duration so QA can trace quality tiers.
- [ ] Text caches (per page and per highlight) log hits/misses and emit structured logs so the shell can correlate cache usage with redraw/coalescing events.
- [ ] Sentence-page hints are captured with canonical sentence IDs; missing hints trigger fallback spans (`pdf.text.fallback`) with the fallback reason (`ocr`, `block`, `page`).
- [ ] OCR alignment artifacts are treated as augmentation only when text extraction reports low confidence; their ingestion emits spans `pdf.ocr.load` with confidence metadata.
- [ ] Sentence sync maps carry the confidence tier and geometry fallback path; transitions between tiers are logged so highlight jumps can explain themselves.
- [ ] Normalization parity rules (ligatures, hyphenation, duplicate glyph suppression, repeated headers/footers) are documented, and each normalization step logs whether it changed the canonical sentence string.
- [ ] Define fallback hierarchy explicitly (exact sentence geometry > fuzzy sentence geometry > block fallback > page-level location > render-only/no-sync) and document how each transition is recorded in tracing fields so the runtime can observe when geometry quality degrades.
- [ ] Define how serialization/persistence of text caches/sync maps flushes during shell lifecycle events (open, close, quit) and what metrics/logs are emitted to verify persistence success.
- Phase exit:
- [ ] text extraction, normalization, caches, and sync maps are explicit, traced, and ready for native implementation.

## Phase 4: Highlight, Overlay, And Jump Semantics
- [ ] Define page-relative overlay geometry for highlights as the stable rendering primitive.
- [ ] Preserve explicit downgrade rules from sentence -> block -> page fallback.
- [ ] Define cleanup rules so stale overlays and previous-sentence highlights are always removed.
- [ ] Define click/jump behavior:
- [ ] jump to current spoken sentence
- [ ] reverse navigation from clicked page/text region to canonical sentence
- [ ] no random full-page fallback reuse unless the current sentence genuinely lacks better geometry
- Phase exit:
- [ ] highlight and jump behavior are precise enough to prevent current PDF instability classes.

## Phase 5: Zoom, Scroll, And Interaction Model
- [ ] Define egui-native zoom/pan/scroll behavior for PDFs.
- [ ] Preserve current anti-jitter rules:
- [ ] no repeated recenters on same location
- [ ] scroll only on location changes or explicit jump
- [ ] highlight survives rerender/zoom cycles without changing target unexpectedly
- [ ] Define selection policy and optional copy support if retained.
- Phase exit:
- [ ] viewport and interaction semantics are decision-complete.

## Phase 6: OCR And Quality Modes
- [ ] Preserve current PDF classification/runtime policy model.
- [ ] Define where OCR runs in the Rust-native app lifecycle.
- [ ] Keep explicit degraded behavior for:
- [ ] trustworthy embedded text
- [ ] mixed/fuzzy text trust
- [ ] OCR-required text
- [ ] render-only/no-sync state
- [ ] Ensure logs and UI communicate confidence rather than faking exactness.
- Phase exit:
- [ ] PDF quality and degraded-mode semantics match current roadmap contracts.

## Risks / Failure Modes
- Rust-native PDF rendering may lag behind browser-quality fidelity if rendering evaluation is rushed.
- Sync quality can regress if extraction and rendering stacks diverge too far semantically.
- Egui texture/memory churn can hurt large-document performance if page lifecycle rules are underspecified.
- OCR-heavy PDFs can dominate schedule if mixed into the base rendering work instead of treated as an explicit track.

## Test / Parity Requirements
- [ ] Rust tests for sync artifact normalization and confidence scoring.
- [ ] Rust integration tests for viewport scheduling, zoom, and overlay lifecycle.
- [ ] Representative manual QA on structured, multi-column, OCR-heavy, rotated, and header/footer-heavy PDFs.
- [ ] Explicit parity gate against current PDF reader checklist.
- [ ] Full implementation-phase build verification excluding AppImage/RPM/DEB packaging outputs.

## Acceptance Criteria
- [ ] A Rust-native PDF rendering/sync strategy is fully specified without WebView fallback.
- [ ] Existing PDF quality contracts and degraded modes remain intact.
- [ ] Page rastering, text extraction, overlay sync, and jump behavior all have explicit Rust-native owners.
- [ ] The roadmap is decision-complete enough to start implementation and crate evaluation immediately.
