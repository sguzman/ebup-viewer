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

## P2.5 — Connect LanternLeaf to Caliberate as a first-class library service — A7.1 FOLLOW-STATE CORRECTION

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


A5.1 accepted:
- one canonical ReaderSession handle now serves normal reader effects, TTS, and persistence;
- TTS/persistence hot paths are structurally snapshot-free on 10k+ sentence regressions;
- accepted A5 Arc-backed frame state, bounded pretty rendering, off-main TTS submission, and optimized QA profile remain in place;
- require successful real-desktop large-EPUB responsiveness + Windows TTS Play/seek/pause/resume/stop before Gate 2.5 completion.


A6 correction priority:
- preserve the now-successful A5/A5.1 native large-EPUB responsiveness work;
- make omitted TTS backend platform-aware (Windows on Windows, Piper elsewhere);
- make normal `qa.ps1` deterministically stage/verify Windows TTS even with pre-existing QA state;
- surface backend/config synthesis failures visibly;
- prove staged Windows QA config through Windows voice synthesis + WAV decode before another human run.

A6 accepted:
- normal Windows QA now deterministically selects Windows TTS despite stale staged config;
- platform-aware omitted-backend defaults and portable Piper defaults are integrated;
- staged Windows QA synthesis/decode coverage and actionable native TTS errors are green;
- large-EPUB native responsiveness is already positively observed;
- final requirement: one real-desktop `.\qa.ps1` run with audible Windows speech plus next/previous/pause/resume/stop.


A7 correction priority:
- preserve confirmed fast/snappy large-EPUB native performance and working Windows speech/voice selection;
- replace normalized-text HashMap/proportional-anchor TTS targeting with ordered canonical display-sentence -> pretty target alignment;
- make auto-scroll target-aware and durable until successful scroll/supersession;
- drive follow behavior only from canonical display-cursor transitions;
- stabilize variable-height virtual scrolling without restoring full-document layout;
- add 10k-sentence / 1.5k-block duplicate/unmapped/100+ transition regressions before another human run.

Deferred, nonblocking: interactive Piper switching/provisioning in the normal Windows QA context should be revisited after Goal 0008; Windows TTS is sufficient for current A7 verification.


A7.1 correction priority:
- preserve first-A7 ordered canonical pretty-target mapping, no proportional TTS fallback, sentence-range highlighting, and variable-height bounded virtualization;
- repair explicit Jump-to-highlight so it actually re-arms the current canonical target;
- extract/test the production cursor-transition -> follow-target rule across 100+ advances and Next/Prev/Pause/Resume/Repeat/unchanged voice/settings events;
- make committed duplicate-follow identity source-aware;
- no human QA until A7.1 director acceptance.
