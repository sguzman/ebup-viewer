# LanternLeaf Restart Master Roadmap — September 2026

This is the active restart roadmap. It does not erase older detailed roadmaps; it orders them against the current Windows/native-egui restart.

Roadmap state is evidence-driven. Completion means accepted implementation plus validation, not old checkbox history.

## Gate 0 — Recover trustworthy baseline

**STATUS: COMPLETE**

Goals 0001-0003 repaired the build/test substrate and deterministic core/PDF failures.

Goal 0004 completed the final CI topology correction.

Authoritative hosted Windows evidence now proves:

- prerequisites provision;
- `cargo check --workspace`;
- `cargo build --workspace`;
- normal-parallel `cargo test --workspace`.

The hosted renderer probe is a separate capability channel and truthfully reports the known graphics limitation without blocking required CI.

Interactive GUI launch remains a later real-machine verification item, not a Gate 0 blocker.

## Gate 1 — TTS backend boundary + Windows TTS

**STATUS: IMPLEMENTED IN GOAL 0004; CORRECTNESS CLOSURE IN GOAL 0005**

Goal 0004 implemented:

`canonical sentence -> backend synthesis -> cached WAV -> shared Rodio/Sonic playback -> session progression`

with:

- Piper retained as default;
- WinRT Windows voice discovery;
- selected/default Windows voice synthesis;
- backend-aware sentence cache;
- shared playback;
- config/runtime/UI backend and voice controls.

Director review identified two correctness holes that must close before Gate 1 is called complete:

- active backend/voice settings changes do not currently force immediate TTS resynchronization;
- the implicit Windows-default cache identity does not resolve to the actual current default voice ID.

Goal 0005 fixes these plus backend-specific initialization/error-message cleanup and stronger WAV decode evidence.

Gate 1 exit:

- backend/voice changes during active speech rebuild safely from canonical state;
- cache identity uses the actual synthesis voice/model identity;
- Piper-only initialization stays Piper-only;
- Windows synthesized output is explicitly proven readable by the shared decoder path;
- required Windows CI stays green.

## Workflow UX — automatic macro-goal completion notification

**STATUS: ACTIVE IN GOAL 0005**

From Goal 0005 onward, Codex owns launching a detached Windows watcher.

The human should receive exactly one terminal desktop notification when the repo goal reaches `done/` or `blocked/`, with no extra command and no intermediate-turn spam.

Notification failure must never alter product-goal correctness.

## Gate 2 — Non-PDF reader/TTS parity

**NEXT MAJOR PRODUCT GATE AFTER GOAL 0005**

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
- close/cancel semantics;
- Piper/Windows backend switching on representative text sources.

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
