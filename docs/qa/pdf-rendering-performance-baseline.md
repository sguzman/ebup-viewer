# PDF Rendering Performance Baseline

## Scope

- Baseline the current viewer scheduling and budget behavior for the five roadmap document classes.
- Use the current render scheduler, open plan, and runtime performance profiles as the source of truth.
- Treat this as the repeatable before/after baseline for future performance work.

## Scenarios

| Scenario | Profile | Total Pages | Visible Pages | Canvas Pages | Text Layer Pages | Medium Priority | Low Priority | Deferred Open Pages |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| small_text_pdf | balanced | 12 | 1 | 2 | 1 | 1 | 2 | 10 |
| large_academic_pdf | balanced | 240 | 3 | 6 | 4 | 4 | 3 | 237 |
| image_heavy_pdf | low_memory | 80 | 2 | 3 | 3 | 2 | 2 | 77 |
| two_column_pdf | balanced | 120 | 3 | 5 | 3 | 3 | 4 | 117 |
| tts_playback_long_pdf | high_memory | 320 | 3 | 7 | 4 | 4 | 4 | 317 |

## Readout

- Initial open is bounded to one immediate target page plus adjacent pages; the rest of the document stays deferred.
- Expensive work remains local to the viewport, active TTS target, or explicit jump target.
- Low-memory profile reduces overscan and cache budgets to protect weaker devices.
- High-memory profile expands cache and overscan budgets without reverting to whole-document work.
- TTS playback scenarios keep active-page and jump-page priority without expanding text layers to the whole document.

## Repeatability

- Scenario evaluation lives in [pdfPerformanceScenario.ts](/win/linux/Code/projects/lantern-leaf/ui/src/components/pdfPerformanceScenario.ts).
- The baseline assertions live in [pdfPerformanceScenario.test.ts](/win/linux/Code/projects/lantern-leaf/ui/tests/pdfPerformanceScenario.test.ts).
- Budget selection logic lives in [pdfPerformanceProfile.ts](/win/linux/Code/projects/lantern-leaf/ui/src/components/pdfPerformanceProfile.ts).
