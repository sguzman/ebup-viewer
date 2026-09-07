# 0006 — Non-PDF reader and TTS parity

## Outcome

Establish current, restart-era parity evidence for LanternLeaf's four primary non-PDF reader families:

- TXT;
- Markdown;
- HTML;
- EPUB.

Do not trust historical roadmap checkboxes as proof. Build a representative deterministic repository corpus and exercise the actual current Rust pipeline from source ingestion through reader/session behavior, pretty projection, simulated TTS orchestration, persistence/reopen, and Windows backend-neutrality.

The end state is not merely “the files parse.” It is:

> representative non-PDF sources behave coherently as LanternLeaf reading sessions, with canonical sentence/TTS ownership preserved across presentation modes and reader interactions.

Execute all authorized stages in this single macro-goal without requiring a new human prompt between them.

## Read first

- `AGENTS.md`
- `ARCHITECTURE.md`
- `docs/project/current-status.md`
- `docs/project/roles-and-workflow.md`
- `docs/work/reviews/0005.md`
- `docs/roadmaps/restart-master-roadmap-2026-09.md`
- `docs/roadmaps/egui-reader-rendering-roadmap.md`
- `docs/roadmaps/egui-tts-audio-and-playback-roadmap.md`
- `docs/roadmaps/egui-testing-and-parity-roadmap.md`
- `docs/roadmaps/native-html-epub-rendering-and-tts-sync-roadmap.md`
- `docs/qa/native-html-epub-checklist.md`

Historical checkmarks describe intended/previous behavior only. Current code and new verification evidence are authoritative.

# Stage A — live parity matrix and representative corpus

## A1 — create the parity matrix

Create:

`docs/qa/non-pdf-reader-parity.md`

It must track at least these feature domains across TXT / Markdown / HTML / EPUB:

- source ingestion;
- canonical `tts_text`;
- pretty payload kind;
- canonical sentence generation;
- page/sentence accounting;
- sentence anchor mapping;
- text-only / pretty projection switch;
- sentence click/highlight;
- next/previous/repeat;
- play from page start;
- play from highlight;
- search next/previous;
- auto-scroll / jump mapping semantics;
- persistence / reopen;
- delete/cache cleanup;
- simulated TTS events/cancellation;
- Piper/Windows reader-state neutrality;
- automated evidence;
- remaining manual Windows QA.

Use evidence/status labels that distinguish:

- verified automated;
- verified Windows-specific;
- manual verification required;
- known degraded behavior;
- blocked/defect.

Do not mark a cell complete solely because an old roadmap said so.

## A2 — expand the repository-owned fixture corpus

Current `tests/fixtures/source-ingestion/` contains only tiny TXT/Markdown/HTML fixtures and no representative EPUB.

Add deterministic representative fixtures for all four source families.

### TXT fixture

Include enough structure to exercise:

- multiple sentences;
- multiple paragraphs/sections;
- punctuation;
- Unicode;
- search terms repeated in more than one place;
- enough content for multiple reader pages/windows under test configuration.

### Markdown fixture

Include representative:

- headings;
- paragraphs;
- emphasis;
- list;
- link;
- image syntax where practical;
- repeated search terms;
- enough canonical text for navigation.

The canonical TTS text must not accidentally preserve raw Markdown syntax where plain speech text is expected.

### HTML fixture

Include representative:

- headings;
- paragraphs;
- emphasis;
- internal anchor/link;
- external link;
- image;
- list;
- table;
- secondary/footnote-like content if current sanitizer/render model supports it;
- repeated search terms.

Keep it deterministic and self-contained. Local fixture assets may be added.

### EPUB fixture

The repository currently lacks a representative source-ingestion EPUB fixture.

Add either:

- a small deterministic repo-owned EPUB; or
- a deterministic test builder that constructs an EPUB from repository-owned source assets with no network dependency.

It should include multiple sections/chapters and representative HTML structure. Include an image/internal link if reasonably supported without turning fixture creation into a separate project.

Do not pull test content from the network.

## A3 — fixture policy

- keep fixtures small enough for fast CI;
- use LF-stable repository policy;
- avoid copyright-sensitive third-party book text; use project-authored fixture prose;
- fixtures should make expected sentence/search/navigation behavior obvious.

# Stage B — actual source -> session contracts

For each TXT / Markdown / HTML / EPUB fixture, exercise the real loader and reader/session construction rather than synthetic snapshots.

## B1 — ingestion contract

Verify:

- source opens successfully;
- `tts_text` is nonempty and contains expected canonical prose;
- expected presentation artifact exists:
  - TXT: no pretty artifact required;
  - Markdown: `reading_markdown`;
  - HTML: `reading_html`;
  - EPUB: `reading_html`;
- no wrong-format presentation artifact silently takes ownership.

## B2 — canonical text cleanliness

Verify canonical TTS text does not contain obvious source syntax/noise that should have been removed.

Examples:

