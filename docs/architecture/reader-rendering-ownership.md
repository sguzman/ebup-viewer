# Reader Rendering Ownership

## Frontend ownership boundaries

- `prettyHtml.ts` owns native HTML sanitization, asset URL rewriting, and iframe-ready HTML document output.
- `markdownRender.ts` owns markdown-to-HTML rendering and image resolution for pretty markdown pages.
- `htmlSync.ts` owns HTML anchor extraction heuristics and sentence-to-anchor matching.
- `useReaderHighlightSync.ts` owns DOM-facing reader sync behavior:
  - pretty-anchor lookup
  - pretty highlight application
  - auto-scroll alignment
  - iframe/native HTML anchor rebuilds
- `ReaderShell.tsx` owns layout composition and selects which reader pane or panel is visible.
- `readerPanels.tsx` owns top-bar, search-bar, settings, stats, and TTS side-panel UI.
- `useReaderSessionStats.ts` owns session-scoped reading metrics derived from stable reader stats plus transient local timers.

## Data ownership boundaries

- Text-only sentences remain the canonical unit for playback and cursor movement.
- Pretty HTML and pretty markdown are presentation layers.
- `sentence_anchor_map` is the backend hint surface for matching text-only sentences back into pretty content.
- HTML sync heuristics may refine those hints at runtime, but they do not replace text-only sentence ownership.
