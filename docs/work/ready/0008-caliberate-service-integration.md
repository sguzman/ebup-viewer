# 0008 — First-class Caliberate service integration

## Outcome

Make LanternLeaf a first-class consumer of the Caliberate HTTP/JSON library service while preserving existing Calibre content-server compatibility.

The intended local product relationship is:

```text
Caliberate library/database
  -> Caliberate service at http://127.0.0.1:8181
  -> LanternLeaf library/catalog browser
  -> selected book format streamed/materialized locally
  -> existing LanternLeaf source/session/TTS pipeline
```

This goal should make the existing LanternLeaf library browser useful against a running local Caliberate instance without requiring the human to manually add files one at a time.

## Why now

Real Windows desktop bootstrap/build/launch has now been observed successfully. The current QA launch intentionally uses isolated state, so the app can appear empty until content is opened.

LanternLeaf already contains a Calibre browser/catalog subsystem, but its HTTP transport targets the legacy Calibre content-server web UI contract:

- `/interface-data/books-init?...library_id=...`;
- `/get/<FORMAT>/<id>/<library_id>`;
- optional old cover endpoints.

Caliberate now exposes a source-neutral versioned HTTP API intended for downstream consumers. LanternLeaf is the natural first major consumer, and this integration materially strengthens both products:

- Caliberate proves it can act as a reusable library service rather than only a standalone clone;
- LanternLeaf gains a real library/catalog source instead of relying primarily on manual file opening.

## Authoritative Caliberate v1 service contract

The Caliberate repository currently exposes these relevant routes:

- `GET /health`;
- `GET /api/v1/books`;
- `POST /api/v1/books/query`;
- `GET /api/v1/search`;
- `GET /api/v1/books/{id}`;
- `GET /api/v1/books/{id}/formats`;
- `GET /api/v1/books/{id}/content`;
- `GET /api/v1/books/{id}/content/{format}`;
- `GET /api/v1/facets/{kind}`.

`GET /api/v1/books` returns a page object with:

- `items`;
- `total`;
- `offset`;
- `limit`.

Each item includes, at minimum:

- `id`;
- `title`;
- `primary_format`;
- `formats[]` where each format has `format` and optional `size_bytes`;
- `authors[]`;
- `pubdate`;
- additional metadata not all required by LanternLeaf yet.

The server caps one page at 500 items.

Format bytes are available from:

`GET /api/v1/books/{id}/content/{format}`

Caliberate auth, when enabled, accepts either:

- `Authorization: Bearer <api-key>`; or
- `x-api-key: <api-key>`.

The normal local development target is:

`http://127.0.0.1:8181`

## Starting evidence

Current LanternLeaf code already has:

- `CalibreConfig`;
- `CalibreBook`;
- cached catalog persistence;
- thumbnail cache;
- catalog loading/cancellation;
- materialization/download cache;
- existing library browser UI;
- the normal source/session opening path after a book is materialized.

Current defects relative to Caliberate:

1. `src/calibre/catalog.rs::fetch_books` assumes the Calibre web UI `books-init` JSON shape.
2. It requires a `#library_id` fragment in `library_url`, which Caliberate does not need.
3. `materialize_book_path` downloads from legacy `/get/...` routes instead of the versioned Caliberate content endpoint.
4. The checked-in `conf/calibre.toml` points at an obsolete private-LAN server and contains stale default credentials.
5. Remote thumbnail logic assumes Calibre-specific cover endpoints; Caliberate v1 currently exposes `has_cover` metadata but no dedicated cover route.

## Architecture decision

Do **not** create a second parallel library UI.

Keep one library/catalog browser and introduce a small provider/transport boundary behind it.

Conceptually:

```text
Library browser/UI
  -> catalog service abstraction
      -> Caliberate v1 provider
      -> legacy Calibre content-server provider
```

The existing `CalibreBook` model may remain for this bounded goal if renaming it would cause broad churn. Prefer adding a narrow provider kind/transport abstraction over a repository-wide terminology rewrite.

Caliberate v1 is the preferred/default local provider.

Legacy Calibre support remains as a compatibility path and must not be silently broken.

## Authorized passes / subtracks

### A. Provider configuration boundary

Extend the existing integration config with an explicit provider selection.

Acceptable shape:

- `provider = "caliberate"`;
- `provider = "calibre"`;
- optionally `provider = "auto"` if the implementation can keep detection deterministic and well-tested.

Requirements:

- old configs without the new field remain parseable;
- checked-in default targets `http://127.0.0.1:8181`;
- no library fragment is required for Caliberate;
- remove stale checked-in username/password defaults;
- add optional Caliberate API-key configuration if needed;
- never log secret API-key values.

Do not force the human to copy a local payload or create an out-of-repo config merely to use the standard local service.

### B. Caliberate catalog adapter

Implement Caliberate catalog retrieval using `GET /api/v1/books`.

Requirements:

- page through the API until all results are retrieved or cancellation occurs;
- respect the server's maximum page size rather than assuming one giant response;
- deterministic ordering, preferably requesting `sort=title&direction=asc`;
- preserve cancellation checks between pages and while mapping rows;
- map:
  - `id`;
  - `title`;
  - authors joined for the current UI model;
  - year from `pubdate` when parseable;
  - supported formats;
  - selected extension;
  - selected format size.
- select the first format matching LanternLeaf's configured `allowed_extensions` priority;
- if `formats[]` is absent/empty but `primary_format` is valid, allow it as a fallback;
- skip entries that have no LanternLeaf-readable format;
- keep cache behavior deterministic and provider-scoped so Caliberate and legacy Calibre catalogs cannot collide.

### C. Caliberate content materialization

For a Caliberate-backed book, materialize through:

`GET /api/v1/books/{id}/content/{format}`

Requirements:

- stream bytes into the existing LanternLeaf download cache;
- use temporary/partial file + atomic rename semantics;
- preserve current source extension;
- actionable error on non-success response;
- cancellation where the current abstraction permits it;
- downloaded result must enter the existing document/session path exactly like a locally opened source;
- do not add a Caliberate-specific reader/session implementation.

### D. Authentication

If Caliberate API-key auth is configured:

- send a supported Caliberate auth header;
- keep legacy Basic-auth behavior isolated to the legacy Calibre provider;
- do not reinterpret legacy username/password as a Caliberate API key;
- redact secrets from tracing/errors.

Unauthenticated local Caliberate remains the default.

### E. Thumbnail behavior

Do not block the goal on a new Caliberate cover API.

For Caliberate-backed books:

- existing locally materialized EPUB cover extraction may continue to provide thumbnails after materialization;
- initial remote cover thumbnail may remain absent when the service has no cover endpoint;
- do not repeatedly hammer legacy `get/thumb` or `get/cover` endpoints against a Caliberate service.

If a tiny provider-aware suppression is needed in `thumbnails.rs`, it is authorized.

A new Caliberate cover endpoint is a future enhancement unless it is unexpectedly trivial and independently tested.

### F. Diagnostics and status

Add useful `tracing` around:

- selected provider kind;
- base URL;
- catalog page requests/results;
- total mapped/filtered books;
- content materialization;
- provider-specific failures/fallback decisions.

Never log passwords or API keys.

The UI should present a useful error when the configured service is unreachable rather than silently showing an empty catalog as though no books exist.

### G. Regression tests

Add deterministic tests using a local/mock HTTP server or equivalent in-process test harness.

Cover at least:

1. Caliberate page parsing.
2. Multi-page catalog retrieval.
3. supported-format priority selection.
4. `primary_format` fallback.
5. author/year/size mapping.
6. unreadable-format filtering.
7. content download route and bytes.
8. no Caliberate library fragment requirement.
9. API-key header behavior without secret leakage.
10. legacy Calibre path remains functional at its existing contract boundary.
11. Caliberate provider does not probe legacy thumbnail endpoints.
12. cache signature/provider isolation.
13. unreachable service produces an actionable error rather than a successful empty catalog.

