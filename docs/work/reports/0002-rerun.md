# Goal 0002 — Close Trustworthy Baseline (rerun)

## Result

BLOCKED at the authorized PDF Gate 3/4 escalation boundary. This rerun completed the remaining baseline-only repair: repository-owned TXT, Markdown, and HTML smoke fixtures are now used by the Rust ingestion tests. Native startup, truthful CI smoke, prerequisite diagnostics, live configuration, and PDF fixture parsing remain repaired from the reviewed implementation carried forward onto this branch.

## Native Startup

- `eframe::run_native` errors are returned as `NativeRunError::Eframe` instead of discarded.
- The root native entry point propagates startup errors and exits nonzero with an actionable message.
- Single-instance lock I/O errors and an already-running instance remain distinct.
- Windows CI initializes MSVC, provisions Pandoc, and fails if the native process exits during the smoke interval, including exit code 0.

## Test Failure Classification

The focused core suite is 158 passed / 9 failed out of 167 tests. TXT and Markdown source smoke pass using repository fixtures. The two HTML failures are actionable missing-Pandoc environment failures. The seven PDF failures are preserved product-contract failures:

| Test | Class | Disposition |
| --- | --- | --- |
| `epub_loader::source_pipeline::tests::classification_fixture_matrix_matches_expected_contracts` | PDF classifier policy mismatch for `academic-layout-hostile` | Escalate to PDF Gate 3/4 |
| `epub_loader::source_pipeline::tests::classification_rollup_prefers_scan_when_sampled_pages_are_image_only` | PDF rollup selects hidden-overlay instead of weak-OCR scan | Escalate to PDF Gate 3/4 |
| `epub_loader::source_pipeline::tests::derive_pdf_ocr_pipeline_summary_captures_engine_and_fallback_policy` | OCR summary misses footnote-marker adjustment | Escalate to PDF Gate 3/4 |
| `epub_loader::source_pipeline::tests::hidden_overlay_and_scan_fixtures_record_ocr_confidence_thresholds` | Scanned fixture misses OCR confidence contract | Escalate to PDF Gate 3/4 |
| `epub_loader::source_pipeline::tests::normalize_pdf_text_for_reader_tracks_ocr_normalization_edits` | OCR normalization misses the expected `Alphabeta line.` output | Escalate to PDF Gate 3/4 |
| `epub_loader::source_pipeline::tests::page_reading_order_resolver_covers_ocr_layout_families` | Page 4 resolves as `OuterMarginSidenotes`, expected `BottomFootnoteBand` | Escalate to PDF Gate 3/4 |
| `epub_loader::source_pipeline::tests::pdf_cache_roundtrip_preserves_chunk_page_ranges_and_meta` | Cache metadata resolves as `EmbeddedClean`, expected `LayoutHostileDocument` | Escalate to PDF Gate 3/4 |
| `epub_loader::tests::html_source_emits_native_html_without_markdown_fallback` | Pandoc is absent locally | CI provisions Pandoc; preserve test |
| `epub_loader::tests::html_source_tts_text_drops_non_text_noise_from_plain_conversion` | Pandoc is absent locally | CI provisions Pandoc; preserve test |

No assertions were weakened and no tests were deleted or skipped.

## Changes

- Added `tests/fixtures/source-ingestion/sample.txt`, `sample.md`, and `sample.html`.
- Updated the Rust TXT/Markdown/HTML smoke tests to load those repository-owned fixtures.
- Explicitly report the installed Rust MSVC target in `scripts/windows-dev-check.ps1`.
- Carried forward the reviewed native startup, CI, Pandoc, abbreviation, and PDF fixture repairs from commit `179b0ed`.

## Validation

- `cargo test -p lanternleaf-core --lib -- --test-threads=1`: 158 passed, 9 failed; all failures are classified above.
- TXT smoke: passed.
- Markdown smoke: passed.
- `cargo check --workspace`: reaches `espeak-rs-sys` CMake, then fails because the local shell has no C/C++ compiler (`No CMAKE_C_COMPILER could be found`).
- `cargo build --workspace`: fails at the same native prerequisite boundary; subsequent generated-cache reuse reports missing `install.vcxproj`.
- `git diff --check`: passed.
- Windows diagnostic: Rust host and `x86_64-pc-windows-msvc` target detected; Cargo and CMake detected; Pandoc, `cl.exe`, and `MSBuild.exe` missing. Exit code 1 is intentional and actionable.
- Full rustfmt check still reports pre-existing repository-wide formatting drift; no mass formatting was introduced.

## External Prerequisites

The local environment lacks Pandoc and an initialized/installed MSVC C/C++ toolchain. CI now runs `ilammy/msvc-dev-cmd@v1` and installs Pandoc before the checks. No privileged local installation was attempted.

## Remaining Product Defects

The seven PDF failures above require PDF classifier/OCR/reading-order/cache behavior changes and therefore enter the explicit PDF Gate 3/4 non-goal.

## Recommended Next Goal

Authorize a PDF Gate 3/4 goal for the seven preserved defects, then rerun the workspace checks on a Windows environment with the MSVC workload and Pandoc available.

## Git

- Branch: `codex/0002-close-trustworthy-baseline-rerun`
- Commits: `179b0ed`, `9983cb0`, `97cabdd`
- Goal status: blocked by the authorized PDF escalation condition.
