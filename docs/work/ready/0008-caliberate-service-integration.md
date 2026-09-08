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
