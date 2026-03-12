# PDF OCR Geometry-Aware Alignment Roadmap

## Objective
- [ ] Build a dedicated OCR geometry pipeline for scanned and OCR-heavy PDFs that can align canonical `tts_text` sentences back onto the native PDF render with explicit confidence and stable visual behavior.
- [x] Keep `tts_text` as the only owner of playback, search, bookmarks, resume, and sentence indexing.
- [x] Treat OCR geometry as a probabilistic alignment surface, never as guaranteed truth.
- [x] Support degraded but honest behavior for PDFs where OCR text exists but sentence-level geometry is weak.

## Success Criteria
- [ ] High-quality scans can highlight the currently spoken sentence at the correct on-page location.
- [ ] Medium-quality scans can degrade to stable line/block highlighting without visibly wrong sentence jumps.
- [ ] Raw scans with unusable OCR never fake precise geometry and remain renderable with clearly gated or degraded TTS sync.
- [x] The reader preserves deterministic reopen, bookmark, search, and playback behavior across OCR cache reuse and rebuilds.

## PDF Classes and Guarantees
- [x] Define explicit OCR geometry quality classes:
- [x] `ocr_high_trust`: OCR text and page geometry are stable enough for sentence-level mapping.
- [x] `ocr_mixed_trust`: OCR text is usable, but geometry requires line/block fallback.
- [x] `ocr_text_only`: OCR text is usable for Text-only/TTS, but geometry is too weak for reliable highlight sync.
- [x] `ocr_failed_or_unusable`: OCR did not produce trustworthy text for TTS ownership.
- [x] Define allowed guarantees by class:
- [x] `ocr_high_trust` -> sentence-level highlight with multi-rect support.
- [x] `ocr_mixed_trust` -> line/block highlight only, with downward-only fallback.
- [x] `ocr_text_only` -> text-only/TTS/search allowed, native PDF highlight disabled or page-level only.
- [x] `ocr_failed_or_unusable` -> native PDF render only, text/TTS gated.

## Phase 1: OCR Output Contract
- [ ] Define a canonical OCR output contract independent of the renderer:
- [ ] recognized text blocks
- [ ] recognized lines
- [ ] recognized words or tokens
- [ ] page-local bounding boxes for each level
- [ ] reading-order indices at block and line granularity
- [ ] confidence per word, line, block, and page
- [x] source markers distinguishing embedded text, OCR text, and mixed merged text
- [x] Persist the contract in cache as a versioned OCR geometry artifact.
- [x] Add tracing for OCR engine, OCR mode, per-page confidence summary, and total recognized token counts.

## Phase 2: OCR Text Ownership and Normalization
- [ ] Derive canonical `tts_text` from OCR output without losing page-local sentence provenance.
- [ ] Define OCR normalization parity rules:
- [ ] broken line joins
- [ ] hyphenated word recovery
- [ ] ligatures and OCR unicode noise
- [ ] repeated headers and footers
- [ ] margin note and sidenote suppression or segregation
- [ ] table cell reading-order normalization
- [ ] footnote marker and citation handling
- [ ] punctuation repair only when confidence permits
- [ ] Keep a sentence-to-source-token trail so each `tts_text` sentence can be traced back to OCR blocks/lines/words.
- [ ] Add tracing for normalization edits, dropped noise, merged lines, and repeated boilerplate suppression.

## Phase 3: OCR Reading Order Recovery
- [ ] Build a page reading-order resolver for OCR-heavy pages:
- [ ] single-column pages
- [ ] strong two-column pages
- [ ] mixed column + full-width caption bands
- [ ] bottom footnote bands
- [ ] outer-margin sidenotes
- [ ] tables and grid-like content
- [ ] rotated pages
- [ ] rotated blocks within otherwise upright pages
- [ ] figures and captions separated from body order
- [ ] Keep reading-order decisions explicit and persisted per page.
- [ ] Add diagnostics for why a page was classified as single-column, multi-column, table-like, caption-banded, or fallback.

## Phase 4: OCR Geometry Alignment Artifact
- [x] Define the canonical OCR alignment artifact:
- [x] `sentence_idx -> page_idx + rects[] + line_rects[] + block_rects[]`
- [x] support one sentence spanning multiple lines
- [x] support one sentence spanning multiple OCR blocks
- [ ] support one sentence crossing column boundaries only when confidence justifies it
- [x] store confidence and fallback reason per mapping
- [ ] store contributing OCR token ids for auditability
- [x] Build the artifact deterministically from canonical `tts_text` and OCR geometry, not from renderer DOM guesses alone.
- [x] Persist the artifact alongside OCR text output and cache signatures.

