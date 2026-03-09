import { Button, ButtonGroup, CircularProgress, Stack, Typography } from "@mui/material";
import type { PDFDocumentProxy } from "pdfjs-dist/legacy/build/pdf.mjs";
import { forwardRef, useCallback, useEffect, useImperativeHandle, useRef, useState } from "react";

import { backendApi, type PdfSentenceLocation } from "../api/tauri";
import type { ReaderSnapshot } from "../types";
import { recordPerfMeasure } from "../perf/debug";
import { clamp, normalizeNumber } from "./readerShared";
import {
  buildPdfSentenceSpanMap,
  findNearestSentenceForPageIndex,
  findNearestSentenceForSpanIndex,
  type PdfSentenceMatch,
  type PdfTextSpan
} from "./pdfTextSync";
import { applyPdfHighlightDom } from "./pdfHighlightDom";
import { orderPdfTextLayerSpans } from "./pdfTextLayer";
import { normalizeSyncText } from "./htmlSync";
let pdfJsImportPromise: Promise<typeof import("pdfjs-dist/legacy/build/pdf.mjs")> | null = null;
let pdfJsWorkerImportPromise: Promise<typeof import("pdfjs-dist/legacy/build/pdf.worker.mjs")> | null = null;
type PdfJsModule = typeof import("pdfjs-dist/legacy/build/pdf.mjs");

function ensurePromiseWithResolvers(): void {
  const promiseCtor = Promise as PromiseConstructor & {
    withResolvers?: <T>() => {
      promise: Promise<T>;
      resolve: (value: T | PromiseLike<T>) => void;
      reject: (reason?: unknown) => void;
    };
  };
  if (typeof promiseCtor.withResolvers === "function") {
    return;
  }
  promiseCtor.withResolvers = function withResolvers<T>() {
    let resolve!: (value: T | PromiseLike<T>) => void;
    let reject!: (reason?: unknown) => void;
    const promise = new Promise<T>((res, rej) => {
      resolve = res;
      reject = rej;
    });
    return { promise, resolve, reject };
  };
}

function ensureReadableStreamAsyncIterator(): void {
  const readableStreamCtor = globalThis.ReadableStream as
    | (typeof ReadableStream & {
        prototype: ReadableStream<unknown> & {
          [Symbol.asyncIterator]?: () => AsyncIterableIterator<unknown>;
        };
      })
    | undefined;
  if (!readableStreamCtor?.prototype || typeof readableStreamCtor.prototype[Symbol.asyncIterator] === "function") {
    return;
  }
  Object.defineProperty(readableStreamCtor.prototype, Symbol.asyncIterator, {
    configurable: true,
    writable: true,
    value: async function* readableStreamAsyncIterator(this: ReadableStream<unknown>) {
      const reader = this.getReader();
      try {
        while (true) {
          const { done, value } = await reader.read();
          if (done) {
            return;
          }
          yield value;
        }
      } finally {
        reader.releaseLock();
      }
    }
  });
}

async function importPdfJsBrowserSafe(): Promise<typeof import("pdfjs-dist/legacy/build/pdf.mjs")> {
  if (pdfJsImportPromise) {
    return pdfJsImportPromise;
  }
  const globalScope = globalThis as typeof globalThis & { process?: unknown };
  const originalProcess = globalScope.process;
  const hadOwnProcess = Object.prototype.hasOwnProperty.call(globalThis, "process");
  try {
    if (hadOwnProcess) {
      Reflect.deleteProperty(globalScope, "process");
    } else {
      Object.defineProperty(globalThis, "process", {
        configurable: true,
        enumerable: false,
        value: undefined,
        writable: true
      });
    }
    pdfJsImportPromise = import("pdfjs-dist/legacy/build/pdf.mjs");
    return await pdfJsImportPromise;
  } finally {
    if (hadOwnProcess) {
      globalScope.process = originalProcess;
    } else {
      Reflect.deleteProperty(globalScope, "process");
    }
  }
}

async function importPdfJsWorkerBrowserSafe(): Promise<typeof import("pdfjs-dist/legacy/build/pdf.worker.mjs")> {
  if (pdfJsWorkerImportPromise) {
    return pdfJsWorkerImportPromise;
  }
  const globalScope = globalThis as typeof globalThis & { process?: unknown };
  const originalProcess = globalScope.process;
  const hadOwnProcess = Object.prototype.hasOwnProperty.call(globalThis, "process");
  try {
    if (hadOwnProcess) {
      Reflect.deleteProperty(globalScope, "process");
    } else {
      Object.defineProperty(globalThis, "process", {
        configurable: true,
        enumerable: false,
        value: undefined,
        writable: true
      });
    }
    pdfJsWorkerImportPromise = import("pdfjs-dist/legacy/build/pdf.worker.mjs");
    return await pdfJsWorkerImportPromise;
  } finally {
    if (hadOwnProcess) {
      globalScope.process = originalProcess;
    } else {
      Reflect.deleteProperty(globalScope, "process");
    }
  }
}

async function ensurePdfJsFakeWorkerGlobal(): Promise<void> {
  const globalScope = globalThis as typeof globalThis & {
    pdfjsWorker?: { WorkerMessageHandler?: unknown };
  };
  if (globalScope.pdfjsWorker?.WorkerMessageHandler) {
    return;
  }
  const workerModule = await importPdfJsWorkerBrowserSafe();
  globalScope.pdfjsWorker = workerModule as unknown as { WorkerMessageHandler?: unknown };
}

interface ReaderPrettyPdfPaneProps {
  onSentenceClick: (idx: number) => Promise<void>;
  reader: ReaderSnapshot;
  sourcePath: string;
}

export interface ReaderPrettyPdfPaneHandle {
  jumpToHighlightedSentence: () => void;
}

interface RenderedPdfPage {
  container: HTMLDivElement;
  pageIndex: number;
  spans: PdfTextSpan[];
}

interface CachedPdfMatchResult {
  key: string;
  matches: PdfSentenceMatch[];
  summary: {
    exact: number;
    fallback: number;
    pageOnly: number;
    missing: number;
    cappedLeaps: number;
  };
}

interface CachedPdfLocationMatchSet {
  matches: PdfSentenceMatch[];
  summary: {
    exact: number;
    fallback: number;
    pageOnly: number;
    missing: number;
  };
  locations: Array<PdfSentenceLocation | null>;
}

interface PdfShellMetric {
  width: number;
  height: number;
}

function flattenRenderedPdfSpans(pages: RenderedPdfPage[]): PdfTextSpan[] {
  return pages.flatMap((page) => page.spans);
}

function setRenderedPdfSpanIndexes(pages: RenderedPdfPage[]): void {
  let spanIndex = 0;
  for (const page of pages) {
    for (const span of page.spans) {
      span.element.setAttribute("data-ll-pdf-span-idx", String(spanIndex));
      spanIndex += 1;
    }
  }
}

function globalSentenceStartForReader(reader: ReaderSnapshot): number {
  return reader.stats.sentences_read_up_to_page_start;
}

