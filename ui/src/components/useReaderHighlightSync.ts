import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  type MouseEvent,
  type MutableRefObject
} from "react";

import { recordPerfMeasure } from "../perf/debug";
import type { ReaderSnapshot } from "../types";
import { scrollSentenceIntoView } from "./readerDom";
import { buildReaderHtmlSyncMap, collectIndexedAnchors } from "./readerHtmlSync";

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
  const prettyAnchorElementsRef = useRef<{
    html: Map<number, HTMLElement>;
    markdown: Map<number, HTMLElement>;
  }>({ html: new Map(), markdown: new Map() });
  const htmlSentenceAnchorCacheRef = useRef<{ key: string; map: number[] }>({ key: "", map: [] });
  const prettyAnchorLookupKeyRef = useRef<string>("");
  const prettyHighlightedNodeRef = useRef<HTMLElement | null>(null);
  const prettyLastAutoScrollAnchorRef = useRef<number | null>(null);
  const prettyLastAutoScrollPageRef = useRef<number | null>(null);
  const pendingScrollFrameRef = useRef<number | null>(null);

  const sentencesKey = useMemo(() => reader.sentences.join("\n"), [reader.sentences]);
  const sentenceAnchorHintKey = useMemo(
    () => reader.sentence_anchor_map.map((value) => (value ?? "null")).join(","),
    [reader.sentence_anchor_map]
  );

  const resolvePrettyAnchorIdx = useCallback(
    (idx: number): number | null => {
      if (reader.pretty_kind === "html") {
        const anchors = htmlSentenceAnchorCacheRef.current.map;
        let anchorIdx = anchors[idx] ?? null;
        if (anchorIdx === null || anchorIdx === undefined) {
          for (let offset = 1; offset < anchors.length; offset += 1) {
            const prev = idx - offset;
            const next = idx + offset;
            if (prev >= 0 && anchors[prev] !== null && anchors[prev] !== undefined) {
              anchorIdx = anchors[prev];
              break;
            }
            if (next < anchors.length && anchors[next] !== null && anchors[next] !== undefined) {
              anchorIdx = anchors[next];
              break;
            }
          }
        }
        if (anchorIdx !== null && anchorIdx !== undefined) {
          return anchorIdx;
        }
      }
      const anchors = reader.sentence_anchor_map;
      let anchorIdx = anchors[idx] ?? null;
      if (anchorIdx === null || anchorIdx === undefined) {
        for (let offset = 1; offset < anchors.length; offset += 1) {
          const prev = idx - offset;
          const next = idx + offset;
          if (prev >= 0 && anchors[prev] !== null && anchors[prev] !== undefined) {
            anchorIdx = anchors[prev];
            break;
          }
          if (next < anchors.length && anchors[next] !== null && anchors[next] !== undefined) {
            anchorIdx = anchors[next];
            break;
          }
        }
      }
      return anchorIdx ?? null;
    },
    [reader.pretty_kind, reader.sentence_anchor_map]
  );

  const getPrettyAnchorNode = useCallback(
    (anchorIdx: number): HTMLElement | null => {
      const key = reader.pretty_kind === "html" ? "html" : "markdown";
      return prettyAnchorElementsRef.current[key].get(anchorIdx) ?? null;
    },
    [reader.pretty_kind]
  );

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

  const activePrettyAnchorIdx = useMemo(() => {
    const idx = reader.highlighted_sentence_idx;
    if (idx === null || idx === undefined) {
      return null;
    }
    return resolvePrettyAnchorIdx(idx);
  }, [reader.highlighted_sentence_idx, resolvePrettyAnchorIdx]);

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

  const prettyLookupKey = useMemo(() => {
    if (hasPrettyHtml) {
      return `html:${reader.source_path}:${reader.current_page}:${renderedNativeHtml}`;
    }
    if (hasPrettyMarkdown) {
      return `markdown:${reader.source_path}:${reader.current_page}:${renderedMarkdownHtml}`;
    }
    return "";
  }, [
    hasPrettyHtml,
    hasPrettyMarkdown,
    reader.current_page,
    reader.source_path,
    renderedMarkdownHtml,
    renderedNativeHtml
  ]);

  useEffect(() => {
    if (!prettyLookupKey) {
      prettyAnchorElementsRef.current.html.clear();
      prettyAnchorElementsRef.current.markdown.clear();
      prettyAnchorLookupKeyRef.current = "";
      return;
    }
    if (prettyAnchorLookupKeyRef.current === prettyLookupKey) {
      return;
    }
    const kind = hasPrettyHtml ? "html" : hasPrettyMarkdown ? "markdown" : null;
    if (!kind) {
      return;
    }
    const attribute = kind === "html" ? "data-ll-html-anchor" : "data-ll-md-anchor";
    const root =
      kind === "html"
        ? nativeHtmlFrameRef.current?.contentDocument
        : sentenceScrollRef.current;
    if (!root) {
      return;
    }
    prettyAnchorElementsRef.current[kind] = collectIndexedAnchors(root, attribute);
    if (kind === "html") {
      prettyAnchorElementsRef.current.markdown.clear();
    } else {
      prettyAnchorElementsRef.current.html.clear();
    }
    prettyAnchorLookupKeyRef.current = prettyLookupKey;
  }, [hasPrettyHtml, hasPrettyMarkdown, prettyLookupKey, sentenceScrollRef]);

  useEffect(() => {
    if (!hasPrettyHtml) {
      return;
    }
    const frame = nativeHtmlFrameRef.current;
    const doc = frame?.contentDocument;
    if (!frame || !doc) {
      return;
    }

    const rebuildHtmlAnchors = (): void => {
      prettyAnchorElementsRef.current.html = collectIndexedAnchors(doc, "data-ll-html-anchor");
      prettyAnchorLookupKeyRef.current = prettyLookupKey;
    };

    const handleLoad = (): void => {
      rebuildHtmlAnchors();
    };

    const handleClick = (event: globalThis.MouseEvent): void => {
      const target = event.target as HTMLElement | null;
      const anchor = target?.closest("a");
      if (!anchor) {
        return;
      }
      const href = (anchor.getAttribute("href") ?? "").trim();
      if (!href || href.startsWith("#")) {
        return;
      }
      event.preventDefault();
      window.open(href, "_blank", "noopener,noreferrer");
    };

    frame.addEventListener("load", handleLoad);
    doc.addEventListener("click", handleClick);
    rebuildHtmlAnchors();

    return () => {
      frame.removeEventListener("load", handleLoad);
      doc.removeEventListener("click", handleClick);
    };
  }, [hasPrettyHtml, prettyLookupKey]);

  useEffect(() => {
    if (!hasPrettyHtml || !renderedNativeHtml) {
      htmlSentenceAnchorCacheRef.current = { key: "", map: [] };
      return;
    }
    const cacheKey = [
      reader.source_path,
      reader.current_page,
      renderedNativeHtml,
      sentencesKey,
      sentenceAnchorHintKey
    ].join("\n");
    if (htmlSentenceAnchorCacheRef.current.key === cacheKey) {
      return;
    }
    const anchors = Array.from(prettyAnchorElementsRef.current.html.values());
    if (anchors.length === 0 || reader.sentences.length === 0) {
      htmlSentenceAnchorCacheRef.current = { key: cacheKey, map: [] };
      return;
    }
    const startedAt = typeof performance !== "undefined" ? performance.now() : 0;
    const { map, diagnostics } = buildReaderHtmlSyncMap(
      anchors,
      reader.sentences,
      reader.sentence_anchor_map
    );
    if (import.meta.env.DEV) {
      console.debug("reader pretty html sync map", {
        anchors: anchors.length,
        cappedLeaps: diagnostics.cappedLeaps,
        confidentMatches: diagnostics.confidentMatches,
        fallbackMatches: diagnostics.fallbackMatches,
        sentences: reader.sentences.length
      });
    }
    recordPerfMeasure("ReaderShell.buildHtmlSentenceAnchorMap", startedAt);
    htmlSentenceAnchorCacheRef.current = { key: cacheKey, map };
  }, [
    hasPrettyHtml,
    reader.current_page,
    reader.sentences,
    reader.source_path,
    reader.sentence_anchor_map,
    renderedNativeHtml,
    sentenceAnchorHintKey,
    sentencesKey
  ]);

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
