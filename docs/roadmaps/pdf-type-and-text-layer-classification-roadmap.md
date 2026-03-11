# PDF Type and Text Layer Classification Roadmap

## Tranche Status
- [x] Tranche 1: sampled-page probe expansion, page/document classification, OCR recommendation, cache persistence, and coarse runtime policy mapping
- [x] Tranche 2: classification versioning plus reader/UI diagnostics for class, OCR recommendation, and rationale
- [ ] Tranche 3: stronger embedded-text trust signals, hidden-OCR-overlay heuristics tied to image/raster evidence, and richer runtime policy coverage
- [ ] Tranche 4: fixture calibration set, regression matrix, and remaining diagnostics depth

## Objective
- [ ] Build a stronger PDF classifier that can reliably determine what kind of PDF the app is dealing with before ingestion, OCR, sync, and highlighting decisions are made.
- [ ] Distinguish clean embedded-text PDFs, noisy embedded-text PDFs, mixed/hybrid PDFs, hidden-OCR-overlay PDFs, scan-first PDFs, and image-only PDFs.
- [ ] Drive OCR, native-PDF sync, fallback, and UI behavior from explicit classification results instead of only coarse sampled text heuristics.

## Success Criteria
- [ ] The app can explain why a PDF was classified the way it was.
- [x] Classification is performed per page first, then rolled up to a document-level mode.
- [ ] Borderline PDFs land in explicit mixed/degraded classes instead of being misrepresented as clean embedded text.
- [ ] OCR, TTS, search, pretty-view sync, and degraded behavior all use the same classification contract.
- [x] Classification is deterministic across reopen/cache reuse unless the source file actually changes.

## Target PDF Classes
- [x] Define stable document/page classes:
- [x] `embedded_clean`
- [x] `embedded_noisy`
- [x] `embedded_sparse`
- [x] `hidden_ocr_overlay`
- [x] `scan_with_good_ocr`
- [x] `scan_with_weak_ocr`
- [x] `image_only_no_text`
- [x] `hybrid_mixed_document`
- [x] `layout_hostile_document`
- [ ] Define allowed runtime behavior for each class:
- [ ] exact sentence sync allowed
- [ ] fuzzy/paragraph fallback only
- [ ] OCR required before TTS/sync
- [ ] text-only allowed, pretty sync disabled
- [ ] render-only with no text ownership

## Phase 1: Probe Contract Expansion
- [x] Expand the PDF probe stage to collect more than average chars/garbage/whitespace:
- [x] page count
- [x] text-object presence by page
- [x] text-object density by page
- [ ] image coverage ratio by page
- [ ] bitmap/full-page raster heuristics
- [ ] hidden or invisible text-layer presence
- [ ] duplicate glyph / overlapping text rate
- [x] repeated header/footer rate
- [x] line/word box coherence metrics
- [x] copy-paste corruption indicators
- [ ] mixed text-and-image page ratio
- [x] Persist raw probe features in cache for later audit.
- [x] Add tracing for all probe features at both page and document level.

## Phase 2: Page-Level Classification
- [x] Implement a page-level classifier that scores each page independently.
- [ ] Define feature thresholds and weighted scores for:
- [x] strong embedded-text page
- [x] noisy embedded-text page
- [x] likely scanned page
- [x] hidden-OCR-overlay page
- [x] image-only page
- [x] layout-hostile page
- [x] Add confidence per page classification.
- [x] Persist page classifications and confidence in cache.
- [ ] Expose page classifications to diagnostics and QA tooling.

## Phase 3: Document-Level Rollup
- [x] Aggregate page-level classes into a document-level class.
- [x] Keep per-page overrides even when the document gets one primary class.
- [ ] Define rollup rules for:
- [x] mostly embedded text with a scanned appendix
- [x] mostly scanned text with a few embedded-text pages
- [x] mixed page classes across chapters
- [x] pages with text but unusable geometry
- [x] hidden OCR overlays that should not be trusted as clean embedded text
- [x] Persist document-level class plus page distribution summary.
- [x] Add tracing for rollup rationale and class distribution.

