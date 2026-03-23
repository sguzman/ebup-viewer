# Dual View Ingestion and TTS Pipeline Roadmap

## Objective
- [x] Deliver two reader outputs per source where possible:
- [x] `reading_markdown` for Pretty Text rendering.
- [x] `tts_text` for Text-only view and the full TTS/normalization/sentence pipeline.
- [x] Keep TTS strictly sourced from `tts_text` to simplify behavior and reduce drift.

## Phase 1: Data Model and Contracts
- [x] Add a normalized content model with explicit optional fields:
- [x] `reading_markdown: Option<String>`
- [x] `tts_text: String`
- [x] `has_structured_markdown: bool`
- [x] Add source capability metadata to session snapshot/event payloads.
- [x] Update Rust DTOs and contract tests for the new model.
- [x] Add tracing for model creation and field availability decisions.

## Phase 2: Ingestion Pipeline by Source Type
- [x] EPUB ingest emits `reading_markdown + tts_text`.
- [x] HTML ingest emits `reading_markdown + tts_text`.
- [x] DOCX ingest emits `reading_markdown + tts_text`.
- [x] PDF ingest attempts structured extract and emits `reading_markdown + tts_text` when viable.
- [x] PDF raw/scan fallback emits `tts_text` only with `reading_markdown = None`.
- [x] Add tracing spans per ingest stage with source type, duration, and fallback reason.

## Phase 3: TTS and Normalization Ownership
- [x] Route all sentence splitting/normalization/TTS planning to `tts_text` only.
- [x] Prevent Pretty Text view switches from changing TTS sentence indexing.
- [x] Confirm bookmark/highlight resume uses `tts_text` sentence indices.
- [x] Add tracing to prove which payload (`tts_text`) fed each TTS runtime step.

## Phase 4: Reader Rendering Modes
- [x] Pretty Text mode renders `reading_markdown` when present.
- [x] Pretty Text mode falls back to rendered `tts_text` when markdown is unavailable.
- [x] Text-only mode always renders plain `tts_text`.
- [x] Expose non-blocking UI indicator when markdown is unavailable for current source.
- [x] Preserve existing settings behavior across both modes (font, spacing, highlight, search).

## Phase 5: Cross-View Alignment and Mapping
- [x] Define and persist a mapping from `tts_text` sentence indices to Pretty Text rendered anchors.
- [x] Use mapping for highlight sync and jump-to-spoken-sentence in Pretty Text mode.
- [x] Add robust fallback when an anchor is missing (nearest sentence or direct text-only jump).
- [x] Add tracing for mapping hits/misses and fallback path frequency.

## Phase 6: Cache and Persistence
- [x] Extend cache layout to store both artifacts (`reading_markdown`, `tts_text`) and mapping data.
- [x] Add cache version bump and migration logic for prior single-output entries.
- [x] Ensure recent-book deletion clears all new artifacts without hard failures on transient races.
- [x] Add tracing around cache read/write/migration outcomes.

## Phase 7: Validation and Test Coverage
- [x] Unit tests for each ingest adapter output contract.
- [x] Unit tests for markdown-unavailable fallback behavior.
- [x] Unit tests for mapping generation and lookup edge cases.
- [x] Integration tests for TTS continuity across Pretty Text/Text-only toggles.
- [x] Regression tests for PDF structured vs scan fallback branches.
- [x] Manual QA checklist for EPUB/HTML/DOCX/PDF including recent delete and resume.

## Phase 8: Rollout and Observability
- [x] Gate with config flag for staged rollout.
- [x] Add metrics/log summaries for markdown availability rate by source type.
- [x] Add metrics/log summaries for mapping fallback frequency.
- [x] Remove flag after stability window and confirm docs are updated.

## Acceptance Criteria
- [x] TTS behavior is identical in both views because `tts_text` is the sole TTS input.
- [x] Pretty Text is visually richer for structured sources and never blocks reading for unstructured PDFs.
- [x] No regression in bookmark resume, sentence highlight, auto-scroll, or recent delete flows.
- [x] Build, tests, and legacy packaging compile paths pass in CI (excluding `deb`/`rpm` targets).
