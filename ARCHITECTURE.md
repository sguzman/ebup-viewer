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

Target direction:

- a stable backend abstraction;
- existing Piper support retained;
- native Windows TTS added as another backend;
- backend selection should not leak platform details through reader UI/state logic;
- sentence/chunk completion events drive highlight and navigation state.

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

## Dependency direction

Prefer:

`UI -> app/runtime services -> core/domain`

Format/platform services may feed app/runtime state but should not own UI state.

Avoid circular ownership between UI, playback, document extraction, and rendering.

## Historical architecture

The repository contains substantial Tauri/React migration history and web-oriented PDF work. It may be mined for behavioral contracts and prior bug knowledge. It must not be revived as production ownership without an explicit director-level architecture change.
