import {
  useCallback,
  useEffect,
  useRef,
  type MouseEvent,
  type MutableRefObject
} from "react";

import type { ReaderSnapshot } from "../types";
import { scrollSentenceIntoView } from "./readerDom";
import { useHtmlSentenceAnchorMap } from "./useHtmlSentenceAnchorMap";
import { useReaderScrollSync } from "./useReaderScrollSync";

interface UseReaderHighlightSyncArgs {
  hasPrettyHtml: boolean;
  hasPrettyMarkdown: boolean;
  reader: ReaderSnapshot;
  renderedMarkdownHtml: string;
  renderedNativeHtml: string;
  sentenceRefs: MutableRefObject<Record<number, HTMLButtonElement | null>>;
  sentenceScrollRef: MutableRefObject<HTMLDivElement | null>;
}

export interface ReaderHighlightSyncState {
  handlePrettyContentClick: (event: MouseEvent<HTMLDivElement>) => void;
  jumpToHighlightedSentence: () => void;
  nativeHtmlFrameRef: MutableRefObject<HTMLIFrameElement | null>;
}

export function useReaderHighlightSync({
  hasPrettyHtml,
  hasPrettyMarkdown,
  reader,
  renderedMarkdownHtml,
  renderedNativeHtml,
  sentenceRefs,
  sentenceScrollRef
}: UseReaderHighlightSyncArgs): ReaderHighlightSyncState {
  const nativeHtmlFrameRef = useRef<HTMLIFrameElement | null>(null);
  const prettyHighlightedNodeRef = useRef<HTMLElement | null>(null);
  const { prettyAnchorElementsRef, resolvePrettyAnchorIdx } = useHtmlSentenceAnchorMap({
    hasPrettyHtml,
    hasPrettyMarkdown,
    nativeHtmlFrameRef,
    reader,
    renderedMarkdownHtml,
    renderedNativeHtml,
    sentenceScrollRef
  });

  const getPrettyAnchorNode = useCallback(
    (anchorIdx: number): HTMLElement | null => {
      const key = reader.pretty_kind === "html" ? "html" : "markdown";
      return prettyAnchorElementsRef.current[key].get(anchorIdx) ?? null;
    },
    [reader.pretty_kind]
  );

  const activePrettyAnchorIdx = (() => {
    const idx = reader.highlighted_sentence_idx;
    if (idx === null || idx === undefined) {
      return null;
    }
    return resolvePrettyAnchorIdx(idx);
  })();

  const { jumpToHighlightedSentence } = useReaderScrollSync({
    getPrettyAnchorNode,
    reader,
    resolvePrettyAnchorIdx,
    sentenceRefs,
    sentenceScrollRef
  });

  const handlePrettyContentClick = useCallback((event: MouseEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement | null;
    const anchor = target?.closest("a");
    if (!anchor) {
      return;
    }
    const href = (anchor.getAttribute("href") ?? "").trim();
    if (!href.startsWith("#")) {
      return;
    }
    event.preventDefault();
    const container = sentenceScrollRef.current;
    if (!container) {
      return;
    }
    const id = href.slice(1);
    if (!id) {
      return;
    }
    const escapedId =
      typeof CSS !== "undefined" && typeof CSS.escape === "function" ? CSS.escape(id) : id;
    const element = container.querySelector(`#${escapedId}`) as HTMLElement | null;
    if (!element) {
      return;
    }
    scrollSentenceIntoView(container, element, false, "smooth");
  }, [sentenceScrollRef]);

  const applyPrettyHighlight = useCallback((): boolean => {
    if (reader.text_only_mode) {
      if (prettyHighlightedNodeRef.current) {
        prettyHighlightedNodeRef.current.classList.remove("reader-pretty-highlight");
        prettyHighlightedNodeRef.current = null;
      }
      return false;
    }
    if (activePrettyAnchorIdx === null || activePrettyAnchorIdx === undefined) {
      if (prettyHighlightedNodeRef.current) {
        prettyHighlightedNodeRef.current.classList.remove("reader-pretty-highlight");
        prettyHighlightedNodeRef.current = null;
      }
      return false;
    }
    const target = getPrettyAnchorNode(activePrettyAnchorIdx);
    if (!target) {
      return false;
    }
    if (prettyHighlightedNodeRef.current && prettyHighlightedNodeRef.current !== target) {
      prettyHighlightedNodeRef.current.classList.remove("reader-pretty-highlight");
    }
    target.classList.add("reader-pretty-highlight");
    prettyHighlightedNodeRef.current = target;
    return true;
  }, [activePrettyAnchorIdx, getPrettyAnchorNode, reader.text_only_mode]);

  useEffect(() => {
    const frame = requestAnimationFrame(() => {
      applyPrettyHighlight();
    });
    return () => cancelAnimationFrame(frame);
  }, [applyPrettyHighlight, reader.current_page]);

  return {
    handlePrettyContentClick,
    jumpToHighlightedSentence,
    nativeHtmlFrameRef
  };
}