### H. Repo-native QA integration

Update the repo-native QA/default config so that a normal:

`git pull -> .\qa.ps1`

launch will naturally target `http://127.0.0.1:8181` for the library provider when the local Caliberate service is running.

Do not make LanternLeaf responsible for starting/stopping Caliberate in this goal.

Do not add Caliberate itself as a Scoop dependency.

## Non-goals

- Do not redesign the whole library UI.
- Do not rename every `Calibre*` type in the repository.
- Do not embed Caliberate or link its crates directly into LanternLeaf.
- Do not bypass HTTP by opening Caliberate's SQLite database directly.
- Do not make LanternLeaf manage the Caliberate server process.
- Do not add cloud discovery/service registries.
- Do not add synchronization/write-back/editing of Caliberate metadata.
- Do not redesign LanternLeaf reader/TTS state.
- Do not begin Gate 3 PDF rendering work.
- Do not require new manual downloaded payloads.
- Do not require the human to reconfigure the existing Caliberate database.
- Do not add a dedicated Caliberate cover endpoint unless it remains a genuinely tiny bounded add-on.

## Constraints

- Rust-native implementation.
- Preserve the existing UI -> app/runtime -> core/domain dependency direction.
- Do not grow the existing egui god file merely to add the transport.
- Prefer provider-specific modules over more branching in one large catalog function.
- Keep blocking-network behavior off the egui frame loop.
- Preserve existing catalog cancellation/cache semantics.
- Use current compatible dependencies only when a concrete need exists; prefer existing `reqwest`/serde machinery.
- Default service target is local loopback `http://127.0.0.1:8181`.
- HTTP is the product boundary between LanternLeaf and Caliberate.

## Acceptance gates

1. Checked-in LanternLeaf config defaults to the Caliberate provider at `http://127.0.0.1:8181`.
2. Stale checked-in private-LAN target and default admin/admin credentials are removed.
3. Caliberate provider lists all pages from `/api/v1/books`.
4. Caliberate response rows map correctly into the existing browser model.
5. Allowed-format priority is deterministic.
6. Caliberate content materializes from `/api/v1/books/{id}/content/{format}`.
7. Materialized books open through the existing LanternLeaf source/session pipeline.
8. Caliberate does not require a library `#fragment`.
9. Optional API-key auth works without secret logging.
10. Legacy Calibre HTTP behavior remains available.
11. Caliberate catalog/cache entries cannot collide with a legacy provider pointed at the same textual base URL.
12. Caliberate provider does not waste time probing legacy cover endpoints.
13. Unreachable/malformed service failures are explicit.
14. Existing Windows workspace check/build/test remains green.
15. New provider tests are deterministic and green.
16. No broad UI or reader-state redesign is introduced.
17. `docs/work/reports/0008.md` records implementation, validation, and exact remaining human verification.

## Validation

Required:

- `cargo fmt --all -- --check`;
- `cargo check --workspace`;
- `cargo test --workspace`;
- `cargo build --workspace`;
- targeted Caliberate-provider tests;
- existing Windows CI baseline.

Where practical, run an integration probe against a locally spawned mock Caliberate server that serves at least two pages and a downloadable representative EPUB/TXT payload.

Do not claim connection to the human's real `127.0.0.1:8181` service from hosted CI.

## Repository handoff

Branch:

`codex/0008-caliberate-service-integration`

Report:

`docs/work/reports/0008.md`

Report sections:

- Result
- Provider Boundary
- Caliberate Catalog Mapping
- Pagination
- Content Materialization
- Authentication
- Legacy Calibre Compatibility
- Thumbnail Behavior
- Cache / Persistence
- Diagnostics
- Tests
- Windows CI
- Remaining Human Verification
- Recommended Next Goal
- Git

Use the standard post-push terminal notification protocol.

## Human verification

After director acceptance/integration, the intended real-machine verification is deliberately small:

1. keep the already-working Caliberate service running at `127.0.0.1:8181`;
2. pull accepted LanternLeaf `main`;
3. run `.\qa.ps1`;
4. confirm the library browser shows Caliberate books;
5. open one EPUB/TXT/HTML/Markdown/PDF book;
6. confirm it enters the normal reader;
7. exercise Windows TTS on that real Caliberate-sourced book.

No separate `scoop import`, manual file copying, payload download, or alternate QA distribution should be needed.

## Stop / escalation conditions

Stop and report a true blocker only if:

- the current library UI is inseparably coupled to the legacy Calibre response shape in a way that requires a director-level model redesign;
- preserving legacy Calibre compatibility would require broad architecture churn inconsistent with this goal;
- Caliberate content responses cannot be safely materialized through the current source/session pipeline;
- authentication requires a new secret-storage architecture decision rather than an optional config field;
- an unrelated baseline regression prevents required validation.

Do not block merely because hosted CI cannot reach the human's local `127.0.0.1:8181` service.


## Director correction continuation — attempt A2

Director review found two remaining acceptance defects. Read `docs/work/reviews/0008.md` before editing.

Attempt A2 is limited to:

1. making stale catalog fallback provider-safe so Caliberate can never receive a legacy Calibre catalog (and vice versa) through signature-mismatch fallback;
2. adding deterministic HTTP regression coverage for the retained legacy Calibre catalog + content download + Basic-auth path.

Preserve the accepted provider boundary and Caliberate implementation from attempt A1. Do not broaden scope.


## Director correction continuation — attempt A3: real-desktop EPUB open failure

### Current evidence

Real Windows QA against the human's actual Caliberate service invalidated acceptance gate 7.

Verified runtime facts:

- the Caliberate catalog populates in LanternLeaf;
- selecting a real EPUB still does not enter the reader;
- the post-acceptance timeout/progress hotfix and the large-catalog in-memory selection hotfix did not resolve the open;
- commit `caea5741a6b2fea2754082268169f166a1c61c8e` removed the 100k-book catalog reread and duplicate opens, but the real EPUB still fails after that change.

The previous automated evidence was insufficient. In particular, the Caliberate materialization regression named `fetches_all_pages_with_api_key_and_materializes_versioned_content` serves the literal byte string `representative book bytes` as an EPUB and asserts only that the bytes were downloaded. It never proves that a valid EPUB served through Caliberate can enter LanternLeaf's document/session pipeline.

Two additional concrete risk points are now part of the correction evidence:

1. Caliberate content-cache reuse currently trusts an already-existing materialized path without validating that an EPUB is readable.
2. EPUB ingestion currently performs `load_with_pandoc(path, "plain", ...)` before the native `EpubDoc` pretty-content pass. That launches an external Pandoc process with `Command::output()` and no process timeout, then parses the same EPUB again natively.

These are hypotheses/defects to resolve with deterministic evidence; do not assume either one alone is the entire root cause.

### Authorized correction passes

#### A. Real Caliberate -> EPUB -> reader regression

Replace the fake-byte blind spot with a deterministic test that serves a **valid minimal EPUB** from the mock Caliberate content endpoint.

The regression must exercise, at minimum:

`mock Caliberate HTTP -> content materialization -> normal LanternLeaf EPUB/source loader -> canonical reader/session content`

Use a project-owned deterministic EPUB fixture/helper. The test must assert semantic content from the EPUB (for example known chapter text) in canonical `tts_text` and structured/native reading content. Merely asserting downloaded bytes is not sufficient.

Where dependency layering makes the full egui effect inappropriate for a unit test, the boundary may stop at the canonical core session/source loader, but it must cross both the Caliberate HTTP materialization boundary and the real EPUB ingestion boundary.

#### B. Remove the unbounded EPUB subprocess dependency from the critical open path

EPUB already has a Rust-native `EpubDoc` parser and native chapter HTML path. Do not require an unbounded external Pandoc child process merely to obtain EPUB TTS text before that native parse.

Prefer one native EPUB ingestion pass that derives:

- chapter/native reading HTML;
- canonical plain TTS text from the same chapter content using existing Rust-native text extraction machinery such as `html2text`;
- the existing source/session structures.

