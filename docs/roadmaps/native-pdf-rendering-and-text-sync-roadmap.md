# Native PDF Rendering and Text Sync Roadmap

## Objective
- [ ] Render PDF sources natively in Pretty Text mode as the actual PDF document, not a converted HTML/markdown approximation.
- [ ] Keep Text-only mode strictly bound to extracted plain text from the PDF.
- [ ] Ensure TTS, normalization, sentence splitting, and playback control are driven only by extracted plain text.
- [ ] Synchronize the Text-only/TTS cursor back onto the native PDF render with stable visual highlight and scroll behavior.

## Phase 1: Source Contracts and Ownership
- [ ] Define PDF source contract with two canonical payloads:
- [ ] `pretty_pdf: PdfRenderHandle` or equivalent native render descriptor for Pretty Text mode.
- [ ] `tts_text: String` for Text-only rendering and TTS ownership.
- [ ] Document that `tts_text` is the only input to normalization, sentence planning, bookmarks, and audio playback.
- [ ] Add tracing fields showing source type, extraction mode, render mode, and sync strategy.

## Phase 2: PDF Ingestion and Text Extraction
- [ ] Build a dedicated PDF ingest path that outputs native PDF render metadata plus extracted `tts_text`.
- [ ] Support structured PDFs with selectable/extractable text as the primary happy path.
- [ ] Preserve page boundaries, block order, and reading order metadata during extraction when available.
- [ ] Normalize extracted text into stable `tts_text` with reliable whitespace and paragraph boundaries.
- [ ] Add explicit fallback handling for low-quality extraction, duplicated glyphs, headers/footers, and multi-column layouts.
- [ ] Add tracing spans for extraction duration, detected PDF text quality, and fallback decisions.

## Phase 3: Native PDF Pretty View
- [ ] Render the actual PDF file in Pretty Text mode using a native PDF rendering path.
- [ ] Choose and document the native PDF renderer contract explicitly:
- [ ] renderer implementation (`pdf.js`, browser-native embed, or another owned render path)
- [ ] whether the renderer exposes a trustworthy text layer, selection layer, or only painted pages
- [ ] which layer owns page metrics, zoom state, and page-to-viewport transforms
- [ ] Preserve page geometry, embedded images, figures, tables, and document layout.
- [ ] Support zoom, page navigation, and scroll without converting the PDF into markdown or HTML.
- [ ] Keep rendering isolated so PDF styles/assets do not affect the surrounding app UI.
- [ ] Add tracing for PDF page render timing, viewport state, and render errors.

## Phase 4: Text-only View and TTS Ownership
- [ ] Text-only mode renders only extracted `tts_text`.
- [ ] Sentence splitting runs only against `tts_text`.
- [ ] TTS playback plans are generated only from `tts_text`.
- [ ] Pretty Text/Text-only toggles do not alter sentence indices, playback position, bookmarks, or search ownership.
- [ ] Add explicit tracing proving each playback step originated from `tts_text`.

## Phase 5: PDF Text Geometry and Sync Map
- [ ] Build a persistent mapping from `tts_text` sentence indices back to PDF page coordinates.
- [ ] Define the canonical sync artifact shape explicitly:
- [ ] `sentence_idx -> page_idx + rects[]` or equivalent quad/box list
- [ ] support one sentence mapping to multiple disjoint rectangles across lines
- [ ] support one sentence spanning multiple text blocks or column boundaries
- [ ] Use PDF text geometry when available:
- [ ] page number
- [ ] text block or line bounds
- [ ] glyph/span coordinates where possible
- [ ] Keep mapping deterministic even when extraction is imperfect or text spans cross line breaks.
- [ ] Define normalization parity rules between extracted `tts_text` and PDF-visible text:
- [ ] ligatures (`fi`, `fl`)
- [ ] soft hyphenation and line-wrap joins
- [ ] collapsed whitespace and paragraph boundaries
- [ ] hidden text-layer artifacts, duplicated glyphs, and copy/paste noise
- [ ] Define mismatch handling when extracted `tts_text` and renderer text-layer text diverge:
- [ ] exact geometry match
- [ ] fuzzy span match with confidence downgrade
- [ ] paragraph/block fallback
- [ ] unmappable sentence with explicit degraded behavior
- [ ] Add confidence scoring for each mapped sentence or paragraph.
- [ ] Persist sync artifacts in cache alongside extracted text.
- [ ] Add tracing for mapping hits, low-confidence matches, missing spans, and fallback behavior.

