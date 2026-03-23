# Egui Native PDF Renderer Decision

This document records the renderer stack decision for the egui-native PDF subsystem and the fallback criteria for changing course.

## Primary recommendation
Adopt a Rust-native PDF stack split into:
- page raster/rendering
- text extraction/layout metadata
- optional OCR augmentation
- egui texture presentation

## Selected stack (current)
- **Renderer**: `pdfium_render` with `pdfium_auto` bundled binaries for deterministic raster output.
- **Text extraction**: Pdfium text APIs for basic extraction, augmented by existing OCR alignment artifacts when quality degrades.
- **OCR augmentation**: existing OCR alignment artifacts under `.cache/lantern-leaf/<source_hash>/content/pdf-*`.
- **Texture presentation**: egui textures with explicit render requests, cache hits/misses, and upload spans.

## Evaluation targets
- Deterministic raster output and text extraction sufficient to preserve sync ownership.
- Render output cached and uploaded into egui textures efficiently with predictable eviction.
- Licensing and maintenance acceptable for long-term desktop shipping.

## Fallback criteria (deviating from single-stack)
- Text extraction quality insufficient for current PDF sync contracts.
- Rendering fidelity/performance not competitive on representative PDFs.
- Viewport memory behavior not controllable enough for multi-page documents.

If any criteria trigger:
- Split rendering and text extraction into separate Rust-owned components.
- Keep all integration native and in-process.
- Do not reintroduce legacy browser/WebView ownership as a fallback.

## Instrumentation alignment
- Render requests and completions emit `pdf.render.request` / `pdf.render.complete` with zoom, priority, and cache hit metadata.
- Viewport updates emit `pdf.viewport.update` with visible/overscan ranges and trigger.
- Highlight overlay updates emit `pdf.highlight.apply` / `pdf.highlight.cleanup` with sentence and anchor metadata.
