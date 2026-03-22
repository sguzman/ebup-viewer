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
- [x] Define the Rust-native services and instrumentation contracts for:
- [x] `PageRenderService`: page raster generation, zoom scaling, caching, and texture uploading; emits spans for render requests, cache hits/misses, and upload latency aligned with the shell performance tracing plan.
- [x] `TextExtractionService`: canonical page text extraction, normalization, and persistence/ cache updates; logs fallback decisions (exact vs fuzzy vs block) and surfaces text quality tiers to the instrumentation schema.
- [x] `SyncMapBuilder`: sentence-to-geometry and overlay metadata creation; keeps traceable references to the originating sentence ID for highlight sync in the overlay manager.
- [x] `OCRArtifactLoader`: optional OCR supplementation, artifact ingestion, and fallback labeling; reports confidence events to the shared tracing schema (match `highlight.anchor` tags).
- [x] `ViewportLifecycleManager`: page lifecycle, virtualization, eviction, and overscan scheduling; enforces the shell’s redraw budget by exposing APIs for requesting frames only when visibility warrants it.
- [x] `OverlayAndHighlightManager`: overlay geometry application, cleanup, and click/jump handling; emits spans tied to the shell command/effect model for diagnostics.
- [x] Define caching/persistence responsibilities (render images, text caches, sync maps) and how they flush during shell events (source open, close, safe quit) while emitting structured logs for QM.
- [x] Define how existing PDF artifacts from the current Rust side are reused or rebuilt in the egui app, including migration paths for cached sync maps and highlight geometry.
- [x] Document instrumentation expectations: each subsystem must raise telemetry for request start/completion, cache behavior, fallback reason, and error path so the tracing plan can correlate PDF behavior with shell performance.
### Phase Exit
- [x] implementers have an explicit subsystem map, service API definitions, and tracing requirements for each PDF component before implementation begins.

## Phase 2: Rendering And Viewport Model
- [x] Define page raster/render strategy with instrumentation that ties into the shell’s redraw and coalescing plan:
- [x] `PageRenderService` exposes `request_render(page_id, zoom_level, priority)` and emits spans `pdf.render.request`/`pdf.render.complete` containing zoom, priority, and cache_hit metadata so shell telemetry can gate frame requests.
- [x] Zoom scaling policy includes discrete zoom levels, smooth transitions, and inertial zoom damping; zoom change events must throttle repaint requests per the shell coalescing rules to prevent repeated reflows.
- [x] Texture upload/cache policy handles GPU/egui texture creation with async `UploadHandle` futures; cache hits/misses emit spans and inform the virtualization scheduler whether to reuse existing textures or re-render.
- [x] Visible-page and overscan scheduling rely on a virtualization scheduler service that tracks viewport extent and emits spans `pdf.viewport.update` with visible_range, overscan_range, and activation triggers (scroll, jump, navigation). Scheduler decisions must respect the shell redraw budget, only requesting renders if viewport changes exceed configurable deltas.
- [x] Eviction and reuse rules define when textures/textures are released or reused; eviction operations emit structured logs with reason (`viewport`, `memory_pressure`, `oob`) and coordinate with the tracing plan to prevent audit gaps.
- [x] Preserve current priority ordering (visible, active TTS sentence page, jump target, overscan) but also allow urgent commands (e.g., highlight jumps) to preempt the scheduler with documented guard rails.
- [x] Document how virtualization decisions feed back into the command/effect pipeline so page renders triggered by auto-scroll or reader navigation are traceable to the originating `ShellCommand`.
- Phase exit:
- [x] the egui PDF viewer has a virtualization lifecycle contract, with rendering and viewport scheduling tightly instrumented and obeying the shell redraw/coalescing constraints.

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
- [x] Define page-relative overlay geometry for highlights as the stable rendering primitive tied to canonical sentence anchors and the tracing schema:
- [ ] Each highlight span carries the originating sentence ID, page_id, and sync_map_quality so the overlay manager can emit `pdf.highlight.apply` spans with `highlight.anchor` metadata aligned to reader tracing.
- [ ] Preserve downgrade rules (sentence → block → page → render-only) and log the downgrade reason per overlay so QA can correlate with past instability classes.
- [ ] Overlay cleanup rules must ensure stale highlights and previous sentence rectangles are removed before new spans render; these transitions emit `pdf.highlight.cleanup` events with cleanup reason (new sentence, page change, closing source).
- [ ] Jump and click behavior:
- [x] highlight updates triggered by TTS playback or sentence focus emit commands `JumpToSentence` with canonical index and tracer-friendly fields; highlight spans follow the same instrumentation (see `crates/lanternleaf-egui/src/main.rs` for the JumpToSentence spans that now carry `anchor_path`, overlay diagnostics, and the simplified PDF preview).
- [ ] Mouse clicks on overlays reverse-map geometry to the canonical sentence; emit `pdf.highlight.click` spans containing both geometry and sentence metadata before forwarding the command to the runtime pipeline.
- [x] Surface overlay budget pressure events (native render spans and evictions) inside the diagnostics panel so QA can replay shell.performance_budget traces when the budget is contended.
- [ ] Avoid reusing random full-page fallbacks unless no better geometry is present; document how the overlay manager detects “render-only” state and reports it via tracing/diagnostics so the shell reduces expectation.
- [ ] Define overlay layering rules so the reader and shell stay in sync (e.g., highlight overlays render above page textures but below modal overlays), and the tracer records which layer produced the highest priority spans.
- [ ] Phase exit:
- [ ] highlight geometry, cleanup, and jump semantics are documented, instrumented, and aligned with the canonical sentence anchors so the native overlay manager can be implemented without re-opening behavior debates.

