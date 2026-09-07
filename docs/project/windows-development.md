# Windows Development Environment

LanternLeaf's current Windows Rust target is `x86_64-pc-windows-msvc`.

## Canonical repo-native workflow

The repository owns Windows setup and launch. Routine testing does not require downloading CI artifacts or manually reconstructing PATH state.

Normal real-desktop QA:

```powershell
.\qa.ps1
```

`qa.ps1` invokes the dependency bootstrap automatically, prepares isolated state under `.qa\windows\`, builds LanternLeaf, and launches it.

## Scoop dependency declaration

Windows command-line dependencies are declared in the repo's `Scoopfile.json`, using Scoop's native export/import format.

You can restore them directly with:

```powershell
scoop import .\Scoopfile.json
```

The current Scoop-owned project tools are:

- `main/cmake`;
- `main/pandoc`;
- `main/vswhere`.

Scoop's `main` bucket is the canonical source for these tools.

`deps.ps1` is the safe full bootstrap entrypoint:

```powershell
.\deps.ps1
```

It:

1. installs Scoop from `get.scoop.sh` if Scoop is absent;
2. runs `scoop import .\Scoopfile.json`;
3. ensures a Rust toolchain is available;
4. ensures the MSVC Rust target exists;
5. ensures the Visual Studio C++ workload exists;
6. enters/verifies the native Windows development environment.

## Rust

Rust is declared separately through `rust-toolchain.toml`, because Rust's own toolchain manager is the correct authority for channel/components/targets.

If `rustup` itself is missing, `deps.ps1` installs `main/rustup` through Scoop. Existing rustup installations are left alone rather than forcibly replaced.

## Visual Studio C++ workload

The Visual Studio C++ workload is the one intentional non-Scoop exception.

Scoop's own Rust manifests note that Microsoft's C++ Build Tools and Windows SDK are required separately for the MSVC toolchain. `deps.ps1` therefore detects an existing Visual Studio instance and adds `Microsoft.VisualStudio.Workload.VCTools` through the Visual Studio Installer, or installs Build Tools through WinGet when no instance exists.

This exception is about the compiler workload only. Auxiliary CLI tooling remains Scoop-owned.

## MSVC environment

`scripts/windows-dev-env.ps1` resolves Scoop shims, prefers Scoop's `vswhere.exe`, locates the Visual Studio C++ instance, imports `VsDevCmd.bat`, and verifies Cargo, CL, MSBuild, CMake, and Pandoc.

The human does not need to open a special Developer PowerShell.

## CI

Hosted Windows CI exercises the same dependency path by running:

```powershell
.\deps.ps1
.\qa.ps1 -SkipDependencyCheck -PrepareOnly
cargo check --workspace
cargo build --workspace
cargo test --workspace
```

The hosted renderer capability probe remains separate because GitHub-hosted Windows does not expose a suitable graphics adapter/context for the current egui renderer.

## Native Windows TTS

Windows TTS uses WinRT `Windows.Media.SpeechSynthesis::SpeechSynthesizer` and the shared Rodio playback path. Hosted CI can verify synthesis/decoding without physical speaker playback; real speaker/egui behavior is verified through `.\qa.ps1` on a real desktop.