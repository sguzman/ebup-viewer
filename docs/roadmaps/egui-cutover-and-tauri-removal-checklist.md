# Egui Cutover And Tauri Removal Checklist

## Objective
- [ ] Provide the final readiness and deletion checklist for cutting over from the Tauri + React/TypeScript app to the egui desktop app.
- [ ] Ensure no dependency, build, docs, or QA responsibility is removed before parity is proven.
- [ ] Define the final sequence for retiring `src-tauri/` and `ui/`.

## Current-State Grounding In This Repo
- Tauri is currently a workspace member and shipping desktop entrypoint via `src-tauri/Cargo.toml`.
- The React/TypeScript app under `ui/` currently owns all production UI surfaces.
- CI/build/test expectations still include TypeScript, Vitest, Playwright, and Tauri-specific checks.
- Existing parity and QA docs already give a starting point for cutover gating, but they are Tauri-era documents.

## Target End State Under Egui
- The egui desktop crate is the sole production app entrypoint.
- `src-tauri/` and `ui/` are no longer needed for shipped functionality.
- Build, CI, QA, and developer docs are updated to reflect a Rust-only desktop application.

## Key Architectural Decisions Already Chosen
- Tauri and TypeScript are removed only after full parity and cutover readiness gates pass.
- Browser-tab import, Calibre, PDF, TTS, and persistence are in-scope for cutover readiness.
- End state is pure Rust desktop shipping; hybrid long-term ownership is not allowed.

## Readiness Gates
- [ ] Master migration roadmap Gates A-G all pass.
- [ ] Non-PDF reader parity passes.
- [ ] PDF parity passes.
- [ ] Starter/local-open/recents parity passes.
- [ ] Calibre parity passes.
- [ ] Browser-tab import parity passes.
- [ ] TTS/runtime parity passes.
- [ ] Persistence/safe-quit/delete/reopen parity passes.
- [ ] New testing and manual QA gates pass on the egui app.
- [ ] Full implementation-phase build verification passes excluding AppImage/RPM/DEB packaging outputs.

## Dependency Removal Order
- [ ] Remove Tauri as the default app entrypoint in developer docs and build scripts.
- [ ] Remove TS binding generation and bridge compatibility checks after Rust-native replacements are in place.
- [ ] Remove frontend build/test commands from required CI once native replacements are authoritative.
- [ ] Remove `ui/` package dependencies and scripts only after no production/runtime/test gate depends on them.
- [ ] Remove `src-tauri/` dependencies and workspace membership only after the egui app fully replaces its responsibilities.

## Package / Build Cleanup
- [ ] Update workspace manifests to add the egui app crate as the canonical desktop target.
- [ ] Remove Tauri packaging/build instructions from root docs and scripts.
- [ ] Update developer setup instructions for Rust-only desktop development.
- [ ] Update release/check/build scripts to target the egui app.
- [ ] Confirm no leftover codegen path depends on TS or Tauri artifacts.

## Code / Directory Deletion Targets
- [ ] delete `ui/` after test and doc replacement is complete
- [ ] delete `src-tauri/` after shell/runtime responsibilities are replaced
- [ ] remove root/package-manager artifacts no longer needed for the shipped product
- [ ] remove old browser/Tauri-specific docs after equivalent egui docs are published
- [ ] remove legacy migration shims and dual-run compatibility code once no longer needed

## Docs / CI / QA Updates
- [ ] Update README and architecture docs for the egui app.
- [ ] Replace Tauri-era QA checklists with egui-native checklists.
- [ ] Update parity acceptance checklist to point at native test/QA gates.
- [ ] Update CI to run Rust-native checks and native app build/test flows.
- [ ] Archive or delete obsolete Tauri/WebView/browser-test documentation.

## Final Acceptance Checklist
- [ ] A clean checkout can build and run the egui desktop app without Node, pnpm, Vite, or Tauri tooling.
- [ ] All user-facing features documented in README and roadmap docs are available in the egui app.
- [ ] Manual QA and parity signoff are complete.
- [ ] Legacy stacks are removed without leaving orphaned build/test/doc references.
- [ ] The shipped product is a pure Rust desktop application.

## Risks / Failure Modes
- Removing old stacks too early can erase useful parity baselines.
- CI/docs drift can leave the repo in a confusing mixed state after cutover.
- Package cleanup can break contributor workflows if developer docs are not updated in lockstep.

## Test / Parity Requirements
- [ ] Final cutover requires explicit parity evidence from subsystem roadmaps and updated QA checklists.
- [ ] Final removal PRs must include full build verification excluding AppImage/RPM/DEB packaging outputs.
- [ ] Final removal must also verify no obsolete docs/scripts reference Tauri or TS production paths.

## Acceptance Criteria
- [ ] The checklist is sufficient to execute final cutover without reopening sequencing decisions.
- [ ] Removal order is explicit and safe.
- [ ] Documentation and CI cleanup are part of cutover, not deferred cleanup work.
- [ ] Final end state is unambiguously a pure Rust egui desktop app.
