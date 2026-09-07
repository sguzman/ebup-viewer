# LanternLeaf Current Status

Updated: 2026-09-07 after director review of Goal 0004

This file contains verified or explicitly bounded evidence only. Roadmap checkboxes and historical parity claims are not accepted as current proof.

## Workspace / architecture

**VERIFIED PRESENT**

- Rust workspace contains root package plus `lanternleaf-core`, `lanternleaf-app`, and `lanternleaf-egui`.
- Root `src/main.rs` dispatches TTS worker mode and otherwise calls the native egui application.
- Native Rust + egui remains the authoritative desktop architecture.
- The obsolete active Tauri/React/Playwright migration workflow has been removed.

## Gate 0 / Windows baseline

**COMPLETE**

Authoritative hosted Windows run `34146382188` passed:

- prerequisite setup;
- `cargo check --workspace`;
- `cargo build --workspace`;
- normal-parallel `cargo test --workspace`.

Observed test results in that run include:

- core unit tests: 168 passed / 0 failed;
- Windows TTS integration test: 1 passed / 0 failed;
- app/runtime integration suites: green.

The renderer check is now a separate capability probe and no longer blocks required build/test evidence.

## Egui shell

**STARTUP ERROR BOUNDARY VERIFIED; HOSTED-GPU RUNTIME VERIFICATION UNSUPPORTED**

Native startup errors are observable and nonzero.

The hosted renderer probe truthfully classifies the GitHub-hosted Windows environment as unavailable when it exposes only unsupported OpenGL/no-adapter capability. This is separate from product correctness.

Interactive real-desktop GUI verification remains pending.

## TXT / Markdown / HTML ingestion

**RUST-LAYER SMOKE PRESENT; END-TO-END READER/TTS UNVERIFIED**

- Repository fixtures have explicit LF normalization.
- TXT and Markdown ingestion smoke exists.
- HTML tests pass when Pandoc is provisioned.
- Pandoc remains a current prerequisite for HTML canonical-text and DOC/DOCX conversion paths.

## TTS architecture

**BACKEND-NEUTRAL SENTENCE SYNTHESIS IMPLEMENTED; CORRECTNESS FOLLOW-UP REQUIRED**

Goal 0004 implemented:

`canonical sentence -> selected backend -> cached WAV -> shared Rodio/Sonic playback -> canonical session progression`

Piper remains the default backend.

Shared playback still owns:

- Rodio audio output;
- Sonic speed transformation;
- volume;
- pause/resume/stop;
- pause-after-sentence;
- sentence timing/progression.

## Piper TTS

**PRESERVED / BUILDS AND TESTS GREEN; INTERACTIVE AUDIO UNVERIFIED IN RESTART**

Piper keeps its model/eSpeak worker-pool and sentence-cache behavior.

Director review found one small ownership leak: `TtsEngine::new` still performs Piper/eSpeak environment setup even when the selected backend is Windows. Goal 0005 must isolate that initialization to Piper.

## Native Windows TTS

**IMPLEMENTED WITH WINRT; SENTENCE-WAV PATH PRESENT; INTERACTIVE PLAYBACK UNVERIFIED**

Goal 0004 added WinRT `Windows.Media.SpeechSynthesis::SpeechSynthesizer` support:

- installed voice enumeration;
- stable configured voice IDs;
- default-voice fallback;
- sentence WAV synthesis;
- backend-aware cache identity;
- egui backend/voice controls.

The pushed report records a local Windows probe with three installed voices and a non-empty synthesized WAV.

Hosted CI executed the Windows TTS test successfully, but the passing test captures stdout and may return early when no usable hosted voice exists; therefore the hosted log does not independently prove the runner synthesized speech.

### Known Goal-0004 correctness defects

Director review found two material follow-ups:

1. `patch_has_tts_fields` recognizes backend/voice changes, but `should_sync_tts_after_reader_command` does not include `tts_backend` or `windows_voice_id`. Changing backend/voice during active playback can therefore leave the current request running on the old backend instead of rebuilding immediately.
2. the implicit Windows-default cache identity is currently the literal `windows:default`, not the actual resolved default voice ID. If the OS default voice changes, stale cached speech can be reused under the same identity.

Goal 0005 begins by fixing both.

### Additional hardening

- Make runtime error strings backend-neutral where they still say Piper.
- Make the Windows TTS test explicitly decode synthesized WAV through the same decoder assumptions used by shared playback.
- Expose hosted voice-count/synthesis evidence with `--nocapture` or equivalent diagnostics.
- Avoid further growth of the large egui `app/mod.rs`; extract the new Windows voice/settings helpers where practical.

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

## Goal completion notifications

**REQUESTED / GOAL 0005 BOOTSTRAPS IMPLEMENTATION**

From Goal 0005 forward, the human should not manually start a watcher.

The repository protocol now requires Codex on Windows to automatically launch a detached watcher that emits one desktop notification only when the repository macro-goal reaches `done/` or `blocked/`.

Notification failure is non-fatal workflow UX.

## Historical Tauri / React / WebView implementation

**HISTORICAL / OBSOLETE AS PRODUCTION TARGET**

It remains useful as behavioral evidence only.
