# LanternLeaf Restart Master Roadmap — September 2026

This is the active restart roadmap. It orders the detailed roadmaps against the current Windows/native-egui restart.

Roadmap state is evidence-driven. Completion means accepted implementation plus validation, not historical checkbox state.

## Gate 0 — Recover trustworthy baseline

**STATUS: COMPLETE**

Goals 0001-0005 established reproducible Windows CI, deterministic test/cache behavior, truthful renderer capability separation, and repaired bounded PDF core contracts.

## Gate 1 — TTS backend boundary + Windows TTS

**STATUS: COMPLETE AT ARCHITECTURE / SYNTHESIS LAYER**

Accepted flow:

`canonical sentence -> backend synthesis -> cached WAV -> shared Rodio/Sonic playback -> session progression`

Piper and WinRT Windows synthesis share the same playback/session model.

## Workflow UX — automatic macro-goal completion notification

**STATUS: IMPLEMENTED / MULTI-ATTEMPT HARDENED**

Repository goal identity is durable; Codex Goal sessions are disposable execution attempts.

Correction attempts reuse the repository goal ID, re-arm the detached watcher, push before signaling terminal state, and do not require the human to operate notification plumbing.

## Gate 2 — Non-PDF reader/TTS parity

**STATUS: AUTOMATED PARITY COMPLETE — REPO-NATIVE REAL-DESKTOP SIGNOFF PENDING**

Goal 0006 re-proved current Rust-native behavior for:

- TXT;
- Markdown;
- HTML;
- EPUB.

Accepted automated evidence covers:

- representative deterministic fixtures;
- canonical text/sentence ownership;
- canonical syntax cleanliness;
- search navigation;
- sentence click/highlight contracts;
- native pretty structures;
- anchor fallback;
- auto-scroll decision logic;
- persistence/reopen/cleanup;
- simulated TTS across all four formats;
- Piper/Windows reader-state neutrality;
- real source/session -> Windows synthesis continuity.

Gate 2 still requires a bounded real-Windows manual signoff for visible/interactive behavior that hosted GPU-less CI cannot prove.

Goal 0007's downloadable QA-bundle approach was technically valid but rejected as human-workflow friction.

The replacement is repo-native:

`git pull -> .\qa.ps1`

with `deps.ps1` as the checked-in idempotent Windows dependency/bootstrap contract. Ordinary QA must not require downloading CI artifacts.

Gate 2 exit:

- automated Goal 0006 evidence accepted;
- representative TXT/Markdown/HTML/EPUB GUI smoke completed on a real Windows desktop;
- Windows voice selection and speaker playback confirmed;
- major interaction defects, if any, are repaired or explicitly bounded.

## Gate 2.5 — First-class Caliberate library service

**STATUS: GOAL 0008 READY**

Before moving fully into PDF work, connect the now-working native reader to the user's existing Caliberate library service.

Target relationship:

`Caliberate -> HTTP/JSON v1 at 127.0.0.1:8181 -> LanternLeaf library browser -> materialized source -> existing reader/TTS pipeline`

Goal 0008 keeps one browser UI, adds a narrow provider boundary, makes Caliberate the preferred local provider, and preserves legacy Calibre content-server compatibility.

Exit:

- paged Caliberate catalog appears through the existing library browser;
- a supported book format materializes from the Caliberate content endpoint;
- the materialized book enters the existing LanternLeaf session/TTS path;
- legacy Calibre remains available behind the provider adapter;
- real Windows QA confirms a Caliberate-sourced book can be opened and spoken.

## Gate 3 — Native PDF visual stability

**NEXT CORE RENDERING GATE AFTER CALIBERATE INTEGRATION / REMAINING GATE 2 SIGNOFF**

Focus:

- page raster;
- texture cache;
- viewport scheduling;
- zoom/scroll;
- memory/performance;
- stable rendering independent of TTS.

## Gate 4 — PDF text, TTS, and highlight synchronization

After visual stability:

- canonical sentence/page mapping;
- geometry confidence;
- overlay lifecycle;
- jump/follow behavior;
- playback transition smoothness;
- OCR/degraded modes;
- regression corpus.

## Gate 5 — Format expansion and ingestion cleanup

- DOCX/Word hardening;
- common source/document boundaries;
- format-level regression tests beyond the Gate 2 reader families.

## Gate 6 — Ergonomics, performance, packaging

- startup/TTS latency;
- UI cleanup;
- large-document behavior;
- Calibre/import polish;
- release packaging;
- dependency cleanup justified by measured problems.

## Director rule

ChatGPT may compress several related passes into one macro-goal when doing so removes needless human/Codex round trips without opening architectural ambiguity.

Detailed subsystem roadmaps remain subordinate evidence.
