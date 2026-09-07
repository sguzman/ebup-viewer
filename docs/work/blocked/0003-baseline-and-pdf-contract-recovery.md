# 0003 — Baseline closure and PDF contract recovery

## Outcome

Close the trustworthy native Windows baseline and, without another handoff, immediately repair the bounded deterministic PDF core-contract failures already exposed by that baseline.

This is a staged macro-goal:

1. **Stage A — eliminate remaining test/CI isolation defects and prove the exact stable PDF failure set.**
2. **Stage B — repair that bounded PDF classifier/OCR/reading-order/cache contract set.**
3. **Stage C — make the authoritative native Windows CI baseline green and execute the truthful native launch smoke.**

Continue through all three stages in one run unless a genuine unresolved architecture contract prevents safe implementation.

## Why now

Goal 0002 improved the baseline but its single-thread audit missed failures that appear under normal parallel `cargo test`.

The authoritative Windows run for commit `9a8d9fe` shows:

- prerequisites: PASS;
- `cargo check --workspace`: PASS;
- `cargo build --workspace`: PASS;
- HTML/Pandoc tests: PASS;
- core tests: **156 passed / 11 failed**;
- native launch smoke: SKIPPED because tests failed first.

Four of those eleven were not included in the claimed seven-PDF-defect boundary.

We should not start Windows TTS on top of a noisy baseline, but we also should not spend another human round trip merely fixing three small harness races and then asking permission to touch the already-known PDF contract cluster.

## Read before implementation

In addition to the standard `AGENTS.md` reading order, read:

- `docs/work/reports/0002-rerun.md`;
- `docs/work/reviews/0002.md`;
- `docs/project/current-status.md`;
- `docs/roadmaps/pdf-type-and-text-layer-classification-roadmap.md`;
- `docs/roadmaps/pdf-ocr-geometry-aware-alignment-roadmap.md`;
- `docs/roadmaps/native-pdf-rendering-and-text-sync-roadmap.md`.

Treat old roadmap checkbox completion as historical claims. The written contracts and current failing regression tests are evidence; do not assume every checked box is actually correct.

# Stage A — finish the parallel baseline

## A1 — reproduce normal parallel failures

Do NOT use `--test-threads=1` as the authoritative result.

Run normal:

```powershell
cargo test -p lanternleaf-core --lib
cargo test --workspace
```

Use Windows CI as authoritative when the local machine cannot compile the native workspace.

Record the exact failure set.

## A2 — remove the cache environment race

Current evidence:

`cache::tests::cache_root_uses_env_override_when_present` mutates process-wide `LANTERNLEAF_CACHE_DIR` while other tests call `cache_root()`.

Parallel CI then shows metadata writes followed by immediate load failures in:

- `pdf_sync_meta_roundtrip_preserves_geometry_mode_and_strategy`;
- `pdf_sync_meta_roundtrip_preserves_runtime_policy`.

Preferred correction:

- refactor cache-root resolution so the environment read is a thin outer boundary and the actual resolution logic can be tested with an injected override value;
- make the env-override unit test test a pure/helper boundary without mutating process-global environment.

Do not solve this by forcing the entire suite single-threaded.

If another process-global mutation remains, isolate it narrowly and document why.

After repair, stress the normal parallel core suite multiple times to prove the race is gone.

## A3 — make source-ingestion fixtures cross-platform

Current Windows failure:

`Plain text source fixture.\r\n` vs expected `Plain text source fixture.\n`.

Preserve product semantics. Fix repository fixture determinism rather than casually changing TXT ingestion behavior.

Preferred solution: explicit repository line-ending policy for the source-ingestion fixtures, e.g. `.gitattributes` with LF enforcement.

Do not weaken the smoke into a meaningless contains-only assertion.

## A4 — classify the parallel-only OCR alignment failure

The normal Windows run additionally fails:

`session::tests::pdf_ocr_alignment_artifact_populates_token_lineage_and_cross_column_contract`.

It passes in Codex's reported single-thread failure set.

