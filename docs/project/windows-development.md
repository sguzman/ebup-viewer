# Windows Development Environment

LanternLeaf's current Windows Rust target is `x86_64-pc-windows-msvc`.

## Canonical repo-native workflow

The repository owns Windows setup and launch. The human should not manually reconstruct the toolchain or download CI artifacts for routine testing.

From the repository root:

```powershell
.\deps.ps1
```

is the idempotent dependency/bootstrap contract.

For real-desktop QA:

```powershell
.\qa.ps1
```

is the normal entrypoint. It checks/repairs dependencies, enters the MSVC environment automatically, creates isolated QA config/cache/log state under ignored `.qa\windows\`, materializes the representative EPUB fixture, then builds and launches LanternLeaf.

CI validates the same repo-native path with:

```powershell
.\deps.ps1 -CheckOnly
.\qa.ps1 -SkipDependencyCheck -PrepareOnly
```

## Dependency contract

`deps.ps1` owns these Windows prerequisites:

- stable Rust / rustup;
- Rust target `x86_64-pc-windows-msvc`;
- Visual Studio 2022 Build Tools / C++ workload;
- CMake;
- Pandoc.

When missing, the script uses WinGet package identities maintained in the repo:

- `Rustlang.Rustup`;
- `Microsoft.VisualStudio.2022.BuildTools`;
- `Kitware.CMake`;
- `JohnMacFarlane.Pandoc`.

The C++ workload is `Microsoft.VisualStudio.Workload.VCTools`.

The dependency script is intentionally imperative rather than introducing Nix/mise solely for four Windows-native prerequisites. If the dependency surface becomes materially larger or cross-platform reproducibility becomes painful, a declarative environment manager can be reconsidered.

## MSVC environment

The project does not require the human to open a special Developer PowerShell.

`scripts/windows-dev-env.ps1` locates the installed Visual Studio instance through `vswhere.exe`, imports `VsDevCmd.bat` into the current PowerShell process, and verifies Cargo, CL, MSBuild, CMake, and Pandoc.

LLVM-MinGW is not treated as a drop-in compiler for the MSVC Rust target.

## Pandoc

Pandoc remains a runtime/development prerequisite for the current HTML/DOC/DOCX-backed conversion paths.

Local QA installs/uses Pandoc through the repo bootstrap rather than bundling a second copy or requiring artifact extraction.

## Hosted CI / renderer limitation

Hosted Windows CI remains authoritative for:

```powershell
cargo check --workspace
cargo build --workspace
cargo test --workspace
```

The hosted renderer capability probe remains separate because GitHub-hosted Windows exposes an unsuitable graphics environment for the current egui renderer. That limitation must not be confused with a normal Windows desktop failure.

## Native Windows TTS

Windows TTS uses WinRT `Windows.Media.SpeechSynthesis::SpeechSynthesizer` and the shared Rodio playback path.

Hosted CI can verify synthesis/decoding without a physical speaker. Real speaker playback and egui interaction require real-desktop QA through `.\qa.ps1`.