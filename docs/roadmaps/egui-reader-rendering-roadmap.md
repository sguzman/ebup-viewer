# Egui Reader Rendering Roadmap

## Objective
- [ ] Rebuild EPUB/TXT/Markdown/HTML reading in egui while preserving current text ownership, sentence highlighting, click-to-play, and reading controls.
- [ ] Replace DOM/CSS/browser rendering assumptions with a Rust-native render model suitable for immediate-mode UI.
- [ ] Keep canonical sentence/TTS ownership entirely in Rust domain state.

## Current-State Grounding In This Repo
- Current non-PDF reader rendering is spread across:
- `ui/src/components/contentRender.ts`
- `prettyHtml.ts`
- `markdownRender.ts`
- `readerContentPanes.tsx`
- `readerDom.ts`
- `readerHtmlSync.ts`
- `useReaderHighlightSync.ts`
- `useHtmlSentenceAnchorMap.ts`
- The existing architecture already defines:
- `tts_text` as canonical
- `pretty_kind` as presentation selector
- `sentence_anchor_map` as a mapping hint surface
- Current pretty rendering relies on browser primitives:
- HTML sanitization and DOM output
- CSS layout and scrolling
- clickable spans/anchors
- iframe/native HTML assumptions for rich content

## Target End State Under Egui
- Reader rendering is Rust-native for:
- text-only sentence list view
- pretty text for EPUB/HTML/Markdown
- inline images/assets
- sentence highlight and click targeting
- scrolling/jump-to-highlight
- typography and spacing settings
- The render model uses a Rust-native intermediate representation instead of DOM ownership.
- Canonical document/session/playback state remains Rust-owned and renderer-agnostic.

## Key Architectural Decisions Already Chosen
- Text-only mode remains the canonical playback/search/cursor surface.
- Pretty rendering is a projection of canonical text and source artifacts, not a second ownership layer.
- HTML must be converted into a Rust-native intermediate content model rather than requiring an embedded WebView.
- Markdown and HTML should converge onto a shared rich-text/content-block model wherever possible.
- Image rendering, anchor mapping, and scroll targeting must be implemented in Rust-native UI logic.

## Target Reader Render Model
- [x] Define a shared intermediate representation for pretty content:
- [x] block nodes
- [x] inline text runs
- [x] headings
- [x] paragraphs
- [x] lists
- [x] images/figures/captions
- [x] tables or simplified table blocks
- [x] anchor metadata for sync
- [x] source metadata for links and assets
- [x] Define a text-only representation optimized for sentence interaction and playback cursor ownership.
- [x] Define how canonical sentence indices map into pretty content anchors/blocks.

## Phase 1: Data And View Contracts
- [x] Preserve current reader payload concepts in Rust:
- [x] `page_text`
- [x] `sentences`
- [x] `sentence_anchor_map`
- [x] `images`
- [x] `pretty_kind`
- [x] Replace browser-facing HTML/markdown output assumptions with Rust-native view model outputs.
- [x] Decide per-source representation:
- [x] plain text source -> sentence/block model directly
- [x] markdown source -> markdown-to-content-block conversion
- [x] HTML/EPUB source -> sanitized HTML-to-content-block conversion
- Phase exit:
- [x] there is a decision-complete Rust render-model contract for all non-PDF reader sources.

## Phase 2: Text-Only Reader In Egui
- [x] Rebuild sentence list rendering with:
- [x] clickable sentence rows/spans
- [x] highlight styling
- [x] search hit styling
- [x] jump-to-highlight behavior
- [x] text selection policy
- [x] Preserve canonical sentence ownership and click-to-play semantics.
- [x] Implement typography controls using egui text styling rather than CSS.
- Phase exit:
- [x] text-only reader parity is reachable without any WebView dependency.
- Runtime integration:
- [x] Consume `AppRuntime::state_snapshot` for canonical sentences and highlight data instead of duplicating the data into UI-local stores.
- [x] Emit `AppRuntime::plan_command(AppCommand::Reader(session::SessionCommand::SentenceClick { ... }))` or `SessionCommand::NextSentence` when the user clicks a sentence or uses reader navigation so the runtime command/effect pipeline stays in sync with shortcut bindings.

