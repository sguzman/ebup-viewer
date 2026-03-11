# PDF Classification Calibration Report

This report records the current fixture-backed calibration expectations for the PDF classifier. It is tied to [pdf-classification-fixtures.toml](/win/linux/Code/projects/lantern-leaf/tests/fixtures/pdf-classification-fixtures.toml) and the classifier test matrix in [source_pipeline.rs](/win/linux/Code/projects/lantern-leaf/src/epub_loader/source_pipeline.rs).

## Covered Classes

- `embedded_clean`
- `embedded_noisy`
- `embedded_sparse`
- `hidden_ocr_overlay`
- `scan_with_weak_ocr`
- `image_only_no_text`
- `hybrid_mixed_document`
- `layout_hostile_document`

## Recorded Calibration Goals

- Clean embedded text should not be misclassified as scan-first or render-only.
- Scan-first PDFs should not be promoted to embedded-clean.
- Hidden OCR overlays should not be promoted to embedded-clean.
- Mixed documents should remain in explicit mixed/degraded classes instead of collapsing into one overly strong class.

## Current Fixture Expectations

- `publisher-clean` calibrates exact-sync clean embedded text.
- `malformed-embedded` calibrates noisy embedded text with degraded sync.
- `sparse-presentation` calibrates sparse but still textual PDFs.
- `academic-layout-hostile` calibrates layout-hostile embedded text with figures/footnotes.
- `scanned-book` calibrates scan-first OCR-dependent pages.
- `photocopy-image-only` calibrates image-only render-only behavior.
- `ocr-overlay` calibrates hidden OCR overlay detection.
- `hybrid-mixed` calibrates mixed embedded/image-heavy documents.

## Threshold Notes

- OCR replace confidence is currently treated as meaningful at `>= 0.74`.
- OCR augment confidence is currently treated as meaningful at `>= 0.58`.
- These thresholds are recorded in trust diagnostics and surfaced in the reader diagnostics UI for PDF sessions.

## Determinism Rule

- The classifier remains deterministic because fixtures are classified from explicit sampled features, sorted reasons, and versioned cache artifacts.
- Any future learned scorer must remain advisory unless it can emit stable, explainable reasons that preserve these diagnostics and cache contracts.