Pandoc may remain for source families that genuinely need it, but a real EPUB open must not be able to hang forever in an external `pandoc` process.

Preserve canonical sentence/session semantics; do not invent a Caliberate-specific or EPUB-specific reader state machine.

#### C. Validate/recover materialized EPUB cache entries

A pre-existing Caliberate `.epub` materialization must not be trusted solely because the path exists.

Add bounded validation sufficient to detect a zero-length, truncated, non-ZIP/non-EPUB, or otherwise unreadable cached EPUB before handing it to the reader. If a cached Caliberate EPUB is invalid:

1. discard only that poisoned materialization;
2. download it again once from the normal content endpoint;
3. validate the replacement;
4. return an actionable error if the replacement is still invalid.

Do not wipe unrelated catalog or reader caches.

#### D. Stage-level diagnostics

Keep the existing `materializing` / `loading_reader` progress, and add enough structured tracing around the real EPUB path to distinguish:

- content cache hit vs network download;
- downloaded byte count;
- EPUB validation start/result;
- native EPUB parse/text extraction start/result;
- session load completion/failure.

Failures must terminate the source-open operation and surface an actionable visible error. A failed worker must not leave the UI permanently believing a source open is active.

#### E. Preserve accepted behavior

Do not regress:

- the in-memory selected-book handoff from `caea574...`;
- duplicate-open suppression;
- 10-minute Caliberate content timeout;
- provider-safe catalog caches;
- Caliberate API-key behavior;
- legacy Calibre catalog/download/Basic-auth behavior;
- loopback default `127.0.0.1:8181`;
- repo-native Windows QA workflow.

### Correction acceptance gates

Attempt A3 is not complete until all of the following are true:

1. A deterministic mock Caliberate server serves a syntactically valid EPUB, not placeholder bytes.
2. The materialized result is accepted by LanternLeaf's real EPUB loader/session path.
3. The resulting canonical TTS text contains known fixture chapter text.
4. Structured/native EPUB reading content is present and contains known fixture chapter text.
5. EPUB opening no longer depends on an unbounded external Pandoc process.
6. An invalid pre-existing Caliberate EPUB cache entry is rejected and recovered by one clean re-download, with a deterministic regression.
7. A still-invalid replacement fails explicitly and leaves source-open state recoverable.
8. Existing Caliberate catalog/auth/cache tests remain green.
9. Existing legacy Calibre HTTP compatibility regression remains green.
10. `cargo check --workspace`, `cargo build --workspace`, and `cargo test --workspace` pass on Windows CI.
11. The report records exact tests and the corrected end-to-end EPUB path.
12. No PDF work, cover-polish work, broad library UI redesign, or reader/TTS state redesign is introduced.

### Human verification for attempt A3

**No human QA during implementation/correction.**

The human has already supplied the failing real-desktop evidence. Do not ask them to repeat the same EPUB click before the automated blind spot above is repaired and the director has reviewed/integrated A3.

After director acceptance only, request one repo-native `git pull -> .\qa.ps1` run against the already-running Caliberate service and one real EPUB open before proceeding to Windows TTS.


## Director correction continuation — attempt A4: real-desktop reader startup / normalization stall

### Why A4 exists

Real Windows QA after accepted A3 proves that Caliberate transport and native EPUB ingestion are no longer the blocking stage, but the book still does not become usable because LanternLeaf performs pathological whole-book TTS normalization synchronously before the reader is allowed to finish opening.

The observed real-desktop sequence for Caliberate book id `35656` is:

```text
Caliberate materialization
  -> ~154 ms
native EPUB parse + TTS/HTML extraction
  -> ~158 ms
complete source load including image extraction
  -> ~325 ms
EPUB pretty repagination
  -> one logical page
sentence-anchor map
  -> 3,178 sentences
synchronous normalization precompute
  -> stalls for minutes
reader never becomes interactively available during the observed run
```

A second already-materialized Recent EPUB reproduces the same defect without requiring a Caliberate download:

- native EPUB source load completes in under one second;
- pretty EPUB mode contains 10,488 sentences in one logical page;
- the session again enters `Precomputing normalization cache for loaded book`;
- the normalizer repeatedly reapplies the same abbreviation machinery sentence by sentence.

This is therefore a canonical reader/session startup defect, not a Caliberate-storage latency defect.

### Root-cause evidence in current code

Current session architecture compounds several individually reasonable historical decisions into a pathological open path:

1. EPUB pretty mode deliberately represents the complete EPUB as one logical reader page so HTML and canonical text remain in one coordinate space.
2. `load_session_for_source_with_cancel` synchronously calls `precompute_normalization_cache` before returning the session.
3. The precompute parallelizes by page. A one-page EPUB therefore collapses `normalizer_threads = 8` to one worker.
4. Sentence-mode normalization walks every sentence in that one page.
5. `apply_abbreviation_map` recompiles configuration-driven regexes repeatedly for each sentence. The observed configuration contains 5 regex rules, 154 case-sensitive abbreviation rules, and 8 case-insensitive rules.
6. Initial reader snapshot/TTS metadata paths can also request the current full-page audio plan, so simply deleting the explicit precompute is not sufficient if the first snapshot immediately rebuilds the same thousands-of-sentence plan.
7. Sentence-mode cache misses currently serialize individual per-sentence normalization cache files, multiplying startup IO and serialization work.

Historical intent must be preserved: the old eager precompute existed to avoid later page-turn/TTS stalls. A4 must replace that behavior with bounded/lazy/background work rather than merely moving the same multi-minute stall to the first Play action.

### Authorized correction passes

#### A. Remove whole-book normalization from the synchronous source-open critical path

A normal EPUB source open must be able to return a reader-ready session after source ingestion, pagination/anchor setup, and other bounded reader initialization.

`load_session_for_source_with_cancel` must not synchronously normalize every sentence merely to construct the session.

The existing historical `precompute_normalization_cache` behavior may be:

- removed from synchronous open;
- converted to a bounded background warmup;
- replaced with demand-driven normalization;
- or otherwise restructured.

Do not make reader visibility depend on completion of whole-book TTS preparation.

#### B. Initial snapshot must remain bounded while TTS is idle

The first `ReaderSnapshot` must not immediately recreate the same full-book stall through `tts_view`, `current_audio_sentences`, `current_audio_highlight_idx`, or equivalent helpers.

While TTS is idle, the UI may derive cheap metadata from canonical/display sentence counts, cached-plan metadata, or an explicit not-yet-prepared state, provided the existing public contract remains coherent.

Do not introduce a parallel reader state machine.

#### C. First TTS interaction must also be bounded

Do not merely move the multi-minute stall from book open to the first TTS Play/Seek.

On initial TTS use, prepare only what is needed for prompt playback plus bounded lookahead, or provide an equivalently fast bulk-normalization implementation.

Required behavior:

- Play becomes responsive without full-book preprocessing;
- current sentence can be normalized and spoken promptly;
- next/previous sentence navigation remains correct;
- canonical display <-> audio ownership remains correct when one display sentence expands into multiple audio chunks;
- background warming may continue after playback starts, but must not block the UI.

#### D. Compile stable normalization matchers once per TextNormalizer configuration

Configuration-driven regular expressions and token matchers must not be recompiled for every sentence.

At minimum review and eliminate per-sentence stable-regex compilation for:

- configured abbreviation regex rules;
- case-sensitive abbreviation token patterns;
- case-insensitive abbreviation token patterns;
- brand pronunciation map patterns;
- custom-pronunciation map patterns;
- acronym token patterns;
- year matching.

Prefer a compiled normalizer/runtime representation built when `TextNormalizer` is created or loaded. Invalid user-config rules must still produce useful warnings and be skipped safely.

Preserve normalization semantics unless a test demonstrates an existing bug.

#### E. Eliminate thousands of tiny synchronous cache writes from reader open

