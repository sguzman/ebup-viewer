# PDF Renderer Contract

## Ownership
- `tts_text` is the canonical text source for normalization, sentence planning, search ownership, bookmarks, resume, and TTS playback.
- Pretty Text PDF rendering is a separate visual projection of the source PDF and must never redefine sentence ownership.

## Renderer
- The native PDF renderer is the egui PDF pipeline owned by `crates/lanternleaf-egui/src/pdf_renderer.rs`.
- The rendered canvas layer owns visual fidelity for page paint, images, figures, and page geometry.
- The `pdf.js` text layer is treated as a quality-classified hint surface for sync and reverse navigation. It is not assumed to be exact for every PDF.

## Geometry
- Page metrics, zoom state, and viewport transforms are owned by the `pdf.js` page viewport returned from `page.getViewport(...)`.
- Sentence sync works against text-layer spans produced for that viewport.
- Highlight overlays must either:
  - resolve to sentence spans with `exact_geometry`
  - resolve to a local fuzzy/block fallback
  - degrade to page-only location
  - or disable sync entirely in `render_only_no_sync`

## Fallback Contract
- `high_text_trust` prefers sentence spans.
- `mixed_text_trust` may use fuzzy sentence geometry or paragraph/block fallback.
- `ocr_required` keeps native PDF rendering active but does not present exact sync unless text quality justifies it.
- `render_only_no_sync` renders the PDF without pretending highlight sync exists.

## Cache Contract
- Cache artifacts store extracted `tts_text`, PDF sync metadata, and content layout versioning under the hashed source cache directory.
- Corrupt PDF sync metadata is removed and rebuilt non-destructively on the next PDF load.
