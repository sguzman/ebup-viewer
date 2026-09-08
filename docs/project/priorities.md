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

## P2.5 — Connect LanternLeaf to Caliberate as a first-class library service — A5.1 SESSION AUTHORITY CORRECTION

- use Caliberate's versioned HTTP/JSON API rather than direct database coupling;
- default local provider target `http://127.0.0.1:8181`;
- page/catalog/search-compatible provider boundary behind the existing library browser;
- stream/materialize supported formats into the normal LanternLeaf reader/session pipeline;
- preserve legacy Calibre content-server compatibility;
- keep the human workflow repo-native: `git pull -> .\qa.ps1`;
- preserve A3 valid-EPUB materialization/native-ingestion correctness;
- A4 now removes synchronous whole-book normalization from reader open and idle snapshot;
- A4 bounds first TTS activation to a 64-display-sentence lazy plan window and precompiles stable normalizer matchers;
- deterministic 3,500-sentence regression evidence is green;
- A4 now proves the large real EPUB can enter Reader mode in an acceptable ~1–2 seconds;
- A5 must eliminate per-frame deep cloning of the 104k catalog/heavy reader payloads;
- A5 must virtualize/bound pretty rendering instead of rebuilding all 1,644+ blocks per egui frame;
- A5 must move TTS control/planning off the egui main thread and remove the observed Play-triggered stack overflow;
- A5 must make repo-native `qa.ps1` launch a representative optimized interactive profile;
- require one successful real-desktop large-EPUB responsiveness + Windows TTS verification before Gate 2.5 is complete.

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


A5.1 correction priority:
- preserve Arc-backed frame-state and bounded pretty-render improvements from the rejected A5 implementation;
- replace cloned effect/TTS ReaderSession values with one canonical shared session handle;
- remove full ReaderSnapshot construction from TTS worker plan/progress/control paths;
- remove full ReaderSnapshot construction from persistence flushes;
- add deterministic 10k+ sentence tests proving zero heavyweight snapshots and cross-path cursor authority.