Opening a large EPUB must not synchronously write one TOML file per normalized sentence before the reader appears.

Caching may remain sentence-addressable, but population must be lazy/batched/background, or the representation may be redesigned behind the existing cache boundary.

Cache correctness and normalizer-config invalidation semantics must remain deterministic.

#### F. Large-document deterministic regression

Add a project-owned deterministic regression that exercises a realistically large EPUB/session shape.

Required fixture/evidence:

- at least one generated/owned EPUB or canonical session with **3,000+ sentences**;
- preferably also a 10,000-sentence stress case if it remains practical for ordinary CI;
- do not commit a giant binary when a tiny generated fixture can express the same shape.

The regression must prove structurally, not only by optimistic wall-clock timing, that:

1. source/session open does not eagerly normalize every sentence;
2. initial reader snapshot while TTS is idle does not eagerly normalize every sentence;
3. opening does not create thousands of sentence-normalization cache files;
4. first TTS use prepares bounded work rather than the complete book;
5. canonical normalized sentence content remains correct;
6. display-to-audio and audio-to-display mappings remain correct for chunked sentences.

Instrumentation/counters exposed only under tests are acceptable if they keep the assertion deterministic.

A generous performance smoke assertion may supplement these structural checks, but must not be the sole acceptance evidence.

#### G. Stage-level timing and progress diagnostics

Add enough tracing to distinguish at least:

- source content load duration;
- EPUB repagination duration;
- sentence/anchor construction duration;
- synchronous normalization work performed during open;
- initial snapshot duration;
- TTS-plan preparation count and duration;
- optional background warmup count/duration.

A normal successful open must transition out of `loading_reader` after bounded non-TTS initialization.

#### H. Preserve all accepted Goal 0008 behavior

A4 must not regress:

- Caliberate paged catalog loading;
- in-memory selected-book handoff;
- duplicate-open suppression;
- the 10-minute slow-storage-safe content timeout;
- valid EPUB cache validation/recovery from A3;
- native EPUB ingestion from A3;
- provider-safe catalog caches;
- Caliberate API-key auth;
- legacy Calibre catalog/download/Basic-auth compatibility;
- local default `127.0.0.1:8181`;
- repo-native Windows QA;
- source-open failure cleanup.

The black/blank Caliberate cover issue remains explicitly out of scope for A4.

Do not start PDF work.

### Attempt A4 acceptance gates

A4 is not complete until all of the following are true:

1. `load_session_for_source_with_cancel` no longer requires all-sentence/all-page normalization cache precompute before returning a normal reader session.
2. Initial reader snapshot while TTS is idle does not force normalization of the complete single-page EPUB.
3. First TTS Play/Seek does not synchronously normalize the complete 3,000+/10,000-sentence EPUB before becoming usable.
4. Stable configuration-driven regex/token matchers are compiled once per loaded normalizer configuration rather than once per sentence.
5. Large-book tests prove bounded normalization/cache work before reader visibility.
6. Large-book tests prove bounded first-TTS preparation.
7. Existing normalization semantics and display/audio mapping regressions remain green.
8. Existing A3 valid Caliberate EPUB -> materialization -> native source loader -> canonical session regression remains green.
9. Poisoned Caliberate EPUB cache recovery remains green.
10. Legacy Calibre HTTP compatibility remains green.
11. `cargo check --workspace`, `cargo build --workspace`, and `cargo test --workspace` pass on Windows CI.
12. The Goal 0008 report records exact A4 architecture, tests, before/after startup stages, and remaining human verification.
13. No PDF work, cover work, broad library UI redesign, or duplicate reader state machine is introduced.

### Human verification for attempt A4

**No human QA during A4 implementation/correction.**

The human has already supplied sufficient real-desktop evidence. Do not ask them to reopen the same EPUB until A4 is implemented, validated, director-reviewed, and integrated.

After director acceptance only:

1. keep Caliberate running at `127.0.0.1:8181`;
2. `git pull -> .\qa.ps1`;
3. open one real Caliberate EPUB;
4. confirm the reader appears promptly;
5. immediately test Windows TTS Play/next/previous/pause/resume/stop so A4 cannot hide the same stall behind first TTS activation.


## Correction attempt A5 — native egui frame budget and TTS main-thread crash

### Triggering real-desktop evidence

A4 fixed the previous reader-open stall. The accepted A4 build was pulled and exercised on the human Windows machine against a real already-materialized Caliberate EPUB.

The source-open path is now acceptable:

- native EPUB parse/text extraction completed in about 704 ms;
- bounded repagination/sentence-anchor setup completed in about 874 ms;
- Reader mode became visible roughly 1–2 seconds after the open began.

The opened EPUB is deliberately large enough to expose real reader scaling:

- reading HTML: 1,342,983 chars;
- canonical TTS text: 1,291,569 chars;
- canonical sentences: 10,488;
- native pretty parse: 1,644 blocks / 4,726 spans in 48 ms.

The human then observed catastrophic steady-state native UI latency: button hover feedback itself took roughly 2–3 seconds. This is a hard failure of the native egui performance objective, not polish.

Pressing TTS Play then terminated LanternLeaf:

```text
thread 'main' (...) has overflowed its stack
LanternLeaf exited with code -1073740771
```

The crash occurred after the reader command/persistence snapshots and before the existing `Starting TTS runtime playback job` diagnostic appeared. A4 therefore fails real-desktop signoff.

### Director localization

The current architecture contains multiple deterministic sources of pathological native-frame work:

1. `AppRuntime::state_snapshot()` deep-clones the complete `AppState` on every egui `update()`.
2. `AppState::starter.calibre_books` contains the full real catalog (104,732 DTOs), so the reader pays to clone the library even when the library is not being rendered.
3. `ReaderSnapshot` carries heavyweight document payloads, including the complete ~1.34 MB HTML stream and all canonical sentences; these are cloned into app/runtime/event snapshots.
4. `resolve_highlight_color` calls `runtime.state_snapshot()` again from a leaf reader-rendering function, causing another heavyweight state clone during the same frame.
5. The HTML pretty cache avoids reparsing after the first frame, but `render_pretty_page` still walks every cached block and rebuilds egui `LayoutJob`/widget state for all 1,644 blocks on every frame. `ScrollArea::show` does not virtualize this work.
6. Native QA currently launches the unoptimized dev profile, which magnifies immediate-mode layout cost. That does not excuse the architectural work above, but the real-desktop QA binary should also use a representative optimized profile.
7. A TTS UI command is applied through two paths: the normal reader effect/session and a second cloned `TtsRuntime` session. The egui button path then calls `tts_runtime.apply_command(...)` synchronously on the main/UI thread. Normalization-plan/cache/snapshot work therefore occurs before the runtime spawns its playback worker. The observed stack overflow happened on this main-thread control path.

A5 must repair these ownership and bounded-work violations. Do not hide them behind a larger stack size, a release-only build, or a Caliberate-specific special case.

### A5-A — eliminate heavyweight per-frame state cloning

Make egui frame-state access scale with the visible UI, not the total catalog/document size.

Requirements:

- do not deep-clone the 104k-book catalog on every frame;
- do not deep-clone megabyte reader HTML or the full canonical sentence corpus on every frame;
- remove `runtime.state_snapshot()` calls from leaf rendering helpers such as highlight-color resolution;
- introduce an explicit lightweight UI projection and/or shared immutable `Arc`-backed heavyweight payloads;
- preserve thread-safe event application without holding a global state mutex across egui rendering or command execution;
- catalog and reader payload ownership must remain explicit and testable.

Deterministic regression evidence must construct a large catalog/document state and prove that ordinary reader-frame projection shares or omits heavyweight payloads rather than cloning them.

### A5-B — virtualize/bound native pretty rendering

A native reader may not submit the complete EPUB to egui every frame.

Requirements:

