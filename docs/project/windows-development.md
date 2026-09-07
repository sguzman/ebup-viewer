# Windows Development Environment

LanternLeaf's current Windows Rust target is `x86_64-pc-windows-msvc`.

## Required native toolchain

The current vendored eSpeak/native dependency build requires an MSVC-compatible C/C++ toolchain.

Recommended installation family:

- Visual Studio 2022 Build Tools or Visual Studio 2022;
- Desktop development with C++ / MSVC v143 tools;
- current Windows 10/11 SDK;
- CMake support.

LLVM-MinGW is not treated as a drop-in compiler for the MSVC Rust target. Mixing MinGW CRT/ABI assumptions into an MSVC Rust build is unsupported for this project.

## Rust

Use current stable Rust unless a future repository toolchain file pins otherwise.

Useful checks:

```powershell
rustc -Vv
cargo -V
rustup show
```

## Pandoc

The current source pipeline uses Pandoc for:

- canonical text conversion of HTML;
- DOC;
- DOCX;
- other Pandoc-backed dual-source conversions.

Until that ingestion architecture changes, Pandoc is a runtime/development prerequisite for those source types.

Hosted Windows CI provisions Pandoc before source-ingestion tests.

## Build baseline

Hosted Windows CI has verified:

```powershell
cargo check --workspace
cargo build --workspace
```

Goal 0004 Stage A makes the full workspace test gate independent of renderer capability so it can also become an authoritative required CI result.

## Interactive launch and hosted runner limits

Native startup errors are observable and nonzero.

Goal 0003 proved the GitHub-hosted `windows-latest` environment is not a valid interactive-renderer test machine for the current egui shell:

- the glow/OpenGL path exposes only OpenGL 1.1 and cannot satisfy the required OpenGL 2.0+ context;
- a bounded WGPU experiment found no suitable adapter;
- the experiment was reverted.

This must not be interpreted as a normal-Windows application failure.

Policy:

- hosted build/test verification remains required;
- hosted renderer capability probing may classify the runner as unsupported;
- actual interactive window verification requires a real graphics-capable Windows environment;
- do not weaken an app launch check until early exit becomes success merely to make hosted CI green.

## Native Windows TTS

The selected implementation API is WinRT `Windows.Media.SpeechSynthesis::SpeechSynthesizer`.

It is suitable for hosted non-GUI verification because synthesis returns an audio stream and does not require the egui graphics path or physical audio playback.

Windows TTS runtime audio output still uses LanternLeaf's shared Rodio playback path after synthesis.
