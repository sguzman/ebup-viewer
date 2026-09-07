# 0001 — Windows recovery and verified baseline

## Outcome

Recover a reproducible Windows 11 development baseline for the current native Rust + egui LanternLeaf application, get as far through full workspace build/test/launch as the current architecture permits, and leave durable evidence of the actual current code/runtime state.

This is a macro-goal. Continue through multiple diagnosis/fix/test passes without requesting a new prompt for each directly-related Windows/build/bootstrap blocker.

## Why now

The project has been dormant, the user does not trust the recorded project status, and a fresh Windows build currently fails before the application can be exercised.

All later work—Windows TTS, non-PDF stabilization, and PDF synchronization—depends on a trustworthy baseline.

## Starting evidence

Fresh Windows command:

```powershell
cargo check --workspace --verbose --jobs 16 --color always
```

fails during native dependency setup because `bindgen 0.59.2` cannot locate `clang.dll` / `libclang.dll`.

Immediately before the failure, native code compilation proceeds successfully. Prove the exact dependency chain rather than assuming which crate owns bindgen.

The root entrypoint currently calls `lanternleaf_egui::app::run()` outside TTS worker mode.

## Authorized passes / subtracks

### Pass A — dependency/toolchain diagnosis

Record:

- `git status --short`
- `git log -10 --oneline`
- `rustc -Vv`
- `cargo -V`
- `cargo tree -i bindgen@0.59.2`
- additional `cargo tree` evidence needed to prove the owner chain.

Determine:

- which crate invokes failing bindgen;
- why runtime libclang is required;
- whether LanternLeaf directly controls the dependency;
- whether compatible newer versions, pregenerated bindings, vendored patches, or feature choices can remove the machine-global prerequisite.

### Pass B — reproducible Windows build repair

Prefer, in order:

1. remove unnecessary build-time libclang need;
2. use pregenerated/vendored bindings where sensible;
3. safely update/patch the responsible dependency;
4. if libclang genuinely remains required, make setup/detection deterministic and documented.

Do not hardcode a personal user path.

Do not merely set `LIBCLANG_PATH` in the current shell and call the goal complete.

Preserve the current espeak `.cargo/config.toml` behavior unless evidence proves it wrong.

### Pass C — continue through directly-related Windows blockers

After the first failure is fixed, continue `cargo check --workspace`.

Fix additional small Windows portability/build/bootstrap blockers encountered while reaching the acceptance gates.

This authorization includes:

- cfg mistakes;
- path handling;
- missing Windows dependency features;
- native-library detection/bootstrap;
- straightforward startup/bootstrap failures.

It does NOT include feature redesign.

### Pass D — build, tests, native launch

Target:

```powershell
cargo check --workspace
cargo build --workspace
cargo test --workspace
```

Then run the native application sufficiently to determine whether the egui shell launches.

If the app hits a straightforward Windows/bootstrap failure, fix it and retry.

If it hits a substantial functional/architectural defect, record it as the next blocker and stop that subtrack.

### Pass E — Windows CI

Inspect `.github/workflows`.

Add or repair one useful Windows gate so the recovered baseline does not silently regress.

At minimum it should exercise appropriate equivalents of workspace check and practical tests, including deterministic setup for any required native prerequisite.

### Pass F — verified capability inventory

Do not edit director-owned philosophy/product-priority documents.

Instead, put evidence in `docs/work/reports/0001.md` classifying, as far as this goal actually proves:

- egui shell;
- EPUB/TXT/Markdown;
- HTML;
- Piper TTS;
- PDF render;
- PDF text extraction;
- PDF TTS/highlight sync;
- Calibre;
- browser/import;
- persistence/cache/bookmarks/config;
- tests/CI;
- Windows-specific behavior.

Use:

- VERIFIED WORKING;
- PARTIAL;
- PRESENT BUT UNVERIFIED;
- BROKEN;
- NOT IMPLEMENTED;
- HISTORICAL/OBSOLETE.

ChatGPT/director will update `docs/project/current-status.md` after reviewing the branch.

## Non-goals

Do not:

- implement native Windows TTS;
- redesign the TTS backend architecture beyond what is strictly required to build;
- finish PDF rendering;
- change PDF synchronization algorithms;
- redesign the GUI;
- add DOCX;
- migrate back to Tauri/React/WebView;
- mass-update dependencies;
- perform large unrelated refactors;
- rewrite director-owned philosophy/priorities/roadmaps.

## Constraints

- Native Rust + egui is authoritative.
- Historical web code/docs are reference only.
- Preserve existing behavior unless a concrete Windows blocker requires change.
- Keep platform code explicit and isolated.
- Preserve `tracing`; improve actionable diagnostics where useful.
- Add regression coverage for concrete fixed failures where feasible.

## Acceptance gates

The goal is DONE only when all achievable baseline gates pass:

1. exact original dependency-chain root cause is documented;
2. Windows build prerequisite handling is reproducible;
3. `cargo check --workspace` passes;
4. `cargo build --workspace` passes;
5. practical workspace tests pass, with any unavoidable exclusions explicitly justified;
6. the native egui app launch was actually attempted on Windows and outcome recorded;
7. Windows CI meaningfully exercises the recovered path;
8. capability inventory is evidence-based;
9. all intentional changes are committed/pushed.

If a substantial application defect prevents gates 3-7 and cannot be fixed without violating non-goals, mark BLOCKED with precise evidence rather than broadening architecture.

## Validation

At minimum:

```powershell
cargo fmt --all -- --check
cargo check --workspace
cargo build --workspace
cargo test --workspace
```

Run Clippy if the changed Rust surface is large enough to make it useful.

## Repository handoff

Branch:

`codex/0001-windows-recovery-and-baseline`

Use the standard repository lifecycle from `AGENTS.md` and `docs/work/README.md`.

The final report must contain:

### Root Cause
### Changes Made
### Current Verified State
### Verification
### Remaining Blockers
### Recommended Next Goal
### Git

After pushing, return the shared checkout to `main`.

## Human verification

Only ask the human for observations that cannot be established by Codex locally, such as subjective audio/render behavior.

Do not ask the human to manually ferry diffs, reports, or architecture decisions.

## Stop / escalation conditions

Stop and mark BLOCKED if:

- the next necessary fix requires a cross-subsystem architecture decision not already made;
- a destructive change to existing user data would be required;
- unrelated human working-tree changes prevent safe branch work;
- satisfying the goal would require implementing one of the explicit non-goals.
