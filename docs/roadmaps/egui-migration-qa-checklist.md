# Egui Migration QA Checklist

## Objective
- [ ] Provide a migration-era QA checklist for validating the egui app against the current Tauri app during parity work.
- [ ] Focus on user-observable behaviors that must remain stable during the migration.

## Current-State Grounding In This Repo
- Current QA baselines already exist for native PDF and native HTML/EPUB flows, but they assume the Tauri app.
- The egui migration needs a cross-stack checklist that can be run while both apps still exist.

## Target End State Under Egui
- QA can validate the egui app on representative source sets before old stacks are removed.
- Checklist evidence feeds cutover readiness gates.

## Key Architectural Decisions Already Chosen
- Dual-stack validation is allowed during migration.
- PDF, HTML/EPUB, starter/import, TTS, and persistence all require explicit human validation.

## Phased Plan With Explicit Phase Exits

### Phase 1: Starter / Import QA
- [ ] Open local EPUB, PDF, and markdown sources from starter mode.
- [ ] Confirm recent entries appear, reopen correctly, and delete cleanly.
- [ ] Confirm Calibre listing, search, sorting, and open behavior.
- [ ] Confirm browser-tab import health, selection, import, refresh, reopen, and delete behavior.
- Phase exit:
- [ ] starter/import regressions are visible independently from reader regressions.

### Phase 2: Reader / TTS QA
- [ ] Validate text-only mode on EPUB, markdown, and browser-tab sources.
- [ ] Validate pretty mode on HTML/EPUB and markdown sources.
- [ ] Confirm sentence click, play-from-highlight, play-from-page, next/prev sentence, pause/resume.
- [ ] Confirm search query + next/prev behavior.
- [ ] Confirm stats/settings panel behavior and exclusivity.
- Phase exit:
- [ ] non-PDF reading and playback parity are confirmed.

### Phase 3: PDF QA
- [ ] Validate structured PDF rendering fidelity.
- [ ] Validate degraded PDF modes and explicit messaging.
- [ ] Confirm jump-to-highlight, auto-scroll, and same-sentence stability.
- [ ] Validate one multi-column PDF, one OCR-heavy PDF, one rotated PDF, and one header/footer-heavy PDF.
- Phase exit:
- [ ] native PDF parity is evidenced before cutover.

### Phase 4: Persistence / Lifecycle QA
- [ ] Close and reopen the same source and confirm bookmark and config restore.
- [ ] Delete recent entries and confirm source-specific cache artifacts are removed.
- [ ] Quit during idle and during active runtime work and confirm safe persistence behavior.
- Phase exit:
- [ ] persistence and lifecycle semantics are validated in the egui app.

## Risks / Failure Modes
- Human QA may drift without a single migration checklist spanning all source types.
- PDF regressions can appear late if not tested on difficult fixtures throughout the migration.

## Test / Parity Requirements
- [ ] Run this checklist alongside subsystem automated coverage during migration milestones.
- [ ] Pair checklist results with tracing/log evidence for difficult regressions.

## Acceptance Criteria
- [ ] The egui app has an explicit migration-era QA checklist covering starter, reader, PDF, TTS, and persistence.
- [ ] Checklist results can be used directly for parity and cutover decisions.