- Markdown heading/list/link syntax should not be spoken literally merely because it appears in source markup;
- HTML tags/scripts/styles should not appear in canonical speech text;
- EPUB markup should not leak into canonical speech text.

If current Markdown canonicalization is wrong, fix the bounded production behavior rather than weakening the expectation.

## B3 — session invariants

For each format verify:

- correct `PrettyKind`;
- canonical sentence count is stable and nonzero;
- page sentence counts are internally consistent;
- sentence anchor map length/indices are valid;
- current audio/display sentence mapping starts in a valid state;
- canonical `tts_text` remains the source for audio sentence ownership.

## B4 — presentation changes do not change canonical ownership

Test that:

- toggling text-only vs pretty does not replace/reorder canonical audio sentences;
- changing display-only settings does not alter canonical sentence identity;
- repagination may change display pages but must preserve the logical reading position/canonical audio ownership as defined by current architecture.

# Stage C — reader interaction parity

Use real sessions from the representative fixtures.

For all formats where the operation is meaningful, cover:

- sentence click;
- highlighted sentence vs audio cursor;
- next sentence;
- previous sentence;
- repeat sentence;
- play from page start;
- play from highlight;
- next/previous page;
- search set query;
- search next;
- search previous;
- selected search result;
- text-only toggle;
- original/display-text toggle if applicable.

Assertions must emphasize canonical position, not UI implementation details.

While TTS is playing, `tts_current_sentence_text` should agree with the canonical sentence selected by the current audio cursor.

# Stage D — pretty projection and anchor behavior

Applies to Markdown / HTML / EPUB.

## D1 — current Rust-native pretty model

Exercise the current Rust-native conversion/render-model helpers, such as the current Markdown/HTML-to-block path.

Representative fixtures should produce readable expected blocks for the structures the application claims to support.

At minimum verify available support for:

- headings;
- paragraphs;
- emphasis;
- lists;
- links;
- images;
- tables/secondary blocks where applicable.

Unsupported rich HTML behavior may degrade, but degradation must be explicit/readable rather than silently corrupting canonical text.

Do not introduce a WebView/DOM dependency.

## D2 — anchor mapping

Verify sentence-to-pretty mapping invariants:

- exact mappings when available;
- deterministic bounded fallback when exact mapping is unavailable;
- no random unrelated-block highlight;
- toggling presentation mode does not reset canonical sentence/search state.

## D3 — auto-scroll / jump contracts

Hosted GPU-less CI need not visually scroll an egui window.

Test the pure/current decision logic that determines whether:

- the active sentence should trigger a jump/scroll;
- the same sentence should not cause repeated jitter;
- fallback strength influences scroll decisions correctly.

Do not claim visual fidelity from these logic tests.

# Stage E — TTS runtime parity using real sessions

Use the simulated TTS runtime with sessions loaded from the real representative fixtures.

Do not rely only on synthetic `ReaderSnapshot` helpers.

For each format, cover the meaningful subset of:

- Play;
- Pause;
- TogglePlayPause;
- PlayFromPageStart;
- PlayFromHighlight;
- SeekNext;
- SeekPrev;
- RepeatSentence;
- Stop;
- cancellation;
- progress/completion/cancel events.

Verify:

- canonical cursor advances correctly;
- stale cancelled requests cannot mutate the active canonical cursor;
- backend settings do not change reader index semantics;
- Piper vs Windows backend selection remains a synthesis concern, not a reader-state fork.

No physical audio device is required for this stage.

## E2 — Windows source-to-synthesis contract

If practical without GUI/audio output, add a Windows-only integration probe that:

1. loads one representative non-PDF fixture through the real loader/session path;
2. obtains one canonical sentence;
3. prepares/synthesizes it through the Windows backend;
4. verifies the resulting WAV decodes.

This complements the generic Windows synthesis probe with a source -> session -> synthesis path.

If this duplicates existing coverage without adding real evidence, document why and keep the existing probe.

# Stage F — persistence, reopen, and cleanup

For representative non-PDF sources, add/strengthen parameterized tests for the current product contract.

Verify:

- bookmark/progress persistence;
- reopen restores the expected reading position/canonical cursor where supported;
- cache artifacts can be reused/rebuilt deterministically;
- corrupt recoverable state does not permanently poison reopen;
- recent/source delete and cache cleanup is idempotent.

Do not reintroduce process-global cache/config races.

If current persistence tests require environment mutation, either keep synchronization narrow or refactor the directly-needed seam; do not globally serialize the entire test suite.

# Stage G — Windows validation and manual-QA preparation

## G1 — full automated validation

Required:

```powershell
cargo check --workspace
cargo build --workspace
cargo test --workspace
git diff --check
```

Normal parallel tests are authoritative.

Hosted Windows CI with Pandoc/MSVC is authoritative for source paths that depend on Pandoc.

The hosted renderer capability probe remains separate and must not be changed to fake GUI success.

## G2 — real local launch if already possible

If the current local Windows environment can build/run the app safely:

- launch the real LanternLeaf binary;
- verify it remains alive for a bounded smoke interval;
- capture startup diagnostics.

