# 0004 — Green Windows baseline and native Windows TTS

## Outcome

In one macro-goal:

1. finish Gate 0 by separating required hosted build/test verification from hosted renderer capability;
2. make TTS sentence-audio synthesis backend-neutral without changing reader/session ownership;
3. retain Piper behavior;
4. add a native Windows TTS backend using WinRT `Windows.Media.SpeechSynthesis::SpeechSynthesizer`;
5. add Windows backend/voice selection to configuration/runtime/egui;
6. verify Windows voice enumeration and sentence-to-WAV synthesis in hosted Windows CI without depending on a graphics adapter or physical audio device.

This goal should continue through all authorized stages without requesting a new prompt between them.

## Architecture authority

Read:

- `ARCHITECTURE.md`;
- `docs/project/current-status.md`;
- `docs/project/windows-development.md`;
- `docs/work/reviews/0003.md`;
- `docs/work/reports/0003.md`;
- `docs/roadmaps/restart-master-roadmap-2026-09.md`;
- existing TTS roadmaps.

The director-selected TTS flow is:

`canonical sentence -> selected synthesis backend -> cached WAV -> shared Rodio/Sonic playback -> runtime sentence completion -> reader/session state`

Do not create a second backend-specific playback/session state machine.

## External API direction

Use the current supported Rust `windows` crate behind Windows-only target configuration.

Selected API:

`Windows.Media.SpeechSynthesis::SpeechSynthesizer`

Use:

- `SpeechSynthesizer::AllVoices` for installed voice discovery;
- `VoiceInformation::Id` as the stable configured identity;
- display name/language/gender/description for UI metadata where practical;
- `SetVoice` for selection;
- `SynthesizeTextToStreamAsync` for text synthesis;
- the returned `SpeechSynthesisStream` as an `audio/wav` source written into LanternLeaf's sentence-audio cache.

Reference:

- Microsoft Learn: Windows.Media.SpeechSynthesis SpeechSynthesizer / VoiceInformation;
- Rust for Windows `windows` crate, current `Media_SpeechSynthesis` feature.

Do not switch to legacy SAPI COM in this goal unless WinRT is proven technically unusable for LanternLeaf's required WAV-producing contract. If that occurs, document evidence and escalate rather than silently changing architecture.

# Stage A — finish the required Windows baseline

## A1 — split CI evidence channels

Current defect:

the known-host-limited renderer smoke occurs before `cargo test --workspace` in the same sequential required job, so it prevents tests from running.

Restructure `.github/workflows/windows-baseline.yml` so required hosted CI can prove:

- prerequisite setup;
- `cargo check --workspace`;
- `cargo build --workspace`;
- `cargo test --workspace`.

The required build/test result must not depend on hosted GPU availability.

## A2 — renderer capability probe

Preserve renderer truth without blocking required non-GUI validation.

Preferred shape:

- a separate job or step explicitly named as a **hosted renderer capability probe**, not a successful native-launch smoke;
- attempt the real binary only when useful;
- classify:
  - `renderer_supported` only if the process remains alive for the smoke window;
  - `hosted_renderer_unavailable` only for the already-proven graphics-capability class (OpenGL < 2.0 / no suitable adapter);
  - unexpected application/startup failures as actual failures.

It is acceptable for the hosted renderer probe to report a known environment limitation without making the required build/test baseline red.

Do not change application renderer/backend merely to satisfy hosted CI.

Do not restore the old false-positive semantics where early code-0 exit means success.

## A3 — obtain authoritative full test evidence

Run normal parallel:

```powershell
cargo test --workspace
```

on hosted Windows with Pandoc/MSVC provisioned.

If bounded regressions remain from Goal 0003, fix them and continue.

Do not use `--test-threads=1` as the authoritative result.

### Stage A checkpoint

Before continuing, required Windows check/build/tests must be green or a new genuine product defect must be explicitly classified.

Do not stop for a new human prompt when Stage A passes. Continue to Stage B.

# Stage B — backend-neutral sentence synthesis

## B1 — preserve canonical runtime ownership

Reader/session remains owner of:

