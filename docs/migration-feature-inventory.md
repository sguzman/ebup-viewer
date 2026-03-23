# Migration Feature Inventory

This inventory captures the currently implemented migration-shell features that must remain at parity with legacy behavior.

## Starter

- Open source path (`.epub`, `.pdf`, `.txt`, `.md`, `.markdown`).
- Open clipboard text as a persisted source.
- Recent list rendering under Welcome.
- Recent delete action removes entry and cache artifacts.
- Calibre list load, refresh, filter, sort, and open.
- Calibre list virtualization for large catalogs.
- Starter status/event reporting for source open, calibre load, PDF transcription.
- Runtime log-level selector and apply action.

## Reader

- Pretty text and text-only modes.
- Sentence click updates highlight and playback anchor.
- Prev/next page and direct page input.
- Prev/next sentence controls.
- Reader search with regex support and next/prev navigation.
- Close session returns to starter with housekeeping.

## TTS

- Play/pause/toggle.
- Play from page start and play from current highlight.
- Seek next/prev sentence and repeat sentence.
- Pause-after-sentence and speed/volume controls.
- 3-decimal progress display and state tracking.
- TTS state event stream from backend.

## Settings And Stats

- Settings and stats panel mutual exclusivity.
- TTS panel toggle and responsive non-vertical-collapse behavior.
- Numeric slider/text hybrid controls with validation and wheel adjust.
- Reader stats panel metrics (page/book progress, word/sentence counters, percentages).

## Ingestion And Processing

- EPUB text + image extraction.
- PDF ingestion through in-process quack-check pipeline with fallback logic.
- Clipboard text ingestion and cache persistence.
- Normalization/chunk mapping between display sentences and audio sentences.

## Runtime And Persistence

- Bookmark/config persistence and resume semantics.
- Safe quit command and close housekeeping.
- Runtime log-level update and config persistence.
- Bridge event channels for source/calibre/session/reader/TTS/PDF/log-level.

## Testing Coverage In Migration Shell

- Rust unit tests for bridge/session/normalizer/pipeline logic.
- Rust unit/integration tests for adapters, state behavior, layout/typography policies, list virtualization.
- Native UI smoke scenarios for starter-to-reader-to-TTS flows and performance baseline capture.
