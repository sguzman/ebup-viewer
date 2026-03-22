# Egui Migration Cache/Config Compatibility

This document defines how cache/config/bookmark data remains compatible through the egui migration.

## Compatibility rules
- Preserve existing cache/config/bookmark data formats during migration.
- If a data format change is required, add deterministic migration steps and version tags.
- Provide deterministic invalidation rules when migration is not possible, and record the reason in tracing.

## Migration/invalidation telemetry
- Emit tracing spans for migration or invalidation events with the data kind and version.
- Avoid silent data loss; report migrations in app diagnostics.