function resolveLikelyRenderedPages(
  pages: RenderedPdfPage[],
  locations: PdfSentenceLocation[] | null,
  reader: ReaderSnapshot
): RenderedPdfPage[] {
  if (pages.length === 0) {
    return [];
  }

  const globalSentenceStart = globalSentenceStartForReader(reader);
  const globalSentenceEnd = globalSentenceStart + reader.sentences.length;
  const cachedPageIndexes = (locations ?? [])
    .filter((location) => (
      location.sentence_idx >= globalSentenceStart
      && location.sentence_idx < globalSentenceEnd
      && location.page_idx !== null
      && location.page_idx !== undefined
    ))
    .map((location) => location.page_idx as number);

  let minPageIndex: number;
  let maxPageIndex: number;
  if (cachedPageIndexes.length > 0) {
    minPageIndex = Math.max(0, Math.min(...cachedPageIndexes) - 1);
    maxPageIndex = Math.max(...cachedPageIndexes) + 1;
  } else {
    const totalPages = pages.length;
    const startPct = clamp(reader.stats.page_start_percent, 0, 100) / 100;
    const endPct = clamp(reader.stats.page_end_percent, 0, 100) / 100;
    const estimatedStart = clamp(Math.floor(startPct * (totalPages - 1)), 0, totalPages - 1);
    const estimatedEnd = clamp(Math.ceil(endPct * (totalPages - 1)), estimatedStart, totalPages - 1);
    minPageIndex = Math.max(0, estimatedStart - 1);
    maxPageIndex = Math.min(totalPages - 1, estimatedEnd + 1);
  }

  const filtered = pages.filter((page) => page.pageIndex >= minPageIndex && page.pageIndex <= maxPageIndex);
  return filtered.length > 0 ? filtered : pages;
}

function estimatePdfTargetPage(
  locations: PdfSentenceLocation[] | null,
  reader: ReaderSnapshot,
  totalPages: number,
  pageTexts?: Map<number, string>
): number {
  if (totalPages <= 1) {
    return 0;
  }
  const globalSentenceStart = globalSentenceStartForReader(reader);
  const globalSentenceIdx = globalSentenceStart + (reader.highlighted_sentence_idx ?? 0);
  const direct = locations?.find((location) => location.sentence_idx === globalSentenceIdx);
  if (direct?.page_idx !== null && direct?.page_idx !== undefined) {
    return clamp(direct.page_idx, 0, totalPages - 1);
  }
  const normalizedQuery = normalizeSyncText(reader.page_text || reader.sentences.join(" "));
  if (pageTexts && normalizedQuery.length > 24) {
    const queryTokens = normalizedQuery.split(" ").filter((token) => token.length > 2);
    let bestPage = -1;
    let bestScore = 0;
    for (const [pageIndex, pageText] of pageTexts.entries()) {
      if (!pageText) {
        continue;
      }
      let hits = 0;
      for (const token of queryTokens) {
        if (pageText.includes(token)) {
          hits += 1;
        }
      }
      const score = queryTokens.length === 0 ? 0 : hits / queryTokens.length;
      if (score > bestScore) {
        bestScore = score;
        bestPage = pageIndex;
      }
    }
    if (bestPage >= 0 && bestScore >= 0.42) {
      return clamp(bestPage, 0, totalPages - 1);
    }
  }
  const startPct = clamp(reader.stats.page_start_percent, 0, 100) / 100;
  return clamp(Math.round(startPct * Math.max(0, totalPages - 1)), 0, totalPages - 1);
}

function pageIndexesAround(pageIndex: number, totalPages: number, radius: number): number[] {
  const indexes = new Set<number>();
  for (let delta = -radius; delta <= radius; delta += 1) {
    indexes.add(clamp(pageIndex + delta, 0, Math.max(0, totalPages - 1)));
  }
  return Array.from(indexes).sort((left, right) => left - right);
}

function extractNormalizedPdfPageText(
  textContent: Awaited<ReturnType<PDFDocumentProxy["getPage"]>> extends infer T
    ? T extends { getTextContent: (...args: never[]) => Promise<infer U> }
      ? U
      : never
    : never
): string {
  const items = Array.isArray((textContent as { items?: unknown[] }).items)
    ? (textContent as { items: Array<{ str?: string }> }).items
    : [];
  const raw = items
    .map((item) => item.str ?? "")
    .filter((value) => value.trim().length > 0)
    .join(" ");
  return normalizeSyncText(raw);
}

function logPdfDebug(event: string, payload: Record<string, unknown>): void {
  if (!import.meta.env.DEV) {
    return;
  }
  console.debug(`ReaderPrettyPdfPane.${event}`, payload);
}

function applyPdfHighlightColor(root: HTMLElement, reader: ReaderSnapshot): void {
  const color = reader.settings.theme === "night"
    ? reader.settings.night_highlight
    : reader.settings.day_highlight;
  root.style.setProperty("--reader-pdf-highlight-r", String(Math.round(color.r * 255)));
  root.style.setProperty("--reader-pdf-highlight-g", String(Math.round(color.g * 255)));
  root.style.setProperty("--reader-pdf-highlight-b", String(Math.round(color.b * 255)));
  root.style.setProperty("--reader-pdf-highlight-a", String(color.a));
}

