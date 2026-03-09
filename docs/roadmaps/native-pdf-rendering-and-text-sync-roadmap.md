# Native PDF Rendering and Text Sync Roadmap

## Objective
- [x] Render PDF sources natively in Pretty Text mode as the actual PDF document, not a converted HTML/markdown approximation.
- [x] Keep Text-only mode strictly bound to extracted plain text from the PDF.
- [x] Ensure TTS, normalization, sentence splitting, and playback control are driven only by extracted plain text.
- [ ] Synchronize the Text-only/TTS cursor back onto the native PDF render with stable visual highlight and scroll behavior.

## Geometry Robustness Contract
- [x] Treat PDF text geometry as a quality-classified signal, not as an always-correct source of truth.
- [x] Classify each opened PDF into one of these runtime geometry modes:
- [x] `high_text_trust`: embedded text is clean, ordered, and geometrically mappable at sentence level
- [x] `mixed_text_trust`: embedded text exists but requires fuzzy matching, block fallback, or partial sentence mapping
- [x] `ocr_required`: no reliable embedded text layer; OCR is required for meaningful sync
- [x] `render_only_no_sync`: the PDF can be rendered, but no trustworthy text/geometry mapping is available for cursor sync
- [x] Persist the detected geometry mode and confidence summary in cache so reopen behavior is deterministic.
- [x] Never present low-confidence geometry as exact sentence sync.
- [x] Require every highlight/scroll decision to carry an explicit confidence tier and fallback reason.

## Geometry Guarantees by PDF Class
- [x] For `high_text_trust` PDFs:
- [x] target sentence-level mapping as the primary contract
- [x] allow multi-rect highlights for a single sentence across wrapped lines or split spans
- [x] keep playback, search, and click-jump behavior sentence-accurate
- [x] For `mixed_text_trust` PDFs:
- [x] allow fuzzy text alignment and paragraph/block fallback
- [x] prefer stable local-region highlighting over visually wrong sentence-level highlight
- [x] degrade to block highlight when sentence geometry is ambiguous
- [x] preserve `tts_text` ownership even when pretty-view sync becomes approximate
- [x] For `ocr_required` PDFs:
- [x] keep native PDF rendering available regardless of OCR readiness
- [ ] support OCR-backed text-only/TTS only when OCR output reaches minimum confidence thresholds
- [ ] distinguish OCR text confidence from embedded-text confidence in logs and cache
- [x] For `render_only_no_sync` PDFs:
- [x] show the native PDF in Pretty Text mode without pretending highlight sync is available
- [ ] keep Text-only/TTS functionality gated by available extracted text quality
- [x] expose a clear degraded-mode contract for no-highlight or page-level-only sync

## Geometry Failure and Fallback Rules
- [x] Define strict fallback order for sync resolution:
- [x] exact sentence geometry
- [x] fuzzy sentence geometry
- [x] paragraph/block geometry
- [x] page-level location only
- [x] render-only with no sync overlay
- [x] Require fallback to move only downward in confidence; never jump back to a stronger mode without new evidence.
- [x] Reject geometry matches that would place the cursor on the wrong page, wrong column, or visually distant region when a weaker but stable fallback exists.
- [ ] Add explicit handling for difficult PDF structures:
- [ ] dense academic two-column layouts
- [ ] repeated headers and footers
- [ ] footnotes and sidenotes
- [ ] tables with interleaved text order
- [ ] figures and long captions
- [ ] rotated pages or rotated text blocks
- [ ] hidden OCR layers, invisible text, duplicated glyph streams, and malformed copy/paste order
- [ ] large heavy-text PDFs where mapping must remain incremental and cacheable rather than recomputed on every view update

## Phase 1: Source Contracts and Ownership
- [x] Define PDF source contract with two canonical payloads:
- [x] `pretty_pdf: PdfRenderHandle` or equivalent native render descriptor for Pretty Text mode.
- [x] `tts_text: String` for Text-only rendering and TTS ownership.
- [x] Document that `tts_text` is the only input to normalization, sentence planning, bookmarks, and audio playback.
- [x] Add tracing fields showing source type, extraction mode, render mode, and sync strategy.

## Phase 2: PDF Ingestion and Text Extraction
- [x] Build a dedicated PDF ingest path that outputs native PDF render metadata plus extracted `tts_text`.
- [x] Support structured PDFs with selectable/extractable text as the primary happy path.
- [ ] Preserve page boundaries, block order, and reading order metadata during extraction when available.
- [x] Normalize extracted text into stable `tts_text` with reliable whitespace and paragraph boundaries.
- [ ] Add explicit fallback handling for low-quality extraction, duplicated glyphs, headers/footers, and multi-column layouts.
- [x] Add tracing spans for extraction duration, detected PDF text quality, and fallback decisions.