- parse HTML/Markdown into pretty blocks once per content/config identity;
- render only the visible block window plus bounded overscan, or implement an equivalent bounded pagination/window contract;
- scrolling must remain visually continuous and usable;
- highlighted/spoken sentence navigation and auto-scroll must be able to bring an off-screen block into the active window;
- no ordinary frame may iterate/build layout jobs for all 1,644+ blocks merely because the EPUB is one logical HTML stream;
- preserve headings, paragraphs, lists, block quotes, code, tables, images, and existing highlight semantics for blocks that enter the active render window;
- do not fall back to a WebView/Tauri renderer.

Add a deterministic large-pretty-document regression proving that the active render set stays bounded when the document contains thousands of blocks. Structural bounds are preferred over flaky CI wall-clock thresholds.

### A5-C — remove heavyweight full-document payloads from high-frequency snapshots/events

Split document identity/content from playback/UI deltas.

Requirements:

- TTS progress/state events must not clone the full reading HTML or complete canonical sentence corpus;
- persistence flushes must use direct bookmark/config/housekeeping data rather than manufacturing a full UI `ReaderSnapshot` just to persist progress;
- ordinary reader commands should update lightweight state where possible;
- heavyweight immutable document content may be shared by identity/handle/`Arc` rather than copied into every event;
- preserve public behavior and deterministic state ownership.

### A5-D — TTS control must never do planning/synthesis work on the egui main thread

The Play button must be a bounded control submission.

Requirements:

- egui command handling queues/sends the TTS command and returns without normalization, cache IO, snapshot construction, engine creation, or synthesis on the UI thread;
- the TTS worker/runtime owns bounded first-window preparation and playback startup;
- eliminate or formally collapse the current double-application of TTS commands across the effect-session copy and `TtsRuntime` session copy so there is one authoritative cursor/state transition contract;
- reader UI state must receive lightweight playback deltas/events from that authority;
- Play, next, previous, pause/resume, repeat, backend/voice changes, and stop must retain existing cursor semantics;
- do not “fix” the crash by increasing the process/main-thread stack size.

Add a large-session regression (10k+ display sentences) that exercises the native TTS command submission/control path and proves command submission is bounded/nonblocking and does not recurse/stack-overflow before the playback worker begins.

### A5-E — representative native QA performance profile

Keep tests/checks truthful, but launch the human desktop QA binary with a representative optimized Cargo profile rather than the fully unoptimized dev binary.

Requirements:

- define a repo-owned QA profile (or equivalent) with optimization suitable for interactive egui performance while retaining useful diagnostics;
- `qa.ps1` builds/launches that profile automatically;
- no new manual command or payload ceremony for the human;
- this profile change is supplemental: A5-A/B/C/D must still repair the pathological work structurally.

### A5-F — diagnostics and acceptance

Add bounded diagnostics that make future native-frame failures localizable without producing multi-megabyte noise.

At minimum record/aggregate:

- egui frame/update duration or slow-frame events;
- state-projection duration and whether heavyweight payloads were shared;
- pretty render window: total blocks, active blocks, overscan, render duration;
- TTS control submission duration and worker-start latency.

Avoid per-block/per-frame spam at normal debug/info levels.

A5 automated acceptance requires:

1. normal workspace check/build/test remains green;
2. Windows TTS probes remain green;
3. large catalog UI projection is structurally non-copying/bounded;
4. large pretty document active rendering is bounded;
5. 10k+ sentence TTS command submission is off-main/bounded;
6. existing A3/A4 Caliberate materialization/open/lazy-normalization regressions remain green;
7. no new Caliberate-specific reader or TTS state machine.

### Human QA contract for attempt A5

**No human QA during A5 implementation/correction.**

After director acceptance only:

1. keep Caliberate running at `127.0.0.1:8181`;
2. `git pull -> .\qa.ps1`;
3. open the same large Recent/Caliberate EPUB;
4. verify pointer hover/button feedback is effectively immediate rather than multi-second;
5. scroll through the pretty EPUB and verify no pathological stalls;
6. press Windows TTS Play and verify the process stays alive and speech starts;
7. verify next/previous/pause/resume/stop.

Gate 2.5 remains blocked until this real-desktop signoff succeeds.


## Director correction continuation — attempt A5.1: canonical session authority and truly lightweight TTS/persistence

### Director decision on first A5 implementation

**REJECTED. Do not integrate worker terminal head `6378ffc07e7b8b6da77dce59157f43d25627c45b` as-is.**

The A5 implementation at `ca91d6ce6adc557fbd62e7cccd66d22234b4bc3d` made real progress on native frame cost:

- large catalog and reader document state are Arc-backed in AppState;
- ordinary egui state projection no longer deep-copies the 104,732-book catalog;
- native pretty rendering uses a bounded viewport window instead of laying out all 1,644 blocks;
- egui-facing TTS submission goes through a dedicated control worker;
- repo-native QA uses an optimized `profile.qa`.

However, the implementation fails A5-C/A5-D's ownership and heavyweight-work requirements.

### Rejection evidence

#### 1. Persistence still constructs a full ReaderSnapshot

`crates/lanternleaf-egui/src/effects.rs::handle_persistence_flush` still does:

```text
lock canonical effect session
-> session.snapshot(panels, normalizer)
-> clone config
-> ReaderHousekeeping { snapshot, config }
```

`ReaderSession::snapshot` clones/collects heavyweight document data, including:

- complete current-page/plain TTS text;
- complete native reading HTML for one-stream EPUB;
- canonical sentence corpus;
- sentence-anchor map;
- page sentence counts;
- reader images and other UI payload.

Meanwhile `execute_command` still calls `apply_persistence_trigger` for every `AppCommand::Reader(_)`, including TTS Play/Next/Prev/Pause/Stop.

Therefore A5 did not satisfy the explicit requirement:

> persistence flushes must use direct bookmark/config/housekeeping data rather than manufacturing a full UI ReaderSnapshot just to persist progress.

#### 2. TTS worker events are lightweight only after heavyweight snapshots are built

`TtsRuntime::apply_command` still calls:

```text
reader.apply_command(...)
-> SessionEvent { snapshot: reader.snapshot(...) }
-> derive ReaderPlaybackState / TtsState from that full snapshot
-> emit event with snapshot = None
```

`build_tts_playback_plan`, `collect_tts_playback_plan`, pause transitions, and runtime step transitions also call full `reader.snapshot(...)`.

Thus the event channel no longer carries the megabyte payload, but the TTS worker still repeatedly constructs it internally. The 10,488-sentence A5 test only proves:

- `submit_command` returns within 50 ms;
- the worker eventually emits a state event;
- emitted events have `snapshot.is_none()`.

It does **not** structurally prove that no heavyweight `ReaderSnapshot` was constructed inside the worker.

#### 3. There are still two ReaderSession authorities

The egui app still maintains:

- `effect_session: Arc<Mutex<Option<ReaderSession>>>` used by normal reader effects/persistence;
- a separate `TtsRuntime.session: Arc<Mutex<Option<ReaderSession>>>`.

`sync_tts_runtime_session` clones the effect session only when the source path changes.

A5 then deliberately skips the effect-session command path for TTS commands and applies those commands only to the TTS-runtime copy.

This means:

- a non-TTS sentence click/search/navigation change can mutate the effect session without updating the TTS session;
- subsequent Play/Play-from-highlight can begin from stale cursor state;
- TTS progression mutates the TTS copy while command-triggered persistence may snapshot the stale effect copy;
- there is no single canonical ReaderSession cursor/state owner.

That violates A5-D's explicit requirement to collapse the double-application model into one authoritative cursor/state transition contract.

### Required correction architecture

A5.1 must finish A5 rather than layering another synchronization shim.

#### A. One canonical ReaderSession object

Normal reader commands, TTS commands, persistence, and playback planning must operate on the same authoritative `ReaderSession` instance/handle.

Preferred shape:

```text
Arc<Mutex<Option<ReaderSession>>>
        ^
        |
  one canonical owner
   /      |       \
reader   TTS    persistence
effects worker   projection
```