- play/pause/stop state;
- current sentence;
- seek/repeat;
- highlight/navigation progression.

Backend selection must not leak a separate cursor into reader state.

## B2 — separate synthesis backend from shared playback

Refactor the existing Piper-specific TTS engine so the conceptual boundary is sentence audio synthesis.

A backend abstraction should expose enough capability for:

- backend identity;
- voice/model identity used for caching;
- optional voice discovery;
- sentence/batch WAV preparation;
- cancellation/resource cleanup;
- actionable backend errors.

The exact trait/enum structure may be selected by Codex if it remains modular and keeps platform details isolated.

Shared code retains:

- sentence cache management where practical;
- WAV duration/readback;
- Rodio playback;
- Sonic speed transformation;
- volume;
- pause-after-sentence;
- pause/resume/stop;
- runtime progress/cursor logic.

Do not duplicate Rodio playback per backend.

## B3 — preserve Piper

Piper remains a first-class backend.

Requirements:

- current model/eSpeak config remains backward-compatible;
- current worker-pool/preload/cache behavior remains functional;
- existing Piper tests continue to pass;
- default backend remains Piper unless there is strong migration evidence otherwise.

## B4 — backend-aware cache identity

Prevent audio collisions across synthesis implementations.

Cache identity must include at least:

- backend kind;
- Piper model identity OR Windows voice ID;
- normalized sentence text;
- synthesis-affecting settings if introduced.

Do not include playback-only speed/volume when speed remains a post-synthesis Sonic transform and volume remains playback state.

# Stage C — native Windows synthesis

## C1 — Windows-only module/dependency

Use target-specific dependency/features.

Keep WinRT code behind a Windows-specific module/`cfg(windows)`.

Non-Windows builds must not require Windows APIs.

## C2 — voice catalog

Expose installed Windows voices as backend-neutral descriptors containing at least:

- stable ID;
- display name;
- language.

Include gender/description if straightforward.

Voice ordering should be deterministic.

Expose the current/default Windows voice when available.

## C3 — selected voice behavior

Config stores an optional Windows voice ID.

Rules:

- if configured ID exists, select it;
- if no ID is configured, use the Windows default voice;
- if configured ID no longer exists, surface an actionable unavailable-voice error/state and provide enough UI information to choose another voice;
- do not silently map a missing configured voice to an unrelated voice without telling the user.

## C4 — sentence WAV synthesis

For each normalized canonical sentence:

- create/configure SpeechSynthesizer;
- select configured/default voice;
- call `SynthesizeTextToStreamAsync`;
- await it safely from the runtime worker context;
- verify stream content type is WAV when practical;
- write bytes to a temp cache path;
- atomically publish the completed cache file;
- reject empty/corrupt output.

The resulting file must be decodable by the existing shared WAV/audio path.

## C5 — concurrency/apartment behavior

Respect WinRT threading/apartment requirements.

Do not assume the GUI thread owns the Windows synthesizer.

If synthesizers are not safely shareable across the existing prefetch threads, use a bounded backend-specific worker/resource strategy rather than scattering COM/WinRT initialization through UI code.

Cancellation must prevent stale prepared audio from becoming active playback even if an in-flight OS synthesis call cannot be instantly aborted.

# Stage D — config, runtime, and egui wiring

## D1 — config

Add a stable serialized backend enum, for example:

- `piper`;
- `windows`.

Add Windows voice ID storage.

Backward compatibility:

- old configs with no backend field resolve to Piper;
- existing Piper model/eSpeak fields remain valid.

## D2 — settings patch/runtime

Extend reader/runtime settings so backend and Windows voice changes are treated as TTS-affecting settings.

Changing backend/voice during playback must safely cancel/rebuild from the canonical current sentence rather than allowing old-backend audio to continue invisibly.

Runtime diagnostics/events should identify backend and voice/model identity where useful.

## D3 — egui

Add compact TTS controls for:

- backend selection;
- Windows voice selection when Windows backend is selected;
- refresh/reload voice catalog if useful;
- explicit unavailable/error state.

Do not make Windows API calls every frame.

