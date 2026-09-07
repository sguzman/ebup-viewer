# LanternLeaf Current Status

Updated: 2026-09-06 after review of Goal 0001

This file contains verified or explicitly bounded evidence only. Roadmap checkboxes and historical parity claims are not accepted as current proof.

## Workspace / architecture

**VERIFIED PRESENT**

- Rust workspace contains root package plus `lanternleaf-core`, `lanternleaf-app`, and `lanternleaf-egui`.
- Root `src/main.rs` dispatches TTS worker mode and otherwise calls `lanternleaf_egui::app::run()`.
- Native Rust + egui remains the authoritative desktop architecture.

## Windows build

**VERIFIED WORKING IN WINDOWS CI / LOCAL TOOLCHAIN INCOMPLETE**

Goal 0001 removed the runtime `bindgen` / `libclang` requirement from both native dependency chains that blocked the restart:

- Sonic: `bindgen 0.59.2 -> sonic-rs-sys 0.1.9`
- eSpeak: `bindgen 0.69.5 -> espeak-rs-sys 0.1.9 -> espeak-rs 0.1.9 -> piper-rs 0.1.9`

The vendored FFI declarations were reviewed against the vendored C headers and match the used ABI surface.

Windows GitHub Actions now proves:

- `cargo check --workspace` passes;
- `cargo build --workspace` passes.

The maintainer machine still lacks the Visual Studio/MSVC C/C++ toolchain required by the `x86_64-pc-windows-msvc` native build. LLVM-MinGW is not treated as an ABI-compatible substitute.

## Egui shell

**PRESENT, BUILDS, RUNTIME NOT YET VERIFIED**

The native binary is produced on Windows CI.

The Goal 0001 CI launch check is **not accepted as proof of a working GUI**. It treats an early code-0 process exit as success, while `crates/lanternleaf-egui/src/app/mod.rs` currently discards the result of `eframe::run_native(...)` via `let _ = ...`.

This creates a false-positive path where native window startup can fail and the process still exits successfully.

Goal 0002 must make native startup failures observable and make the smoke check truthful.

## EPUB / TXT / Markdown

**PRESENT BUT UNVERIFIED END-TO-END**

The code compiles and related tests execute, but restart-era interactive reader/TTS behavior has not yet been established.

## HTML

**PARTIAL**

The current source pipeline recognizes native HTML and uses Pandoc for canonical plain-text extraction. Existing HTML tests fail in the current Windows CI environment when Pandoc is absent.

This is a real current external-tool dependency and test-environment issue, not yet proof of an HTML rendering defect.

## DOC / DOCX

**PRESENT IN THE SOURCE PIPELINE, UNVERIFIED**

The source pipeline currently advertises `.doc` and `.docx` support through the Pandoc conversion path. User-facing completeness is unverified.

## Piper TTS

**COMPILES ON WINDOWS / RUNTIME UNVERIFIED**

Goal 0001 verified the Piper/eSpeak/Sonic dependency chain can build in Windows CI without libclang.

No restart-era audio playback behavior has been verified.

## Native Windows TTS

**NOT IMPLEMENTED**

This remains the next major feature gate after the baseline is trustworthy.

## PDF rendering

**PARTIAL**

Native egui PDF modules compile, but visual runtime behavior is not yet verified. Historical code and roadmaps are substantial; the user reports the native PDF implementation was never finished.

## PDF text extraction

**PARTIAL / TEST DRIFT PRESENT**

Existing core tests expose PDF fixture/expectation drift. Goal 0001 intentionally did not redesign PDF behavior.

## PDF TTS / highlight synchronization

**PARTIAL / KNOWN QUALITY AREA**

Historical commits show extensive attempts to stabilize PDF TTS follow-scroll, highlight rebinding, and stale-state problems. The user reports this area was never completed satisfactorily. No restart-era runtime verification exists yet.

## Calibre integration

**PRESENT BUT UNVERIFIED**

Recent implementation exists and compiles. Runtime behavior has not yet been exercised in the restart.

## Browser/import integrations

**PRESENT BUT UNVERIFIED**

Do not assume historical parity claims remain true until exercised.

## Persistence / bookmarks / config / cache

**PARTIAL**

Substantial implementation exists. Some current tests fail around shared temporary/cache/config state, indicating test isolation or behavior drift that Goal 0002 must classify and repair.

## Tests / CI

**PARTIAL**

Windows CI currently proves workspace check/build.

The latest reviewed CI run completed the core test binary with:

- 153 passed;
- 14 failed.

Observed failure classes include:

- missing Pandoc for HTML conversion tests;
- PDF fixture/contract drift;
- PDF OCR/normalization expectations;
- shared temp/cache/config interactions;
- abbreviation/live-config expectations.

The native launch smoke currently produces a false positive and must be corrected.

## Historical Tauri / React / WebView implementation

**HISTORICAL / OBSOLETE AS PRODUCTION TARGET**

It remains useful as behavioral evidence only.