The TTS runtime may hold a clone of the **Arc handle**, not a clone of the `ReaderSession` value.

Do not add version counters or periodic whole-session copy reconciliation unless unavoidable and director-reviewed.

Acceptance regression must prove:

1. sentence click/search/highlight on normal reader command path;
2. immediately queue TTS Play/Play-from-highlight;
3. worker starts from that exact canonical cursor;
4. TTS seek updates are visible to subsequent normal reader operations/persistence from the same session.

#### B. Add lightweight session projections

Introduce explicit ReaderSession APIs for the data needed by hot paths, for example:

- `playback_view()/playback_state()`;
- `tts_state_view()`;
- `tts_plan_input()/current_tts_slice()`;
- `persistence_housekeeping()/bookmark + config`;
- lightweight cursor/page/source metadata.

Names are implementation detail; the contract is not.

These APIs must not call `ReaderSession::snapshot()` internally.

A full `ReaderSnapshot` remains appropriate for source-open/full-document UI publication and explicit low-frequency document refreshes, but not for TTS stepping, plan collection, or persistence.

#### C. Remove full snapshots from TTS worker hot path

After A5.1:

- `TtsRuntime::apply_command` must not require a full ReaderSnapshot merely to emit playback/TTS deltas;
- `build_tts_playback_plan` and `collect_tts_playback_plan` must not call full `reader.snapshot`;
- runtime seek/progress/pause transitions must emit lightweight playback/cursor/TTS state directly;
- high-frequency TTS progress must not clone reading HTML, canonical sentence corpus, images, or sentence-anchor maps.

A deterministic test-only heavy-snapshot counter/hook is acceptable.

Required 10,000+ sentence regression must assert **zero full ReaderSnapshot constructions** for:

1. TTS command submission;
2. worker command application;
3. first plan build;
4. at least 100 TTS progress/seek transitions.

#### D. Persistence must not use ReaderSnapshot

Refactor `ReaderHousekeeping` / persistence boundary so a flush receives only the fields it actually persists.

At minimum derive directly from canonical session:

- bookmark/cursor;
- source path;
- config/settings fields required by persistence;
- any existing small persistence metadata.

Do not manufacture a UI snapshot.

Required regression:

- large EPUB-shaped session with 10,000+ sentences / large reading HTML;
- ReaderCommand/TTS-triggered persistence flush;
- zero full ReaderSnapshot constructions;
- correct persisted bookmark/cursor.

Also review whether every TTS progress/control action needs a generic `PersistenceTrigger::ReaderCommand` flush now that the TTS runtime already persists canonical progress. Remove redundant/stale-race-producing flushes if they are no longer semantically required.

#### E. Preserve the good A5 work

A5.1 must preserve and revalidate:

- Arc-backed large catalog state;
- Arc-backed reader document publication;
- no leaf `runtime.state_snapshot()` heavyweight re-clone;
- bounded pretty viewport rendering;
- sentence-to-block highlight index;
- off-main egui TTS command submission;
- optimized `profile.qa`;
- A3 native EPUB ingestion/cache recovery;
- A4 bounded lazy normalization;
- Windows TTS backend behavior.

#### F. Correct the A5 regression suite

The prior large-session TTS test is insufficient by itself.

Add tests that fail if:

- a full ReaderSnapshot is constructed in TTS worker hot paths;
- persistence constructs a full ReaderSnapshot;
- normal reader cursor and TTS worker cursor diverge;
- TTS Play-from-highlight ignores a cursor change made through the normal reader command path.

Keep the existing structural frame/pretty-window tests.

### A5.1 acceptance gates

1. One canonical ReaderSession value is shared by normal reader effects and TTS worker.
2. TTS runtime does not own a cloned ReaderSession value.
3. TTS worker Play/plan/progress path constructs zero full ReaderSnapshots in the 10k+ regression.
4. Persistence flush constructs zero full ReaderSnapshots.
5. Cursor changes from normal reader commands are immediately authoritative for TTS Play/Play-from-highlight.
6. TTS cursor changes are immediately authoritative for persistence and normal reader state.
7. A5 large-catalog Arc-sharing regression remains green.
8. A5 bounded 1,644-block pretty-window regression remains green.
9. A5 off-main command-submission regression remains green.
10. A3/A4 EPUB/materialization/lazy-normalization regressions remain green.
11. Windows normal workspace check/build/test and Windows TTS probe pass.
12. `qa.ps1` still defaults to the optimized QA profile.
13. No main-thread stack-size increase, WebView fallback, Caliberate-specific reader state, or broad unrelated redesign.

### Human QA

**No human QA during A5.1.**

After director acceptance, repeat the same real-desktop large EPUB test. No new manual ceremony.


## Director correction continuation — attempt A6: Windows QA must actually use Windows TTS

### Triggering real-desktop evidence

A5.1 real-desktop verification produced two distinct results:

1. **Native reader responsiveness passed.** The same large EPUB that previously had multi-second hover latency is now described by the human as “waaaaay less lag” and “pretty snappy.”
2. **Windows TTS signoff failed because Windows TTS was never selected by the QA configuration.**

The runtime log is explicit:

```text
Starting TTS runtime playback job ...
Initializing TTS engine backend=Piper
model=/home/admin/Music/models/piper/en-US/female/en_US-amy-medium.onnx
...
tts-worker error: Piper config not found at
/home/admin/Music/models/piper/en-US/female/en_US-amy-medium.onnx.json
...
Failed to prepare TTS audio batch: Worker process closed its stdout
Prepared TTS batch was empty
```

This is not a synthesis-layer Windows failure. The Windows backend was bypassed.

### Director localization

The configuration path deterministically explains the failure:

- `conf/config.toml` omits `tts_backend`;
- `TtsBackend::default()` is `Piper`;
- `TtsConfig::default()` is `Piper`;
- `AppConfig::default()` is `Piper`;
- `qa.ps1` copies `conf/config.toml` into `.qa/windows/conf/config.toml` only when that destination does not already exist;
- therefore the Windows QA environment defaults to Piper and preserves that choice across runs;
- the same config also carries a Linux-specific Piper model/eSpeak path, so the resulting failure is guaranteed on a clean Windows QA machine unless the human manually changes the backend.

This violates the explicit QA contract that Windows TTS is the critical audio path.

### A6-A — platform-aware backend default

Make omitted-backend configuration platform-correct:

- on Windows, an omitted `tts_backend` must resolve to `Windows`;
- on non-Windows platforms, omitted `tts_backend` may continue to resolve to `Piper`;
- an explicit `tts_backend = "piper"` on Windows must still be respected;
- an explicit `tts_backend = "windows"` on non-Windows must continue to fail explicitly rather than silently substituting another backend.

Prefer a single `default_tts_backend()` function used consistently by serde defaults, `AppConfig::default`, and table defaults rather than divergent hard-coded defaults.

Add deterministic cfg-aware tests for omitted and explicit backend behavior.

### A6-B — Windows QA must force/verify the intended backend every run

`qa.ps1` must not rely on stale `.qa/windows/conf/config.toml` contents to decide the critical audio path.

Requirements:

- repo-native Windows QA must guarantee the effective backend is Windows on every QA run unless an explicit QA override flag is intentionally supplied;
- this must work even when `.qa/windows` already exists from earlier runs;
- do not require the human to manually edit TOML or click a backend selector before the signoff test;
- prefer a repo-owned Windows-QA config overlay or a small structured config preparation step rather than fragile text substitution;
- print the effective QA TTS backend before launching LanternLeaf;
- where possible resolve/log the effective Windows voice ID used for QA.

`.\qa.ps1` remains the only normal human command.

### A6-C — remove machine-specific backend assumptions from portable defaults

Do not bake a particular developer machine into portable config defaults.

At minimum review:

- the Windows default Piper model path that currently names a specific user profile;
- the repository `conf/config.toml` Linux-specific Piper path.

