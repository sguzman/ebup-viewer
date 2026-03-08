# Native PDF Reader QA Checklist

## Render Fidelity
- Open a structured PDF in Pretty Text mode and confirm the visible page layout matches the source PDF rather than markdown/HTML approximation.
- Verify figures, tables, and page breaks remain visible in Pretty Text PDF mode.
- Zoom in and out several times and confirm the text layer and page canvas remain aligned.

## Text Ownership
- Switch between Text-only and Pretty Text PDF views while paused and confirm the current sentence label and playback progress do not change.
- Start playback in Text-only mode, switch to Pretty Text PDF mode, and confirm the same sentence remains active.
- Run search from Text-only mode and confirm the search cursor continues to refer to `tts_text`, not PDF visual order.

## Highlight and Scroll
- Start playback on a high-text-trust PDF and confirm the spoken sentence highlight appears on top of the rendered PDF text.
- Leave playback on the same sentence and confirm the PDF view does not jitter or repeatedly re-center.
- Advance playback to the next sentence and confirm the PDF view only scrolls when the mapped sentence location changes.
- Trigger manual jump-to-highlight and confirm the PDF pane scrolls to the current spoken location even if playback is paused.
- Open a degraded PDF and confirm the UI clearly reports fallback/page-only sync rather than pretending the highlight is exact.

## Resume and Reopen
- Close and reopen the same PDF and confirm geometry mode/sync mode remain stable after cache reuse.
- Delete the recent PDF entry and confirm cached `tts_text` and PDF sync metadata are removed.
- Reopen a PDF after corrupting or removing its PDF sync metadata and confirm the app rebuilds the artifacts without crashing.

## Difficult PDFs
- Validate one multi-column academic PDF.
- Validate one scan/OCR-heavy PDF.
- Validate one PDF with repeated headers/footers or long captions.
- Validate one PDF with rotated pages or rotated text blocks.
