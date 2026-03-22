# Egui State Update Rules

This document records the identity/update rules used by the Rust runtime so egui widgets can update without guessing ownership or invalidation behavior.

## Core principles
- State slices update independently; reader playback updates do not mutate reader document payloads.
- Session/panel changes do not invalidate heavy reader content.
- Runtime progress events are coalesced by the progress batcher to reduce redraw pressure.

## Ownership mapping
- `ReaderDocumentState` holds canonical page payloads, images, and PDF sync metadata derived from `ReaderSnapshot`.
- `ReaderPlaybackDomainState` holds playback cursor, TTS state, and playback stats derived from `ReaderPlaybackState`.
- `ReaderUiState` derives UI-only fields (search, panels, settings) from `ReaderSnapshot` without touching document or playback state.
- `RuntimeJobState` tracks background job progress events (source open, Calibre load, PDF transcription).

## Update rules
- Reader document updates are applied only on `ReaderUpdated` or explicit session reset.
- Reader playback updates are applied only on `ReaderPlaybackUpdated` or explicit session reset.
- Panel toggles update `SessionState` without altering `ReaderDocumentState`.
- Progress events (Calibre load, PDF transcription, source open) only mutate `RuntimeJobState` and operation scopes.

These rules correspond to the checks in `docs/roadmaps/egui-state-and-runtime-roadmap.md` Phase 2 and Phase 3.