The correction does not need to provision Piper models on Windows. Piper may remain opt-in and may require a configured model. But a default/QA Windows TTS run must not depend on a developer-specific Piper path.

Preserve explicit user-supplied Piper paths.

### A6-D — TTS failure must be visibly actionable

The current log correctly emits `CommandFailed { scope=reader_tts, code=tts_runtime_failed }`, but the human experienced only silence.

Verify the native egui shell surfaces a persistent/clear visible TTS error when synthesis startup fails.

For backend/config failure, the UI message should include enough context to distinguish at least:

- selected backend;
- missing Piper model/config;
- unavailable/missing Windows voice;
- audio output failure.

Do not dump stack traces or giant logs into the UI.

Add a deterministic egui/app regression that injects a TTS runtime failure and proves a visible error notification/state is produced.

### A6-E — Windows TTS signoff regression

Add/extend Windows CI coverage so the same repo-native QA configuration path used by the human proves the effective backend is Windows.

Required automated evidence:

1. prepare Windows QA state through the repo-native script;
2. load the staged QA config through the real config parser;
3. assert effective `tts_backend == Windows`;
4. resolve an installed Windows voice;
5. synthesize a deterministic sentence to WAV;
6. decode the WAV through the shared Rodio path as existing probes do.

Do not claim physical speaker output from CI.

### A6-F — diagnostics cleanup

The real log is flooded by html5ever DEBUG lines, making the useful TTS failure harder to see.

Without reducing useful project diagnostics:

- suppress dependency-internal html5ever tree-builder debug spam at the normal QA debug level, or otherwise configure dependency filters so LanternLeaf debug logs remain useful;
- ensure `qa.ps1` handoff error extraction includes `tts`, `piper`, `voice`, `audio`, and `synth` terms.

### A6 acceptance gates

1. Omitted TTS backend resolves to Windows on Windows and Piper elsewhere.
2. Explicit Piper on Windows remains supported.
3. `qa.ps1` deterministically stages/verifies Windows backend on every normal Windows QA run, including pre-existing QA state.
4. No manual backend-selection ritual is required for the human signoff.
5. Windows QA config no longer depends on a Linux-only or developer-profile Piper path for its default critical audio path.
6. Backend/config TTS failures surface a visible actionable native UI error.
7. Windows CI proves staged QA config -> Windows backend -> installed voice -> WAV synthesis -> shared decode.
8. A3/A4/A5/A5.1 EPUB, lazy-normalization, bounded-render, canonical-session, and snapshot-free regressions remain green.
9. Normal workspace check/build/test and Windows TTS probes pass.
10. No PDF work, no Caliberate-specific TTS state machine, and no manual payload/dependency ceremony.

### Human QA

**No human QA during A6 implementation.**

After director acceptance:

```powershell
git pull --ff-only
.\qa.ps1
```

Then:

1. open the same large Caliberate/Recent EPUB;
2. press Play without manually selecting a backend;
3. confirm actual Windows speech is audible;
4. verify next/previous/pause/resume/stop.

If audio still fails, the visible error plus focused handoff diagnostics become the next evidence.


## Director correction continuation — attempt A7: deterministic pretty-sentence highlight and scroll synchronization

### Triggering real-desktop evidence

A6 real-desktop verification closes the Windows synthesis uncertainty:

- normal `qa.ps1` reports `Effective QA TTS backend: Windows (repo-native default)`;
- the human hears Windows speech;
- installed Windows voices can be selected and the audible voice changes;
- the same large Caliberate EPUB now opens quickly and the native egui shell is described as fast/snappy.

Goal 0008 nevertheless remains blocked because the pretty reader's spoken-sentence highlighting and viewport following are severely incorrect during real TTS playback.

Human observation:

- the highlight sometimes appears on the actually spoken sentence;
- other transitions jump to a wildly inaccurate/unrelated location;
- while speech continues the viewport can alternate between the correct highlight and another location;
- sometimes the window fails to update to the active sentence at all.

This is a synchronization/canonical-identity failure, not a TTS synthesis failure and not a general egui frame-performance failure.

The real runtime log supports that distinction. The canonical TTS cursor itself progresses coherently: the accepted large book prepares a bounded plan window for display sentences 8..72, begins with `highlighted_audio_idx=9 highlighted_display_idx=16`, and later advances to `highlighted_audio_idx=10 highlighted_display_idx=17` while Windows TTS continues normally.

### Director localization

The current pretty synchronization path has multiple structurally unsafe identity/fallback mechanisms.

#### 1. Pretty sentence identity is text-keyed, not canonical-index keyed

`refresh_pretty_cache` builds:

```rust
HashMap<String, usize> // normalized sentence text -> block index
```

by splitting each pretty block into sentences and inserting the first block seen for each normalized string.

That is not a sentence identity.

It fails when:

- the same sentence text appears multiple times;
- headings, captions, quotations, repeated UI-like prose, chapter titles, or boilerplate repeat;
- HTML-to-text and canonical EPUB extraction differ in whitespace/punctuation/entity normalization;
- a canonical sentence spans rich-text boundaries differently from the pretty parser.

A repeated string can only point to one block even when multiple distinct canonical sentence occurrences exist.

#### 2. HTML fallback is explicitly proportional rather than semantic

When direct text lookup misses, `render_pretty_page` falls back to `sentence_anchor_map`.

For native HTML, `ReaderSession::build_sentence_anchor_map_for_page` creates that map with `proportional_html_anchor_map(...)`: canonical sentence position is projected proportionally across HTML anchors.

That may be useful as weak coarse navigation metadata, but it must not be treated as authoritative spoken-sentence identity. A missed exact-text lookup can therefore send TTS highlighting to a structurally unrelated anchor.

#### 3. Auto-scroll is a one-shot boolean consumed before successful resolution

`TtsRuntimeEvent` handling calls `auto_scroll_state.note_auto_scroll()` whenever an event contains a current TTS audio sentence.

`render_pretty_page` immediately consumes that boolean before it knows whether:

- the canonical display sentence mapped to a pretty target;
- the target block is in the virtual render window;
- a concrete response/rect exists to scroll to;
- `scroll_to_me` was actually issued.

An unresolved/off-screen transition can therefore lose its only follow request.

#### 4. Scroll triggering uses TTS event presence instead of canonical display-cursor transition

The runtime already publishes the authoritative canonical display highlight through `ReaderPlaybackState.highlighted_sentence_idx`. Pretty auto-scroll should follow changes in that canonical display index.

Using `tts.current_sentence_idx.is_some()` as the trigger mixes the bounded-window audio index with UI-follow intent and can retrigger on queued/state events that did not change the visible canonical sentence.

#### 5. Virtual layout still uses one global estimated block height

A5 correctly bounded the number of rendered blocks, but the virtual stream currently represents all off-screen content using:

```text
block_index * one estimated_block_height
```

even though headings, paragraphs, lists, quotes, code, tables, and images have very different measured heights.

This can make viewport-y -> block-index projection drift as rendered windows change. A7 must preserve bounded rendering while making the virtual scroll coordinate stable enough that a successful sentence target does not bounce away on the next frame.

### A7-A — canonical display sentence -> pretty target map

Replace text-keyed identity with an explicit ordered alignment owned by the pretty cache.

Required conceptual shape:

```text
canonical display sentence index
        |
        v
Vec<Option<PrettySentenceTarget>>
        |
        +-- block_index
        +-- local_sentence_index
        +-- text/rich-span range when available
        +-- mapping confidence/source
```

Requirements:

- the authoritative key is canonical **display sentence index**, not audio-window index and not sentence text;
- enumerate pretty sentences in document order with stable occurrence identity;
- align the canonical display sentence stream to the pretty sentence stream monotonically;
- exact normalized matches should be preferred;
- bounded local lookahead/sequence alignment may handle small HTML/canonical normalization differences;
- duplicate sentence strings must map to their distinct occurrences in order rather than all resolving to one block;
- alignment may record `None` when confidence is insufficient;
- for HTML TTS highlight/follow, do **not** fall back from an unmapped canonical sentence to the proportional HTML anchor map;
- an unmapped sentence must never cause a remote/random jump. Keep the last stable viewport/highlight or use only a clearly adjacent, monotonic, confidently mapped target according to an explicit policy.

