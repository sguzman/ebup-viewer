# Store Reader Domain State

## Target reader-state boundary

The frontend store should treat `reader` as a transitional container, not the long-term domain shape.

The intended ownership boundary is:

- `readerDocument`
  - stable page/document payload
  - `source_path`
  - `current_page`
  - `total_pages`
  - `pretty_kind`
  - `reading_markdown_page`
  - `reading_html_page`
  - `page_text`
  - `sentences`
  - `sentence_anchor_map`
  - `images`
- `readerPlayback`
  - highlighted sentence index
  - TTS state
  - TTS progress
  - cursor movement metadata
  - playback-only stats derived from the active cursor
- `readerUi`
  - panel open/closed state
  - text-only vs pretty-text view mode
  - search query and selected match
  - local reader control visibility

## Transitional rule

Until the store is fully reshaped, selectors and component subscriptions should behave as if those domains already exist:

- document consumers should subscribe only to document-shaped data
- playback consumers should subscribe only to cursor/TTS state
- panel toggles must not invalidate document subscribers
- event ingestion should avoid replacing the full `reader` object when only playback changed

## Current mapping

The current codebase approximates this boundary with:

- `useReaderDocumentKey` and `useReaderDocumentState` for document-facing subscriptions
- narrow quick-action selectors for panel state and text-only flags
- event-ingestion guards that preserve identity on no-visible-change updates

That is an intermediate step, not the final store model.
