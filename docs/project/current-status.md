# LanternLeaf Current Status

Updated: 2026-09-07 after director review of Goal 0005

This file contains verified or explicitly bounded evidence only. Historical roadmap checkboxes are not accepted as current proof.

## Workspace / architecture

**VERIFIED PRESENT**

- Native Rust + `eframe`/`egui` is the authoritative desktop architecture.
- Workspace contains the root package plus `lanternleaf-core`, `lanternleaf-app`, and `lanternleaf-egui`.
- Tauri/React/WebView remains historical reference only.

## Gate 0 — Windows baseline

**COMPLETE**

Authoritative hosted Windows run `34150175084` passed:

- prerequisite setup;
- `cargo check --workspace`;
- `cargo build --workspace`;
- normal-parallel `cargo test --workspace`;
- the uncaptured Windows TTS diagnostic probe.

Observed current test summaries include core unit tests at 170 passed / 0 failed and all remaining workspace suites green.

The hosted renderer check remains a separate capability probe and truthfully classifies the GitHub-hosted graphics limitation instead of blocking required build/test evidence.

## Gate 1 — TTS backend boundary + Windows TTS

**IMPLEMENTATION COMPLETE; INTERACTIVE AUDIO PARITY MOVES TO GATE 2**

The active TTS architecture is:

`canonical sentence -> selected synthesis backend -> cached WAV -> shared Rodio/Sonic playback -> canonical session progression`

Goal 0005 closed the two correctness defects found after Goal 0004:

- backend/voice setting changes during active speech now force TTS runtime resynchronization while preserving the canonical cursor;
- implicit Windows-default synthesis resolves the actual Windows default voice ID before cache identity and synthesis.

Additional verified properties:

- Piper remains the default backend;
- Piper/eSpeak environment initialization runs only for Piper;
- Windows uses WinRT `SpeechSynthesizer`;
- missing configured Windows voice IDs remain actionable errors;
- synthesized Windows WAVs decode through the shared Rodio decoder;
- backend-specific runtime diagnostics no longer falsely label every engine as Piper.

Hosted Windows evidence from run `34150175084`:

- `windows_tts_voice_count=3`;
- default voice resolved to the installed David voice ID;
- `windows_tts_synthesis=synthesized`;
- Rodio decoder verification passed.

Real speaker playback, egui voice selection, and long-running reader comfort remain interactive Gate 2 verification.

## Goal-completion notifications

**IMPLEMENTED AND HARDENED IN DIRECTOR REVIEW**

Goal 0005 created the detached Windows watcher. Director review found that its direct repository-state fallback assumed un-slugged goal filenames (`0005.md`) even though real files are named like `0005-tts-correctness-and-goal-notify.md`.

The permanent protocol is now:

1. Codex launches the detached watcher when the goal becomes active;
2. Codex completes, commits, and pushes the terminal implementation branch;
3. after push succeeds, Codex calls `scripts/signal-codex-goal-terminal.ps1`;
4. that helper writes `.git/lanternleaf-goal-state/<id>.terminal` and waits briefly for watcher acknowledgment;
5. the watcher emits exactly one Completed/Blocked notification and exits;
6. Codex restores the checkout to `main` regardless of notification delivery success.

The sentinel is outside the checked-out tree, so checkout restoration cannot race away the terminal signal. Windows CI now tests the watcher/signal scripts. Repository-state fallback remains available for diagnostics and understands slugged goal filenames.

Actual toast visibility is desktop-session behavior and remains best-effort; notification failure never changes product-goal correctness.

## Non-PDF reader

**PARTIAL / GOAL 0006 ACTIVE**

Current Rust ingestion smoke exists for TXT, Markdown, and HTML. EPUB/HTML native-pretty paths and extensive historical tests/code also exist.

However current accepted parity evidence is incomplete:

- the repository source-ingestion corpus is tiny;
- there is no representative current EPUB fixture in `tests/fixtures/source-ingestion/`;
- existing app integration tests mostly use synthetic snapshots rather than real source -> session -> TTS flows;
- old native-HTML/EPUB roadmap checkmarks are historical claims, not restart-era evidence;
- interactive pretty/text toggling, click-to-play, auto-scroll, search, and persistence have not been revalidated across all non-PDF source families.

Goal 0006 builds a current Rust-native parity matrix and representative corpus for TXT, Markdown, HTML, and EPUB.

## PDF

**CORE CONTRACT SET REPAIRED; INTERACTIVE VISUAL/TTS WORK STILL PENDING**

Goal 0003 repaired the bounded deterministic PDF classifier/OCR/reading-order/cache failures exposed by the baseline.

Native page rendering, viewport/zoom behavior, and PDF TTS/highlight synchronization remain later Gate 3/4 work.

## Calibre and browser/import integrations

**PRESENT BUT RESTART-ERA PARITY UNVERIFIED**

They are not part of Goal 0006 unless a directly shared reader defect requires a bounded fix.

## Historical Tauri / React / WebView implementation

**HISTORICAL / OBSOLETE AS PRODUCTION TARGET**

It may be consulted for behavioral evidence only.
