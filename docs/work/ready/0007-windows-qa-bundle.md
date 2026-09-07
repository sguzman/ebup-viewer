# 0007 — Prebuilt Windows QA bundle for Gate 2 signoff

## Outcome

Make the remaining real-desktop Gate 2 verification easy for the human maintainer **without requiring MSVC, Pandoc, a Rust toolchain, or manual notification plumbing**.

This goal does not add new reader features.

By the end of Goal 0007:

1. hosted Windows CI produces a self-contained LanternLeaf QA bundle from accepted `main`;
2. the bundle contains the native executable plus the runtime resources actually required to launch and exercise the Windows TTS/non-PDF reader paths;
3. representative TXT / Markdown / HTML fixtures and a deterministic representative EPUB are available inside the bundle;
4. the bundle contains a one-command QA launcher that runs LanternLeaf from the correct working directory and preserves useful logs;
5. CI verifies the bundle structure and scripts without pretending hosted GPU-less execution proves GUI fidelity;
6. the artifact is published through GitHub Actions with a clear stable artifact name;
7. if practical with the existing local Git/GitHub credentials, Codex downloads/extracts the final artifact into a local ignored QA directory so the human's later action is just running the launcher;
8. the report tells the director exactly what the human must run after integration.

No human action is required while Codex executes this goal.

## Read first

- `AGENTS.md`
- `ARCHITECTURE.md`
- `docs/project/current-status.md`
- `docs/project/roles-and-workflow.md`
- `docs/work/reviews/0006.md`
- `docs/work/reports/0006.md`
- `docs/qa/non-pdf-reader-parity.md`
- `docs/qa/non-pdf-reader-windows-checklist.md`
- `docs/roadmaps/restart-master-roadmap-2026-09.md`
- `docs/project/windows-development.md`

# Stage A — determine the minimal runnable Windows bundle

Inspect current runtime file/resource requirements rather than guessing.

Determine which of these are actually required at runtime from a clean extracted directory:

- `lanternleaf.exe`;
- `conf/`;
- fonts/assets loaded by filesystem path;
- TTS/Piper runtime resources;
- other DLL/data dependencies;
- representative QA fixtures.

For the **Windows TTS QA path**, the bundle must not require the user to configure a Piper model merely to launch and select the native Windows backend.

Do not copy the whole repository into the artifact unless a concrete runtime dependency requires it.

Document the resulting bundle manifest.

# Stage B — deterministic representative QA corpus

Include small project-owned QA sources inside the bundle:

- representative TXT;
- representative Markdown;
- representative HTML;
- deterministic representative EPUB.

Prefer reusing the accepted Goal 0006 corpus/builders instead of inventing a second semantic corpus.

If the EPUB currently exists only as a test-time builder, add a small deterministic script/helper or checked-in generated QA fixture as appropriate. Do not add network dependencies or copyrighted book text.

The human should not need Pandoc installed locally to open the bundled HTML/EPUB through the already-built application.

# Stage C — one-command QA launcher

Add a Windows PowerShell launcher intended to live inside the extracted QA bundle, for example:

`run-lanternleaf-qa.ps1`

Requirements:

- runs the bundled executable from the correct working directory;
- prints the bundle/fixture locations clearly;
- creates or identifies a dedicated QA log directory;
- does not install system software;
- does not alter global environment settings persistently;
- does not silently delete existing user data;
- exits cleanly when LanternLeaf exits;
- after exit, gathers the relevant LanternLeaf logs into a small QA handoff directory or ZIP when practical;
- provides actionable startup failure output.

Do not create a complicated interactive wizard.

The later human flow should be approximately:

```powershell
.\run-lanternleaf-qa.ps1
```

followed by the short existing checklist.

# Stage D — hosted Windows artifact

Extend the existing Windows workflow or add a narrowly-scoped QA artifact job.

Requirements:

- build the actual native Windows executable from the same accepted source revision;
- use the documented MSVC/Pandoc setup;
- stage the minimal runtime bundle;
- include representative fixtures and launcher;
- verify expected bundle files exist;
- ZIP the bundle if useful;
- publish through `actions/upload-artifact@v4`;
- use a recognizable artifact name such as `lanternleaf-windows-qa`;
- retain it long enough for the maintainer to perform QA;
- do not make artifact upload dependent on the hosted renderer probe.