Because this test persists/reloads cache-owned artifacts, first rerun it after the cache-root race is removed.

- If it becomes stable: classify it as the same isolation defect.
- If it still fails deterministically: promote it into the Stage B PDF contract set with evidence.
- If it fails only under parallel execution: find the remaining shared-state leak before Stage B.

Do not label it a PDF algorithm defect merely because its name contains PDF.

## A5 — remove obsolete active Tauri/React CI

Director decision:

`.github/workflows/gui-migration.yml` must no longer be an active GitHub Actions workflow.

It currently runs Tauri bridge, frontend, Playwright, and Tauri E2E jobs, contradicting the authoritative native egui architecture.

Remove it from active workflows.

Do not delete historical Tauri/React source/docs solely because they are obsolete; they remain parity/history reference unless separately authorized.

If any useful native Rust check exists only in that workflow, migrate that check into an appropriate native workflow before removing it.

## A6 — make launch smoke independent of the test gate

The truthful native launch smoke is currently after `cargo test --workspace`, so it is skipped whenever known tests fail.

Reorder the Windows baseline so that after a successful build:

1. native launch smoke runs;
2. then workspace tests run.

This lets startup evidence remain independent while Stage B repairs test failures.

Required smoke semantics:

- early exit of any code = failure;
- process alive for the smoke interval = success, then workflow terminates it;
- startup stderr/error evidence should be visible when possible.

If GitHub Windows runners cannot host the event loop, prove the limitation rather than loosening the check.

## Stage A acceptance checkpoint

Before Stage B, establish:

- cache race failures gone;
- TXT line-ending failure gone;
- parallel-only alignment failure either gone or explicitly promoted;
- obsolete web/Tauri workflow no longer active;
- native launch smoke actually executes after build;
- exact remaining failure set consists only of deterministic PDF core-contract failures.

Do not stop for a new prompt at this checkpoint. Continue directly to Stage B.

# Stage B — repair the bounded PDF core contracts

## Contract authority

Canonical principles:

- `tts_text` remains the owner of playback/search/bookmarks/sentence indexing.
- PDF geometry is quality-classified evidence, not truth.
- fallback moves downward in confidence only.
- classifier decisions must be deterministic/explainable.
- hidden OCR overlays must not be promoted to clean embedded text.
- scan/image-heavy pages must not be misrepresented as trustworthy embedded text.
- reading-order decisions must be explicit and stable.
- cache persistence/reopen must reproduce the same class/policy unless source/version changes.
- OCR normalization may repair text only under the existing normalization contract; do not invent aggressive semantic rewriting.
- cross-column sentence geometry is allowed only when confidence justifies it.

## Known Stage B failures from Goal 0002

Start with the stable set produced by Stage A. The previously observed deterministic failures include:

1. `classification_fixture_matrix_matches_expected_contracts`
   - `academic-layout-hostile` OCR recommendation mismatch.

2. `classification_rollup_prefers_scan_when_sampled_pages_are_image_only`
   - current rollup selects `HiddenOcrOverlay`, expected `ScanWithWeakOcr`.

3. `derive_pdf_ocr_pipeline_summary_captures_engine_and_fallback_policy`
   - normalization summary fails to record expected footnote-marker adjustment.

4. `hidden_overlay_and_scan_fixtures_record_ocr_confidence_thresholds`
   - scanned fixture misses OCR confidence contract.

5. `normalize_pdf_text_for_reader_tracks_ocr_normalization_edits`
   - expected normalized `Alphabeta line.` is not produced.

6. `page_reading_order_resolver_covers_ocr_layout_families`
   - page 4 resolves as `OuterMarginSidenotes`, expected `BottomFootnoteBand`.

7. `pdf_cache_roundtrip_preserves_chunk_page_ranges_and_meta`
   - metadata resolves as `EmbeddedClean`, expected `LayoutHostileDocument`.

8. The cross-column alignment contract from Stage A, **only if it remains deterministic after race repair**.

## Implementation method