## Phase 3: Native PDF Pretty View
- [x] Render the actual PDF file in Pretty Text mode using a native PDF rendering path.
- [x] Choose and document the native PDF renderer contract explicitly:
- [x] renderer implementation (`pdf.js`, browser-native embed, or another owned render path)
- [x] whether the renderer exposes a trustworthy text layer, selection layer, or only painted pages
- [x] which layer owns page metrics, zoom state, and page-to-viewport transforms
- [x] Preserve page geometry, embedded images, figures, tables, and document layout.
- [x] Support zoom, page navigation, and scroll without converting the PDF into markdown or HTML.
- [x] Keep rendering isolated so PDF styles/assets do not affect the surrounding app UI.
- [x] Add tracing for PDF page render timing, viewport state, and render errors.

## Phase 4: Text-only View and TTS Ownership
- [x] Text-only mode renders only extracted `tts_text`.
- [x] Sentence splitting runs only against `tts_text`.
- [x] TTS playback plans are generated only from `tts_text`.
- [x] Pretty Text/Text-only toggles do not alter sentence indices, playback position, bookmarks, or search ownership.
- [x] Add explicit tracing proving each playback step originated from `tts_text`.

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
- [x] Add tracing for highlight target resolution, page changes, and overlay lifecycle.

## Phase 7: Scroll and Cursor Behavior
- [x] Auto-scroll the native PDF view to the active highlighted location during playback.
- [x] Keep scroll stable within the same mapped paragraph or region.
- [x] Only force scroll when playback advances to a new mapped location, page, or explicit jump target.
- [x] Keep Text-only and native PDF views aligned to the same `tts_text` cursor.
- [x] Add tracing for scroll trigger reasons and viewport adjustments.

## Phase 8: Search, Navigation, and Resume Semantics
- [ ] Ensure search in Text-only mode uses `tts_text` and can jump to mapped PDF locations.
- [ ] Ensure bookmarks and resume positions remain owned by `tts_text` indices plus mapped PDF location metadata.
- [x] Preserve deterministic behavior when reopening a PDF after cache reuse or rebuild.
- [x] Keep page navigation and TTS seek operations synchronized across PDF and text-only views.
- [x] Support reverse navigation from native PDF interactions back to `tts_text` ownership:
- [x] click or selection in Pretty Text PDF view can resolve to nearest `tts_text` sentence
- [ ] page jump in PDF view can restore the nearest canonical playback/search cursor

## Phase 9: Cache, Recovery, and Migration
- [x] Extend cache layout to store extracted `tts_text`, PDF sync maps, page geometry metadata, and render descriptors.
- [x] Add cache versioning for PDF dual-payload entries.
- [x] Recover cleanly from missing or corrupted PDF text/sync artifacts by rebuilding them non-destructively.
- [x] Ensure recent-delete clears extracted text, mapping artifacts, thumbnails, and PDF sidecar cache entries consistently.
- [x] Add tracing around cache reads, writes, invalidation, rebuilds, and delete outcomes.

## Phase 10: OCR and Degraded PDF Strategy
- [ ] Define behavior for scanned or image-only PDFs where no reliable embedded text exists.
- [ ] Keep native PDF rendering available even when text extraction quality is poor.
- [ ] Decide whether OCR is deferred, optional, or first-class for scanned PDFs.
- [ ] If OCR is unavailable, present a clear degraded-mode contract for Text-only/TTS support.
- [x] Define degraded behavior by distinct runtime mode:
- [x] renderable PDF + trustworthy text geometry
- [x] renderable PDF + extracted text but low-confidence geometry
- [x] renderable PDF + OCR-derived text/geometry
- [x] renderable PDF + no usable mapping for highlight sync
- [x] Add tracing distinguishing embedded-text PDFs from OCR-required PDFs.

## Phase 11: Validation and Regression Coverage
- [x] Unit tests for PDF text extraction normalization.
- [x] Unit tests for sentence-to-PDF coordinate mapping and confidence scoring.
- [x] Integration tests for playback continuity across Pretty Text and Text-only toggles on PDFs.
- [ ] Regression tests for multi-column PDFs, footnotes, repeated headers, tables, figures, and long captions.
- [ ] Regression tests ensuring highlight overlays remain aligned during zoom and page changes.
- [ ] Regression tests for ligatures, hyphenated line wraps, rotated pages, OCR text layers, and hidden/duplicated embedded text.
- [x] Manual QA checklist covering native rendering fidelity, text cleanliness, playback sync, resume, and delete/reopen behavior.

## Acceptance Criteria
- [ ] Pretty Text mode renders the actual PDF natively.
- [ ] Text-only mode shows only clean extracted text.
- [ ] TTS, normalization, and playback indexing are fully owned by extracted `tts_text`.
- [ ] Native PDF view highlights the currently spoken text at the correct PDF location.
- [x] Auto-scroll in Pretty Text mode follows playback without jitter or premature repositioning.
- [x] Full project build verification passes after implementation, excluding `deb`, `rpm`, and AppImage packaging targets.
