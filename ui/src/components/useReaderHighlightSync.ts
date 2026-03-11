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
  onSentenceClick: (sentenceIdx: number) => Promise<void>;
  onSetPage: (page: number) => Promise<void>;
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
  prettyHtmlContextMenu: {
    open: boolean;
    x: number;
    y: number;
    canJump: boolean;
  };
  closePrettyHtmlContextMenu: () => void;
  jumpToPrettyHtmlContextTarget: () => Promise<void>;
}

export function useReaderHighlightSync({
  hasPrettyHtml,
  hasPrettyMarkdown,
  onSentenceClick,
  onSetPage,
  reader,
  renderedMarkdownHtml,
  renderedNativeHtml,
  sentenceRefs,
  sentenceScrollRef
}: UseReaderHighlightSyncArgs): ReaderHighlightSyncState {
  const nativeHtmlFrameRef = useRef<HTMLIFrameElement | null>(null);
  const prettyHighlightedNodeRef = useRef<HTMLElement | null>(null);
  const prettyHighlightedHtmlNodesRef = useRef<HTMLElement[]>([]);
  const contextTargetSentenceIdxRef = useRef<number | null>(null);
  const [nativeHtmlLoadVersion, setNativeHtmlLoadVersion] = useState(0);
  const [prettyHtmlContextMenu, setPrettyHtmlContextMenu] = useState({
    open: false,
    x: 0,
    y: 0,
    canJump: false
  });
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

  const jumpToGlobalSentence = useCallback(async (globalSentenceIdx: number): Promise<void> => {
    if (!Number.isFinite(globalSentenceIdx) || globalSentenceIdx < 0) {
      return;
    }
    let pageStart = 0;
    for (let pageIdx = 0; pageIdx < reader.page_sentence_counts.length; pageIdx += 1) {
      const count = reader.page_sentence_counts[pageIdx] ?? 0;
      const pageEnd = pageStart + count;
      if (globalSentenceIdx < pageEnd) {
        const localSentenceIdx = globalSentenceIdx - pageStart;
        if (localSentenceIdx < 0 || localSentenceIdx >= count) {
          return;
        }
        if (pageIdx !== reader.current_page) {
          await onSetPage(pageIdx);
        }
        await onSentenceClick(localSentenceIdx);
        return;
      }
      pageStart = pageEnd;
    }
  }, [onSentenceClick, onSetPage, reader.current_page, reader.page_sentence_counts]);

  const resolveHtmlGlobalSentenceIdxFromTarget = useCallback((target: HTMLElement): number | null => {
    const sentenceElement = target.closest("[data-ll-html-sentence]");
    if (sentenceElement) {
      const raw = sentenceElement.getAttribute("data-ll-html-sentence");
      const parsed = raw === null ? Number.NaN : Number.parseInt(raw, 10);
      return Number.isFinite(parsed) ? parsed : null;
    }
    for (const [sentenceIdx, spans] of prettyAnchorElementsRef.current.htmlSentenceSpans.entries()) {
      if (spans.some((span) => span === target || span.contains(target))) {
        return sentenceIdx;
      }
    }
    for (const [sentenceIdx, element] of prettyAnchorElementsRef.current.html.entries()) {
      if (element === target || element.contains(target)) {
        return sentenceIdx;
      }
    }
    const anchorElement = target.closest("[data-ll-html-anchor]");
    if (!anchorElement) {
      return null;
    }
    for (const [sentenceIdx, element] of prettyAnchorElementsRef.current.html.entries()) {
      if (element === anchorElement || element.contains(anchorElement) || anchorElement.contains(element)) {
        return sentenceIdx;
      }
    }
    return null;
  }, [prettyAnchorElementsRef]);

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

  const closePrettyHtmlContextMenu = useCallback(() => {
    contextTargetSentenceIdxRef.current = null;
    setPrettyHtmlContextMenu((current) => (
      current.open
        ? { ...current, open: false, canJump: false }
        : current
    ));
  }, []);

  const jumpToPrettyHtmlContextTarget = useCallback(async (): Promise<void> => {
    const sentenceIdx = contextTargetSentenceIdxRef.current;
    closePrettyHtmlContextMenu();
    if (sentenceIdx === null || sentenceIdx === undefined) {
      return;
    }
    await jumpToGlobalSentence(sentenceIdx);
  }, [closePrettyHtmlContextMenu, jumpToGlobalSentence]);

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

  useEffect(() => {
    if (!hasPrettyHtml) {
      return;
    }
    const doc = nativeHtmlFrameRef.current?.contentDocument;
    const frame = nativeHtmlFrameRef.current;
    if (!doc) {
      return;
    }
    const handleContextMenu = (event: globalThis.MouseEvent): void => {
      const target = event.target as HTMLElement | null;
      if (!target) {
        return;
      }
      const globalSentenceIdx = resolveHtmlGlobalSentenceIdxFromTarget(target);
      event.preventDefault();
      const frameRect = frame?.getBoundingClientRect();
      const offsetX = frameRect ? frameRect.left : 0;
      const offsetY = frameRect ? frameRect.top : 0;
      contextTargetSentenceIdxRef.current = globalSentenceIdx;
      setPrettyHtmlContextMenu({
        open: true,
        x: Math.round(offsetX + event.clientX),
        y: Math.round(offsetY + event.clientY),
        canJump: globalSentenceIdx !== null && globalSentenceIdx !== undefined
      });
    };
    const handlePointerDown = (): void => {
      if (prettyHtmlContextMenu.open) {
        closePrettyHtmlContextMenu();
      }
    };
    doc.addEventListener("contextmenu", handleContextMenu);
    doc.addEventListener("pointerdown", handlePointerDown);
    return () => {
      doc.removeEventListener("contextmenu", handleContextMenu);
      doc.removeEventListener("pointerdown", handlePointerDown);
    };
  }, [
    closePrettyHtmlContextMenu,
    hasPrettyHtml,
    nativeHtmlLoadVersion,
    prettyHtmlContextMenu.open,
    resolveHtmlGlobalSentenceIdxFromTarget
  ]);

  return {
    closePrettyHtmlContextMenu,
    handlePrettyContentClick,
    handleNativeHtmlFrameLoad,
    jumpToHighlightedSentence,
    jumpToPrettyHtmlContextTarget,
    nativeHtmlFrameRef,
    prettyHtmlContextMenu
  };
}
