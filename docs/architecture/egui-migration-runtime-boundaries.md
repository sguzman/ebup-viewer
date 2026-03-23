# Egui Migration Runtime Boundaries

This document captures the in-process runtime boundaries for the egui migration.

## Boundary goals
- Replace legacy command boundaries with Rust-native traits/modules.
- Keep command/event payloads typed and traceable across boundaries.
- Keep ownership explicit across document/session/playback/UI/persistence/runtime services.

## Phase 1 exit criteria
- Implementers can build new UI/runtime work without depending on legacy command semantics.
- Each legacy integration point has a Rust-native owner.
