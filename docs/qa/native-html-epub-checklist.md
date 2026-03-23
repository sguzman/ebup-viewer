# Native HTML EPUB QA Checklist

- [ ] Cover thumbnail appears in both Calibre list and recents for same EPUB.
- [ ] Pretty view renders EPUB/HTML images inline (no raw markdown image snippets).
- [ ] Internal `#anchor` links scroll within the current reader container.
- [ ] External links open in the system browser with safe target/rel attributes.
- [ ] TTS playback sentence highlight remains stable when toggling Pretty/Text-only.
- [ ] Auto-scroll follows mapped pretty anchors across sentence transitions.
- [ ] Reopening previously opened EPUB succeeds after cache reuse/rebuild.
- [ ] Deleting recently opened source is idempotent and does not fail on transient directory races.
