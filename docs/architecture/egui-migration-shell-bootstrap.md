# Egui Migration Shell Bootstrap Plan

This document captures Phase 2 shell bootstrap expectations for the egui migration.

## Shell bootstrap requirements
- Add a new egui desktop crate to the workspace.
- Stand up app window, panel layout, top toolbar, keyboard shortcut capture, modal strategy, and tracing bootstrap.
- Wire the shell to Rust-native mock or real domain state without feature parity.
- Establish redraw/performance discipline and frame-budget telemetry.

## Phase 2 exit criteria
- The workspace can launch the egui shell independently.
- Shell, panel, and command plumbing no longer depends on the web stack.
