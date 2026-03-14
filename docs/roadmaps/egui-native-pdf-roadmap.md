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

## Phase 1: PDF Subsystem Boundaries
- [ ] Define Rust-native subsystem boundaries for:
- [ ] page raster/render service
- [ ] text extraction and page text cache
- [ ] sentence-to-geometry sync map builder
- [ ] OCR/artifact loading
- [ ] viewport/page lifecycle manager
- [ ] overlay/highlight manager
- [ ] Define how existing PDF artifacts from the current Rust side are reused or rebuilt in the egui app.
- Phase exit:
- [ ] implementers have an explicit subsystem map and ownership model for PDF work.

## Phase 2: Rendering And Viewport Model
- [ ] Define page raster/render strategy:
- [ ] page bitmap generation
- [ ] zoom scaling policy
- [ ] texture upload/cache policy for egui
- [ ] visible-page and overscan scheduling
- [ ] eviction and reuse rules
- [ ] Preserve current priorities:
- [ ] visible pages first
- [ ] active TTS page
- [ ] jump target page
- [ ] nearby overscan pages
- Phase exit:
- [ ] the egui PDF viewer has a specified virtualization and render lifecycle contract.

## Phase 3: Text Extraction And Sync Artifact Strategy
- [ ] Define Rust-native page text extraction ownership and cache persistence.
- [ ] Preserve or rebuild:
- [ ] sentence-page hints
- [ ] OCR alignment artifacts
- [ ] sentence sync map
- [ ] confidence tiers and fallback reasons
- [ ] Keep normalization parity rules for ligatures, hyphenation, duplicate glyphs, and repeated headers/footers.
- Phase exit:
- [ ] text extraction and sync-map generation are explicit for native PDF implementation.

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