## Phase 2.5: Shell Performance Integration
- [x] Capture the shell redraw/coalescing constraints specified in `egui-app-shell-and-navigation-roadmap` Phase 6 and tie them to the reader rendering pipeline.
- [x] Define how reader render invalidation scopes align with shell-dictated redraw budgets:
- [x] sentence-level highlights/reactive blocks request repaint only when their canonical state changes.
- [x] panel-induced layout changes recompute reader width via Phase 1 layout helpers without forcing a full scroll reflow.
- [x] In-progress runtime events (TTS heartbeat, ingestion progress) are throttled before reaching the reader render widgets to honor the shell’s coalescing matrix.
- [x] Document hooks for tracing/metrics instrumentation so the reader can emit frame-level spans that feed into the shell’s tracing plan (Chapter 6) for latency and redraw accounting.
- [x] Define the interplay between the reader’s scroll/jump logic and the shell’s performance throttles so auto-scrolls never trigger runaway repaints.
- [x] Surface an anchor diagnostics panel that reports fallback reason counts, JumpToSentence throttle telemetry, and shell-budget tracing markers for the reader viewport.
- [x] Expand the diagnostics surface so overlay budget pressure badges now report native render/eviction spans and point QA toward replaying those spans in context.
- [ ] Phase exit:
- [x] the reader rendering plan articulates how it obeys the shell’s performance expectations before interactive widgets are implemented.

## Phase 3: Pretty Rendering For Markdown / HTML / EPUB
- [x] Build rendering widgets for the content-block model that emit the smallest necessary redraw scopes described in Phase 2.5.
- [x] Support:
- [x] paragraphs/headings with lazy layout so long documents do not force full recomposition.
- [x] inline emphasis with inline spans that only repaint their affected runs.
- [x] links and anchors that map to the canonical anchor map without immobilizing the main thread.
- [x] images/assets with placeholder sizing information and lazy load hooks tied to scroll visibility.
- [x] block spacing and margin controls derived from the layout helper policy so panel resizes have deterministic width adjustments.
- [x] footnote/caption-like secondary content rendered as detachable overlays or collapsible summaries to avoid repaint storms.
- [x] Define explicit degraded behavior for HTML/Markdown features that exceed the Rust content-block model (e.g., complex grids), documenting fallback spacing and diagnostics for QA.
- [x] Connect each content block to the shell tracing plan (Phase 6) so layout recalculation is traced, and highlight updates emit spans tied to the command/effect pipeline.
- [x] Document how the content-block conversion pipeline runs off the UI thread (Phase 4 runtime async plan) before handing data to egui so the render passes remain responsive.
- [x] Phase exit:
- [x] pretty reader behavior is specified without relying on browser DOM/CSS execution, and the content-block widgets understand the performance/coalescing expectations from the shell roadmap.

## Phase 4: Anchor Mapping And Highlight Sync
- [x] Define the canonical sentence anchor ownership model, ensuring the runtime tracing plan can attribute highlight jumps to source artifacts and user commands.
- [x] Port HTML/Markdown sync semantics into Rust-native anchor lookup that populates `SentenceHighlight` data used by the reader renderer.
- [x] Preserve `sentence_anchor_map` as a hint surface; document how runtime mapping logic refines it using layout metadata before broadcasting highlight spans so shell invalidation budgets stay intact.
- [x] Define deterministic fallback order when exact anchors are unavailable and connect each fallback to shell diagnostics/tracing:
- [x] exact anchor (emit span `highlight.anchor=exact`)
- [x] nearest same-block anchor (span `highlight.anchor=same_block`)
- [x] nearest same-section anchor (span `highlight.anchor=same_section`)
- [x] visible-region fallback (span `highlight.anchor=visible`)
- [x] no-op with explicit diagnostics for out-of-sync states (span `highlight.anchor=missing`)
- [ ] Define auto-scroll rules that honor the shell’s redraw budget and coalescing matrix:
- [ ] only trigger scroll when highlight moves outside the current viewport threshold.
- [x] throttle repeated scroll commands from rapid highlight change events per Phase 2.5’s coalescing plan.
- [ ] center or edge align navigation based on user preference (e.g., centered highlight vs. top-aligned on jump) with instrumentation hooks for later perf tuning.
- [ ] Document how copy/paste or search-driven anchor jumps integrate with shell navigation commands so the command/effect pipeline can reconcile UI flows with runtime state updates.
- [ ] Phase exit:
- [ ] sentence highlight, anchor fallback, and auto-scroll rules are explicit, traced, and tied to the shell’s performance budget before UI coding begins.

