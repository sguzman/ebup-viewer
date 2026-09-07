# LanternLeaf Restart Master Roadmap — September 2026

This is the active restart roadmap. It orders the existing detailed roadmaps against the current Windows/native-egui restart.

Roadmap state is evidence-driven. Completion means accepted implementation plus validation, not historical checkbox state.

## Gate 0 — Recover trustworthy baseline

**STATUS: COMPLETE**

Goals 0001-0005 established:

- reproducible Windows MSVC/Pandoc CI;
- green workspace check/build/normal-parallel tests;
- truthful separation of hosted renderer capability from required CI;
- deterministic cache/test isolation;
- repaired bounded PDF core contracts.

## Gate 1 — TTS backend boundary + Windows TTS

**STATUS: COMPLETE AT ARCHITECTURE / SYNTHESIS LAYER**

Accepted flow:

`canonical sentence -> backend synthesis -> cached WAV -> shared Rodio/Sonic playback -> session progression`

Verified:

- Piper retained as default;
- WinRT Windows voice discovery and stable selected/default identity;
- backend-aware cache identity;
- active backend/voice resynchronization;
- Windows WAV synthesis and shared Rodio decode;
- green Windows CI.

Interactive speaker/UI comfort is intentionally part of Gate 2 reader parity rather than reopening Gate 1.

## Workflow UX — automatic macro-goal completion notification

**STATUS: IMPLEMENTED / CI-COVERED**

Codex launches a detached watcher on Windows. After the terminal branch is pushed, Codex signals a checkout-safe `.git` sentinel; the watcher notifies Completed/Blocked exactly once and exits. Notification failure is non-fatal.

## Gate 2 — Non-PDF reader/TTS parity

**STATUS: ACTIVE — GOAL 0006**

Goal 0006 re-proves current native behavior for:

- TXT;
- Markdown;
- HTML;
- EPUB.

Required parity domains:

- canonical `tts_text` and sentence ownership;
- pretty/text-only projection consistency;
- sentence click/highlight/navigation;
- TTS play/pause/play-from/prev/next/repeat/cancellation;
- search;
- auto-scroll/jump mapping contracts;
- persistence/bookmark/cache reopen;
- deterministic representative fixtures;
- Piper/Windows backend neutrality at reader-state level.

Old HTML/EPUB migration checkboxes are not proof. Goal 0006 builds a new parity matrix from current Rust tests and runtime evidence.

Gate 2 exit requires automated parity evidence plus a bounded real-Windows manual QA signoff for the interaction/fidelity pieces that cannot be honestly proven on hosted GPU-less CI.

## Gate 3 — Native PDF visual stability

**NEXT AFTER GATE 2**

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

Detailed older subsystem roadmaps remain subordinate evidence, especially:

- `egui-reader-rendering-roadmap.md`;
- `egui-tts-audio-and-playback-roadmap.md`;
- `egui-testing-and-parity-roadmap.md`;
- `native-html-epub-rendering-and-tts-sync-roadmap.md`;
- PDF roadmaps for later Gates 3/4.