export const ReaderPrettyPdfPane = forwardRef<ReaderPrettyPdfPaneHandle, ReaderPrettyPdfPaneProps>(
  function ReaderPrettyPdfPane({ onSentenceClick, reader, sourcePath }, ref) {
    const containerRef = useRef<HTMLDivElement | null>(null);
    const renderedPagesRef = useRef<RenderedPdfPage[]>([]);
    const pageShellsRef = useRef<Map<number, HTMLDivElement>>(new Map());
    const pageMetricsRef = useRef<Map<string, PdfShellMetric>>(new Map());
    const pdfPageTextsRef = useRef<Map<number, string>>(new Map());
    const renderedPageZoomRef = useRef<Map<number, number>>(new Map());
    const pdfDocRef = useRef<PDFDocumentProxy | null>(null);
    const pdfDocSourcePathRef = useRef<string | null>(null);
    const pdfJsModuleRef = useRef<PdfJsModule | null>(null);
    const renderGenerationRef = useRef(0);
    const highlightedNodesRef = useRef<HTMLElement[]>([]);
    const highlightedOverlayNodesRef = useRef<HTMLDivElement[]>([]);
    const highlightedPagesRef = useRef<HTMLDivElement[]>([]);
    const highlightedSentenceRef = useRef<number | null>(null);
    const lastScrollTargetRef = useRef<string | null>(null);
    const sentenceMatchesRef = useRef<PdfSentenceMatch[]>([]);
    const matchCacheRef = useRef<CachedPdfMatchResult | null>(null);
    const cachedPdfLocationsRef = useRef<PdfSentenceLocation[] | null>(null);
    const persistedSyncKeyRef = useRef<string | null>(null);
    const [zoom, setZoom] = useState(1.2);
    const [viewportVersion, setViewportVersion] = useState(0);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [renderVersion, setRenderVersion] = useState(0);
    const [cachedSyncVersion, setCachedSyncVersion] = useState(0);
    const [pdfPageCount, setPdfPageCount] = useState(0);
    const [pdfPageTextVersion, setPdfPageTextVersion] = useState(0);
    const [mappingSummary, setMappingSummary] = useState<{
      exact: number;
      fallback: number;
      pageOnly: number;
      missing: number;
    } | null>(null);
    const [activeMatch, setActiveMatch] = useState<PdfSentenceMatch | null>(null);

    const canSyncHighlights = reader.pdf_sync_strategy !== "render_only";
    const modeLabel = reader.pdf_geometry_mode ? reader.pdf_geometry_mode.replaceAll("_", " ") : "unknown";
    const strategyLabel = reader.pdf_sync_strategy ? reader.pdf_sync_strategy.replaceAll("_", " ") : "unknown";
    const globalSentenceStart = globalSentenceStartForReader(reader);
    const highlightedSentenceIdx = reader.highlighted_sentence_idx ?? null;
    const pageStartPercent = reader.stats.page_start_percent;
    const pageEndPercent = reader.stats.page_end_percent;

    const resetRenderedPdfDocument = useCallback(() => {
      clearPdfHighlightOverlays(highlightedOverlayNodesRef.current, highlightedPagesRef.current);
      highlightedOverlayNodesRef.current = [];
      const cleared = applyPdfHighlightDom(
        highlightedNodesRef.current,
        highlightedPagesRef.current,
        [],
        [],
        null
      );
      highlightedNodesRef.current = cleared.highlightedNodes;
      highlightedPagesRef.current = cleared.highlightedPages;
      renderedPagesRef.current = [];
      pageShellsRef.current.clear();
      pageMetricsRef.current.clear();
      renderedPageZoomRef.current.clear();
      matchCacheRef.current = null;
      persistedSyncKeyRef.current = null;
      lastScrollTargetRef.current = null;
      const root = containerRef.current;
      if (root) {
        root.innerHTML = "";
      }
    }, []);

    const ensurePdfDocumentLoaded = useCallback(async (): Promise<{ pdfjs: PdfJsModule; pdf: PDFDocumentProxy }> => {
      ensurePromiseWithResolvers();
      ensureReadableStreamAsyncIterator();
      await ensurePdfJsFakeWorkerGlobal();
      const pdfjs = pdfJsModuleRef.current ?? await importPdfJsBrowserSafe();
      pdfJsModuleRef.current = pdfjs;
      if (pdfDocRef.current && pdfDocSourcePathRef.current === sourcePath) {
        return { pdfjs, pdf: pdfDocRef.current };
      }
      if (pdfDocRef.current) {
        await pdfDocRef.current.destroy();
        pdfDocRef.current = null;
      }
      const pdfBytes = await backendApi.readerLoadPdfBytes(sourcePath);
      const loadingTask = pdfjs.getDocument({
        data: pdfBytes,
        disableRange: true,
        disableStream: true,
        disableAutoFetch: true,
        isEvalSupported: false
      });
      const pdf = await loadingTask.promise;
      pdfDocRef.current = pdf;
      pdfDocSourcePathRef.current = sourcePath;
      return { pdfjs, pdf };
    }, [sourcePath]);

    const ensurePageShells = useCallback(async (pdf: PDFDocumentProxy, activeZoom: number) => {
      const root = containerRef.current;
      if (!root) {
        return;
      }
      const firstPage = await pdf.getPage(1);
      const firstViewport = firstPage.getViewport({ scale: clamp(activeZoom, 0.7, 2.5) });
      const baseWidth = firstViewport.width;
      const baseHeight = firstViewport.height;
      if (pageShellsRef.current.size === pdf.numPages && root.childElementCount === pdf.numPages) {
        for (const shell of pageShellsRef.current.values()) {
          shell.style.width = `${baseWidth}px`;
          shell.style.minHeight = `${baseHeight}px`;
        }
        return;
      }
      root.innerHTML = "";
      pageShellsRef.current.clear();
      renderedPagesRef.current = [];
      renderedPageZoomRef.current.clear();
      for (let pageIndex = 0; pageIndex < pdf.numPages; pageIndex += 1) {
        const shell = document.createElement("div");
        shell.className = "reader-pdf-page-shell";
        shell.dataset.pageIndex = String(pageIndex);
        shell.style.width = `${baseWidth}px`;
        shell.style.minHeight = `${baseHeight}px`;
        shell.style.display = "flex";
        shell.style.justifyContent = "center";
        shell.style.alignItems = "flex-start";
        shell.style.contain = "layout paint style";
        root.appendChild(shell);
        pageShellsRef.current.set(pageIndex, shell);
      }
      setPdfPageCount(pdf.numPages);
      logPdfDebug("pageShellsReady", {
        sourcePath,
        pageCount: pdf.numPages,
        baseWidth: Math.round(baseWidth),
        baseHeight: Math.round(baseHeight)
      });
    }, [sourcePath]);

    const ensurePageRendered = useCallback(async (
      pdfjs: PdfJsModule,
      pdf: PDFDocumentProxy,
      pageIndex: number,
      activeZoom: number,
      generation: number
    ) => {
      const shell = pageShellsRef.current.get(pageIndex);
      if (!shell) {
        return;
      }
      if (renderGenerationRef.current !== generation) {
        return;
      }
      if (renderedPageZoomRef.current.get(pageIndex) === activeZoom) {
        return;
      }
      const page = await pdf.getPage(pageIndex + 1);
      if (renderGenerationRef.current !== generation) {
        return;
      }
      const metricKey = `${pageIndex}:${activeZoom}`;
      const viewport = page.getViewport({ scale: clamp(activeZoom, 0.7, 2.5) });
      pageMetricsRef.current.set(metricKey, { width: viewport.width, height: viewport.height });
      shell.style.width = `${viewport.width}px`;
      shell.style.minHeight = `${viewport.height}px`;
      shell.innerHTML = "";
      const pageContainer = document.createElement("div");
      pageContainer.className = "reader-pdf-page";
      pageContainer.dataset.pageIndex = String(pageIndex);
      pageContainer.style.width = `${viewport.width}px`;
      pageContainer.style.height = `${viewport.height}px`;

      const canvas = document.createElement("canvas");
      canvas.className = "reader-pdf-page-canvas";
      const context = canvas.getContext("2d");
      if (!context) {
        throw new Error("Canvas 2D context unavailable for PDF rendering");
      }
      const outputScale = typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1;
      canvas.width = Math.floor(viewport.width * outputScale);
      canvas.height = Math.floor(viewport.height * outputScale);
      canvas.style.width = `${viewport.width}px`;
      canvas.style.height = `${viewport.height}px`;
      context.setTransform(outputScale, 0, 0, outputScale, 0, 0);
      pageContainer.appendChild(canvas);

      const textLayerDiv = document.createElement("div");
      textLayerDiv.className = "reader-pdf-text-layer";
      pageContainer.appendChild(textLayerDiv);
      shell.appendChild(pageContainer);

      await page.render({ canvas, canvasContext: context, viewport }).promise;
      const textContent = await page.getTextContent();
      const textLayer = new pdfjs.TextLayer({
        textContentSource: textContent,
        container: textLayerDiv,
        viewport
      });
      await textLayer.render();

      const spanElements = Array.from(textLayerDiv.querySelectorAll("span")) as HTMLElement[];
      const spans: PdfTextSpan[] = orderPdfTextLayerSpans(
        spanElements.filter((element) => isVisiblePdfTextSpan(element)),
        pageIndex,
        viewport.rotation
      );
      renderedPageZoomRef.current.set(pageIndex, activeZoom);
      renderedPagesRef.current = [
        ...renderedPagesRef.current.filter((entry) => entry.pageIndex !== pageIndex),
        { container: pageContainer, pageIndex, spans }
      ].sort((left, right) => left.pageIndex - right.pageIndex);
      setRenderedPdfSpanIndexes(renderedPagesRef.current);
      setRenderVersion((value) => value + 1);
    }, []);

    const ensureTargetPagesRendered = useCallback(async () => {
      const pdf = pdfDocRef.current;
      const pdfjs = pdfJsModuleRef.current;
      if (!pdf || !pdfjs || pdfPageCount <= 0) {
        return;
      }
      const generation = renderGenerationRef.current;
      const targetPage = estimatePdfTargetPage(
        cachedPdfLocationsRef.current,
        reader,
        pdfPageCount,
        pdfPageTextsRef.current
      );
      for (const pageIndex of pageIndexesAround(targetPage, pdfPageCount, 1)) {
        await ensurePageRendered(pdfjs, pdf, pageIndex, zoom, generation);
      }
    }, [ensurePageRendered, pdfPageCount, reader, zoom]);

    const ensurePdfPageTexts = useCallback(async (pdf: PDFDocumentProxy, generation: number) => {
      if (pdfPageTextsRef.current.size === pdf.numPages) {
        return;
      }
      for (let pageIndex = 0; pageIndex < pdf.numPages; pageIndex += 1) {
        if (renderGenerationRef.current !== generation) {
          return;
        }
        if (pdfPageTextsRef.current.has(pageIndex)) {
          continue;
        }
        const page = await pdf.getPage(pageIndex + 1);
        const textContent = await page.getTextContent();
        pdfPageTextsRef.current.set(pageIndex, extractNormalizedPdfPageText(textContent));
        if ((pageIndex + 1) % 12 === 0 || pageIndex + 1 === pdf.numPages) {
          setPdfPageTextVersion((value) => value + 1);
          await new Promise((resolve) => window.setTimeout(resolve, 0));
        }
      }
    }, []);

    const resolveSentenceMatches = useCallback(() => {
      const candidatePages = resolveLikelyRenderedPages(
        renderedPagesRef.current,
        cachedPdfLocationsRef.current,
        reader
      );
      const spans = flattenRenderedPdfSpans(candidatePages);
      const cacheKey = [
        reader.source_path,
        String(globalSentenceStart),
        candidatePages.map((page) => page.pageIndex).join(","),
        String(renderVersion),
        String(spans.length),
        reader.sentences.join("\n")
      ].join("\u241f");
      if (matchCacheRef.current?.key === cacheKey) {
        logPdfDebug("mappingCacheHit", {
          sourcePath: reader.source_path,
          globalSentenceStart,
          candidatePages: candidatePages.map((page) => page.pageIndex),
          renderVersion,
          spanCount: spans.length,
          sentenceCount: reader.sentences.length
        });
        return {
          spans,
          matches: matchCacheRef.current.matches,
          summary: matchCacheRef.current.summary
        };
      }

      const { matches, diagnostics } = buildPdfSentenceSpanMap(
        spans,
        reader.sentences,
        reader.sentence_anchor_map
      );
      const summary = {
        exact: diagnostics.exactMatches,
        fallback: diagnostics.fallbackMatches,
        pageOnly: diagnostics.pageOnlyMatches,
        missing: diagnostics.missingMatches,
        cappedLeaps: diagnostics.cappedLeaps
      };
      matchCacheRef.current = {
        key: cacheKey,
        matches,
        summary
      };
      logPdfDebug("mappingCacheMiss", {
        sourcePath: reader.source_path,
        globalSentenceStart,
        candidatePages: candidatePages.map((page) => page.pageIndex),
        renderVersion,
        spanCount: spans.length,
        sentenceCount: reader.sentences.length
      });
      return { spans, matches, summary };
    }, [globalSentenceStart, pageEndPercent, pageStartPercent, reader.sentences, reader.sentence_anchor_map, reader.source_path, renderVersion]);

    const applyHighlight = useCallback(
      (behavior: ScrollBehavior, force = false) => {
        const idx = reader.highlighted_sentence_idx;
        const startedAt = typeof performance !== "undefined" ? performance.now() : 0;
        let resolvedScrollTarget: string | null = null;

        if (idx === null || idx === undefined) {
          clearPdfHighlightOverlays(highlightedOverlayNodesRef.current, highlightedPagesRef.current);
          highlightedOverlayNodesRef.current = [];
          const cleared = applyPdfHighlightDom(
            highlightedNodesRef.current,
            highlightedPagesRef.current,
            [],
            [],
            null
          );
          highlightedNodesRef.current = cleared.highlightedNodes;
          highlightedPagesRef.current = cleared.highlightedPages;
          highlightedSentenceRef.current = null;
          recordPerfMeasure("ReaderPrettyPdfPane.resolveHighlight", startedAt);
          return;
        }
        if (!canSyncHighlights) {
          clearPdfHighlightOverlays(highlightedOverlayNodesRef.current, highlightedPagesRef.current);
          highlightedOverlayNodesRef.current = [];
          highlightedSentenceRef.current = idx;
          recordPerfMeasure("ReaderPrettyPdfPane.resolveHighlight", startedAt);
          return;
        }

        const cachedMatchSet = buildCachedPdfLocationMatchSet(
          cachedPdfLocationsRef.current,
          globalSentenceStart,
          reader.sentences.length
        );
        const { spans, matches, summary } = cachedMatchSet
          ? {
              spans: [] as PdfTextSpan[],
              matches: cachedMatchSet.matches,
              summary: {
                exact: cachedMatchSet.summary.exact,
                fallback: cachedMatchSet.summary.fallback,
                pageOnly: cachedMatchSet.summary.pageOnly,
                missing: cachedMatchSet.summary.missing,
                cappedLeaps: 0
              }
            }
          : resolveSentenceMatches();
        sentenceMatchesRef.current = matches;
        setMappingSummary({
          exact: summary.exact,
          fallback: summary.fallback,
          pageOnly: summary.pageOnly,
          missing: summary.missing
        });
        const match = matches[idx];
        setActiveMatch(match ?? null);
        logPdfDebug("mappingSummary", {
          sentenceCount: reader.sentences.length,
          exactMatches: summary.exact,
          fallbackMatches: summary.fallback,
          pageOnlyMatches: summary.pageOnly,
          missingMatches: summary.missing,
          cappedLeaps: "cappedLeaps" in summary ? summary.cappedLeaps : 0,
          lowConfidenceMatches: matches.filter((candidate) => candidate.score > 0 && candidate.score < 0.88).length
        });
        if (!match) {
          clearPdfHighlightOverlays(highlightedOverlayNodesRef.current, highlightedPagesRef.current);
          highlightedOverlayNodesRef.current = [];
          highlightedSentenceRef.current = idx;
          lastScrollTargetRef.current = null;
          logPdfDebug("highlightMissing", {
            sentenceIdx: idx,
            sentenceCount: reader.sentences.length
          });
          recordPerfMeasure("ReaderPrettyPdfPane.resolveHighlight", startedAt);
          return;
        }
        logPdfDebug("highlightResolved", {
          sentenceIdx: idx,
          confidence: match.confidence,
          score: match.score,
          reason: match.reason,
          pageIndex: match.pageIndex,
          spanCount: match.spanIndexes.length
        });
        const cachedLocation = cachedMatchSet?.locations[idx] ?? null;
        let highlighted: { highlightedNodes: HTMLElement[]; highlightedPages: HTMLDivElement[] };
        if (cachedLocation) {
          const overlays = applyPdfLocationHighlightOverlays(
            highlightedOverlayNodesRef.current,
            highlightedPagesRef.current,
            renderedPagesRef.current,
            cachedLocation
          );
          highlightedOverlayNodesRef.current = overlays.highlightedOverlays;
          highlighted = {
            highlightedNodes: overlays.highlightedOverlays,
            highlightedPages: overlays.highlightedPages
          };
          const clearedSpans = applyPdfHighlightDom(
            highlightedNodesRef.current,
            [],
            [],
            [],
            null
          );
          highlightedNodesRef.current = clearedSpans.highlightedNodes;
        } else {
          clearPdfHighlightOverlays(highlightedOverlayNodesRef.current, highlightedPagesRef.current);
          highlightedOverlayNodesRef.current = [];
          highlighted = applyPdfHighlightDom(
            highlightedNodesRef.current,
            highlightedPagesRef.current,
            spans,
            renderedPagesRef.current,
            match
          );
          highlightedNodesRef.current = highlighted.highlightedNodes;
        }
        highlightedPagesRef.current = highlighted.highlightedPages;
        if (match.reason === "page_location_only" && match.pageIndex !== null) {
          const page = highlighted.highlightedPages[0];
          if (page) {
            highlightedSentenceRef.current = idx;
            const shouldAutoScroll =
              force || (reader.settings.auto_scroll_tts && reader.tts.state === "playing");
            resolvedScrollTarget = `page:${match.pageIndex}`;
            const shouldScrollTarget =
              force || (shouldAutoScroll && lastScrollTargetRef.current !== resolvedScrollTarget);
            if (shouldScrollTarget) {
              page.scrollIntoView({
                behavior,
                block: reader.settings.center_spoken_sentence ? "center" : "nearest",
                inline: "nearest"
              });
              logPdfDebug("scrollTarget", {
                reason: force ? "manual_jump" : "page_location_change",
                target: resolvedScrollTarget
              });
            }
            lastScrollTargetRef.current = resolvedScrollTarget;
            recordPerfMeasure("ReaderPrettyPdfPane.resolveHighlight", startedAt);
            return;
          }
        }
        if (match.spanIndexes.length === 0 && !cachedLocation) {
          highlightedSentenceRef.current = idx;
          lastScrollTargetRef.current = null;
          recordPerfMeasure("ReaderPrettyPdfPane.resolveHighlight", startedAt);
          return;
        }

        const elements = highlighted.highlightedNodes;
        highlightedSentenceRef.current = idx;

        const shouldAutoScroll =
          force || (reader.settings.auto_scroll_tts && reader.tts.state === "playing");
        const anchor = elements[0];
        resolvedScrollTarget = cachedLocation
          ? `cached:${idx}:${cachedLocation.page_idx ?? "none"}`
          : match.spanIndexes.join(",");
        const shouldScrollTarget =
          force || (shouldAutoScroll && lastScrollTargetRef.current !== resolvedScrollTarget);
        if (!anchor || !shouldScrollTarget) {
          lastScrollTargetRef.current = resolvedScrollTarget;
          recordPerfMeasure("ReaderPrettyPdfPane.resolveHighlight", startedAt);
          return;
        }
        anchor.scrollIntoView({
          behavior,
          block: reader.settings.center_spoken_sentence ? "center" : "nearest",
          inline: "nearest"
        });
        lastScrollTargetRef.current = resolvedScrollTarget;
        logPdfDebug("scrollTarget", {
          reason: force ? "manual_jump" : "sentence_location_change",
          target: resolvedScrollTarget,
          matchReason: match.reason
        });
        recordPerfMeasure("ReaderPrettyPdfPane.resolveHighlight", startedAt);
      },
      [cachedSyncVersion, canSyncHighlights, globalSentenceStart, reader.highlighted_sentence_idx, reader.sentences, reader.settings.auto_scroll_tts, reader.settings.center_spoken_sentence, reader.tts.state, resolveSentenceMatches]
    );

    useImperativeHandle(ref, () => ({
      jumpToHighlightedSentence() {
        applyHighlight("smooth", true);
      }
    }), [applyHighlight]);

    useEffect(() => {
      const root = containerRef.current;
      if (!root) {
        return;
      }
      applyPdfHighlightColor(root, reader);
    }, [reader]);

    useEffect(() => {
      const root = containerRef.current;
      if (!root) {
        return;
      }
      const observedElement = root.parentElement ?? root;
      let lastObservedWidth = observedElement.getBoundingClientRect().width;
      let frame = 0;
      const scheduleViewportRefresh = (reason: string, nextWidth: number) => {
        if (Math.abs(nextWidth - lastObservedWidth) < 1) {
          return;
        }
        lastObservedWidth = nextWidth;
        if (frame !== 0) {
          cancelAnimationFrame(frame);
        }
        frame = requestAnimationFrame(() => {
          frame = 0;
          logPdfDebug("viewportRefresh", {
            sourcePath,
            zoom,
            reason
          });
          setViewportVersion((value) => value + 1);
        });
      };
      const resizeObserver = new ResizeObserver((entries) => {
        const entry = entries[0];
        const nextWidth = entry?.contentRect.width ?? observedElement.getBoundingClientRect().width;
        scheduleViewportRefresh("resize_observer", nextWidth);
      });
      resizeObserver.observe(observedElement);
      const onWindowResize = () => {
        scheduleViewportRefresh("window_resize", observedElement.getBoundingClientRect().width);
      };
      window.addEventListener("resize", onWindowResize);
      return () => {
        if (frame !== 0) {
          cancelAnimationFrame(frame);
        }
        resizeObserver.disconnect();
        window.removeEventListener("resize", onWindowResize);
      };
    }, [sourcePath, zoom]);

    useEffect(() => {
      resetRenderedPdfDocument();
      const previousDoc = pdfDocRef.current;
      pdfDocRef.current = null;
      pdfDocSourcePathRef.current = null;
      pdfPageTextsRef.current.clear();
      setPdfPageTextVersion(0);
      setPdfPageCount(0);
      if (previousDoc) {
        void previousDoc.destroy();
      }
    }, [resetRenderedPdfDocument, sourcePath]);

    useEffect(() => {
      let cancelled = false;
      cachedPdfLocationsRef.current = null;
      void backendApi.readerLoadPdfSyncMap(sourcePath)
        .then((locations) => {
          if (cancelled) {
            return;
          }
          cachedPdfLocationsRef.current = locations;
          setCachedSyncVersion((value) => value + 1);
          logPdfDebug("loadedCachedSyncMap", {
            sourcePath,
            count: locations.length,
            currentPage: reader.current_page
          });
        })
        .catch((cause: unknown) => {
          if (cancelled) {
            return;
          }
          cachedPdfLocationsRef.current = null;
          setCachedSyncVersion((value) => value + 1);
          logPdfDebug("loadCachedSyncMapError", {
            sourcePath,
            error: cause instanceof Error ? cause.message : String(cause)
          });
        });
      return () => {
        cancelled = true;
      };
    }, [sourcePath]);

    useEffect(() => {
      let cancelled = false;
      const generation = renderGenerationRef.current + 1;
      renderGenerationRef.current = generation;

      const init = async (): Promise<void> => {
        const root = containerRef.current;
        if (!root) {
          return;
        }
        setLoading(true);
        setError(null);
        matchCacheRef.current = null;
        persistedSyncKeyRef.current = null;
        applyPdfHighlightColor(root, reader);
        const startedAt = typeof performance !== "undefined" ? performance.now() : 0;
        logPdfDebug("renderInit", {
          sourcePath,
          zoom,
          viewportVersion
        });
        try {
          const { pdfjs, pdf } = await ensurePdfDocumentLoaded();
          if (cancelled || renderGenerationRef.current !== generation) {
            return;
          }
          renderedPagesRef.current = [];
          renderedPageZoomRef.current.clear();
          await ensurePageShells(pdf, zoom);
          for (const shell of pageShellsRef.current.values()) {
            shell.innerHTML = "";
          }
          void ensurePdfPageTexts(pdf, generation);
          const targetPage = estimatePdfTargetPage(
            cachedPdfLocationsRef.current,
            reader,
            pdf.numPages,
            pdfPageTextsRef.current
          );
          for (const pageIndex of pageIndexesAround(targetPage, pdf.numPages, 1)) {
            await ensurePageRendered(pdfjs, pdf, pageIndex, zoom, generation);
          }
          recordPerfMeasure("ReaderPrettyPdfPane.renderDocument", startedAt);
          logPdfDebug("renderInitComplete", {
            sourcePath,
            zoom,
            targetPage,
            pageCount: pdf.numPages
          });
        } catch (cause) {
          if (!cancelled) {
            setError(cause instanceof Error ? cause.message : String(cause));
          }
        } finally {
          if (!cancelled) {
            setLoading(false);
          }
        }
      };

      void init();
      return () => {
        cancelled = true;
      };
    }, [ensurePageRendered, ensurePageShells, ensurePdfDocumentLoaded, ensurePdfPageTexts, resetRenderedPdfDocument, sourcePath, viewportVersion, zoom]);

    useEffect(() => {
      const root = containerRef.current;
      const pdf = pdfDocRef.current;
      const pdfjs = pdfJsModuleRef.current;
      if (!root || !pdf || !pdfjs || pageShellsRef.current.size === 0) {
        return;
      }
      const generation = renderGenerationRef.current;
      const observer = new IntersectionObserver((entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) {
            continue;
          }
          const target = entry.target as HTMLElement;
          const rawPageIndex = target.dataset.pageIndex;
          const pageIndex = rawPageIndex ? Number.parseInt(rawPageIndex, 10) : Number.NaN;
          if (!Number.isFinite(pageIndex)) {
            continue;
          }
          void ensurePageRendered(pdfjs, pdf, pageIndex, zoom, generation);
        }
      }, {
        root: null,
        rootMargin: "150% 0px 150% 0px",
        threshold: 0.01
      });
      for (const shell of pageShellsRef.current.values()) {
        observer.observe(shell);
      }
      return () => {
        observer.disconnect();
      };
    }, [ensurePageRendered, renderVersion, zoom]);

    useEffect(() => {
      if (loading) {
        return;
      }
      let cancelled = false;
      void ensureTargetPagesRendered().then(() => {
        if (cancelled) {
          return;
        }
        applyHighlight("auto");
      });
      return () => {
        cancelled = true;
      };
    }, [applyHighlight, cachedSyncVersion, ensureTargetPagesRendered, globalSentenceStart, highlightedSentenceIdx, loading, pdfPageTextVersion, zoom]);

    useEffect(() => {
      if (loading || !canSyncHighlights) {
        return;
      }
      const cachedMatchSet = buildCachedPdfLocationMatchSet(
        cachedPdfLocationsRef.current,
        globalSentenceStart,
        reader.sentences.length
      );
      if (cachedMatchSet) {
        logPdfDebug("reuseCachedSyncMap", {
          sourcePath: reader.source_path,
          currentPage: reader.current_page,
          count: cachedMatchSet.locations.filter(Boolean).length
        });
        return;
      }
      const { matches } = resolveSentenceMatches();
      const locations = buildPersistedPdfSentenceLocations(matches, renderedPagesRef.current, globalSentenceStart);
      const persistKey = [
        reader.source_path,
        String(renderVersion),
        String(locations.length)
      ].join("\u241f");
      if (persistedSyncKeyRef.current === persistKey) {
        return;
      }
      persistedSyncKeyRef.current = persistKey;
      void backendApi.readerPersistPdfSyncMap(reader.source_path, locations)
        .then(() => {
          logPdfDebug("persistedSyncMap", {
            sourcePath: reader.source_path,
            count: locations.length,
            renderVersion
          });
        })
        .catch((error: unknown) => {
          persistedSyncKeyRef.current = null;
          logPdfDebug("persistSyncMapError", {
            sourcePath: reader.source_path,
            error: error instanceof Error ? error.message : String(error)
          });
        });
    }, [canSyncHighlights, globalSentenceStart, loading, reader.sentences.length, reader.source_path, renderVersion, resolveSentenceMatches]);

    const handlePdfClick = useCallback((event: React.MouseEvent<HTMLDivElement>) => {
      if (!canSyncHighlights) {
        return;
      }
      const target = event.target as HTMLElement | null;
      const span = target?.closest("[data-ll-pdf-span-idx]") as HTMLElement | null;
      if (!span) {
        return;
      }
      const raw = span.getAttribute("data-ll-pdf-span-idx");
      const spanIdx = raw === null ? Number.NaN : Number.parseInt(raw, 10);
      if (!Number.isFinite(spanIdx)) {
        return;
      }
      const sentenceIdx = findNearestSentenceForSpanIndex(sentenceMatchesRef.current, spanIdx);
      if (sentenceIdx === null) {
        return;
      }
      logPdfDebug("clickResolveSentence", {
        reason: "span_click",
        spanIdx,
        sentenceIdx
      });
      void onSentenceClick(sentenceIdx);
    }, [canSyncHighlights, onSentenceClick]);

    const handlePdfPageMouseUp = useCallback((event: React.MouseEvent<HTMLDivElement>) => {
      if (!canSyncHighlights) {
        return;
      }
      const target = event.target as HTMLElement | null;
      if (!target) {
        return;
      }
      const page = target.closest(".reader-pdf-page") as HTMLElement | null;
      if (!page) {
        return;
      }
      if (target.closest("[data-ll-pdf-span-idx]")) {
        return;
      }
      const rawPageIndex = page.dataset.pageIndex;
      const pageIndex = rawPageIndex === undefined ? Number.NaN : Number.parseInt(rawPageIndex, 10);
      if (!Number.isFinite(pageIndex)) {
        return;
      }
      const sentenceIdx = findNearestSentenceForPageIndex(sentenceMatchesRef.current, pageIndex);
      if (sentenceIdx === null) {
        return;
      }
      logPdfDebug("clickResolveSentence", {
        reason: "page_click",
        pageIndex,
        sentenceIdx
      });
      void onSentenceClick(sentenceIdx);
    }, [canSyncHighlights, onSentenceClick]);

    return (
      <div className="reader-pdf-pane-shell">
        <Stack
          direction="row"
          spacing={1}
          alignItems="center"
          justifyContent="space-between"
          sx={{ mb: 1.25, px: 0.5 }}
        >
          <Typography variant="caption" color="text.secondary">
            Native PDF | geometry: {modeLabel} | sync: {strategyLabel}
          </Typography>
          <ButtonGroup size="small" variant="outlined">
            <Button onClick={() => setZoom((value) => normalizeNumber(value - 0.1, 0.7, 2.5, 0.05, 2))}>
              -
            </Button>
            <Button disabled>{Math.round(zoom * 100)}%</Button>
            <Button onClick={() => setZoom((value) => normalizeNumber(value + 0.1, 0.7, 2.5, 0.05, 2))}>
              +
            </Button>
          </ButtonGroup>
        </Stack>
        {error ? (
          <Typography color="error" variant="body2" data-testid="reader-pretty-pdf-error">
            Failed to render PDF: {error}
          </Typography>
        ) : null}
        {!error && !canSyncHighlights ? (
          <Typography color="text.secondary" variant="caption" data-testid="reader-pretty-pdf-degraded">
            {reader.pdf_geometry_mode === "ocr_required"
              ? "This PDF is renderable now, but precise highlight sync and text playback will stay gated until OCR produces usable text."
              : "This PDF is render-only right now. Text-only/TTS can continue, but precise PDF highlight sync is unavailable."}
          </Typography>
        ) : null}
        {!error && canSyncHighlights && mappingSummary ? (
          <Typography color="text.secondary" variant="caption" data-testid="reader-pretty-pdf-summary">
            Exact: {mappingSummary.exact} | Fallback: {mappingSummary.fallback} | Page-only: {mappingSummary.pageOnly} | Missing: {mappingSummary.missing}
            {activeMatch ? ` | Active: ${activeMatch.confidence} ${activeMatch.score.toFixed(2)} (${activeMatch.reason.replaceAll("_", " ")})` : ""}
          </Typography>
        ) : null}
        {loading ? (
          <Stack alignItems="center" spacing={1.25} sx={{ py: 3 }}>
            <CircularProgress size={24} />
            <Typography variant="caption" color="text.secondary">
              Rendering PDF pages...
            </Typography>
          </Stack>
        ) : null}
        <div
          ref={containerRef}
          className="reader-pdf-document"
          data-testid="reader-pretty-pdf"
          onClick={handlePdfClick}
          onMouseUp={handlePdfPageMouseUp}
          style={{ opacity: loading ? 0.35 : 1 }}
        />
      </div>
    );
  }
);