## Phase 5: Scroll, Jump, And Interaction Semantics
- [ ] Replace browser scroll APIs with `egui::ScrollArea`/native scroll region ownership that respects the shell’s redraw and coalescing budgets (Phase 6 of the app shell roadmap).
- [ ] Define jump-to-highlight behavior that ties back to the command/effect pipeline:
- [ ] navigation commands (keyboard shortcuts, search/click hits) emit `JumpToSentence` with canonical indices so the reader renderer can scroll without triggering redundant layout passes.
- [ ] Auto-scroll engages only when the highlight leaves the visible threshold, and it is throttled per the shell coalescing matrix to avoid repaint storms.
- [ ] Center-tracking and top-alignment modes are defined as switchable behaviors documented in the settings instrumentation so telemetry can capture which preference is active when performance metrics spike.
- [ ] Preserve “do not jitter on the same sentence” and “only scroll on meaningful ISO changes” rules by comparing canonical highlight indices before requesting new scroll frames.
- [x] Document how scroll requests emit tracing spans that feed into the shell’s performance plan (Phase 6), including fields for the initiating command, target sentence, and whether the scroll was auto or manual.
  - JumpToSentence auto-scroll spans now carry `budget_plan=shell.performance_budget`, `target_sentence`, `command=reader.highlight`, `auto_scroll=true`, and `anchor_path` metadata so the shell diagnostics plan can reconcile highlight jumps with redraw budgets.
- [ ] Define link behavior:
- [ ] internal anchor navigation reuses the canonical anchor map and descriptor metadata, yielding deterministic scroll targets and tracing.
- [ ] external links raise shell commands (via the runtime command model) that launch the native system browser and emit telemetry/logs for QA.
- [ ] Provide diagnostics/telemetry for cases when scroll targets cannot be satisfied (e.g., missing anchor) and fall back gracefully with a no-op plus logged warning event.
- [ ] Document how selection, search navigation, and reader interactions produce the same `JumpToSentence` commands so the instrumentation pipeline sees unified flow.
- Phase exit:
- [ ] all reader interactions have native-egui semantics, obey the shell performance instrumentation, and expose parity rules for QA.

## Phase 6: Typography And Reader Settings
- [ ] Port reader settings into `egui` with controls that obey the redraw budgets and coalescing rules from the shell performance roadmap (Phase 6 of the app shell).
- [ ] Typography controls must include:
- [ ] font family/weight selection with preview spans so changes avoid full reflow by resting only on affected text blocks.
- [ ] font size slider that triggers throttled requests to the renderer, emitting spans that record the prior/next size without forcing extra repaints.
- [ ] line spacing, letter spacing, and word spacing knobs that update only relevant layout metadata and signal the shell when a rebuild of cached layout is safe.
- [ ] Horizontal/vertical margin controls impacting the reader padding must call into Phase 1 layout helpers to recompute widths without hitting panel redraw scopes.
- [ ] Highlight color modes (light, dark, custom) must update rendering state incrementally and produce tracing fields linking to the runtime command/effect model—any highlight change should be tracked by the tracing plan from the runtime roadmap.
- [ ] Document acceptable degradation when egui cannot mimic CSS-level shaping (e.g., advanced ligatures) and specify how telemetry/QA should note these differences.
- [ ] Phase exit:
- [ ] reader settings behavior is explicit, instrumented, and implementable without reopening layout/performance questions, keeping the ui responsive and traceable.

## Risks / Failure Modes
- HTML fidelity can regress sharply if the intermediate content model is under-specified.
- Overcommitting to exact browser parity may stall implementation; the roadmap must preserve behavior, not browser internals.
- Scroll/highlight behavior may jitter if anchor and viewport ownership are not explicit.
- Table-rich or footnote-heavy content can become unreadable if the content-block model is too simplistic.

## Test / Parity Requirements
- [ ] Rust tests for markdown/HTML-to-content-block conversion.
- [ ] Rust tests for sentence-to-anchor mapping and fallback logic.
- [ ] Manual QA on EPUB/HTML-heavy books with images, headings, footnotes, and internal links.
- [ ] Parity checks for click-to-play, search navigation, highlight sync, and settings behavior.
- [ ] Full implementation-phase build verification excluding AppImage/RPM/DEB packaging outputs.

## Acceptance Criteria
- [ ] Text-only and pretty-text reader modes are fully specified for a Rust-native egui implementation.
- [ ] Canonical sentence/TTS ownership is preserved and explicit.
- [ ] HTML/markdown rendering no longer depends on DOM/WebView ownership in the target plan.
- [ ] Scroll, jump, and highlight semantics are concrete enough for implementation without reopening design questions.
