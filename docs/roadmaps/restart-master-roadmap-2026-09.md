# LanternLeaf Restart Master Roadmap — September 2026

This is the active restart roadmap. It does not erase older detailed roadmaps; it orders them against the current Windows/native-egui restart.

Roadmap state is evidence-driven. Completion means accepted implementation plus validation, not old checkbox history.

## Gate 0 — Recover trustworthy baseline

Goal family:

- Windows build/toolchain recovery;
- native egui launch;
- current capability inventory;
- Windows CI;
- verified current-status reset.

Exit: the project can be built/tested intentionally on the current Windows environment and we know what actually works.

## Gate 1 — TTS backend boundary + Windows TTS

Preserve Piper while making TTS backend ownership explicit.

Add native Windows speech as a backend with:

- installed voice discovery;
- speak/pause/resume/stop lifecycle;
- cancellation;
- backend-neutral playback/session events;
- UI backend/voice selection;
- tests for state transitions where practical.

Exit: non-PDF text can be spoken using both Piper and Windows TTS without reader-state special cases.

## Gate 2 — Non-PDF reader/TTS parity

Verify/fix:

- EPUB;
- TXT;
- Markdown;
- HTML path;
- sentence mapping;
- click-to-play;
- highlight;
- auto-scroll/jump;
- search;
- bookmarks/config/cache;
- close/cancel semantics.

Exit: representative non-PDF documents are comfortable and deterministic on Windows.

## Gate 3 — Native PDF visual stability

Use the existing detailed native PDF roadmap as subsystem guidance.

Focus first on:

- page raster;
- texture cache;
- viewport scheduling;
- zoom/scroll;
- memory/performance;
- stable rendering independent of TTS.

Exit: PDFs are visually usable before synchronization complexity is layered on top.

## Gate 4 — PDF text, TTS, and highlight synchronization

Then complete:

- extraction;
- canonical sentence/page mapping;
- geometry confidence;
- overlay lifecycle;
- jump/follow behavior;
- playback transition smoothness;
- OCR/degraded modes;
- regression corpus.

Exit: representative text PDFs maintain reliable speech/highlight position; poor PDFs degrade explicitly rather than drift silently.

## Gate 5 — Format expansion and ingestion cleanup

- HTML hardening if still incomplete;
- DOCX/Word ingestion;
- common source/document boundaries;
- fixtures and format-level regression tests.

Exit: source format support no longer requires format-specific TTS semantics.

## Gate 6 — Ergonomics, performance, packaging

- startup/TTS latency;
- UI cleanup;
- large-document behavior;
- Calibre/import polish;
- release packaging;
- dependency cleanup justified by measured problems.

## Director rule

ChatGPT may compress several related passes into one macro-goal when doing so removes needless human/Codex round trips without opening architectural ambiguity.

Detailed older subsystem roadmaps remain useful beneath these gates, especially:

- `egui-migration-master-roadmap.md`;
- `egui-state-and-runtime-roadmap.md`;
- `egui-reader-rendering-roadmap.md`;
- `egui-tts-audio-and-playback-roadmap.md`;
- `egui-native-pdf-roadmap.md`;
- `egui-testing-and-parity-roadmap.md`;
- PDF rendering/sync/OCR roadmaps.