The normal required Windows check/build/test baseline must remain green.

Do not call hosted renderer unavailability a successful GUI launch.

# Stage E — optional local artifact retrieval

After the final hosted artifact is available, attempt to minimize the human's later steps.

If the local environment already has a usable authenticated GitHub CLI or another repository-owned, non-interactive way to download this public-repo artifact:

- download the final artifact;
- extract it under an ignored location such as `.qa/windows/latest/`;
- verify the launcher and executable are present.

Do **not**:

- ask the human for a token;
- alter GitHub credentials;
- install GitHub CLI solely for this;
- commit the downloaded binary artifact to Git;
- fail the goal merely because local artifact download is unavailable.

If automatic local retrieval is unavailable, report the exact successful Actions run/artifact name so the director can give the human the shortest possible download steps.

# Stage F — bundle tests

Add deterministic checks for the bundle assembly/launcher where practical.

Verify at minimum:

- executable exists;
- required runtime resources exist;
- TXT/Markdown/HTML/EPUB QA sources exist;
- launcher resolves paths relative to itself rather than an arbitrary current directory;
- log directory/handoff behavior is deterministic;
- no source-tree-only absolute path is embedded as a requirement.

A hosted CI process-liveness smoke remains subject to the known renderer limitation and is not an acceptance gate for GUI fidelity.

# Stage G — preserve workflow notification behavior

Use the normal Goal 0007 watcher protocol.

The terminal notification should occur only after the implementation branch/report are pushed.

No human watcher command.

# Acceptance gates

DONE requires:

1. a documented minimal Windows QA bundle manifest exists;
2. CI builds and stages the real native Windows executable;
3. the bundle contains required runtime resources;
4. representative TXT/Markdown/HTML/EPUB QA sources are present;
5. one-command `run-lanternleaf-qa.ps1` exists and uses bundle-relative paths;
6. launcher/log handoff behavior has deterministic test coverage where practical;
7. GitHub Actions publishes a successful `lanternleaf-windows-qa` artifact;
8. artifact upload is independent of hosted renderer capability;
9. required Windows check/build/normal-parallel tests remain green;
10. local artifact retrieval is attempted but is non-fatal if existing credentials/tools cannot support it;
11. no compiler/Pandoc installation is required from the human to run the prebuilt bundle;
12. no broad release packaging work is introduced;
13. report contains the exact later human launch path or artifact run/name;
14. branch/report/watcher handoff is complete.

# Non-goals

Do not:

- redesign reader behavior;
- begin PDF Gate 3 work;
- fix hypothetical GUI issues that have not yet been observed unless they directly prevent the QA bundle from launching;
- install Visual Studio Build Tools/Pandoc on the human machine;
- create a general public release pipeline;
- add an installer/MSI;
- sign binaries;
- replace the current renderer;
- require physical audio in hosted CI;
- add cloud services;
- commit built binaries to the repository.

# Validation

At minimum:

```powershell
cargo check --workspace
cargo build --workspace
cargo test --workspace
git diff --check
```

Also validate the bundle assembly and PowerShell launcher.

# Repository handoff

Branch:

`codex/0007-windows-qa-bundle`

Report:

`docs/work/reports/0007.md`

Required report sections:

## Result
## Bundle Manifest
## Runtime Dependencies
## Representative QA Corpus
## QA Launcher
## Artifact
## Local Retrieval Attempt
## Validation
## CI
## Human Next Step
## Goal Notification
## Remaining Gate 2 Manual Evidence
## Git

# Human verification

Do not ask the human to test during the Codex session.

After director review/integration, the director will ask the human to run the prepared bundle and the concise checklist.

If the human finds runtime defects, those observations become the next bounded repair goal. A clean signoff closes Gate 2 and unlocks Gate 3.

# Stop / escalation conditions

Stop and mark BLOCKED only if:

- the native executable cannot be made runnable from a staged directory without a new packaging architecture decision;
- required runtime dependencies cannot legally/technically be bundled;
- artifact publication requires a repository-permission change that Codex cannot make;
- producing the QA bundle reveals a new architecture contradiction.

Do not stop merely because automatic local artifact download is unavailable.