The old proportional anchor map may remain for other coarse-navigation semantics if still useful, but it is not TTS spoken-sentence identity.

### A7-B — sentence-level pretty highlighting

When a canonical sentence has a confident pretty target:

- highlight the targeted sentence occurrence, not merely whichever block shares its normalized string;
- preserve existing rich formatting;
- when technically practical, apply the highlight background to the sentence's text/rich-span range rather than coloring the entire paragraph/block;
- if sentence-range geometry cannot be represented for a specific block kind, use an explicit bounded block-level fallback for that target rather than selecting a different sentence.

The highlighted visual sentence must correspond to `ReaderPlaybackState.highlighted_sentence_idx`.

### A7-C — durable auto-scroll target state

Replace the one-shot `pending_auto_scroll: bool` semantics with a target-aware follow request.

Requirements:

- pending follow state identifies at least source/document identity and canonical display sentence index;
- a request is cleared only after the target is successfully resolved and a real scroll operation has been committed, or when a newer authoritative cursor supersedes it;
- if the target is temporarily outside the current virtual render window, force a bounded render window around that target and retain the request until the target response/rect exists;
- a failed/unmapped target does not consume into a random fallback;
- repeated events for the same canonical sentence do not produce repeated jumps;
- a newer sentence supersedes an older unresolved request rather than queueing stale jumps;
- pause/resume with an unchanged cursor does not move the viewport;
- Next/Prev produce exactly one new canonical follow target;
- voice/backend/settings events that do not change the display cursor do not move the viewport;
- honor `auto_scroll_tts` and `center_spoken_sentence` settings explicitly.

### A7-D — stabilize virtual pretty scroll geometry without restoring full-document layout

Preserve A5's bounded active block window.

Replace the single global block-height approximation with a stable virtual-layout strategy, for example per-block measured heights plus prefix sums / retained estimates, or an equivalent bounded design.

Requirements:

- rendered/measured block heights feed future virtual offsets;
- unmeasured blocks may use estimates, but refining measurements must not cause uncontrolled viewport oscillation;
- a sentence target that was successfully centered must remain in/near the active window on the following frame;
- manual scrolling must remain continuous enough for normal reading;
- headings, long paragraphs, lists, quotes, code blocks, tables, and images must not all be modeled permanently as one identical height;
- ordinary frames remain bounded; do not solve synchronization by rendering all 1,644 blocks again.

### A7-E — canonical event source for UI following

Drive pretty highlight/follow from the shared canonical reader playback state.

- `ReaderPlaybackState.highlighted_sentence_idx` / the canonical display cursor is the UI identity.
- TTS audio-window indices remain synthesis/navigation implementation details.
- `handle_tts_runtime_events` should request auto-scroll only when the authoritative display cursor changes (or an explicit Jump-to-highlight command is issued).
- Queued/state/latency events with no display-cursor change must not trigger scrolling.
- Keep one canonical ReaderSession authority from A5.1.

### A7-F — deterministic regressions that reproduce the hard cases

Add project-owned regressions that are structural, not timing-flaky.

At minimum:

1. Generate a large HTML/EPUB-shaped pretty stream with 1,500+ blocks and 10,000+ canonical display sentences.
2. Include repeated identical sentence strings at widely separated positions. Assert each canonical occurrence maps to the correct ordered pretty occurrence.
3. Include whitespace/entity/rich-span differences that normalize equivalently. Assert bounded monotonic alignment preserves occurrence identity.
4. Include deliberately unmappable canonical sentences. Assert they do not resolve through a proportional/distant HTML anchor and do not request a random jump.
5. Simulate 100+ sequential TTS display-cursor advances. Assert mapped target block progression is monotonic for the generated forward-reading fixture and never jumps backward to an earlier duplicate.
6. Exercise Next, Prev, Pause/Resume, Repeat, and an unchanged-cursor voice/settings event. Assert only real canonical cursor transitions create new follow targets.
7. Prove a pending off-screen follow request survives until the target block enters the bounded render window and clears only after successful scroll commitment.
8. Exercise strongly variable block heights and measurement refinement. Assert the target remains in/near the virtual window after the follow frame rather than oscillating to an unrelated range.
9. Preserve the A5 bounded-render invariant: thousands of blocks exist, but only a bounded active window is laid out.
10. Where sentence-range highlighting is supported, assert the selected canonical sentence range—not another identical occurrence—is marked.

### A7-G — transition-focused diagnostics

Add diagnostics that make identity failures observable without per-frame spam.

On canonical highlight transitions, record at debug/trace as appropriate:

- source identity;
- canonical display sentence index;
- current audio index when useful;
- resolved pretty block/local sentence target;
- mapping confidence/source;
- previous -> new target;
- follow state: requested / waiting-for-window / scrolled / deliberately-unmapped / superseded;
- virtual window start/end and target inclusion when a follow occurs.

Aggregate/warn if unmapped or low-confidence transitions become frequent. Do not emit the same mapping line every egui frame.

### A7-H — preserve accepted work and scope boundaries

Preserve:

- A3 Caliberate/native EPUB ingestion and cache recovery;
- A4 bounded lazy normalization;
- A5/A5.1 Arc-backed state, bounded rendering, off-main TTS, canonical shared ReaderSession, snapshot-free hot paths;
- A6 Windows backend/voice/synthesis behavior and QA configuration;
- legacy Calibre provider behavior.

Do not:

- reintroduce full-book render/layout;
- build a Caliberate-specific highlight state machine;
- use WebView/Tauri;
- change PDF synchronization as part of this correction;
- treat a larger scroll throttle as the fix;
- use proportional HTML anchors as the authoritative TTS target.

### Piper note — nonblocking for A7

The normal Windows QA process intentionally starts with a QA-scoped Windows backend override. The human observed that changing the in-app dropdown to Piper during this QA session does not appear to take effect reliably.

Do **not** mix Piper provisioning/backend-switch polish into A7. Record it as a separate follow-up after Goal 0008 unless implementation inspection reveals a tiny direct regression caused by A6. Windows voices and physical Windows speech are now verified.

### A7 acceptance gates

1. Canonical display sentence index has a first-class ordered pretty target map.
2. Duplicate sentence text is not used as global identity.
3. HTML TTS highlighting never falls back to proportional/distant anchors when canonical->pretty alignment is missing.
4. Auto-scroll follows canonical display-cursor changes, not generic TTS event presence.
5. Pending follow requests survive until success/supersession rather than being consumed blindly.
6. Unmapped sentences do not cause remote/random viewport jumps.
7. Variable-height virtual geometry remains stable while active rendering stays bounded.
8. 10k+/1.5k+ generated alignment/follow regressions cover duplicates, normalization differences, unmapped sentences, and 100+ transitions.
9. Windows TTS, voice selection, A3-A6 regressions, workspace tests, and Windows CI remain green.
10. No PDF redesign, WebView fallback, or full-document layout regression.

### Human QA contract for A7

**No human QA during A7 implementation/correction.**

After director acceptance only:

1. `git pull --ff-only`;
2. `.\qa.ps1` with normal Windows backend;
3. open the same large Recent/Caliberate EPUB;
4. play through enough consecutive sentences to observe sustained tracking, not just one jump;
5. verify the visible highlight corresponds to the currently audible sentence;
6. verify the viewport follows forward without random remote/backward jumps or oscillation;
7. exercise Next/Prev and Pause/Resume;
8. change Windows voice while staying on the same sentence and verify that the viewport does not jump merely because voice settings changed.

If one sentence cannot be confidently mapped, the acceptable degraded behavior is to avoid a jump until a trustworthy target exists. Random movement is never an acceptable fallback.
