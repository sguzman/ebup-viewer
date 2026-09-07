# LanternLeaf Current Status

Updated: 2026-09-07 after director review of Goal 0003

This file contains verified or explicitly bounded evidence only. Roadmap checkboxes and historical parity claims are not accepted as current proof.

## Workspace / architecture

**VERIFIED PRESENT**

- Rust workspace contains root package plus `lanternleaf-core`, `lanternleaf-app`, and `lanternleaf-egui`.
- Root `src/main.rs` dispatches TTS worker mode and otherwise calls the native egui application.
- Native Rust + egui remains the authoritative desktop architecture.
- The obsolete active Tauri/React/Playwright migration workflow has been removed.

## Windows build

**VERIFIED WORKING IN WINDOWS HOSTED CI / LOCAL TOOLCHAIN INCOMPLETE**

The hosted Windows workflow successfully provisions:

- MSVC;
- CMake;
- Pandoc;
- stable Rust.

It then successfully runs:

- `cargo check --workspace`;
- `cargo build --workspace`.

The maintainer machine still lacks the Visual Studio/MSVC C/C++ workload and Pandoc. `scripts/windows-dev-check.ps1` reports those missing prerequisites explicitly.

## Egui shell

**STARTUP ERROR BOUNDARY VERIFIED; HOSTED-GPU RUNTIME VERIFICATION UNSUPPORTED**

Native startup errors are no longer discarded. The root process exits nonzero on `eframe::run_native` failure and emits actionable diagnostics.

Goal 0003 proved the current hosted `windows-latest` environment cannot validate the shipped renderer:

- the glow path sees only an OpenGL 1.1 context and fails because egui/eframe requires OpenGL 2.0+;
- a bounded eframe 0.26.2 WGPU experiment also found no suitable WGPU adapter;
- the experiment was reverted, so no unverified renderer migration remains in `main`.

This is an environment capability boundary, not evidence that the app fails on a normal Windows desktop.

The next CI shape must therefore separate:

1. required non-interactive Windows build/test verification; and
2. renderer-capability/runtime verification that requires a real graphics-capable Windows environment.

Do not mark hosted renderer failure as application launch success.

## Test isolation

**REPAIRED IN GOAL 0003; FULL HOSTED WORKSPACE RUN STILL PENDING CI TOPOLOGY FIX**

Goal 0003 removed process-global environment mutation from cache/normalizer/text-utils tests and made test source/cache identities collision-resistant under parallel execution.

Local normal-parallel core tests reached:

- 165 passed;
- 2 failed only because local Pandoc is unavailable.

The hosted workflow provisions Pandoc, but its workspace test step did not execute because the renderer smoke failed first. Goal 0004 Stage A must decouple those gates and obtain the authoritative hosted full-workspace result.

## TXT / Markdown / HTML ingestion

**RUST-LAYER SMOKE PRESENT; END-TO-END READER/TTS UNVERIFIED**

- Repository fixtures have explicit LF normalization.
- TXT and Markdown ingestion smoke exists.
- HTML tests pass when Pandoc is provisioned.
- Pandoc remains a current prerequisite for HTML canonical-text and DOC/DOCX conversion paths.

## Piper TTS

**IMPLEMENTED / BUILDS ON WINDOWS / RESTART-ERA AUDIO UNVERIFIED**

Current TTS design is Piper-specific at the synthesis-engine boundary but already has a reusable product shape:

- canonical reader sentences are prepared in batches;
- sentence audio is cached as WAV;
- Rodio owns playback;
- Sonic owns playback speed transformation;
- volume, pause/resume, stop, and sentence progression are runtime-level behaviors.

This shape will be preserved while the synthesis backend becomes explicit.

## Native Windows TTS

**NOT IMPLEMENTED**

Goal 0004 is the active implementation goal.

Director-selected API direction:

- use WinRT `Windows.Media.SpeechSynthesis::SpeechSynthesizer`;
- enumerate installed voices through `AllVoices`;
- identify voices by Windows voice ID;
- synthesize each canonical sentence to the API's WAV speech stream;
- feed those WAVs into the existing shared cache/playback pipeline rather than creating a separate OS-owned playback state machine.

## PDF classification / OCR / text contracts

**BOUNDED GOAL-0003 CONTRACT SET REPAIRED**

Goal 0003 repaired the deterministic classifier/OCR/reading-order/cache contract failures exposed by the restart baseline.

The targeted source-pipeline PDF suite reports 17 passed.

These repairs do not prove full interactive PDF quality.

## PDF rendering

**PARTIAL / INTERACTIVE RUNTIME UNVERIFIED**

Native egui PDF modules compile. Page raster/viewport/zoom/render behavior has not yet been revalidated interactively in the restart.

## PDF TTS / highlight synchronization

**PARTIAL / KNOWN QUALITY AREA**

Historical work is extensive, but the user reports native PDF rendering + TTS + highlight synchronization was never completed satisfactorily.

Interactive verification and later Gate 3/4 work remain pending.

## Calibre integration

**PRESENT BUT UNVERIFIED**

Implementation exists and compiles. Restart-era runtime behavior has not yet been exercised.

## Browser/import integrations

**PRESENT BUT UNVERIFIED**

Historical parity is not assumed.

## CI

**NATIVE WORKFLOW PRESENT; TOPOLOGY NEEDS ONE FINAL CORRECTION**

The active CI surface is now native-only.

Current defect: the renderer smoke is in the same sequential required job before `cargo test --workspace`, so the known hosted GPU limitation prevents the test gate from running.

Goal 0004 must split required build/test verification from hosted renderer capability probing and obtain a green required Windows baseline without falsely claiming hosted GUI launch success.

## Historical Tauri / React / WebView implementation

**HISTORICAL / OBSOLETE AS PRODUCTION TARGET**

It remains useful as behavioral evidence only.
