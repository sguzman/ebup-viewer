# 0002 — Close the trustworthy baseline

## Outcome

Finish restart Gate 0 by turning the recovered Windows build into a truthful, deterministic development baseline:

- native startup errors must be observable instead of silently converted to code 0;
- the Windows launch smoke must stop producing false positives;
- existing workspace tests must be classified and stabilized as far as possible without redesigning product behavior;
- external prerequisites must be explicit and diagnosable;
- basic non-PDF source ingestion must have deterministic smoke coverage.

This is a macro-goal. Continue through diagnosis -> repair -> test -> retest passes until the acceptance gates pass or a real architecture/product-contract ambiguity requires director input.

## Why now

Goal 0001 recovered Windows compilation but exposed two baseline defects:

1. `eframe::run_native(...)` errors are discarded, making the launch smoke untrustworthy.
2. The existing workspace test suite has a cluster of environment/fixture/contract failures.

Starting Windows TTS before these are resolved would make later regressions difficult to distinguish from pre-existing baseline noise.

## Starting evidence

Accepted Goal 0001 state:

- Windows CI `cargo check --workspace`: PASS.
- Windows CI `cargo build --workspace`: PASS.
- Runtime bindgen/libclang dependencies: REMOVED.
- Latest reviewed core tests: 153 passed / 14 failed.
- Native launch smoke: FALSE-POSITIVE CAPABLE.
- Local maintainer environment: missing Visual Studio/MSVC C/C++ tools for the MSVC Rust target.
- HTML conversion tests: current Pandoc dependency is not provisioned in CI.

Read:

- `docs/work/reports/0001.md`
- `docs/work/reviews/0001.md`
- `docs/project/current-status.md`
- relevant existing PDF and reader roadmaps before changing expectations.

## Authorized passes / subtracks

### Pass A — make native startup truthful

Fix the error boundary around the native egui entrypoint.

Requirements:

- do not discard `eframe::run_native` errors;
- propagate or explicitly surface startup failure to the root process;
- a failed native window bootstrap must produce a non-zero process result and actionable tracing/error output;
- preserve the single-instance behavior, but distinguish "another instance owns the lock" from an eframe startup error.

Update the Windows smoke so **early process exit is not automatically success**.

Preferred success condition in CI:

- the process remains alive for a bounded smoke window and is then terminated by the workflow.

If the GitHub Windows runner cannot reliably host a GUI event loop, prove that with captured error evidence and replace the CI check with the strongest non-interactive bootstrap check available. In that case, mark interactive native launch explicitly HUMAN-VERIFICATION-REQUIRED rather than faking success.

### Pass B — classify every current test failure

Run the full workspace tests and create a table in the Goal 0002 report for every failing test:

- test name;
- failure class;
- root cause;
- whether it is a harness/environment defect, stale fixture/expectation, or product behavior defect;
- repair performed or escalation reason.

Do not bulk-update assertions merely to match current output.

For PDF-related failures, consult the existing architecture/roadmap contracts. Change expectations only when the implementation is demonstrably consistent with the intended contract and the fixture/test is stale.

If a failure exposes a real PDF algorithm defect whose repair would require entering Gate 3/4 feature work, preserve the test and escalate instead of silently weakening it.

### Pass C — eliminate test environment races

Repair shared temp/cache/config/environment tests so they are deterministic under normal parallel `cargo test --workspace`.

Prefer:

- unique temp roots;
- injected config/cache roots;
- scoped guards;
- existing synchronization primitives where global process environment cannot yet be removed.

Do not solve races by globally forcing the entire test suite single-threaded unless there is no narrower safe option.

### Pass D — make Pandoc behavior deterministic

The current code uses Pandoc for HTML canonical text and DOC/DOCX conversion.

For this goal:

- make the dependency explicit;
- make missing Pandoc errors actionable;
- ensure CI provisions a known Pandoc environment for tests that require real conversion, OR isolate conversion behind a testable command abstraction with deterministic fixtures;
- do not simply skip all Pandoc tests.

Record whether Pandoc remains a runtime prerequisite after this goal. Do not redesign HTML/DOCX ingestion in this baseline goal.

### Pass E — repair fixture/config drift

