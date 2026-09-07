# LanternLeaf Philosophy

LanternLeaf exists to make long-form reading easier to consume without surrendering control of the document to a browser, cloud service, or opaque reader stack.

## Reading and listening are one activity

TTS is not an optional accessibility checkbox bolted onto a reader. The product treats speech, visual text, sentence identity, navigation, highlighting, and bookmarks as one synchronized reading session.

A good implementation should let the user:

- read normally;
- listen normally;
- jump between reading and listening;
- click text to speak from there;
- see exactly what is being spoken;
- preserve position across sessions.

## Native ownership over fragile browser ownership

The project previously accumulated web/Tauri/WebView machinery. The current direction is deliberately native Rust + egui.

The motivation is not ideological purity. It is control:

- fewer hidden rendering/lifecycle layers;
- explicit state;
- predictable local performance;
- platform integration that can be reasoned about;
- direct ownership of PDF/TTS synchronization.

## Formats converge on semantics

EPUB, HTML, Markdown, TXT, PDF, and Word-like sources are ingestion/presentation formats. They should converge toward a common reading/session model instead of each inventing incompatible playback semantics.

The document's canonical reading identity should survive changes in rendering strategy.

## Degrade explicitly, never fake precision

Some PDFs have excellent embedded text. Others have broken ordering, scans, OCR noise, or no usable text layer.

When exact sentence geometry is unavailable, LanternLeaf should expose a lower-confidence mode rather than pretending synchronization is exact.

## Local-first, observable, recoverable

Core reading should work locally.

Important background work should be visible through structured tracing. Build/runtime failures should produce actionable evidence. Cache and persistent state should be deterministic enough to inspect and repair.

## Platform support is product support

Windows is not a secondary compile target. The restarted project is being actively recovered on Windows 11, while Linux remains a first-class target.

Native Windows TTS is a product feature, not a workaround.
