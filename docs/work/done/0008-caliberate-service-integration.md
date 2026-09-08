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