function buildPersistedPdfSentenceLocations(
  matches: PdfSentenceMatch[],
  pages: RenderedPdfPage[],
  globalSentenceStart: number
): PdfSentenceLocation[] {
  const spans = flattenRenderedPdfSpans(pages);
  const pageBounds = new Map<number, DOMRect>();
  for (const page of pages) {
    pageBounds.set(page.pageIndex, page.container.getBoundingClientRect());
  }

  return matches.map((match, sentenceIdx) => {
    const rects = match.spanIndexes.flatMap((spanIndex) => {
      const span = spans[spanIndex];
      if (!span) {
        return [];
      }
      const pageRect = pageBounds.get(span.pageIndex);
      if (!pageRect || pageRect.width <= 0 || pageRect.height <= 0) {
        return [];
      }
      const rect = span.element.getBoundingClientRect();
      return [{
        left: Number(((rect.left - pageRect.left) / pageRect.width).toFixed(4)),
        top: Number(((rect.top - pageRect.top) / pageRect.height).toFixed(4)),
        width: Number((rect.width / pageRect.width).toFixed(4)),
        height: Number((rect.height / pageRect.height).toFixed(4))
      }];
    });
    const lineRects = mergeRectsIntoLines(rects);
    const blockRects = mergeLineRectsIntoBlocks(lineRects);
    return {
      sentence_idx: globalSentenceStart + sentenceIdx,
      page_idx: match.pageIndex,
      rects,
      line_rects: lineRects,
      block_rects: blockRects,
      confidence: match.confidence,
      reason: match.reason,
      score: match.score
    };
  });
}