## Phase 5: Matching and Confidence Model
- [ ] Implement OCR sentence matching tiers:
- [ ] exact token-chain alignment
- [ ] normalized sentence alignment
- [ ] line-window fuzzy alignment
- [ ] block fallback alignment
- [ ] page-only fallback
- [ ] missing/unmappable
- [ ] Add score components for:
- [ ] text similarity
- [ ] reading-order continuity
- [ ] page continuity
- [ ] geometry compactness
- [ ] OCR confidence
- [ ] distance penalties for visually implausible matches
- [ ] Require fallback to move only downward in confidence.
- [ ] Reject matches that jump across distant regions when a weaker but local fallback exists.

## Phase 6: Native PDF Overlay Contract
- [x] Render OCR-derived highlight overlays directly on top of the native PDF viewport.
- [x] Prefer rect-based overlays over text-layer span classes for OCR-first PDFs.
- [x] Support:
- [x] sentence-level multi-rect overlays
- [x] line-level fallback overlays
- [x] block-level fallback overlays
- [x] page-active fallback when geometry is too weak
- [ ] Ensure overlay alignment remains stable during:
- [ ] zoom changes
- [ ] page resize
- [ ] scroll
- [ ] rotation
- [ ] DPI changes
- [ ] rerender/rebind cycles
- [x] Remove stale overlays cleanly on page or sentence changes.

## Phase 7: OCR Search, Cursor, and Resume Semantics
- [x] Keep search, bookmarks, and resume owned by canonical `tts_text`.
- [x] Persist OCR-specific location metadata in bookmarks:
- [x] page index
- [x] rect set
- [x] confidence tier
- [x] fallback reason
- [x] OCR token lineage if available
- [x] Ensure reopening a scanned PDF restores the nearest stable cursor even if OCR alignment was rebuilt.
- [x] Support reverse navigation from OCR overlay clicks back to the nearest canonical sentence.

## Phase 8: Engine Strategy and Fallback Policy
- [x] Decide and document OCR engine policy:
- [x] embedded text only
- [x] OCR only
- [x] hybrid embedded + OCR merge
- [x] Add engine fallback rules:
- [x] native text -> OCR fallback
- [x] OCR retry with more aggressive settings
- [x] OCR text-only without geometry
- [x] render-only with no sync
- [x] Persist all fallback decisions in cache and logs.

## Phase 9: Performance and Incrementality
- [ ] Keep OCR alignment incremental for large documents:
- [x] page-local alignment cache
- [x] sentence-range cache invalidation
- [ ] viewport-local overlay recomputation only
- [x] avoid rebuilding whole-document alignment on every playback tick
- [ ] Add profiling for:
- [ ] OCR time per page/chunk
- [x] alignment build time
- [ ] overlay update time
- [x] cache hit rates
- [ ] fallback rates by PDF class

## Phase 10: Validation Matrix
- [ ] Add regression fixtures for:
- [ ] clean book scans
- [ ] low-contrast scans
- [ ] skewed pages
- [ ] noisy photocopies
- [ ] two-column scans
- [ ] scans with captions and figures
- [ ] scans with tables
- [ ] scans with footnotes and sidenotes
- [ ] rotated scans
- [ ] scans with marginal annotations
- [ ] mixed embedded-text + image PDFs
- [ ] Add acceptance tests for:
- [ ] sentence-following highlight continuity
- [ ] degraded line/block fallback honesty
- [ ] bookmark/reopen determinism
- [ ] page-click -> sentence resolution
- [ ] TTS continuity across Pretty Text and Text-only views

## Phase 11: Manual QA
- [ ] Create a dedicated manual QA checklist for OCR geometry alignment.
- [ ] Validate all three user-critical classes:
- [ ] high-quality text-laden scanned PDFs
- [ ] mixed-quality medium-text PDFs
- [ ] raw scans with no usable embedded text
- [ ] Require screenshots or captured logs for all low-confidence / degraded decisions during QA.

## Implementation Order
- [x] Step 1: define and persist OCR geometry/cache schema
- [ ] Step 2: derive canonical `tts_text` with OCR token lineage
- [ ] Step 3: build page reading-order resolver
- [x] Step 4: build sentence-to-geometry OCR alignment artifact
- [ ] Step 5: move OCR-first highlight rendering to rect overlays
- [x] Step 6: wire search/bookmark/resume through OCR location metadata
- [x] Step 7: add performance instrumentation and cache invalidation rules
- [ ] Step 8: add regression fixtures and manual QA matrix
