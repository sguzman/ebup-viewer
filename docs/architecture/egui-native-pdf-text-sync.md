# Egui Native PDF Text + Sync Strategy

This document defines how the native PDF subsystem handles text extraction, normalization, sync maps, and persistence. It aligns with the PDF roadmap Phase 3.

## Ownership and responsibilities
- **TextExtractionService** owns canonical page text extraction and normalization.
- **SyncMapBuilder** owns sentence-to-geometry maps and fallback tier labeling.
- **OCRArtifactLoader** augments low-confidence extraction with OCR artifacts when present.

## Extraction + normalization rules
- Canonical sentences are derived from extracted text and normalized via:
  - ligature normalization
  - hyphenation merge rules
  - duplicate header/footer suppression
- Each normalization step logs whether it changed the canonical sentence string.

## Cache and persistence
- Page-level text caches and sync maps are persisted under `.cache/lantern-leaf/<source_hash>/content/pdf-*`.
- Flushes occur on source open, session close, and safe quit.
- Cache hits/misses emit structured logs so redraw/coalescing behavior can be correlated with cache usage.

## Fallback hierarchy
Fallback order is explicit and traced:
1. exact sentence geometry
2. line/phrase geometry
3. block fallback
4. page-level location
5. render-only/no-sync

Each transition is emitted in tracing fields (`fallback_path`) so QA can correlate highlight jumps with PDF metrics.

## Instrumentation requirements
- `pdf.text.extract`: page-level extraction duration and mode (`exact`, `mixed`, `block`).
- `pdf.text.fallback` / `pdf.text.sync`: fallback path and sentence metadata.
- `pdf.ocr.load`: OCR artifact metadata and confidence tier.
- `pdf.sync.build`: sync map build metadata including confidence tier and fallback reason.
