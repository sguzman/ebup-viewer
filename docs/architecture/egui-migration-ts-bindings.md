# Egui Migration TS Bindings Ownership Plan

This document describes how generated TS bindings are collapsed into Rust-native DTO/view-model ownership.

## Plan
- Treat Rust DTOs as the canonical shapes for commands, events, and view models.
- Keep TS bindings as derived artifacts until the egui app fully owns the UI flow.
- Remove TS bindings only after parity gates confirm equivalent Rust-native surfaces.

## Migration notes
- Any shape changes must be versioned in Rust and logged with tracing for parity comparisons.
