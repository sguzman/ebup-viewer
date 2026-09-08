# LanternLeaf Priorities

These priorities are ordered. They may be revised by ChatGPT/director as verified implementation evidence changes.

## P0 — Recover a trustworthy Windows baseline

- reproducible Windows build;
- native egui app launches;
- workspace tests/build gates are meaningful;
- actual current functionality is inventoried from code/runtime evidence;
- Windows CI catches regressions.

## P1 — Stabilize TTS architecture and add native Windows TTS

- make backend ownership explicit;
- preserve Piper;
- add Windows-native voice enumeration/playback;
- keep reader/session semantics backend-neutral;
- verify cancellation, pause/stop, and event semantics.

## P2 — Verify and stabilize non-PDF reading

- EPUB/TXT/Markdown;
- HTML where present/targeted;
- sentence identity;
- highlighting;
- click-to-play;
- bookmarks/config/search/navigation.

## P2.5 — Connect LanternLeaf to Caliberate as a first-class library service — A4 REOPENED FOR READER STARTUP

- use Caliberate's versioned HTTP/JSON API rather than direct database coupling;
- default local provider target `http://127.0.0.1:8181`;
- page/catalog/search-compatible provider boundary behind the existing library browser;
- stream/materialize supported formats into the normal LanternLeaf reader/session pipeline;
- preserve legacy Calibre content-server compatibility;
- keep the human workflow repo-native: `git pull -> .\qa.ps1`;
- preserve A3 valid-EPUB materialization/native-ingestion correctness;
- remove synchronous whole-book normalization from reader open and initial snapshot;
- make first TTS activation bounded rather than moving the same stall behind Play;
- precompile stable normalizer regex/token machinery instead of recompiling per sentence;
- require deterministic large-EPUB regression evidence before another real-desktop test.

## P3 — Make native PDF rendering reliable

- page rendering;
- zoom/viewport lifecycle;
- cache/texture management;
- performance on representative documents.

## P4 — Complete PDF text/TTS/highlight synchronization

- extraction quality;
- sentence/page mapping;
- geometry overlays;
- jump/follow behavior;
- degraded confidence modes;
- regression tests for prior drift/jitter bugs.

## P5 — Broaden format ingestion

- HTML hardening;
- DOCX/Word;
- common document ingestion boundaries;
- format fixtures and regression coverage.

## P6 — Ergonomics and performance

Only after the preceding foundations are trustworthy:

- UI cleanup;
- startup latency;
- TTS latency;
- large-document performance;
- library workflow polish.
