# Implementation Tranche 6: Settings, Persistence, And QA Hardening

This document captures tranche 6 work for settings, persistence, and QA hardening.

## Deliverables
- Rebuild the settings panel as an egui sidebar that emits deterministic commands for typography/reader options, ties into runtime persistence, and emits `settings.command`/`settings.rebuild` spans.
- Validate cache/bookmark persistence APIs under the new shell, add regression tests mirroring the QA checklist, and emit bridging spans (`persistence.flush`, `persistence.evict`).
- Expand the QA diagnostics panel with trace replay helpers that open relevant roadmap/checklist references per span, keeping logging/performance budgets aligned with overlay/panel budgets.
- Capture and document remaining regression scenarios (TTS closing, bookmarks, overlay backlog) so the final cutover has precise QA gates tied to the roadmaps.