function mergeRectsIntoLines(rects: PdfSentenceLocation["rects"]): PdfSentenceLocation["line_rects"] {
  if (rects.length <= 1) {
    return [...rects];
  }
  const sorted = [...rects].sort((left, right) => {
    if (left.top !== right.top) {
      return left.top - right.top;
    }
    return left.left - right.left;
  });
  const merged: PdfSentenceLocation["line_rects"] = [];
  for (const rect of sorted) {
    const current = merged.at(-1);
    if (!current) {
      merged.push({ ...rect });
      continue;
    }
    const sameLine = Math.abs(current.top - rect.top) <= Math.max(current.height, rect.height, 0.012);
    if (!sameLine) {
      merged.push({ ...rect });
      continue;
    }
    const left = Math.min(current.left, rect.left);
    const top = Math.min(current.top, rect.top);
    const right = Math.max(current.left + current.width, rect.left + rect.width);
    const bottom = Math.max(current.top + current.height, rect.top + rect.height);
    current.left = Number(left.toFixed(4));
    current.top = Number(top.toFixed(4));
    current.width = Number((right - left).toFixed(4));
    current.height = Number((bottom - top).toFixed(4));
  }
  return merged;
}

function mergeLineRectsIntoBlocks(lineRects: PdfSentenceLocation["line_rects"]): PdfSentenceLocation["block_rects"] {
  if (lineRects.length <= 1) {
    return [...lineRects];
  }
  const blocks: PdfSentenceLocation["block_rects"] = [];
  for (const rect of lineRects) {
    const current = blocks.at(-1);
    if (!current) {
      blocks.push({ ...rect });
      continue;
    }
    const verticalGap = rect.top - (current.top + current.height);
    const horizontalDrift = Math.abs(rect.left - current.left);
    const sameBlock = verticalGap <= 0.035 && horizontalDrift <= 0.22;
    if (!sameBlock) {
      blocks.push({ ...rect });
      continue;
    }
    const left = Math.min(current.left, rect.left);
    const top = Math.min(current.top, rect.top);
    const right = Math.max(current.left + current.width, rect.left + rect.width);
    const bottom = Math.max(current.top + current.height, rect.top + rect.height);
    current.left = Number(left.toFixed(4));
    current.top = Number(top.toFixed(4));
    current.width = Number((right - left).toFixed(4));
    current.height = Number((bottom - top).toFixed(4));
  }
  return blocks;
}

