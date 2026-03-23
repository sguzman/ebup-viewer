# Reader Rendering Ownership

## Egui ownership boundaries

- `crates/lanternleaf-egui/src/app/ui/reader.rs` is the reader-facing entry point for pretty-content rendering and layout composition.
- `crates/lanternleaf-egui/src/app/reader_html.rs` owns HTML sanitization, asset URL rewriting, and content-block conversion for pretty rendering.
- `crates/lanternleaf-core/src/html_render.rs` owns markdown/HTML parsing into the shared content-block model.
- `crates/lanternleaf-egui/src/app/tts_sync.rs` owns highlight sync behavior:
  - pretty-anchor lookup
  - highlight application
  - auto-scroll alignment

## Data ownership boundaries

- Text-only sentences remain the canonical unit for playback and cursor movement.
- Pretty HTML and pretty markdown are presentation layers.
- `sentence_anchor_map` is the backend hint surface for matching text-only sentences back into pretty content.
- HTML sync heuristics may refine those hints at runtime, but they do not replace text-only sentence ownership.