For every remaining failure:

1. reproduce it individually;
2. inspect the implementation path and fixture;
3. identify the exact contract represented by the test;
4. compare that contract to the current PDF roadmaps and neighboring tests;
5. repair production behavior when the regression test matches the intended contract;
6. change a test/fixture expectation only when stronger architectural evidence proves the expectation itself stale;
7. add/strengthen a regression test when the root cause was under-specified.

Do not simply edit expected enums/numbers until tests turn green.

## Scope boundary

Authorized:

- PDF classification precedence/rollup;
- OCR recommendation/confidence derivation;
- OCR text normalization accounting;
- reading-order classification;
- cache serialization/version/reconstruction related to the failing contracts;
- deterministic alignment artifact construction if promoted from Stage A;
- small helper extraction/refactoring needed to keep fixes understandable.

Not authorized:

- broad native page-renderer redesign;
- egui PDF visual/viewport rewrite;
- interactive TTS follow-scroll redesign;
- new OCR engine;
- Windows TTS;
- mass rewrite of `session.rs` or `source_pipeline.rs`.

Existing very large files are debt. Prefer extracting coherent helpers/modules if a fix would otherwise grow them substantially, but do not turn this goal into an unrelated god-file decomposition project.

# Stage C — green native baseline

After Stage B:

Run:

```powershell
cargo test -p lanternleaf-core --lib
cargo test --workspace
cargo check --workspace
cargo build --workspace
git diff --check
```

Use Windows CI for the full MSVC/Pandoc path.

The final native Windows workflow should prove, in this order or equivalent:

- prerequisites;
- workspace check;
- workspace build;
- truthful native launch smoke;
- workspace tests.

## Acceptance gates

Goal 0003 is DONE only when:

1. normal parallel test isolation defects from Goal 0002 are eliminated;
2. source-ingestion fixture smoke is cross-platform deterministic;
3. the parallel-only alignment failure is correctly classified and resolved;
4. obsolete Tauri/React migration CI is no longer active;
5. native launch smoke actually executes and passes truthfully on Windows CI;
6. every stable bounded PDF contract failure from Stage B is resolved without weakening contracts;
7. `cargo test --workspace` passes in Windows CI;
8. `cargo check --workspace` passes in Windows CI;
9. `cargo build --workspace` passes in Windows CI;
10. the final report distinguishes test-harness fixes from actual PDF behavior fixes;
11. branch/report/handoff protocol is complete.

## Non-goals

Do not:

- implement native Windows TTS;
- redesign TTS backend architecture;
- perform broad native PDF visual rendering work;
- redesign interactive PDF TTS/highlight/follow-scroll;
- add new source formats;
- remove historical Tauri/web docs/source solely for cleanup;
- mass-format the repository;
- suppress/ignore tests to make CI green.

## Repository handoff

Branch:

`codex/0003-baseline-and-pdf-contract-recovery`

Write:

`docs/work/reports/0003.md`

Report sections:

### Result
### Stage A Baseline Isolation
### Native Launch Evidence
### Stable PDF Failure Set
### PDF Contract Repairs
### Validation
### CI State
### Remaining Product Risks
### Recommended Next Goal
### Git

Return the shared checkout to `main` after pushing.

## Human verification

No routine human relay is required.

Do not ask the human to install MSVC/Pandoc merely to satisfy automated evidence if Windows CI can prove the goal.

If Windows CI proves a live native event loop but interactive rendering/audio later needs subjective confirmation, leave that for the director to schedule after integration.

## Stop / escalation conditions

Stop and mark BLOCKED only if:

- two current PDF roadmap/contracts directly contradict each other and choosing one changes user-visible semantics;
- a failing PDF test requires broad visual renderer/TTS synchronization redesign outside this goal;
- safe repair requires destructive cache/persistence migration without an existing versioned recovery path;
- unrelated human working-tree changes prevent safe branch work.

Do not stop merely because Stage A reveals an eighth bounded PDF core-contract failure; promote it into Stage B and continue.