function buildCachedPdfLocationMatchSet(
  locations: PdfSentenceLocation[] | null,
  globalSentenceStart: number,
  sentenceCount: number
): CachedPdfLocationMatchSet | null {
  if (!locations || sentenceCount <= 0) {
    return null;
  }
  const orderedLocations: Array<PdfSentenceLocation | null> = Array.from({ length: sentenceCount }, () => null);
  for (const location of locations) {
    const localSentenceIdx = location.sentence_idx - globalSentenceStart;
    if (localSentenceIdx < 0 || localSentenceIdx >= sentenceCount) {
      continue;
    }
    orderedLocations[localSentenceIdx] = location;
  }
  const hasGeometry = orderedLocations.some((location) => (
    Boolean(location)
    && (
      (location?.rects.length ?? 0) > 0
      || (location?.line_rects.length ?? 0) > 0
      || (location?.block_rects.length ?? 0) > 0
    )
  ));
  if (!hasGeometry) {
    return null;
  }

  let exact = 0;
  let fallback = 0;
  let pageOnly = 0;
  let missing = 0;
  const matches = orderedLocations.map((location) => {
    if (!location) {
      missing += 1;
      return {
        confidence: "missing",
        reason: "missing",
        pageIndex: null,
        spanIndexes: [],
        score: 0
      } satisfies PdfSentenceMatch;
    }
    const confidence = location.confidence;
    if (confidence === "exact") {
      exact += 1;
    } else if (confidence === "fallback") {
      fallback += 1;
    } else if (confidence === "page") {
      pageOnly += 1;
    } else {
      missing += 1;
    }
    return {
      confidence: confidence === "exact" || confidence === "fallback" || confidence === "page" ? confidence : "missing",
      reason: location.reason === "exact_geometry"
        || location.reason === "fuzzy_sentence_geometry"
        || location.reason === "paragraph_fallback"
        || location.reason === "page_location_only"
        ? location.reason
        : "missing",
      pageIndex: location.page_idx,
      spanIndexes: [],
      score: location.score
    } satisfies PdfSentenceMatch;
  });

  return {
    matches,
    summary: { exact, fallback, pageOnly, missing },
    locations: orderedLocations
  };
}

