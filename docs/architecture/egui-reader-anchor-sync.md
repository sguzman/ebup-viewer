# Egui Reader Anchor Sync Plan

This document defines canonical anchor ownership and fallback rules for highlight sync in the egui reader.

## Canonical anchor ownership
- The runtime owns the canonical sentence index and TTS cursor.
- The renderer resolves anchors based on `sentence_refs` from the content-block tree.
- `sentence_anchor_map` remains a hint surface and is refined using layout metadata before emitting highlight spans.

## Rust-native anchor lookup
- HTML/Markdown anchor semantics are mapped into the content-block model during conversion.
- Anchor resolution is deterministic and independent of browser DOM behavior.

## Fallback order and tracing
Fallbacks are explicit and traced:
1. **Exact anchor** (`highlight.anchor=exact`)
2. **Nearest same-block anchor** (`highlight.anchor=same_block`)
3. **Nearest same-section anchor** (`highlight.anchor=same_section`)
4. **Visible-region fallback** (`highlight.anchor=visible`)
5. **Missing/no-op** (`highlight.anchor=missing`)

## Diagnostics
- Each fallback emits a span with the chosen anchor path and sentence index.
- Missing anchors log a warning and avoid scroll jitter.

## Auto-scroll rules
- Auto-scroll only triggers when the highlight moves outside the viewport threshold.
- Navigation alignment respects user preference (centered highlight or edge/top aligned).
- Copy/paste or search-driven jumps flow through the same shell navigation commands so the command/effect pipeline remains consistent.

## Phase 4 exit condition
Sentence highlight, anchor fallback, and auto-scroll rules are explicit, traced, and tied to the shell performance budget.