## Phase 6: Playback Highlight in Native PDF View
- [ ] Highlight the currently spoken unit directly on top of the native PDF render.
- [ ] Support paragraph-level highlighting initially if sentence-level PDF geometry is not yet stable.
- [ ] Allow future refinement to sentence-level highlight without changing `tts_text` ownership.
- [ ] Keep highlight overlays aligned during zoom, page resize, and scroll.
- [ ] Keep highlight overlays aligned during rotation, DPI changes, and viewport transform updates.
- [ ] Remove stale overlays cleanly when page/view state changes.
- [ ] Add tracing for highlight target resolution, page changes, and overlay lifecycle.

## Phase 7: Scroll and Cursor Behavior
- [ ] Auto-scroll the native PDF view to the active highlighted location during playback.
- [ ] Keep scroll stable within the same mapped paragraph or region.
- [ ] Only force scroll when playback advances to a new mapped location, page, or explicit jump target.
- [ ] Keep Text-only and native PDF views aligned to the same `tts_text` cursor.
- [ ] Add tracing for scroll trigger reasons and viewport adjustments.

## Phase 8: Search, Navigation, and Resume Semantics
- [ ] Ensure search in Text-only mode uses `tts_text` and can jump to mapped PDF locations.
- [ ] Ensure bookmarks and resume positions remain owned by `tts_text` indices plus mapped PDF location metadata.
- [ ] Preserve deterministic behavior when reopening a PDF after cache reuse or rebuild.
- [ ] Keep page navigation and TTS seek operations synchronized across PDF and text-only views.
- [ ] Support reverse navigation from native PDF interactions back to `tts_text` ownership:
- [ ] click or selection in Pretty Text PDF view can resolve to nearest `tts_text` sentence
- [ ] page jump in PDF view can restore the nearest canonical playback/search cursor

## Phase 9: Cache, Recovery, and Migration
- [ ] Extend cache layout to store extracted `tts_text`, PDF sync maps, page geometry metadata, and render descriptors.
- [ ] Add cache versioning for PDF dual-payload entries.
- [ ] Recover cleanly from missing or corrupted PDF text/sync artifacts by rebuilding them non-destructively.
- [ ] Ensure recent-delete clears extracted text, mapping artifacts, thumbnails, and PDF sidecar cache entries consistently.
- [ ] Add tracing around cache reads, writes, invalidation, rebuilds, and delete outcomes.

## Phase 10: OCR and Degraded PDF Strategy
- [ ] Define behavior for scanned or image-only PDFs where no reliable embedded text exists.
- [ ] Keep native PDF rendering available even when text extraction quality is poor.
- [ ] Decide whether OCR is deferred, optional, or first-class for scanned PDFs.
- [ ] If OCR is unavailable, present a clear degraded-mode contract for Text-only/TTS support.
- [ ] Define degraded behavior by distinct runtime mode:
- [ ] renderable PDF + trustworthy text geometry
- [ ] renderable PDF + extracted text but low-confidence geometry
- [ ] renderable PDF + OCR-derived text/geometry
- [ ] renderable PDF + no usable mapping for highlight sync
- [ ] Add tracing distinguishing embedded-text PDFs from OCR-required PDFs.

## Phase 11: Validation and Regression Coverage
- [ ] Unit tests for PDF text extraction normalization.
- [ ] Unit tests for sentence-to-PDF coordinate mapping and confidence scoring.
- [ ] Integration tests for playback continuity across Pretty Text and Text-only toggles on PDFs.
- [ ] Regression tests for multi-column PDFs, footnotes, repeated headers, tables, figures, and long captions.
- [ ] Regression tests ensuring highlight overlays remain aligned during zoom and page changes.
- [ ] Regression tests for ligatures, hyphenated line wraps, rotated pages, OCR text layers, and hidden/duplicated embedded text.
- [ ] Manual QA checklist covering native rendering fidelity, text cleanliness, playback sync, resume, and delete/reopen behavior.

## Acceptance Criteria
- [ ] Pretty Text mode renders the actual PDF natively.
- [ ] Text-only mode shows only clean extracted text.
- [ ] TTS, normalization, and playback indexing are fully owned by extracted `tts_text`.
- [ ] Native PDF view highlights the currently spoken text at the correct PDF location.
- [ ] Auto-scroll in Pretty Text mode follows playback without jitter or premature repositioning.
- [ ] Full project build verification passes after implementation, excluding `deb`, `rpm`, and AppImage packaging targets.
