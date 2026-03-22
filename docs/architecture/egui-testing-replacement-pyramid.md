# Egui Testing Replacement Pyramid

This document captures the replacement test pyramid for the egui migration.

## Pyramid layers
- Rust unit tests covering pure domain logic, parsing/conversion, reducers/transitions, and sync/mapping algorithms.
- Rust integration tests covering runtime services, persistence flows, source opening/import flows, TTS/runtime orchestration, and PDF lifecycle/mapping behaviors.
- Native UI behavior checks for panel/command state transitions and selected rendering layout invariants.
- Screenshot/golden comparisons for stable cases where automation is viable.
- Manual QA covering PDF fidelity/sync, EPUB/HTML rich rendering, and starter/library/import ergonomics.
