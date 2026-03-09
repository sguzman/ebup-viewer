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
  const htmlIndexedAnchorsRef = useRef<Map<number, HTMLElement>>(new Map());
  const htmlSentenceAnchorCacheRef = useRef<{ key: string; map: number[] }>({ key: "", map: [] });
  const prettyAnchorLookupKeyRef = useRef<string>("");
  const globalSentenceStart = reader.stats.sentences_read_up_to_page_start;
  const canonicalSentences = useMemo(
    () => (
      reader.pretty_kind === "html" && reader.canonical_sentences.length > 0
        ? reader.canonical_sentences
        : reader.sentences
    ),
    [reader.canonical_sentences, reader.pretty_kind, reader.sentences]
  );

  const sentencesKey = useMemo(() => reader.sentences.join("\n"), [reader.sentences]);
  const canonicalSentencesKey = useMemo(
    () => canonicalSentences.join("\n"),
    [canonicalSentences]
  );
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
      htmlIndexedAnchorsRef.current.clear();
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
      const result = annotateNativeHtmlSentences(root as Document, canonicalSentences);
      htmlIndexedAnchorsRef.current = collectIndexedAnchors(root as Document, attribute);
      prettyAnchorElementsRef.current.html = new Map(result.firstAnchors);
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
    canonicalSentences,
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
      const result = annotateNativeHtmlSentences(doc, canonicalSentences);
      htmlIndexedAnchorsRef.current = collectIndexedAnchors(doc, "data-ll-html-anchor");
      prettyAnchorElementsRef.current.html = new Map(result.firstAnchors);
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
  }, [canonicalSentences, hasPrettyHtml, nativeHtmlFrameRef, nativeHtmlLoadVersion, prettyLookupKey]);

  useEffect(() => {
    if (!hasPrettyHtml || !renderedNativeHtml) {
      htmlSentenceAnchorCacheRef.current = { key: "", map: [] };
      return;
    }
    const cacheKey = [
      reader.source_path,
      renderedNativeHtml,
      canonicalSentencesKey,
      String(globalSentenceStart),
      sentenceAnchorHintKey
    ].join("\n");
    if (htmlSentenceAnchorCacheRef.current.key === cacheKey) {
      return;
    }
    const indexedAnchors = htmlIndexedAnchorsRef.current;
    const anchorElements = Array.from(indexedAnchors.values());
    if (canonicalSentences.length === 0) {
      htmlSentenceAnchorCacheRef.current = { key: cacheKey, map: [] };
      return;
    }
    if (anchorElements.length === 0) {
      htmlSentenceAnchorCacheRef.current = {
        key: cacheKey,
        map: canonicalSentences.map((_, idx) => idx)
      };
      return;
    }
    const hintAnchors = Array.from({ length: canonicalSentences.length }, () => null as number | null);
    for (let localIdx = 0; localIdx < reader.sentence_anchor_map.length; localIdx += 1) {
      const globalIdx = globalSentenceStart + localIdx;
      if (globalIdx >= 0 && globalIdx < hintAnchors.length) {
        hintAnchors[globalIdx] = reader.sentence_anchor_map[localIdx] ?? null;
      }
    }
    const startedAt = typeof performance !== "undefined" ? performance.now() : 0;
    const { map, diagnostics } = buildReaderHtmlSyncMap(
      anchorElements,
      canonicalSentences,
      hintAnchors
    );
    const mergedTargets = new Map<number, HTMLElement>();
    for (let sentenceIdx = 0; sentenceIdx < canonicalSentences.length; sentenceIdx += 1) {
      const exact = prettyAnchorElementsRef.current.htmlSentenceSpans.get(sentenceIdx)?.[0]
        ?? prettyAnchorElementsRef.current.html.get(sentenceIdx)
        ?? null;
      if (exact) {
        mergedTargets.set(sentenceIdx, exact);
        continue;
      }
      const anchorIdx = map[sentenceIdx];
      const mappedAnchor = anchorIdx === null || anchorIdx === undefined
        ? null
        : indexedAnchors.get(anchorIdx) ?? null;
      if (mappedAnchor) {
        mergedTargets.set(sentenceIdx, mappedAnchor);
      }
    }
    prettyAnchorElementsRef.current.html = mergedTargets;
    if (import.meta.env.DEV) {
      console.debug("reader pretty html sync map", {
        anchors: anchorElements.length,
        cappedLeaps: diagnostics.cappedLeaps,
        confidentMatches: diagnostics.confidentMatches,
        fallbackMatches: diagnostics.fallbackMatches,
        sentenceSpanMatches: prettyAnchorElementsRef.current.htmlSentenceSpans.size,
        sentences: canonicalSentences.length
      });
    }
    recordPerfMeasure("ReaderShell.buildHtmlSentenceAnchorMap", startedAt);
    htmlSentenceAnchorCacheRef.current = { key: cacheKey, map };
  }, [
    canonicalSentences,
    canonicalSentencesKey,
    globalSentenceStart,
    hasPrettyHtml,
    nativeHtmlLoadVersion,
    reader.source_path,
    reader.sentence_anchor_map,
    renderedNativeHtml,
    sentenceAnchorHintKey,
  ]);

  const resolvePrettyAnchorIdx = (idx: number): number | null => {
    if (reader.pretty_kind === "html") {
      const globalIdx = globalSentenceStart + idx;
      if (
        prettyAnchorElementsRef.current.htmlSentenceSpans.has(globalIdx)
        || prettyAnchorElementsRef.current.html.has(globalIdx)
      ) {
        return globalIdx;
      }
      const anchors = htmlSentenceAnchorCacheRef.current.map;
      const mappedGlobal = anchors[globalIdx] ?? null;
      if (mappedGlobal !== null && mappedGlobal !== undefined) {
        return globalIdx;
      }
      for (let offset = 1; offset < reader.sentences.length; offset += 1) {
        const prevGlobal = globalIdx - offset;
        const nextGlobal = globalIdx + offset;
        if (
          prevGlobal >= 0
          && (
            prettyAnchorElementsRef.current.htmlSentenceSpans.has(prevGlobal)
            || prettyAnchorElementsRef.current.html.has(prevGlobal)
          )
        ) {
          return prevGlobal;
        }
        if (
          nextGlobal < canonicalSentences.length
          && (
            prettyAnchorElementsRef.current.htmlSentenceSpans.has(nextGlobal)
            || prettyAnchorElementsRef.current.html.has(nextGlobal)
          )
        ) {
          return nextGlobal;
        }
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
