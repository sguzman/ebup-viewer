import { useCallback, useEffect, useRef, type MutableRefObject } from "react";

import type { ReaderSnapshot } from "../types";
import { scrollSentenceIntoView } from "./readerDom";

interface UseReaderScrollSyncArgs {
  getPrettyAnchorNode: (anchorIdx: number) => HTMLElement | null;
  reader: ReaderSnapshot;
  resolvePrettyAnchorIdx: (idx: number) => number | null;
  sentenceRefs: MutableRefObject<Record<number, HTMLButtonElement | null>>;
  sentenceScrollRef: MutableRefObject<HTMLDivElement | null>;
}

export interface ReaderScrollSyncState {
  jumpToHighlightedSentence: () => void;
}

export function useReaderScrollSync({
  getPrettyAnchorNode,
  reader,
  resolvePrettyAnchorIdx,
  sentenceRefs,
  sentenceScrollRef
}: UseReaderScrollSyncArgs): ReaderScrollSyncState {
  const prettyLastAutoScrollAnchorRef = useRef<number | null>(null);
  const prettyLastAutoScrollPageRef = useRef<number | null>(null);
  const pendingScrollFrameRef = useRef<number | null>(null);

  const scrollNativeHtmlAnchorIntoView = useCallback(
    (anchor: HTMLElement, behavior: ScrollBehavior): void => {
      anchor.scrollIntoView({
        behavior,
        block: reader.settings.center_spoken_sentence ? "center" : "nearest",
        inline: "nearest"
      });
    },
    [reader.settings.center_spoken_sentence]
  );

  const alignHighlightedSentence = useCallback(
    (behavior: ScrollBehavior, force = false) => {
      const idx = reader.highlighted_sentence_idx;
      if (idx === null || idx === undefined) {
        return;
      }
      if (!force && !reader.settings.auto_scroll_tts) {
        return;
      }
      const container = sentenceScrollRef.current;
      if (!container) {
        return;
      }
      if (!reader.text_only_mode && reader.pretty_kind === "markdown" && reader.reading_markdown_page) {
        const anchorIdx = resolvePrettyAnchorIdx(idx);
        if (anchorIdx !== null && anchorIdx !== undefined) {
          if (
            !force &&
            prettyLastAutoScrollPageRef.current === reader.current_page &&
            prettyLastAutoScrollAnchorRef.current === anchorIdx
          ) {
            return;
          }
          const anchor = getPrettyAnchorNode(anchorIdx);
          if (anchor) {
            scrollSentenceIntoView(
              container,
              anchor,
              reader.settings.center_spoken_sentence,
              behavior
            );
            prettyLastAutoScrollPageRef.current = reader.current_page;
            prettyLastAutoScrollAnchorRef.current = anchorIdx;
            return;
          }
        }
      }
      if (!reader.text_only_mode && reader.pretty_kind === "html" && reader.reading_html_page) {
        const anchorIdx = resolvePrettyAnchorIdx(idx);
        if (anchorIdx !== null && anchorIdx !== undefined) {
          if (
            !force &&
            prettyLastAutoScrollPageRef.current === reader.current_page &&
            prettyLastAutoScrollAnchorRef.current === anchorIdx
          ) {
            return;
          }
          const anchor = getPrettyAnchorNode(anchorIdx);
          if (anchor) {
            scrollNativeHtmlAnchorIntoView(anchor, behavior);
            prettyLastAutoScrollPageRef.current = reader.current_page;
            prettyLastAutoScrollAnchorRef.current = anchorIdx;
            return;
          }
        }
      }
      const sentence = sentenceRefs.current[idx];
      if (!sentence) {
        return;
      }
      scrollSentenceIntoView(
        container,
        sentence,
        reader.settings.center_spoken_sentence,
        behavior
      );
    },
    [
      getPrettyAnchorNode,
      reader.current_page,
      reader.highlighted_sentence_idx,
      reader.pretty_kind,
      reader.reading_html_page,
      reader.reading_markdown_page,
      reader.settings.auto_scroll_tts,
      reader.settings.center_spoken_sentence,
      reader.text_only_mode,
      resolvePrettyAnchorIdx,
      scrollNativeHtmlAnchorIntoView,
      sentenceRefs,
      sentenceScrollRef
    ]
  );

  const jumpToHighlightedSentence = useCallback(() => {
    alignHighlightedSentence("smooth", true);
  }, [alignHighlightedSentence]);

  useEffect(() => {
    const idx = reader.highlighted_sentence_idx;
    if (idx === null || idx === undefined || !reader.settings.auto_scroll_tts) {
      return;
    }
    if (pendingScrollFrameRef.current !== null) {
      cancelAnimationFrame(pendingScrollFrameRef.current);
    }
    pendingScrollFrameRef.current = requestAnimationFrame(() => {
      pendingScrollFrameRef.current = null;
      const behavior: ScrollBehavior = reader.tts.state === "playing" ? "auto" : "smooth";
      alignHighlightedSentence(behavior);
    });
    return () => {
      if (pendingScrollFrameRef.current !== null) {
        cancelAnimationFrame(pendingScrollFrameRef.current);
        pendingScrollFrameRef.current = null;
      }
    };
  }, [
    alignHighlightedSentence,
    reader.current_page,
    reader.highlighted_sentence_idx,
    reader.settings.auto_scroll_tts,
    reader.settings.center_spoken_sentence,
    reader.settings.font_size,
    reader.settings.letter_spacing,
    reader.settings.line_spacing,
    reader.settings.margin_horizontal,
    reader.settings.margin_vertical,
    reader.settings.word_spacing,
    reader.tts.state
  ]);

  useEffect(() => {
    if (!reader.settings.auto_scroll_tts) {
      return;
    }
    if (!reader.text_only_mode && reader.pretty_kind === "html") {
      return;
    }
    const container = sentenceScrollRef.current;
    if (!container) {
      return;
    }

    const realign = () => {
      requestAnimationFrame(() => {
        alignHighlightedSentence("auto");
      });
    };

    const resizeObserver = new ResizeObserver(() => {
      realign();
    });
    resizeObserver.observe(container);
    window.addEventListener("resize", realign);

    return () => {
      resizeObserver.disconnect();
      window.removeEventListener("resize", realign);
    };
  }, [
    alignHighlightedSentence,
    reader.pretty_kind,
    reader.sentences.length,
    reader.settings.auto_scroll_tts,
    reader.settings.font_size,
    reader.settings.letter_spacing,
    reader.settings.line_spacing,
    reader.settings.margin_horizontal,
    reader.settings.margin_vertical,
    reader.settings.word_spacing,
    reader.text_only_mode,
  ]);

  return {
    jumpToHighlightedSentence
  };
}