Cache/catalog state belongs in app/runtime state, not rendering code.

Do not grow `crates/lanternleaf-egui/src/app/mod.rs` substantially. Put new platform/TTS/UI logic into coherent modules.

# Stage E — verification

## E1 — platform-neutral tests

Add tests for:

- backend config deserialization/default migration;
- backend-aware cache isolation;
- backend selection;
- missing voice handling via testable abstraction;
- backend switch cancellation/restart semantics;
- Piper compatibility.

## E2 — Windows hosted synthesis tests

Hosted Windows CI does not have a useful graphics adapter, but that should not prevent non-GUI native speech verification.

Add a Windows-only test/probe that:

1. enumerates `SpeechSynthesizer::AllVoices`;
2. records the available/default voice metadata;
3. if at least one usable voice exists, synthesizes a short fixed sentence to a WAV stream/file;
4. verifies non-empty output and decode/readability through LanternLeaf's audio/WAV path.

Do not require real speaker playback.

If the hosted Windows image genuinely contains no usable speech voice, document that separately and preserve deterministic unit coverage; do not fake synthesis success.

## E3 — CI artifact

If practical, upload a Windows native build artifact after successful build/test so a real Windows desktop can perform later interactive GUI/TTS verification without requiring a local compiler.

Do not let artifact packaging block the core goal if runtime sidecar discovery becomes unrelated packaging work.

## Validation

At minimum:

```powershell
cargo check --workspace
cargo build --workspace
cargo test --workspace
git diff --check
```

Also run targeted Windows TTS tests/probes in Windows CI.

Do not mass-format unrelated pre-existing files.

## Acceptance gates

DONE requires:

1. required Windows hosted check/build/tests are green and no longer blocked by renderer capability;
2. hosted renderer result is classified truthfully and separately;
3. Piper remains functional under the new backend boundary;
4. backend selection/config is backward-compatible;
5. Windows voice enumeration is implemented;
6. Windows selected/default voice behavior is explicit;
7. Windows sentence synthesis produces a valid cached WAV in a Windows environment with an available voice;
8. shared Rodio/Sonic playback remains backend-neutral;
9. speed/volume/pause/stop/session progression do not fork into Windows-specific reader state;
10. egui exposes backend and Windows voice selection without per-frame platform calls;
11. normal parallel workspace tests pass;
12. report/handoff protocol is complete.

## Non-goals

Do not:

- change the shipped egui renderer solely to satisfy hosted CI;
- implement a second Windows-specific playback cursor/state machine;
- redesign PDF rendering or PDF TTS/highlight sync;
- implement Linux-native TTS yet;
- replace Piper;
- migrate to SAPI unless WinRT is proven unusable and director escalation occurs;
- add cloud TTS;
- perform broad GUI redesign;
- mass-refactor unrelated large files.

## Repository handoff

Branch:

`codex/0004-windows-tts-backend`

Write:

`docs/work/reports/0004.md`

Report sections:

### Result
### Windows Baseline
### Hosted Renderer Classification
### TTS Backend Architecture
### Piper Compatibility
### Windows Voice Catalog
### Windows WAV Synthesis
### Runtime and UI Wiring
### Tests
### CI
### Remaining Runtime Verification
### Recommended Next Goal
### Git

Return the shared checkout to `main` after pushing.

## Human verification

Do not require routine human relay during the goal.

If CI produces a usable Windows artifact, record the artifact/run and a minimal real-desktop verification scenario in the report. The director will decide when to ask the human to run it.

## Stop / escalation conditions

Stop and mark BLOCKED only if:

- WinRT SpeechSynthesizer cannot satisfy the sentence-to-WAV contract with evidence;
- Windows API threading/apartment requirements force a cross-subsystem architecture decision not covered above;
- the backend abstraction would require changing canonical reader/session ownership;
- Piper compatibility cannot be preserved without a director choice;
- Windows hosted tests reveal a new unrelated product defect whose safe fix is outside the TTS/baseline scope.

Do not stop merely because the hosted renderer remains unavailable; that limitation is already accepted and separated from required build/test verification.
