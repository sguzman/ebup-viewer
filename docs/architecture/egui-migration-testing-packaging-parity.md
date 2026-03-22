# Egui Migration Testing And Packaging Parity

This document captures Phase 6 testing and packaging parity expectations.

## Parity requirements
- Replace browser-centric tests with Rust-native unit/integration/UI harness coverage where feasible.
- Add screenshot/manual QA gates where egui automation is weaker.
- Update workspace build, smoke, and release checks for the new desktop crate.
- Preserve the rule that full builds are verified excluding AppImage/RPM/DEB artifact generation during normal engineering validation.

## Phase 6 exit criteria
- The egui app has documented and automated parity gates sufficient for cutover.
