# LanternLeaf Product Scope

This is the director-owned statement of what LanternLeaf is intended to become. Current implementation status is tracked separately in `current-status.md`.

## Core product

A native desktop document reader with tightly synchronized TTS playback and visual reading state.

Primary target surfaces:

- local document open;
- recent/library flow;
- native reader UI;
- persistent reading position;
- search/navigation;
- configurable TTS;
- synchronized highlighting;
- native PDF reading.

## Source formats

Target supported source families:

- EPUB;
- TXT;
- Markdown;
- HTML;
- PDF;
- DOCX / Word documents.

Support may be staged. A format is not considered complete merely because text extraction exists; reading, navigation, TTS, and persistence need coherent behavior.

## TTS

First-class backends:

- Piper/local neural TTS already present in the codebase;
- native Windows TTS to be added as a Windows backend.

The backend architecture should allow additional implementations without entangling reader state.

Core behavior includes:

- voice/backend selection;
- play/pause/stop;
- play from selected/highlighted sentence;
- next/previous/repeat sentence;
- speed/volume controls where supported;
- completion/progress events sufficient for highlight synchronization.

## PDF

PDF support includes both the visual document and the reading text.

Target capabilities:

- native page rendering;
- zoom/scroll/viewport management;
- text extraction;
- search;
- TTS;
- sentence/page geometry synchronization;
- highlight/jump-to-current-sentence;
- explicit degraded modes for OCR/render-only documents.

## Library and integrations

Existing recent-book, Calibre, browser/import, cache/config/bookmark features remain in scope unless a later director decision removes them.

They are subordinate to a stable reading/TTS core.

## Non-goals for the restart phase

The restart is not a license for:

- a new web frontend;
- cloud-first account infrastructure;
- large UI redesign before core behavior is stable;
- replacing working local subsystems merely for novelty;
- mass dependency churn unrelated to concrete blockers.
