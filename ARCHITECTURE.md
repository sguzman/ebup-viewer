# LanternLeaf Architecture

This document describes the intended active architecture for the restarted project. It is a director-owned living document and should be updated when verified implementation reality changes.

## Product shape

LanternLeaf is a native desktop reader whose core value is synchronized reading and speech: documents become a canonical readable text/session model, while presentation layers render EPUB/HTML-like content or PDF pages and remain synchronized with TTS playback.

The shipped desktop target is Rust + `eframe`/`egui`.

## Current workspace shape

The current root workspace includes:

- `crates/lanternleaf-core`: reusable document/session/domain logic.
- `crates/lanternleaf-app`: application/runtime/service boundaries shared by the native app.
- `crates/lanternleaf-egui`: native desktop UI and native rendering integration.
- root `src/main.rs`: worker-mode dispatch followed by the native egui app entrypoint.

Existing historical Tauri/React/WebView design documents are parity/reference material, not the target implementation.

## Ownership boundaries

### Canonical document/session state

Rust owns:

- source identity;
- extracted/canonical text;
- sentence/chunk identity;
- pagination/session state;
- bookmark/config/cache state;
- TTS playback state;
- mapping between display units and audio units.

Presentation code must not become the canonical source of reading position.

### Reader presentation

The native reader may have format-specific presentation paths, but they should converge on common session/TTS semantics.

- EPUB/HTML/Markdown/TXT should expose structured/native reader view models rather than browser DOM ownership.
- PDF pages should be rasterized/rendered natively and shown as egui textures/images.
- Highlight geometry is presentation metadata attached to canonical sentence identity.

### TTS

TTS is a first-class subsystem, not a UI effect.

#### Canonical TTS ownership

Reader/session state owns:

- which canonical sentence is active;
- play/pause/stop intent;
- seek/repeat behavior;
- sentence completion/progression;
- highlight/navigation coupling.

A synthesis backend must not invent an independent playback cursor.

#### Synthesis vs playback

The backend boundary is **sentence audio synthesis**, not whole-session playback.

Target flow:

`canonical sentence -> selected synthesis backend -> cached WAV -> shared Rodio/Sonic playback -> runtime sentence completion -> reader/session state`

Shared playback owns:

- audio-device output;
- pause/resume;
- stop;
- volume;
- pause-after-sentence;
- playback speed/time-stretch;
- sentence duration accounting.

Backend implementations own:

- voice/model discovery and identity;
- conversion of text to sentence audio;
- backend-specific worker/resource management;
- backend-specific errors.

This preserves one playback/session state machine across Piper and Windows TTS.

#### Backend configuration

The cross-platform configuration contract should expose a stable backend kind such as:

- `piper`;
- `windows`.

Piper-specific model/eSpeak settings remain valid and backward-compatible.

Windows-specific voice selection uses a stable Windows voice ID. Selecting a backend that is unavailable on the current platform must produce explicit unavailability/error state; do not silently reinterpret it as another backend.

#### Piper backend

Retain the current Piper worker-pool behavior and WAV caching.

Piper remains supported on Windows and future Linux builds.

#### Windows backend

The selected native Windows implementation is WinRT:

`Windows.Media.SpeechSynthesis::SpeechSynthesizer`

Use it to:

- enumerate installed voices;
- expose voice ID, display name, language, and gender/description where practical;
- select a voice by stable voice ID;
- synthesize text with `SynthesizeTextToStreamAsync`;
- persist the resulting `audio/wav` stream into the same sentence-audio cache contract used by shared playback.

Use the current supported Rust `windows` crate behind `cfg(windows)` / target-specific dependencies.

Do not add a second OS-owned media playback state machine merely because the Windows API can synthesize speech.

#### Cache identity

Sentence audio cache identity must include enough backend identity to prevent collisions:

- backend kind;
- backend voice/model identity;
- normalized sentence text;
- synthesis-affecting settings, if any.

Playback-only speed and volume should not fork synthesis cache entries when they remain post-synthesis transformations.

### PDF

PDF is a first-class subsystem with separate responsibilities for:

- page rendering/raster cache;
- text extraction;
- sentence-to-page/geometry mapping;
- viewport/zoom scheduling;
- overlay/highlight rendering;
- degraded quality modes where exact synchronization is impossible.

Canonical TTS/search text must remain distinguishable from visual PDF ordering.

### Platform integration

Windows/Linux-specific behavior belongs behind explicit modules/traits. Avoid scattered shell commands and machine-specific paths.

## Verification layers

Hosted CI and interactive desktop verification are distinct evidence classes.

Required hosted CI should prove:

- compile/check;
- build;
- deterministic tests;
- platform APIs that do not require a real display/audio device.

Interactive renderer/audio verification may require a graphics/audio-capable real machine and must not be faked by weakening hosted checks.

A hosted environment limitation should be classified explicitly rather than turning the entire non-interactive baseline red forever.

## Dependency direction

Prefer:

`UI -> app/runtime services -> core/domain`

Format/platform services may feed app/runtime state but should not own UI state.

Avoid circular ownership between UI, playback, document extraction, and rendering.

## Historical architecture

The repository contains substantial Tauri/React migration history and web-oriented PDF work. It may be mined for behavioral contracts and prior bug knowledge. It must not be revived as production ownership without an explicit director-level architecture change.
