# LanternLeaf Restart Master Roadmap — September 2026

This is the active restart roadmap. It does not erase older detailed roadmaps; it orders them against the current Windows/native-egui restart.

Roadmap state is evidence-driven. Completion means accepted implementation plus validation, not old checkbox history.

## Gate 0 — Recover trustworthy baseline

**STATUS: IN PROGRESS, NEAR CLOSURE**

Goal 0001 recovered Windows check/build and removed libclang/bindgen blockers.

Goal 0002 made native startup errors observable, made early-exit smoke behavior truthful, provisioned MSVC/Pandoc in Windows CI, added prerequisite diagnostics, and repaired several fixture/config defects.

Director review of Goal 0002 found four additional failures in the normal parallel Windows run that were hidden by Codex's single-thread audit:

- two cache-root environment race failures;
- one CRLF/LF source fixture failure;
- one PDF alignment failure requiring race-vs-product classification.

The active Tauri/React `gui-migration` workflow is also obsolete and must leave the active CI surface.

Goal 0003 begins by closing these residual baseline issues. It then continues directly into the stable bounded PDF contract failures so the required Windows test gate can become green without another human/Codex round trip.

Exit:

- current native CI surface reflects only the native egui architecture;
- workspace check/build pass;
- normal parallel workspace tests pass;
- native startup smoke executes truthfully;
- external prerequisites are deterministic and diagnosable.

## Gate 1 — TTS backend boundary + Windows TTS

**NEXT MAJOR FEATURE AFTER GOAL 0003**

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

The current Goal 0003 only repairs **existing deterministic PDF core contracts** required to close the baseline. It is not authorization for broad visual PDF work.

The later visual gate focuses on:

- page raster;
- texture cache;
- viewport scheduling;
- zoom/scroll;
- memory/performance;
- stable rendering independent of TTS.

Exit: PDFs are visually usable before synchronization complexity is layered on top.

## Gate 4 — PDF text, TTS, and highlight synchronization

The Goal 0003 PDF substage may repair bounded classifier/OCR/reading-order/cache contracts already represented by failing tests. It must not expand into full interactive sync work.

The later full synchronization gate covers:

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
- `pdf-type-and-text-layer-classification-roadmap.md`;
- `pdf-ocr-geometry-aware-alignment-roadmap.md`;
- `native-pdf-rendering-and-text-sync-roadmap.md`.
