# LanternLeaf Current Status

Updated: 2026-09-08 after Goal 0008 A4 reader-normalization correction acceptance/integration

This file contains verified or explicitly bounded evidence only. Historical roadmap checkboxes are not accepted as current proof.

## Workspace / architecture

**VERIFIED PRESENT**

- Native Rust + `eframe`/`egui` is the authoritative desktop architecture.
- Workspace contains the root package plus `lanternleaf-core`, `lanternleaf-app`, and `lanternleaf-egui`.
- Tauri/React/WebView remains historical reference only.

## Gate 0 — Windows baseline

**COMPLETE**

Required hosted Windows CI proves:

- MSVC/Pandoc prerequisite setup;
- `cargo check --workspace`;
- `cargo build --workspace`;
- normal-parallel `cargo test --workspace`.

Hosted renderer capability remains a separate truthful probe and does not block required build/test evidence.

## Gate 1 — TTS backend boundary + Windows TTS

**COMPLETE AT ARCHITECTURE / SYNTHESIS LAYER**

Active flow:

`canonical sentence -> selected synthesis backend -> cached WAV -> shared Rodio/Sonic playback -> canonical session progression`

Verified:

- Piper remains the default backend;
- WinRT Windows TTS uses installed stable voice IDs;
- implicit Windows default resolves to the actual effective voice before cache identity;
- backend/voice changes during active speech resynchronize without losing canonical cursor ownership;
- Piper/eSpeak setup is Piper-only;
- Windows WAV output decodes through shared Rodio;
- missing configured Windows voices fail explicitly.

Real speaker playback and egui interaction remain real-desktop verification items.

## Goal-completion notifications

**IMPLEMENTED / MULTI-ATTEMPT PROTOCOL DEFINED**

Repository macro-goals and Codex Goal UI sessions are now explicitly separate lifecycles.

A director rejection may reopen the same repository goal under a fresh Codex Goal session. The watcher is re-armed per execution attempt, terminal state is signaled only after push, and checkout restoration cannot erase the checkout-safe `.git/lanternleaf-goal-state/<id>.terminal` signal.

The human does not manually start/reset the watcher.

## Gate 2 — Non-PDF reader/TTS parity

**AUTOMATED PARITY COMPLETE; REAL-DESKTOP SIGNOFF PENDING**

Goal 0006 established restart-era evidence for:

- TXT;
- Markdown;
- HTML;
- EPUB.

Representative project-owned fixtures now exercise the real source -> session path.

Verified automated behavior includes:

- source ingestion and canonical `tts_text`;
- source-family syntax cleanliness;
- canonical sentence/page accounting;
- sentence anchors;
- text-only/pretty ownership invariants;
- real-session search set/next/previous selection;
- sentence click/highlight ownership;
- persistence/reopen;
- idempotent source/cache cleanup;
- Rust-native Markdown/HTML/EPUB pretty structures;
- exact/nearest anchor fallback behavior;
- auto-scroll duplicate/fallback decision semantics;
- simulated TTS runtime behavior across all four source families on the Pandoc-capable Windows runner;
- backend-neutral reader cursor semantics;
- source -> real TXT session -> canonical sentence -> Windows synthesis -> Rodio decode.

Authoritative correction run `34159728934` passed both Windows jobs.

Observed final test evidence includes:

- core unit tests: 171 passed / 0 failed;
- non-PDF source/session parity: green;
- non-PDF simulated runtime parity: green;
- native pretty parity: 3 passed / 0 failed;
- Windows TTS integration: 2 passed / 0 failed.

Goal 0006 also fixed bounded Markdown canonicalization so raw representative Markdown syntax is not spoken as canonical text.

### Real-desktop evidence now observed

A repo-native Windows run using `git pull -> .\qa.ps1` successfully completed dependency bootstrap, built LanternLeaf, and entered the native egui shell on the human Windows machine. The run used isolated `.qa/windows` state, so an initially empty library is expected until files or an external library source are opened.

This closes the basic real-desktop build/launch uncertainty. It does **not** yet prove speaker playback, voice selection, visible render quality, or end-user reader ergonomics.

### Remaining Gate 2 evidence

Hosted CI cannot honestly prove:

- visible pretty/text rendering quality;
- actual window scrolling/jump comfort;
- physical speaker playback;
- interactive Windows voice selection;
- end-user play/pause/seek ergonomics;
- reopen behavior as experienced through the GUI.

A concise Windows checklist exists at `docs/qa/non-pdf-reader-windows-checklist.md`.

Goal 0007 proved that a prebuilt QA bundle could be produced, but the human rejected artifact download/extraction as unnecessary workflow friction.

The accepted human workflow is now repo-native:

- `deps.ps1` owns Windows dependency/bootstrap state;
- `qa.ps1` owns isolated real-desktop QA preparation/build/launch;
- generated QA state/fixtures/logs live under ignored `.qa/`;
- GitHub Actions artifacts are not part of ordinary manual testing.

## PDF

**CORE CONTRACT SET REPAIRED; INTERACTIVE VISUAL/TTS WORK PENDING**

Goal 0003 repaired the bounded classifier/OCR/reading-order/cache contract set.

Gate 3 native PDF visual stability begins only after Gate 2 real-desktop signoff.

## Caliberate / Calibre library integration

**A4 ACCEPTED — REAL-DESKTOP READER + WINDOWS TTS SIGNOFF PENDING**

Accepted integration:

- Caliberate is the preferred/default local provider at `http://127.0.0.1:8181`;
- paged `/api/v1/books` catalog retrieval maps into the existing library browser;
- supported formats materialize from `/api/v1/books/{id}/content/{format}`;
- materialized sources enter the existing LanternLeaf source/session/TTS path;
- provider-aware caches prevent Caliberate/legacy Calibre cross-contamination;
- Caliberate API-key auth and legacy Basic auth remain isolated;
- Caliberate does not probe legacy Calibre cover endpoints;
- legacy Calibre catalog/download behavior remains covered by deterministic HTTP regression tests.

Authoritative Windows run `34185172624` passed both jobs and the core suite reached **180 passed / 0 failed**.

A3 remains accepted for Caliberate materialization/native EPUB ingestion/cache recovery. A4 is now accepted and integrated for the reader-startup defect exposed on the human machine: synchronous whole-book normalization has been removed from source open; idle snapshots avoid TTS-plan construction; first TTS activation prepares a bounded 64-sentence window; sentence cache population is bounded/lazy; and stable normalizer regex/token matchers are compiled once per normalizer configuration. A deterministic 3,500-sentence regression verifies zero normalization cache work at idle open, bounded first-Play work, and cursor progression across window boundaries. Windows CI run `34260546736` passed the normal workspace validation path. One real-desktop EPUB open plus immediate Windows TTS interaction remains before Gate 2.5 exit.

## Historical Tauri / React / WebView implementation

**HISTORICAL / OBSOLETE AS PRODUCTION TARGET**

It may be consulted for behavioral evidence only.
