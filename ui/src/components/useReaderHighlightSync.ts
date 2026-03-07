import {
  useCallback,
  useEffect,
  useRef,
  useState,
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
  handleNativeHtmlFrameLoad: () => void;
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
  const prettyHighlightedHtmlNodesRef = useRef<HTMLElement[]>([]);
  const [nativeHtmlLoadVersion, setNativeHtmlLoadVersion] = useState(0);
  const { prettyAnchorElementsRef, resolvePrettyAnchorIdx } = useHtmlSentenceAnchorMap({
    hasPrettyHtml,
    hasPrettyMarkdown,
    nativeHtmlLoadVersion,
    nativeHtmlFrameRef,
    reader,
    renderedMarkdownHtml,
    renderedNativeHtml,
    sentenceScrollRef
  });

  const getPrettyAnchorNode = useCallback(
    (anchorIdx: number): HTMLElement | null => {
      if (reader.pretty_kind === "html") {
        return prettyAnchorElementsRef.current.html.get(anchorIdx) ?? null;
      }
      return prettyAnchorElementsRef.current.markdown.get(anchorIdx) ?? null;
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

  const handleNativeHtmlFrameLoad = useCallback(() => {
    setNativeHtmlLoadVersion((current) => current + 1);
  }, []);

  const applyPrettyHighlight = useCallback((): boolean => {
    if (reader.text_only_mode) {
      if (prettyHighlightedHtmlNodesRef.current.length > 0) {
        for (const node of prettyHighlightedHtmlNodesRef.current) {
          node.classList.remove("reader-pretty-highlight");
        }
        prettyHighlightedHtmlNodesRef.current = [];
      }
      if (prettyHighlightedNodeRef.current) {
        prettyHighlightedNodeRef.current.classList.remove("reader-pretty-highlight");
        prettyHighlightedNodeRef.current = null;
      }
      return false;
    }
    if (activePrettyAnchorIdx === null || activePrettyAnchorIdx === undefined) {
      if (prettyHighlightedHtmlNodesRef.current.length > 0) {
        for (const node of prettyHighlightedHtmlNodesRef.current) {
          node.classList.remove("reader-pretty-highlight");
        }
        prettyHighlightedHtmlNodesRef.current = [];
      }
      if (prettyHighlightedNodeRef.current) {
        prettyHighlightedNodeRef.current.classList.remove("reader-pretty-highlight");
        prettyHighlightedNodeRef.current = null;
      }
      return false;
    }
    if (reader.pretty_kind === "html") {
      const nodes = prettyAnchorElementsRef.current.htmlSentenceSpans.get(activePrettyAnchorIdx) ?? [];
      if (nodes.length > 0) {
        for (const node of prettyHighlightedHtmlNodesRef.current) {
          if (!nodes.includes(node)) {
            node.classList.remove("reader-pretty-highlight");
          }
        }
        for (const node of nodes) {
          node.classList.add("reader-pretty-highlight");
        }
        prettyHighlightedHtmlNodesRef.current = nodes;
        if (prettyHighlightedNodeRef.current && !nodes.includes(prettyHighlightedNodeRef.current)) {
          prettyHighlightedNodeRef.current.classList.remove("reader-pretty-highlight");
          prettyHighlightedNodeRef.current = null;
        }
        return true;
      }
      if (prettyHighlightedHtmlNodesRef.current.length > 0) {
        for (const node of prettyHighlightedHtmlNodesRef.current) {
          node.classList.remove("reader-pretty-highlight");
        }
        prettyHighlightedHtmlNodesRef.current = [];
      }
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
  }, [applyPrettyHighlight, nativeHtmlLoadVersion, reader.current_page]);

  return {
    handlePrettyContentClick,
    handleNativeHtmlFrameLoad,
    jumpToHighlightedSentence,
    nativeHtmlFrameRef
  };
}
