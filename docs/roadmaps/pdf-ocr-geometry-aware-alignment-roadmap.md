# PDF OCR Geometry-Aware Alignment Roadmap

## Objective
- [x] Build a dedicated OCR geometry pipeline for scanned and OCR-heavy PDFs that can align canonical `tts_text` sentences back onto the native PDF render with explicit confidence and stable visual behavior.
- [x] Keep `tts_text` as the only owner of playback, search, bookmarks, resume, and sentence indexing.
- [x] Treat OCR geometry as a probabilistic alignment surface, never as guaranteed truth.
- [x] Support degraded but honest behavior for PDFs where OCR text exists but sentence-level geometry is weak.

## Success Criteria
- [x] High-quality scans can highlight the currently spoken sentence at the correct on-page location.
- [x] Medium-quality scans can degrade to stable line/block highlighting without visibly wrong sentence jumps.
- [x] Raw scans with unusable OCR never fake precise geometry and remain renderable with clearly gated or degraded TTS sync.
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
- [x] Define a canonical OCR output contract independent of the renderer:
- [x] recognized text blocks
- [x] recognized lines
- [x] recognized words or tokens
- [x] page-local bounding boxes for each level
- [x] reading-order indices at block and line granularity
- [x] confidence per word, line, block, and page
- [x] source markers distinguishing embedded text, OCR text, and mixed merged text
- [x] Persist the contract in cache as a versioned OCR geometry artifact.
- [x] Add tracing for OCR engine, OCR mode, per-page confidence summary, and total recognized token counts.

## Phase 2: OCR Text Ownership and Normalization
- [x] Derive canonical `tts_text` from OCR output without losing page-local sentence provenance.
- [x] Define OCR normalization parity rules:
- [x] broken line joins
- [x] hyphenated word recovery
- [x] ligatures and OCR unicode noise
- [x] repeated headers and footers
- [x] margin note and sidenote suppression or segregation
- [x] table cell reading-order normalization
- [x] footnote marker and citation handling
- [x] punctuation repair only when confidence permits
- [x] Keep a sentence-to-source-token trail so each `tts_text` sentence can be traced back to OCR blocks/lines/words.
- [x] Add tracing for normalization edits, dropped noise, merged lines, and repeated boilerplate suppression.

## Phase 3: OCR Reading Order Recovery
- [x] Build a page reading-order resolver for OCR-heavy pages:
- [x] single-column pages
- [x] strong two-column pages
- [x] mixed column + full-width caption bands
- [x] bottom footnote bands
- [x] outer-margin sidenotes
- [x] tables and grid-like content
- [x] rotated pages
- [x] rotated blocks within otherwise upright pages
- [x] figures and captions separated from body order
- [x] Keep reading-order decisions explicit and persisted per page.
- [x] Add diagnostics for why a page was classified as single-column, multi-column, table-like, caption-banded, or fallback.

## Phase 4: OCR Geometry Alignment Artifact
- [x] Define the canonical OCR alignment artifact:
- [x] `sentence_idx -> page_idx + rects[] + line_rects[] + block_rects[]`
- [x] support one sentence spanning multiple lines
- [x] support one sentence spanning multiple OCR blocks
- [x] support one sentence crossing column boundaries only when confidence justifies it
- [x] store confidence and fallback reason per mapping
- [x] store contributing OCR token ids for auditability
- [x] Build the artifact deterministically from canonical `tts_text` and OCR geometry, not from renderer DOM guesses alone.
- [x] Persist the artifact alongside OCR text output and cache signatures.

## Phase 5: Matching and Confidence Model
- [x] Implement OCR sentence matching tiers:
- [x] exact token-chain alignment
- [x] normalized sentence alignment
- [x] line-window fuzzy alignment
- [x] block fallback alignment
- [x] page-only fallback
- [x] missing/unmappable
- [x] Add score components for:
- [x] text similarity
- [x] reading-order continuity
- [x] page continuity
- [x] geometry compactness
- [x] OCR confidence
- [x] distance penalties for visually implausible matches
- [x] Require fallback to move only downward in confidence.
- [x] Reject matches that jump across distant regions when a weaker but local fallback exists.

## Phase 6: Native PDF Overlay Contract
- [x] Render OCR-derived highlight overlays directly on top of the native PDF viewport.
- [x] Prefer rect-based overlays over text-layer span classes for OCR-first PDFs.
- [x] Support:
- [x] sentence-level multi-rect overlays
- [x] line-level fallback overlays
- [x] block-level fallback overlays
- [x] page-active fallback when geometry is too weak
- [x] Ensure overlay alignment remains stable during:
- [x] zoom changes
- [x] page resize
- [x] scroll
- [x] rotation
- [x] DPI changes
- [x] rerender/rebind cycles
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
- [x] Keep OCR alignment incremental for large documents:
- [x] page-local alignment cache
- [x] sentence-range cache invalidation
- [x] viewport-local overlay recomputation only
- [x] avoid rebuilding whole-document alignment on every playback tick
- [x] Add profiling for:
- [x] OCR time per page/chunk
- [x] alignment build time
- [x] overlay update time
- [x] cache hit rates
- [x] fallback rates by PDF class

## Phase 10: Validation Matrix
- [x] Add regression fixtures for:
- [x] clean book scans
- [x] low-contrast scans
- [x] skewed pages
- [x] noisy photocopies
- [x] two-column scans
- [x] scans with captions and figures
- [x] scans with tables
- [x] scans with footnotes and sidenotes
- [x] rotated scans
- [x] scans with marginal annotations
- [x] mixed embedded-text + image PDFs
- [x] Add acceptance tests for:
- [x] sentence-following highlight continuity
- [x] degraded line/block fallback honesty
- [x] bookmark/reopen determinism
- [x] page-click -> sentence resolution
- [x] TTS continuity across Pretty Text and Text-only views

## Phase 11: Manual QA
- [x] Create a dedicated manual QA checklist for OCR geometry alignment.
- [x] Validate all three user-critical classes:
- [x] high-quality text-laden scanned PDFs
- [x] mixed-quality medium-text PDFs
- [x] raw scans with no usable embedded text
- [x] Require screenshots or captured logs for all low-confidence / degraded decisions during QA.

## Implementation Order
- [x] Step 1: define and persist OCR geometry/cache schema
- [x] Step 2: derive canonical `tts_text` with OCR token lineage
- [x] Step 3: build page reading-order resolver
- [x] Step 4: build sentence-to-geometry OCR alignment artifact
- [x] Step 5: move OCR-first highlight rendering to rect overlays
- [x] Step 6: wire search/bookmark/resume through OCR location metadata
- [x] Step 7: add performance instrumentation and cache invalidation rules
- [x] Step 8: add regression fixtures and manual QA matrix
