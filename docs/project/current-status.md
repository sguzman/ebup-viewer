# LanternLeaf Current Status

Updated: 2026-09-07 after director review of Goal 0002

This file contains verified or explicitly bounded evidence only. Roadmap checkboxes and historical parity claims are not accepted as current proof.

## Workspace / architecture

**VERIFIED PRESENT**

- Rust workspace contains root package plus `lanternleaf-core`, `lanternleaf-app`, and `lanternleaf-egui`.
- Root `src/main.rs` dispatches TTS worker mode and otherwise calls the native egui application.
- Native Rust + egui remains the authoritative desktop architecture.
- Tauri/React/WebView remains historical/obsolete as a production target.

## Windows build

**VERIFIED WORKING IN WINDOWS CI / LOCAL TOOLCHAIN INCOMPLETE**

Goal 0001 removed the runtime `bindgen` / `libclang` prerequisite from the Sonic and eSpeak dependency chains.

The current Windows baseline workflow on `windows-latest` successfully completes:

- MSVC environment setup;
- Pandoc provisioning;
- repository prerequisite diagnostics;
- `cargo check --workspace`;
- `cargo build --workspace`.

The maintainer machine still lacks the Visual Studio/MSVC C/C++ workload and Pandoc. `scripts/windows-dev-check.ps1` now reports these prerequisites explicitly.

## Egui shell

**BUILDS; STARTUP ERROR BOUNDARY FIXED; LIVE WINDOW STILL UNVERIFIED**

Goal 0002 changed the native startup boundary so `eframe::run_native(...)` errors are no longer discarded.

Current behavior:

- eframe startup errors propagate as `NativeRunError::Eframe`;
- the root process returns a non-zero result on native startup failure;
- the standalone egui binary prints the startup error and exits 1;
- single-instance lock I/O failure and an already-running instance remain distinct conditions.

The Windows smoke now rejects any early process exit, including code 0.

However, in the latest authoritative Windows CI run the smoke step was skipped because the test step failed first. A live egui event loop has therefore not yet been accepted as verified.

## TXT / Markdown

**RUST INGESTION SMOKE PRESENT; END-TO-END READER/TTS UNVERIFIED**

Repository-owned TXT and Markdown fixtures exist.

The current Windows parallel suite exposed a CRLF-vs-LF fixture expectation failure in the TXT smoke. This is a cross-platform harness defect to be repaired before the ingestion smoke is called deterministic.

## HTML

**RUST INGESTION TESTS PASS IN PROVISIONED WINDOWS CI; END-TO-END READER/TTS UNVERIFIED**

Pandoc is provisioned in the Windows baseline workflow.

The latest Windows CI run shows both HTML source tests passing.

Pandoc remains a current runtime/development prerequisite for the Pandoc-backed ingestion paths.

## DOC / DOCX

**PRESENT IN PANDOC-BACKED SOURCE PIPELINE, UNVERIFIED**

The source pipeline advertises `.doc` and `.docx` support through Pandoc conversion. User-facing completeness remains unverified.

## Piper TTS

**COMPILES ON WINDOWS / RUNTIME UNVERIFIED**

The Piper/eSpeak/Sonic dependency chain builds in Windows CI without libclang.

No restart-era audio behavior has been verified.

## Native Windows TTS

**NOT IMPLEMENTED**

This remains the next major feature gate after the baseline and current PDF contract-test cluster are made trustworthy.

## PDF rendering

**PARTIAL / VISUAL RUNTIME UNVERIFIED**

Native egui PDF modules compile. Visual page rendering, zoom/viewport behavior, and performance have not yet been verified in the restart.

## PDF classification / OCR / text contracts

**PARTIAL / KNOWN FAILING CONTRACT TESTS**

The latest Windows CI run preserves at least seven deterministic PDF contract failures involving:

- classifier/rollup policy;
- OCR recommendation/confidence;
- OCR normalization;
- reading-order classification;
- PDF cache metadata.

An additional cross-column OCR alignment test fails under normal parallel CI but was absent from Codex's single-thread rerun failure set. Its classification is not yet accepted as a product defect until concurrency/state leakage is ruled out.

## PDF TTS / highlight synchronization

**PARTIAL / KNOWN QUALITY AREA**

Historical work is extensive, but the user reports PDF rendering + TTS + highlight synchronization was never completed satisfactorily.

The active restart has not yet reached interactive PDF/TTS verification.

## Cache / config / persistence

**PARTIAL; PARALLEL TEST ISOLATION DEFECT CONFIRMED**

A test mutates the process-wide `LANTERNLEAF_CACHE_DIR` environment variable while other cache/session tests run in parallel.

The authoritative Windows CI run shows cache roundtrip tests failing to reload their just-written metadata, consistent with cache-root switching during the test.

This is baseline test isolation work, not yet a PDF product defect.

## Calibre integration

**PRESENT BUT UNVERIFIED**

Implementation exists and compiles. Restart-era runtime behavior has not yet been exercised.

## Browser/import integrations

**PRESENT BUT UNVERIFIED**

Historical parity is not assumed.

## Tests / CI

**PARTIAL; CURRENT AUTHORITATIVE WINDOWS RESULT: 156 PASSED / 11 FAILED IN CORE**

Windows CI run on the accepted Goal 0002 rerun proved:

- prerequisite setup: PASS;
- workspace check: PASS;
- workspace build: PASS;
- HTML/Pandoc source tests: PASS;
- core test binary: 156 passed / 11 failed.

The 11 failures include:

- seven already-classified PDF contract failures;
- two cache roundtrip failures consistent with `LANTERNLEAF_CACHE_DIR` process-environment races;
- one TXT fixture CRLF/LF mismatch;
- one PDF alignment test that appears only in the parallel run and requires isolation analysis.

Codex's single-thread report of 158 passed / 9 failed is useful but is not the authoritative baseline because normal `cargo test` is parallel.

## CI architecture debt

**BROKEN / OBSOLETE ACTIVE WORKFLOW**

`.github/workflows/gui-migration.yml` is still actively running Tauri bridge, frontend, Playwright, and Tauri E2E jobs.

This conflicts with the active native egui architecture and must be removed from the active CI surface. Historical web/Tauri code and documentation may remain as reference.

## Historical Tauri / React / WebView implementation

**HISTORICAL / OBSOLETE AS PRODUCTION TARGET**

It remains useful as behavioral evidence only.
