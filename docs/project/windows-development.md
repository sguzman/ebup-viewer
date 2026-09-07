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

Goal 0002 adds a repository-owned prerequisite diagnostic and makes Pandoc test behavior deterministic.

## Build baseline

The expected Windows baseline after Goal 0001 is:

```powershell
cargo check --workspace
cargo build --workspace
```

GitHub Actions has verified these commands with the supported MSVC environment.

## Interactive launch

Do not infer GUI health from a code-0 process exit.

As of the Goal 0001 review, native startup errors may be discarded by the current egui entrypoint. Goal 0002 is responsible for making startup failure observable before interactive launch becomes an accepted baseline signal.