## Phase 5: Zoom, Scroll, And Interaction Model
- [ ] Define egui-native zoom/pan/scroll behavior tied to the virtualization scheduler and shell redraw/coalescing/tracing contracts:
- [ ] Zoom behavior must integrate with `PageRenderService` zoom policies; zoom commands emit spans `pdf.zoom.request` and trigger scheduler updates without racing repeated repaints (throttle per shell coalescing settings).
- [ ] Pan/scroll is handled via native `egui::ScrollArea` but only updates the virtualization scheduler when thresholds are exceeded to honor redraw budgets. Scheduler emits `pdf.viewport.update` spans describing visible range changes that include reason (`scroll`, `jump`, `auto-scroll`).
- [ ] Preserve anti-jitter rules by comparing requested view positions against the last committed viewport; identical targets are ignored unless forced by user commands, and any ignored repeats are logged for diagnostics.
- [ ] Scroll requests spawned by reader jumps or highlight movement only proceed when the canonical sentence index moves outside the viewport threshold defined in the virtualization scheduler; this logic emits tracing spans `pdf.highlight.scroll` with sentence metadata to align with the instrumentation plan.
- [ ] Zoom/pan interactions must avoid resetting highlights; document how overlays re-apply after render passes and mention that overlay updates emit spans tracked by the shell instrumentation.
- [ ] Selection policy and optional copy support should be optional nodes on the scroll stack; describe them in terms of commands (e.g., `SelectTextRange`, `CopySelection`) and link to tracing commands for telemetry.
- [ ] Phase exit:
- [ ] viewport behaviors, anti-jitter guards, and interaction commands are concrete, tied to the scheduler, and documented for tracing before building the egui viewer.

## Phase 6: OCR And Quality Modes
- [ ] Preserve the current PDF classification/runtime policy model and align it with tracing/command expectations:
- [ ] Define when OCR runs (pre-render, post-render, on-demand) and how `OCRArtifactLoader` emits spans `pdf.ocr.run/start/complete` with source, confidence, and duration so the shell can correlate heavy jobs with viewport events.
- [ ] Map confidence tiers (`trustworthy_text`, `mixed_fuzzy`, `ocr_required`, `render_only`) to explicit telemetry fields used by both the reader renderer and shell instrumentation to annotate highlight and search results.
- [ ] Ensure degraded behaviors are traceable and actionable:
- [ ] Trustworthy text uses canonical sentence geometry; spans should annotate `highlight.anchor=exact`.
- [ ] Mixed/fuzzy text escalates fallback spans and logs the chosen geometry fallback path when voiceover tries to highlight sentences.
- [ ] OCR-required mode emits UI-visible warnings and tracing breadcrumbs warning about missing high-confidence text, linking to the OCR job that produced the fallback geometry.
- [ ] Render-only/no-sync states emit spans indicating that highlight sync is disabled, but the scroll/page rendering still obeys the virtualization scheduler.
- [ ] Document how logs/UI present confidence rather than pretending the geometry is exact, tying badge states or toasts back to the same tracing fields.
- [ ] Phase exit:
- [ ] OCR integration, confidence tiers, and degraded behaviors are explicitly mapped to tracing/command diagnostics matching the rest of the egui roadmap before implementation.

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
