# Egui Cutover And Tauri Removal Checklist

## Objective
- [x] Provide the final readiness and deletion checklist for cutting over from the legacy UI stack to the egui desktop app.
- [x] Ensure no dependency, build, docs, or QA responsibility was removed before parity was proven.
- [x] Record the completed retirement of the legacy stack.

## Current-State Grounding In This Repo
- The egui app is the only production entrypoint.
- CI/build/test expectations are Rust-native.
- Parity and QA docs are egui-focused and the legacy stack has been removed.

## Target End State Under Egui
- The egui desktop crate is the sole production app entrypoint.
- Build, CI, QA, and developer docs reflect a Rust-only desktop application.

## Key Architectural Decisions Already Chosen
- Tauri and TypeScript are removed only after full parity and cutover readiness gates pass.
- Browser-tab import, Calibre, PDF, TTS, and persistence are in-scope for cutover readiness.
- End state is pure Rust desktop shipping; hybrid long-term ownership is not allowed.

## Readiness Gates
- [x] Master migration roadmap Gates A-G all pass.
- [x] Non-PDF reader parity passes.
- [x] PDF parity passes.
- [x] Starter/local-open/recents parity passes.
- [x] Calibre parity passes.
- [x] Browser-tab import parity passes.
- [x] TTS/runtime parity passes.
- [x] Persistence/safe-quit/delete/reopen parity passes.
- [x] Full implementation-phase build verification passes excluding AppImage/RPM/DEB packaging outputs.

## Dependency Removal Order
- [x] Remove the legacy app entrypoint in developer docs and build scripts.
- [x] Remove TS binding generation and bridge compatibility checks after Rust-native replacements are in place.
- [x] Remove frontend build/test commands from required CI once native replacements are authoritative.
- [x] Remove frontend package dependencies and scripts only after no production/runtime/test gate depends on them.

## Package / Build Cleanup
- [x] Update workspace manifests to add the egui app crate as the canonical desktop target.
- [x] Remove Tauri packaging/build instructions from root docs and scripts.
- [x] Update developer setup instructions for Rust-only desktop development.
- [x] Update release/check/build scripts to target the egui app.
- [x] Confirm no leftover codegen path depends on TS or Tauri artifacts.

## Code / Directory Deletion Targets
- [x] delete the legacy frontend after test and doc replacement is complete
- [x] delete the legacy Tauri shell after shell/runtime responsibilities are replaced
- [x] remove root/package-manager artifacts no longer needed for the shipped product
- [x] remove old browser/Tauri-specific docs after equivalent egui docs are published
- [x] remove legacy migration shims and dual-run compatibility code once no longer needed

## Docs / CI Updates
- [x] Update README and architecture docs for the egui app.
- [x] Update parity acceptance checklist to point at native test/QA gates.
- [x] Update CI to run Rust-native checks and native app build/test flows.
- [x] Archive or delete obsolete Tauri/WebView/browser-test documentation.

## Final Acceptance Checklist
- [x] A clean checkout can build and run the egui desktop app without Node or legacy frontend tooling.
- [x] All user-facing features documented in README and roadmap docs are available in the egui app.
- [x] Automated regression and parity signoff are complete.
- [x] Legacy stacks are removed without leaving orphaned build/test/doc references.
- [x] The shipped product is a pure Rust desktop application.

## Risks / Failure Modes
- Removing old stacks too early can erase useful parity baselines.
- CI/docs drift can leave the repo in a confusing mixed state after cutover.
- Package cleanup can break contributor workflows if developer docs are not updated in lockstep.

## Test / Parity Requirements
- [x] Final cutover requires explicit parity evidence from subsystem roadmaps and updated QA checklists.
- [x] Final removal PRs must include full build verification excluding AppImage/RPM/DEB packaging outputs.
- [x] Final removal must also verify no obsolete docs/scripts reference Tauri or TS production paths.

## Acceptance Criteria
- [x] The checklist is sufficient to execute final cutover without reopening sequencing decisions.
- [x] Removal order is explicit and safe.
- [x] Documentation and CI cleanup are part of cutover, not deferred cleanup work.
- [x] Final end state is unambiguously a pure Rust egui desktop app.
