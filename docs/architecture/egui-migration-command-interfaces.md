# Egui Migration Command Interface Inventory

This document records the requirement to formalize Rust interfaces for legacy bridge commands consumed by the UI.

## Interface requirements
- Enumerate existing legacy commands used by the UI and map each to a Rust-native trait or module boundary.
- Preserve command/event payload shapes as Rust DTOs until the egui app fully owns the flow.
- Keep tracing fields for command invocation, latency, and outcomes so parity checks can compare legacy vs. egui behavior.
