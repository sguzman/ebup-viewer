import { useEffect, useMemo, useRef, type MutableRefObject } from "react";

import { recordPerfMeasure } from "../perf/debug";
import type { ReaderSnapshot } from "../types";
import { annotateNativeHtmlSentences } from "./nativeHtmlSentenceAnchors";
import { buildReaderHtmlSyncMap, collectIndexedAnchors } from "./readerHtmlSync";

interface UseHtmlSentenceAnchorMapArgs {
  hasPrettyHtml: boolean;
  hasPrettyMarkdown: boolean;
  nativeHtmlLoadVersion: number;
  nativeHtmlFrameRef: MutableRefObject<HTMLIFrameElement | null>;
  reader: ReaderSnapshot;
  renderedMarkdownHtml: string;
  renderedNativeHtml: string;
  sentenceScrollRef: MutableRefObject<HTMLDivElement | null>;
}

export interface HtmlSentenceAnchorMapState {
  prettyAnchorElementsRef: MutableRefObject<{
    html: Map<number, HTMLElement>;
    htmlSentenceSpans: Map<number, HTMLElement[]>;
    markdown: Map<number, HTMLElement>;
  }>;
  resolvePrettyAnchorIdx: (idx: number) => number | null;
}

export function useHtmlSentenceAnchorMap({
  hasPrettyHtml,
  hasPrettyMarkdown,
  nativeHtmlLoadVersion,
  nativeHtmlFrameRef,
  reader,
  renderedMarkdownHtml,
  renderedNativeHtml,
  sentenceScrollRef
}: UseHtmlSentenceAnchorMapArgs): HtmlSentenceAnchorMapState {
  const prettyAnchorElementsRef = useRef<{
    html: Map<number, HTMLElement>;
    htmlSentenceSpans: Map<number, HTMLElement[]>;
    markdown: Map<number, HTMLElement>;
  }>({ html: new Map(), htmlSentenceSpans: new Map(), markdown: new Map() });
  const htmlSentenceAnchorCacheRef = useRef<{ key: string; map: number[] }>({ key: "", map: [] });
  const prettyAnchorLookupKeyRef = useRef<string>("");

  const sentencesKey = useMemo(() => reader.sentences.join("\n"), [reader.sentences]);
  const sentenceAnchorHintKey = useMemo(
    () => reader.sentence_anchor_map.map((value) => (value ?? "null")).join(","),
    [reader.sentence_anchor_map]
  );

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
      prettyAnchorElementsRef.current.htmlSentenceSpans.clear();
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
    if (kind === "html") {
      const result = annotateNativeHtmlSentences(root as Document, reader.sentences);
      prettyAnchorElementsRef.current.html = result.firstAnchors;
      prettyAnchorElementsRef.current.htmlSentenceSpans = result.sentenceAnchors;
      prettyAnchorElementsRef.current.markdown.clear();
      if (import.meta.env.DEV) {
        console.debug("reader native html sentence anchors", result.diagnostics);
      }
    } else {
      prettyAnchorElementsRef.current[kind] = collectIndexedAnchors(root, attribute);
      prettyAnchorElementsRef.current.html.clear();
      prettyAnchorElementsRef.current.htmlSentenceSpans.clear();
    }
    prettyAnchorLookupKeyRef.current = prettyLookupKey;
  }, [
    hasPrettyHtml,
    hasPrettyMarkdown,
    nativeHtmlFrameRef,
    nativeHtmlLoadVersion,
    prettyLookupKey,
    reader.sentences,
    sentenceScrollRef
  ]);

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
      const result = annotateNativeHtmlSentences(doc, reader.sentences);
      prettyAnchorElementsRef.current.html = result.firstAnchors;
      prettyAnchorElementsRef.current.htmlSentenceSpans = result.sentenceAnchors;
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
  }, [hasPrettyHtml, nativeHtmlFrameRef, nativeHtmlLoadVersion, prettyLookupKey, reader.sentences]);

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
    if (reader.sentences.length === 0) {
      htmlSentenceAnchorCacheRef.current = { key: cacheKey, map: [] };
      return;
    }
    if (anchors.length === 0) {
      htmlSentenceAnchorCacheRef.current = {
        key: cacheKey,
        map: reader.sentences.map((_, idx) => idx)
      };
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
        sentenceSpanMatches: prettyAnchorElementsRef.current.htmlSentenceSpans.size,
        sentences: reader.sentences.length
      });
    }
    recordPerfMeasure("ReaderShell.buildHtmlSentenceAnchorMap", startedAt);
    htmlSentenceAnchorCacheRef.current = { key: cacheKey, map };
  }, [
    hasPrettyHtml,
    nativeHtmlLoadVersion,
    reader.current_page,
    reader.sentences,
    reader.source_path,
    reader.sentence_anchor_map,
    renderedNativeHtml,
    sentenceAnchorHintKey,
    sentencesKey
  ]);

  const resolvePrettyAnchorIdx = (idx: number): number | null => {
    if (reader.pretty_kind === "html") {
      if (prettyAnchorElementsRef.current.htmlSentenceSpans.has(idx)) {
        return idx;
      }
      const anchors = htmlSentenceAnchorCacheRef.current.map;
      const exactAnchorIdx = anchors[idx] ?? null;
      if (exactAnchorIdx !== null && exactAnchorIdx !== undefined) {
        return exactAnchorIdx;
      }
      for (let offset = 1; offset < reader.sentences.length; offset += 1) {
        const prev = idx - offset;
        const next = idx + offset;
        if (prev >= 0 && prettyAnchorElementsRef.current.htmlSentenceSpans.has(prev)) {
          return prev;
        }
        if (next < reader.sentences.length && prettyAnchorElementsRef.current.htmlSentenceSpans.has(next)) {
          return next;
        }
      }
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
  };

  return {
    prettyAnchorElementsRef,
    resolvePrettyAnchorIdx
  };
}