Address stale deterministic fixtures/config expectations that can be corrected without product redesign, including the currently observed abbreviation/live-config failures and PDF fixture calibration drift.

Rules:

- preserve intended behavior from current architecture/roadmap contracts;
- add comments or fixture metadata where a non-obvious expectation represents a deliberate contract;
- do not hide known product defects.

### Pass F — non-PDF ingestion smoke corpus

Add a small deterministic source-ingestion smoke set covering at least:

- TXT;
- Markdown;
- HTML.

Include EPUB if a compact stable fixture already exists or can be added without scope expansion.

The smoke should prove source -> canonical text / presentation payload behavior at the Rust layer. It does not need GUI/TTS parity yet.

Use tiny repository fixtures, not developer-machine paths.

### Pass G — Windows developer prerequisite diagnostics

Add a small repository-owned PowerShell diagnostic, for example:

`scripts/windows-dev-check.ps1`

It should detect and clearly report at least:

- Rust host/target;
- Visual Studio/MSVC C/C++ availability for the MSVC target;
- CMake availability;
- Pandoc availability;
- any other native prerequisite actually required by the current build.

Do not hardcode the maintainer's machine paths.

It may print exact installation guidance, but do not perform privileged installations automatically.

### Pass H — CI baseline

Update Windows CI so the baseline is meaningful:

- workspace check;
- workspace build;
- workspace tests;
- truthful native/bootstrap smoke;
- deterministic Pandoc/native prerequisite provisioning where needed.

The job should be green only when its checks actually pass. Do not use `if: always()` to turn a failed required gate into an apparently healthy baseline.

## Non-goals

Do not:

- implement native Windows TTS;
- redesign TTS backend ownership;
- finish PDF rendering;
- redesign PDF sync/OCR algorithms;
- redesign the GUI;
- add new source formats;
- replace Pandoc with a new ingestion architecture;
- mass-format the repository;
- weaken/delete/ignore failing tests just to get green.

## Constraints

- Native Rust + egui remains authoritative.
- Director-owned project philosophy/product scope/priorities/roadmap ordering must not be rewritten.
- Preserve Goal 0001 vendored FFI fixes.
- Keep platform-specific code isolated.
- Use tracing/actionable errors rather than silent failure.
- Keep changes reviewable despite the macro-goal size.

## Acceptance gates

DONE requires:

1. native `run_native` failure is no longer silently discarded;
2. CI launch/bootstrap smoke cannot succeed merely because the process exited code 0 early;
3. every previously failing test is classified in the report;
4. all harness/environment/fixture failures that can be repaired without product redesign are fixed;
5. `cargo test --workspace` passes, unless remaining failures are proven product defects that require a later architecture-approved PDF goal—in that case mark BLOCKED with the exact preserved tests;
6. Pandoc-required tests are deterministic in CI and missing-Pandoc behavior is actionable;
7. TXT/Markdown/HTML Rust ingestion smoke coverage passes;
8. Windows prerequisite diagnostic script exists and is useful;
9. Windows CI truthfully reflects check/build/test/bootstrap state;
10. branch/report/handoff protocol is complete.

## Validation

At minimum:

```powershell
cargo check --workspace
cargo build --workspace
cargo test --workspace
git diff --check
```

Run formatting on files you change. Do not create a repository-wide formatting churn merely because the pre-existing tree is not globally rustfmt-clean.

Run the Windows diagnostic script and include its summarized output in the report.

## Repository handoff

Branch:

`codex/0002-close-trustworthy-baseline`

Write:

`docs/work/reports/0002.md`

Final report sections:

### Result
### Native Startup
### Test Failure Classification
### Changes
### Validation
### External Prerequisites
### Remaining Product Defects
### Recommended Next Goal
### Git

Return the shared checkout to `main` after pushing.

## Human verification

Do not require routine human relay.

If the implementation proves that CI cannot verify a real GUI event loop, include one tiny maintainer test command/scenario in the report. The director will decide when to ask the human to perform it.

## Stop / escalation conditions

Stop and mark BLOCKED only when:

- a remaining failing test proves a real product defect whose fix enters PDF Gate 3/4 architecture work;
- a native startup failure requires changing the chosen egui architecture;
- safe resolution requires a destructive persistence/data migration;
- unrelated human working-tree changes prevent safe branch work.
