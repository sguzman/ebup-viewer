# PDF OCR Geometry Manual QA Checklist

## Evidence Rules
- [ ] Capture at least one screenshot for every `ocr_high_trust`, `ocr_mixed_trust`, and `ocr_text_only` run.
- [ ] Attach relevant `tracing` log lines for every degraded or low-confidence OCR decision.
- [ ] Record the source file name, page count, OCR quality class, and runtime policy for each run.

## High-Trust PDFs
- [ ] Verify the currently spoken sentence highlights at the correct on-page location for a text-laden scanned PDF.
- [ ] Verify sentence highlight remains stable during zoom in, zoom out, and page resize.
- [ ] Verify highlight remains stable when scrolling away and back to the active page.
- [ ] Verify right after reopening the document, bookmark resume restores the same sentence and overlay location.

## Mixed-Trust PDFs
- [ ] Verify line/block fallback is visually honest and never paints an obviously wrong sentence.
- [ ] Verify page click and overlay click both jump to the nearest canonical sentence.
- [ ] Verify fallback remains stable across rerender cycles and page-shell replacement.
- [ ] Verify the diagnostics panel shows non-zero degraded fallback rate for the document.

## Text-Only OCR PDFs
- [ ] Verify Text-only mode and TTS remain usable while native PDF highlight stays degraded or disabled.
- [ ] Verify toggling between Pretty Text and Text-only preserves the spoken sentence position.
- [ ] Verify bookmarks and reopen restore the nearest stable canonical sentence even when only page-level geometry exists.

## Raw or Unusable OCR PDFs
- [ ] Verify the reader never paints fake sentence precision for render-only/no-sync PDFs.
- [ ] Verify the UI surfaces the gated/degraded explanation instead of pretending highlight sync is available.
- [ ] Verify captured logs include the runtime policy explanation and degraded reasons.
