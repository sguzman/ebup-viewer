# Non-PDF Reader Windows QA (5–10 minutes)

From the repository root, run:

```powershell
.\qa.ps1
```

The script prepares the representative sources under `.qa\windows\fixtures\`, isolates QA config/cache/log state from tracked files, builds LanternLeaf, and launches it. No CI artifact download is part of this workflow.

Use those prepared fixtures or equivalent small local files. Record any failure with the source format and sentence/search term.

- [ ] TXT: open, search repeated term, click a sentence, play/pause, next/previous, and confirm highlight follows speech.
- [ ] Markdown: open in Pretty mode, verify headings/emphasis/list/link/image are readable, toggle text-only, and play from highlight.
- [ ] HTML: open in Pretty mode, verify heading/links/image/list/table, toggle text-only, search, and confirm jump/highlight behavior.
- [ ] EPUB: open both chapters, verify HTML pretty content and internal chapter navigation, toggle text-only, search, and play from highlight.
- [ ] Windows TTS: select an installed voice and confirm the selected voice remains active after reopening the reader.
- [ ] Switch Piper ↔ Windows while reading; confirm the canonical sentence/cursor does not reset or fork.
- [ ] Exercise play, pause, next, previous, repeat, play-from-page-start, and play-from-highlight.
- [ ] Confirm auto-scroll/jump follows the spoken sentence without repeated jitter.
- [ ] Reopen each representative source and confirm bookmark/progress returns to the expected sentence.