function clearPdfHighlightOverlays(
  previousOverlays: HTMLDivElement[],
  previousPages: HTMLDivElement[]
): void {
  for (const overlay of previousOverlays) {
    overlay.remove();
  }
  for (const page of previousPages) {
    page.classList.remove("reader-pdf-page-active");
  }
}

function applyPdfLocationHighlightOverlays(
  previousOverlays: HTMLDivElement[],
  previousPages: HTMLDivElement[],
  pages: Array<{ pageIndex: number; container: HTMLDivElement }>,
  location: PdfSentenceLocation
): { highlightedOverlays: HTMLDivElement[]; highlightedPages: HTMLDivElement[] } {
  clearPdfHighlightOverlays(previousOverlays, previousPages);
  if (location.page_idx === null) {
    return { highlightedOverlays: [], highlightedPages: [] };
  }
  const page = pages.find((candidate) => candidate.pageIndex === location.page_idx)?.container;
  if (!page) {
    return { highlightedOverlays: [], highlightedPages: [] };
  }
  if (location.reason === "page_location_only") {
    page.classList.add("reader-pdf-page-active");
    return { highlightedOverlays: [], highlightedPages: [page] };
  }
  const rects = location.line_rects.length > 0
    ? location.line_rects
    : location.block_rects.length > 0
      ? location.block_rects
      : location.rects;
  const overlays = rects.map((rect) => {
    const overlay = document.createElement("div");
    overlay.className = "reader-pdf-highlight-overlay";
    overlay.style.left = `${rect.left * 100}%`;
    overlay.style.top = `${rect.top * 100}%`;
    overlay.style.width = `${rect.width * 100}%`;
    overlay.style.height = `${rect.height * 100}%`;
    page.appendChild(overlay);
    return overlay;
  });
  return { highlightedOverlays: overlays, highlightedPages: [] };
}

