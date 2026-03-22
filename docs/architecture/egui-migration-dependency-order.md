# Egui Migration Dependency Order

This document records the dependency order for the egui migration work.

## Dependency order
1. Extract stable Rust-side application/domain boundaries from the current Tauri/UI split.
2. Create the new egui desktop crate and shell/runtime skeleton.
3. Move app state/event/effect ownership into Rust-native services.
4. Rebuild shell, panels, and reader navigation in egui.
5. Rebuild EPUB/HTML/Markdown reader rendering.
6. Rebuild TTS controls/runtime flow on top of Rust-native app state.
7. Rebuild native PDF rendering/sync.
8. Rebuild starter/library/browser-tab/Calibre flows.
9. Replace JS/Tauri test coverage with Rust-native and manual QA equivalents.
10. Cut over packaging/CI/build scripts.
11. Remove Tauri/UI stacks after final parity gate.