## Phase 4: Embedded Text Quality Analysis
- [x] Strengthen detection of whether existing PDF text is actually trustworthy:
- [x] token continuity
- [x] line coherence
- [ ] block coherence
- [ ] coordinate sanity
- [ ] duplicate/invisible text suppression need
- [ ] reading-order stability
- [x] paragraph reconstruction quality
- [x] Promote only clearly trustworthy pages to `embedded_clean`.
- [x] Downgrade malformed but present text to `embedded_noisy` or `embedded_sparse`.

## Phase 5: Hidden OCR Overlay Detection
- [x] Add explicit heuristics for hidden OCR text layers:
- [ ] full-page image plus sparse text objects
- [ ] invisible or zero-opacity text
- [ ] duplicated text stacked over image content
- [x] OCR-like token quality with weak geometry coherence
- [ ] mixed embedded and OCR text behavior on the same page
- [x] Route these PDFs into a dedicated class instead of treating them as clean embedded text.

## Phase 6: OCR Readiness and Need Detection
- [x] Decide whether OCR is required, optional, or unnecessary from classification alone.
- [ ] Separate:
- [x] OCR needed for text ownership
- [x] OCR needed only for better geometry
- [x] OCR not needed
- [x] OCR unlikely to help enough
- [ ] Add confidence thresholds for when OCR output may replace or augment embedded text.
- [x] Record OCR recommendation in cache and runtime state.

## Phase 7: Runtime Policy Mapping
- [x] Map classification directly to runtime behavior:
- [x] native PDF pretty mode policy
- [ ] text-only availability
- [ ] TTS availability
- [ ] sentence highlight policy
- [ ] line/block fallback policy
- [x] render-only policy
- [ ] bookmark/resume policy
- [ ] search policy
- [x] Ensure the UI can surface the chosen class and why degraded behavior was selected.

## Phase 8: Cache and Contracts
- [x] Extend cache layout to store:
- [x] probe feature summaries
- [x] per-page class results
- [x] document rollup result
- [x] OCR recommendation
- [ ] embedded-text trust diagnostics
- [x] classification version
- [x] Add cache invalidation/versioning rules when classification logic changes.
- [x] Ensure reopen behavior reuses cached classification when valid.

## Phase 9: Model and Heuristic Calibration
- [ ] Build a labeled fixture set of PDFs across all target classes.
- [ ] Measure false positives and false negatives for:
- [ ] clean embedded text misclassified as scan
- [ ] scans misclassified as embedded text
- [ ] hidden OCR overlays misclassified as clean text
- [ ] mixed documents forced into one overly strong class
- [ ] Tune thresholds with recorded fixture outcomes.
- [ ] Keep classifier deterministic and explainable even if a learned scorer is introduced later.

## Phase 10: Validation Matrix
- [ ] Add regression fixtures for:
- [ ] high-quality publisher PDFs
- [ ] malformed embedded-text PDFs
- [ ] sparse-text presentation PDFs
- [ ] academic PDFs with figures/tables/footnotes
- [ ] scanned books
- [ ] photocopies
- [ ] OCR-overlay PDFs
- [ ] hybrid documents with mixed page classes
- [ ] Add tests for page-level and document-level classification rollup.
- [ ] Add tests proving runtime behavior selection matches classification.

## Phase 11: Diagnostics and UX
- [ ] Add developer diagnostics for:
- [x] detected page classes
- [x] document class
- [x] confidence levels
- [x] OCR recommendation
- [x] key feature signals
- [ ] fallback triggers
- [x] Add user-facing explanation strings for degraded PDF behavior driven by classification.
- [ ] Provide a way to inspect why a PDF was treated as scan/mixed/render-only.

## Recommended Order
- [x] Step 1: expand probe feature collection
- [x] Step 2: add page-level classifier and cache schema
- [x] Step 3: add document rollup and runtime policy mapping
- [ ] Step 4: add hidden OCR overlay detection
- [x] Step 5: add OCR recommendation logic
- [ ] Step 6: wire UI diagnostics and explanation strings
- [ ] Step 7: add regression fixtures and calibration loop
