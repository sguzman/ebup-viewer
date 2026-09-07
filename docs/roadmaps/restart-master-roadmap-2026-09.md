# LanternLeaf Restart Master Roadmap — September 2026

This is the active restart roadmap. It does not erase older detailed roadmaps; it orders them against the current Windows/native-egui restart.

Roadmap state is evidence-driven. Completion means accepted implementation plus validation, not old checkbox history.

## Gate 0 — Recover trustworthy baseline

**STATUS: IMPLEMENTATION REPAIRS COMPLETE; FINAL HOSTED TEST PROOF MOVED INTO GOAL 0004 STAGE A**

Goals 0001-0003 completed the substantive recovery work:

- removed libclang/bindgen blockers;
- established reproducible MSVC/Pandoc hosted setup;
- made native startup errors observable;
- removed parallel environment/cache races;
- made source fixtures cross-platform deterministic;
- removed obsolete Tauri/React CI;
- repaired the bounded PDF contract failures exposed by the baseline.

Goal 0003 also proved a hard hosted-runner renderer limitation:

- glow sees only OpenGL 1.1;
- a bounded WGPU experiment found no suitable adapter;
- the experiment was reverted.

The remaining Gate 0 issue is CI topology, not product behavior: the known-host-limited renderer smoke currently prevents the hosted workspace tests from running.

Goal 0004 Stage A separates those evidence channels and obtains the final required Windows build/test result.

Gate 0 exit:

- required Windows check/build/tests are green;
- hosted renderer capability is classified honestly as unsupported when applicable;
- interactive GUI launch remains a separate real-machine verification item rather than a fake hosted pass.

## Gate 1 — TTS backend boundary + Windows TTS

**STATUS: ACTIVE — GOAL 0004**

Preserve Piper while making sentence-audio synthesis backend-neutral.

Director architecture:

`canonical sentence -> backend synthesis -> cached WAV -> shared Rodio/Sonic playback -> session progression`

Goal 0004 adds native Windows TTS using WinRT `Windows.Media.SpeechSynthesis::SpeechSynthesizer` with:

- installed voice discovery;
- stable voice-ID selection;
- sentence-to-WAV synthesis;
- backend-aware cache identity;
- shared pause/resume/stop/speed/volume semantics;
- backend/voice UI settings;
- Windows-hosted synthesis tests that do not require a graphics or audio device.

Exit: the same reader/TTS runtime can prepare and play sentence audio from both Piper and Windows backends without format-specific or UI-specific playback state.

## Gate 2 — Non-PDF reader/TTS parity

Verify/fix:

- EPUB;
- TXT;
- Markdown;
- HTML;
- sentence mapping;
- click-to-play;
- highlight;
- auto-scroll/jump;
- search;
- bookmarks/config/cache;
- close/cancel semantics.

Exit: representative non-PDF documents are comfortable and deterministic on Windows.

## Gate 3 — Native PDF visual stability

Focus on:

- page raster;
- texture cache;
- viewport scheduling;
- zoom/scroll;
- memory/performance;
- stable rendering independent of TTS.

Exit: PDFs are visually usable before synchronization complexity is layered on top.

## Gate 4 — PDF text, TTS, and highlight synchronization

The bounded classifier/OCR/reading-order/cache contracts exposed by baseline tests were repaired in Goal 0003.

Later full synchronization work covers:

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