Do not claim visual reader parity from “process stayed alive.”

If the local checkout has only stale build artifacts or lacks the documented MSVC/Pandoc prerequisites, record that and continue. A narrow target/build-artifact cleanup is authorized when clearly safe; do not destroy unknown human work.

## G3 — manual QA checklist

Create:

`docs/qa/non-pdf-reader-windows-checklist.md`

Keep it concise enough to run in roughly five to ten minutes later.

It should ask a human to verify representative:

- TXT open/read/search/highlight/TTS;
- Markdown pretty/text-only and TTS;
- HTML pretty/text-only and TTS;
- EPUB chapter/pretty/text-only and TTS;
- Windows voice selection;
- Piper/Windows backend switch;
- play/pause/next/previous;
- jump/auto-scroll;
- reopen/persistence.

This goal must not require the human to run it now. The director decides after review whether a manual QA pass is necessary before closing Gate 2.

# Goal-completion notification handoff

Use the repository protocol from `AGENTS.md`.

At goal activation:

- launch `scripts/codex-goal-notify.ps1` detached for Goal 0006.

At terminal state:

1. write report;
2. move goal to `done/` or `blocked/`;
3. commit and push the terminal implementation branch;
4. after push succeeds, call:
   `scripts/signal-codex-goal-terminal.ps1 -GoalId 0006 -RepoRoot <repo> -State <done|blocked>`
5. allow the helper to wait briefly for watcher acknowledgment;
6. notification/ack failure is non-fatal;
7. restore the shared checkout to `main`.

Record watcher startup/signal/ack status in the report.

# Acceptance gates

DONE requires:

1. a live non-PDF parity matrix exists;
2. deterministic representative TXT/Markdown/HTML/EPUB corpus exists;
3. all four families load through the actual current loader/session path;
4. canonical TTS text is clean enough for its source family and markup does not silently own speech;
5. canonical sentence ownership remains stable across presentation toggles/settings;
6. sentence click/highlight/navigation/search contracts pass;
7. Markdown/HTML/EPUB pretty projection and anchor behavior have current Rust-native tests;
8. auto-scroll/jump decision semantics have deterministic test coverage where feasible;
9. simulated TTS runtime parity uses real fixture sessions;
10. cancellation/stale-request behavior remains safe;
11. persistence/reopen/delete behavior has representative coverage;
12. Piper/Windows reader-state semantics remain backend-neutral;
13. Windows source-to-synthesis probe is added if it materially improves evidence, or the report explains why existing coverage is sufficient;
14. required hosted Windows check/build/normal-parallel tests pass;
15. old historical checkmarks are not used as sole acceptance evidence;
16. no PDF/DOCX/browser-tab/Calibre scope creep occurs except a bounded shared reader defect;
17. the manual Windows parity checklist is created;
18. watcher is launched automatically and post-push terminal signal/ack behavior is reported;
19. branch/report/handoff protocol is complete.

# Non-goals

Do not:

- work on PDF rendering or PDF TTS/highlight synchronization;
- expand or harden DOC/DOCX;
- perform browser-tab or Calibre parity work except a directly shared reader defect;
- add a new TTS backend;
- resurrect Tauri/React/WebView;
- perform broad GUI redesign;
- mass-refactor large modules;
- grow `crates/lanternleaf-egui/src/app/mod.rs` for new reader logic when a coherent module can own it;
- change the renderer to satisfy hosted CI;
- make physical speaker playback an automated gate;
- begin packaging/release work.

# Stop / escalation conditions

Stop and mark BLOCKED only if:

- current canonical reader/session contracts for these source families materially contradict one another and require a director semantics choice;
- fixing a discovered defect requires a broad architecture change outside the existing canonical-text/projection model;
- deterministic EPUB test construction is impossible without adopting a new external/runtime dependency requiring director approval;
- an unrelated product failure outside this bounded goal blocks required Windows tests.

Do not stop for ordinary reader bugs, mapping defects, fixture problems, or test gaps inside TXT/Markdown/HTML/EPUB. Diagnose, fix, and continue.

# Repository handoff

Branch:

`codex/0006-non-pdf-reader-parity`

Report:

`docs/work/reports/0006.md`

Required report sections:

## Result
## Parity Matrix
## Representative Corpus
## Source / Session Contracts
## Reader Interactions
## Pretty Projection / Anchor Mapping
## TTS Runtime Parity
## Persistence / Reopen
## Windows Validation
## Manual QA Remaining
## Goal Notification
## CI
## Remaining Product Risks
## Recommended Next Goal
## Git


# Director correction pass — required before integration

The first implementation pass was reviewed and **is not yet accepted**.

Read and execute:

`docs/work/reviews/0006.md`

This is a continuation of the same numbered Goal 0006. Reuse the existing Codex Goal lifecycle and implementation branch. Do not start Goal 0007 and do not treat the first green CI run as final acceptance.

All original non-goals remain in force. The correction is limited to evidence/behavior gaps identified in the director review.
