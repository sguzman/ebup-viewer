# Egui Reader Render Model

This document defines the Rust-native render model for non-PDF reader content. It formalizes the intermediate representation, payload mapping, and anchor strategy used by egui widgets.

## Shared intermediate representation (pretty content)
The pretty renderer consumes a content-block tree with explicit block and inline nodes:
- **Block nodes:** paragraph, heading, list, list item, image/figure, caption, table (simplified grid).
- **Inline runs:** plain text, emphasis, strong, code, link spans.
- **Anchors:** each block may carry `anchor_id` and `sentence_refs` (canonical sentence indices) to support highlight sync.
- **Source metadata:** links and assets retain `href`/`asset_ref` and `source_path` to preserve provenance and external navigation behavior.

### Required block fields
- `block_id`: stable identifier for tracing and layout caching.
- `kind`: paragraph/heading/list/list_item/image/caption/table.
- `anchor_id`: optional anchor identifier for highlighting and navigation.
- `sentence_refs`: zero-or-more canonical sentence indices covered by this block.
- `source`: source metadata (`source_path`, `href`, `asset_ref`).

### Required inline fields
- `run_id`: stable identifier for incremental redraw.
- `text`: normalized display string.
- `style`: emphasis/strong/code/link style tags.
- `anchor_id`: optional anchor identifier for links/spans.

## Text-only representation
Text-only view remains canonical for TTS/search/highlight:
- `page_text` and `sentences` are the source of truth.
- `sentence_anchor_map` is a hint surface for pretty content mapping.
- `highlighted_sentence_idx` drives selection and auto-scroll in the sentence list.

### Selection policy
- Text selection is intentionally minimized in the sentence list to preserve click-to-play semantics.
- Selection behaviors should be exposed via explicit copy actions or future selectable labels, not by hijacking sentence click events.

## Canonical sentence to pretty mapping
- Each pretty block stores `sentence_refs` computed during conversion.
- `sentence_anchor_map` seeds the mapping; the converter may refine it using layout metadata.
- The renderer resolves highlight anchors by matching the active sentence index to the nearest block with `sentence_refs`, using deterministic fallback rules (same-block → same-section → visible-region → missing).

## Payload preservation
The render model preserves the existing Rust payload concepts:
- `page_text`
- `sentences`
- `sentence_anchor_map`
- `images`
- `pretty_kind`

## Per-source representation
- Plain text: directly mapped into sentence blocks.
- Markdown: parsed into the content-block tree with inline runs.
- HTML/EPUB: sanitized and converted into the same content-block tree.

## View-model output
The output of conversion is a Rust-native view model and **must not** rely on DOM/CSS primitives. Eguis’s layout system owns positioning, spacing, and scrolling.

## Runtime integration
- The egui reader consumes `AppRuntime::state_snapshot` for canonical sentences, highlight state, and anchor hints.
- Reader interactions emit runtime commands (`AppCommand::Reader` with `SessionCommand::SentenceClick` / navigation) so the command/effect pipeline remains authoritative.

## Phase 1 contract completeness
This document plus the existing reader contracts are considered sufficient to proceed with native rendering for all non-PDF sources without reopening data ownership or conversion policy questions.
