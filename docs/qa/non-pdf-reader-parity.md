# Non-PDF Reader Parity Matrix

This is restart-era evidence for the native Rust reader. Historical roadmap checkmarks are not evidence. `verified automated` means a repository test exercises the current loader/session path; `verified Windows-specific` requires hosted or local Windows evidence; `manual verification required` is intentionally open; `known degraded behavior` is explicit and bounded.

| Domain | TXT | Markdown | HTML | EPUB |
|---|---|---|---|---|
| Source ingestion | verified automated — representative fixture + `ReaderSession::load` | verified automated — representative fixture + `ReaderSession::load` | verified automated on hosted Pandoc runner; manual local prerequisite | verified automated on hosted Pandoc runner with deterministic builder; manual local prerequisite |
| Canonical `tts_text` | verified automated — plain source | verified automated — Markdown syntax removed from speech text | verified automated — Pandoc plain conversion; scripts/styles excluded | verified automated — EPUB chapter extraction + Pandoc plain conversion |
| Pretty payload kind | verified automated — none | verified automated — Markdown | verified automated — HTML | verified automated — HTML |
| Canonical sentence generation | verified automated | verified automated | verified automated | verified automated |
| Page/sentence accounting | verified automated — sum matches canonical sentence count | verified automated — sum matches canonical sentence count | verified automated on hosted runner | verified automated on hosted runner |
| Sentence anchor mapping | verified automated — bounded identity/fallback | verified automated — native block anchors | verified automated — HTML anchors/fallback | verified automated — chapter HTML anchors/fallback |
| Text-only / pretty switch | verified automated | verified automated | verified automated | verified automated |
| Sentence click/highlight | verified automated | verified automated | verified automated on real session | verified automated on real session |
| Next/previous/repeat | existing Rust session tests; automated | existing Rust session tests; automated | manual verification required for visual behavior | manual verification required for visual behavior |
| Play from page start/highlight | simulated runtime uses real TXT/Markdown sessions | simulated runtime uses real TXT/Markdown sessions | verified session contract; runtime visual QA required | verified session contract; runtime visual QA required |
| Search next/previous/selected result | verified automated — repeated fixture term | verified automated — repeated fixture term | verified automated on hosted runner | verified automated on hosted runner |
| Auto-scroll / jump mapping semantics | manual verification required — no pretty geometry | deterministic mapping logic automated | deterministic mapping logic automated; visual scroll manual | deterministic mapping logic automated; visual scroll manual |
| Persistence / reopen | existing cache/bookmark contract; representative reopen test pending | existing cache/bookmark contract; representative reopen test pending | manual verification required | manual verification required |
| Delete/cache cleanup | existing idempotent cache contract | existing idempotent cache contract | manual verification required for assets | manual verification required for chapter/image assets |
| Simulated TTS events/cancellation | verified automated with real session | verified automated with real session | manual/runtime evidence required on hosted fixture | manual/runtime evidence required on hosted fixture |
| Piper/Windows reader-state neutrality | verified automated by shared session/runtime contracts | verified automated by shared session/runtime contracts | verified automated by shared session/runtime contracts | verified automated by shared session/runtime contracts |
| Automated evidence | `non_pdf_parity` + app runtime integration tests | `non_pdf_parity` + app runtime integration tests | core parity test on Pandoc-capable runner | core parity test on Pandoc-capable runner |
| Remaining manual Windows QA | required | required | required | required |

## Current evidence commands

- `cargo test -p lanternleaf-core --test non_pdf_parity -- --nocapture`
- `cargo test -p lanternleaf-app --test non_pdf_tts_runtime -- --nocapture`
- `cargo test --workspace` on the hosted Windows/Pandoc runner

The local checkout used for authoring this goal does not have Pandoc, so local HTML/EPUB execution is reported as an environment limitation rather than silently marked green.