async function renderPdfPage(
  TextLayerImpl: typeof import("pdfjs-dist/legacy/build/pdf.mjs")["TextLayer"],
  pdf: PDFDocumentProxy,
  root: HTMLDivElement,
  zoom: number,
  pageIndex: number,
  cancelled: boolean,
  renderedPagesRef: React.MutableRefObject<RenderedPdfPage[]>
): Promise<void> {
  if (cancelled) {
    return;
  }
  const startedAt = typeof performance !== "undefined" ? performance.now() : 0;
  const pageNumber = pageIndex + 1;
  const page = await pdf.getPage(pageNumber);
  const viewport = page.getViewport({ scale: clamp(zoom, 0.7, 2.5) });

  const pageContainer = document.createElement("div");
  pageContainer.className = "reader-pdf-page";
  pageContainer.dataset.pageIndex = String(pageIndex);
  pageContainer.style.width = `${viewport.width}px`;
  pageContainer.style.height = `${viewport.height}px`;

  const canvas = document.createElement("canvas");
  canvas.className = "reader-pdf-page-canvas";
  const context = canvas.getContext("2d");
  if (!context) {
    throw new Error("Canvas 2D context unavailable for PDF rendering");
  }
  const outputScale = typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1;
  canvas.width = Math.floor(viewport.width * outputScale);
  canvas.height = Math.floor(viewport.height * outputScale);
  canvas.style.width = `${viewport.width}px`;
  canvas.style.height = `${viewport.height}px`;
  context.setTransform(outputScale, 0, 0, outputScale, 0, 0);
  pageContainer.appendChild(canvas);

  const textLayerDiv = document.createElement("div");
  textLayerDiv.className = "reader-pdf-text-layer";
  pageContainer.appendChild(textLayerDiv);

  root.appendChild(pageContainer);
  await page.render({ canvas, canvasContext: context, viewport }).promise;

  const textContent = await page.getTextContent();
  const textLayer = new TextLayerImpl({
    textContentSource: textContent,
    container: textLayerDiv,
    viewport
  });
  await textLayer.render();

  const spanElements = Array.from(textLayerDiv.querySelectorAll("span")) as HTMLElement[];
  const spans: PdfTextSpan[] = orderPdfTextLayerSpans(
    spanElements.filter((element) => isVisiblePdfTextSpan(element)),
    pageIndex,
    viewport.rotation
  );

  renderedPagesRef.current.push({
    container: pageContainer,
    pageIndex,
    spans
  });
  recordPerfMeasure("ReaderPrettyPdfPane.renderPage", startedAt);
  logPdfDebug("renderPage", {
    pageIndex,
    zoom,
    width: Math.round(viewport.width),
    height: Math.round(viewport.height),
    textSpanCount: spans.length
  });
}

function isVisiblePdfTextSpan(element: HTMLElement): boolean {
  if (element.hidden) {
    return false;
  }
  const style = window.getComputedStyle(element);
  if (
    style.display === "none"
    || style.visibility === "hidden"
    || style.opacity === "0"
  ) {
    return false;
  }
  const width = Number.parseFloat(style.width || "0");
  const height = Number.parseFloat(style.height || "0");
  if (Number.isFinite(width) && Number.isFinite(height) && width <= 0 && height <= 0) {
    return false;
  }
  const rects = element.getClientRects();
  if (rects.length > 0) {
    const hasArea = Array.from(rects).some((rect) => rect.width > 0 && rect.height > 0);
    if (!hasArea) {
      return false;
    }
  }
  return true;
}
