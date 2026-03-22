# Egui Migration Rollback Points

This document records rollback safeguards for the egui migration.

## Rollback points
- Keep the Tauri app runnable until Gate G.
- Preserve cache/config compatibility or provide versioned migration until the egui app is stable.
- Support side-by-side validation builds during shell, reader, and PDF phases.
- Do not remove TypeScript/Tauri tests until equivalent parity evidence exists.
