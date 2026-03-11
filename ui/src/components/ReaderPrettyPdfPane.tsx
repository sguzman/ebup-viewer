import { Button, ButtonGroup, CircularProgress, Stack, Typography } from "@mui/material";
import type { PDFDocumentProxy } from "pdfjs-dist/legacy/build/pdf.mjs";
import { forwardRef, useCallback, useEffect, useImperativeHandle, useRef, useState } from "react";

import { backendApi, type PdfSentenceLocation } from "../api/tauri";
import type { ReaderSnapshot } from "../types";
import { recordPerfMeasure } from "../perf/debug";
import { clamp, normalizeNumber } from "./readerShared";
import {
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

interface DirectSentenceMatchResult {
  match: PdfSentenceMatch | null;
  spans: PdfTextSpan[];
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

function hasPdfLocationGeometry(location: PdfSentenceLocation | null | undefined): boolean {
  return Boolean(location) && (
    (location?.rects.length ?? 0) > 0
    || (location?.line_rects.length ?? 0) > 0
    || (location?.block_rects.length ?? 0) > 0
  );
}

function estimatePdfTargetPageForSentence(
  sentence: string,
  pageTexts: Map<number, string>,
  totalPages: number,
  hintPageIndex?: number | null
): number | null {
  if (totalPages <= 0) {
    return null;
  }
  const normalizedSentence = normalizeSyncText(sentence);
  const tokens = normalizedSentence.split(" ").filter((token) => token.length > 2);
  if (tokens.length === 0) {
    return hintPageIndex ?? 0;
  }

  const scorePage = (pageIndex: number): number => {
    const pageText = pageTexts.get(pageIndex) ?? "";
    if (!pageText) {
      return 0;
    }
    let hits = 0;
    for (const token of tokens) {
      if (pageText.includes(token)) {
        hits += 1;
      }
    }
    return hits / tokens.length;
  };

  let bestPage = -1;
  let bestScore = 0;
  const searchOrder: number[] = [];
  if (hintPageIndex !== null && hintPageIndex !== undefined) {
    for (const pageIndex of pageIndexesAround(hintPageIndex, totalPages, 6)) {
      searchOrder.push(pageIndex);
    }
  }
  for (let pageIndex = 0; pageIndex < totalPages; pageIndex += 1) {
    if (!searchOrder.includes(pageIndex)) {
      searchOrder.push(pageIndex);
    }
  }

  for (const pageIndex of searchOrder) {
    const score = scorePage(pageIndex);
    if (score > bestScore) {
      bestScore = score;
      bestPage = pageIndex;
    }
  }

  if (bestPage >= 0 && bestScore >= 0.34) {
    return bestPage;
  }
  return hintPageIndex ?? null;
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

function normalizePdfMatchText(value: string): string {
  return normalizeSyncText(
    value
      .replace(/[\u00AD\u200B\u200C\u200D\u2060\uFEFF]/g, "")
      .replace(/[\u2010\u2011\u2012\u2013\u2014\u2015]/g, "-")
  );
}

function needsPdfMatchJoiner(left: string, right: string): boolean {
  if (!left || !right) {
    return false;
  }
  const leftChar = left[left.length - 1] ?? "";
  const rightChar = right[0] ?? "";
  return /\p{L}|\p{N}/u.test(leftChar) && /\p{L}|\p{N}/u.test(rightChar);
}

function collectPageSentenceSpanIndexes(
  ranges: Array<{ start: number; end: number }>,
  start: number,
  end: number
): number[] {
  const indexes: number[] = [];
  for (let idx = 0; idx < ranges.length; idx += 1) {
    const range = ranges[idx];
    if (range.start < 0 || range.end <= start || range.start >= end) {
      continue;
    }
    indexes.push(idx);
  }
  return indexes;
}

function scoreSentenceWindow(windowText: string, sentenceTokens: string[]): number {
  if (sentenceTokens.length === 0) {
    return 0;
  }
  const normalizedWindow = normalizePdfMatchText(windowText);
  let hits = 0;
  for (const token of sentenceTokens) {
    if (normalizedWindow.includes(token)) {
      hits += 1;
    }
  }
  return hits / sentenceTokens.length;
}

function scoreMatchConfidence(match: PdfSentenceMatch | null): number {
  if (!match) {
    return -1;
  }
  if (match.confidence === "exact") {
    return 3 + match.score;
  }
  if (match.reason === "fuzzy_sentence_geometry") {
    return 2 + match.score;
  }
  if (match.reason === "paragraph_fallback") {
    return 1 + match.score;
  }
  if (match.confidence === "page") {
    return 0.5 + match.score;
  }
  return match.score;
}

function findSentenceMatchInRenderedPages(
  pages: RenderedPdfPage[],
  sentence: string
): DirectSentenceMatchResult {
  const normalizedSentence = normalizePdfMatchText(sentence);
  const sentenceTokens = normalizedSentence.split(" ").filter((token) => token.length > 2);
  if (!normalizedSentence || pages.length === 0) {
    return { match: null, spans: [] };
  }

  const spans = flattenRenderedPdfSpans(pages);
  let pageSpanOffset = 0;
  let bestMatch: PdfSentenceMatch | null = null;
  for (const page of pages) {
    const normalizedSpanTexts = page.spans.map((span) => normalizePdfMatchText(span.text));
    let normalizedPageText = "";
    const ranges: Array<{ start: number; end: number }> = [];
    for (let idx = 0; idx < normalizedSpanTexts.length; idx += 1) {
      const text = normalizedSpanTexts[idx] ?? "";
      if (!text) {
        ranges.push({ start: -1, end: -1 });
        continue;
      }
      if (normalizedPageText && needsPdfMatchJoiner(normalizedPageText, text)) {
        normalizedPageText += " ";
      }
      const start = normalizedPageText.length;
      normalizedPageText += text;
      ranges.push({ start, end: normalizedPageText.length });
    }

    const exactStart = normalizedPageText.indexOf(normalizedSentence);
    if (exactStart >= 0) {
      const localSpanIndexes = collectPageSentenceSpanIndexes(ranges, exactStart, exactStart + normalizedSentence.length);
      if (localSpanIndexes.length > 0) {
        return {
          match: {
            confidence: "exact",
            reason: "exact_geometry",
            pageIndex: page.pageIndex,
            spanIndexes: localSpanIndexes.map((index) => pageSpanOffset + index),
            score: 1
          },
          spans
        };
      }
    }

    let bestPageMatch: PdfSentenceMatch | null = null;
    for (let startIdx = 0; startIdx < page.spans.length; startIdx += 1) {
      const startText = normalizedSpanTexts[startIdx] ?? "";
      if (!startText) {
        continue;
      }
      for (let endIdx = startIdx; endIdx < Math.min(page.spans.length, startIdx + 20); endIdx += 1) {
        const windowText = normalizedSpanTexts
          .slice(startIdx, endIdx + 1)
          .filter((value) => value.length > 0)
          .join(" ");
        const tokenScore = scoreSentenceWindow(windowText, sentenceTokens);
        if (tokenScore < 0.58) {
          continue;
        }
        const lengthPenalty = Math.min(
          0.2,
          Math.abs(windowText.length - normalizedSentence.length) / Math.max(normalizedSentence.length, 1)
        );
        const score = Number(Math.max(0, tokenScore - lengthPenalty).toFixed(2));
        const candidate: PdfSentenceMatch = {
          confidence: "fallback",
          reason: "fuzzy_sentence_geometry",
          pageIndex: page.pageIndex,
          spanIndexes: Array.from({ length: endIdx - startIdx + 1 }, (_, offset) => pageSpanOffset + startIdx + offset),
          score
        };
        if (scoreMatchConfidence(candidate) > scoreMatchConfidence(bestPageMatch)) {
          bestPageMatch = candidate;
        }
      }
    }

    if (!bestPageMatch && sentenceTokens.length > 0) {
      let bestSpanScore = 0;
      let bestSpanIndex: number | null = null;
      for (let idx = 0; idx < normalizedSpanTexts.length; idx += 1) {
        const score = scoreSentenceWindow(normalizedSpanTexts[idx] ?? "", sentenceTokens);
        if (score > bestSpanScore) {
          bestSpanScore = score;
          bestSpanIndex = idx;
        }
      }
      if (bestSpanIndex !== null && bestSpanScore >= 0.34) {
        bestPageMatch = {
          confidence: "fallback",
          reason: "paragraph_fallback",
          pageIndex: page.pageIndex,
          spanIndexes: [pageSpanOffset + bestSpanIndex],
          score: Number(bestSpanScore.toFixed(2))
        };
      }
    }

    if (scoreMatchConfidence(bestPageMatch) > scoreMatchConfidence(bestMatch)) {
      bestMatch = bestPageMatch;
    }
    pageSpanOffset += page.spans.length;
  }

  return {
    match: bestMatch,
    spans
  };
}

function estimateCandidatePdfPagesForSentence(
  sentence: string,
  pageTexts: Map<number, string>,
  totalPages: number,
  hintPageIndex?: number | null
): number[] {
  if (totalPages <= 0) {
    return [];
  }
  const normalizedSentence = normalizePdfMatchText(sentence);
  const tokens = normalizedSentence.split(" ").filter((token) => token.length > 2);
  const scoredPages: Array<{ pageIndex: number; score: number }> = [];
  for (let pageIndex = 0; pageIndex < totalPages; pageIndex += 1) {
    const pageText = pageTexts.get(pageIndex) ?? "";
    if (!pageText) {
      continue;
    }
    let hits = 0;
    for (const token of tokens) {
      if (pageText.includes(token)) {
        hits += 1;
      }
    }
    scoredPages.push({
      pageIndex,
      score: tokens.length === 0 ? 0 : hits / tokens.length
    });
  }
  scoredPages.sort((left, right) => right.score - left.score);

  const candidatePages: number[] = [];
  const pushPage = (pageIndex: number | null | undefined) => {
    if (pageIndex === null || pageIndex === undefined) {
      return;
    }
    const normalizedIndex = clamp(pageIndex, 0, totalPages - 1);
    if (!candidatePages.includes(normalizedIndex)) {
      candidatePages.push(normalizedIndex);
    }
  };

  pushPage(hintPageIndex ?? null);
  if (hintPageIndex !== null && hintPageIndex !== undefined) {
    for (const pageIndex of pageIndexesAround(hintPageIndex, totalPages, 2)) {
      pushPage(pageIndex);
    }
  }
  for (const page of scoredPages.slice(0, 5)) {
    pushPage(page.pageIndex);
  }
  if (candidatePages.length === 0) {
    pushPage(0);
  }
  return candidatePages;
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
    const cachedPdfLocationsRef = useRef<PdfSentenceLocation[] | null>(null);
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

    const canSyncHighlights = reader.pdf_runtime_policy
      ? reader.pdf_runtime_policy.pretty_sync_enabled
      : reader.pdf_sync_strategy !== "render_only";
    const modeLabel = reader.pdf_geometry_mode ? reader.pdf_geometry_mode.replaceAll("_", " ") : "unknown";
    const strategyLabel = reader.pdf_sync_strategy ? reader.pdf_sync_strategy.replaceAll("_", " ") : "unknown";
    const documentClassLabel = reader.pdf_classification?.document_class
      ? reader.pdf_classification.document_class.replaceAll("_", " ")
      : "unknown";
    const ocrRecommendationLabel = reader.pdf_classification?.ocr_recommendation
      ? reader.pdf_classification.ocr_recommendation.replaceAll("_", " ")
      : "unknown";
    const highlightPolicyLabel = reader.pdf_runtime_policy?.sentence_highlight_policy
      ? reader.pdf_runtime_policy.sentence_highlight_policy.replaceAll("_", " ")
      : strategyLabel;
    const globalSentenceStart = globalSentenceStartForReader(reader);
    const highlightedSentenceIdx = reader.highlighted_sentence_idx ?? null;

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

    const ensureRenderedPageWindow = useCallback(async (targetPage: number, radius: number) => {
      const pdf = pdfDocRef.current;
      const pdfjs = pdfJsModuleRef.current;
      if (!pdf || !pdfjs || pdfPageCount <= 0) {
        return;
      }
      const generation = renderGenerationRef.current;
      for (const pageIndex of pageIndexesAround(targetPage, pdfPageCount, radius)) {
        await ensurePageRendered(pdfjs, pdf, pageIndex, zoom, generation);
      }
    }, [ensurePageRendered, pdfPageCount, zoom]);

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

    const resolveCurrentSentenceHighlight = useCallback(async (sentenceIdx: number) => {
      const sentence = reader.sentences[sentenceIdx] ?? "";
      if (!sentence.trim()) {
        return {
          match: null as PdfSentenceMatch | null,
          cachedLocation: null as PdfSentenceLocation | null,
          spans: [] as PdfTextSpan[]
        };
      }
      const globalSentenceIdx = globalSentenceStart + sentenceIdx;
      const cachedLocation = cachedPdfLocationsRef.current?.find(
        (location) => location.sentence_idx === globalSentenceIdx
      ) ?? null;
      if (cachedLocation && hasPdfLocationGeometry(cachedLocation)) {
        return {
          match: {
            confidence: cachedLocation.confidence === "exact" || cachedLocation.confidence === "fallback" || cachedLocation.confidence === "page"
              ? cachedLocation.confidence
              : "missing",
            reason: cachedLocation.reason === "exact_geometry"
              || cachedLocation.reason === "fuzzy_sentence_geometry"
              || cachedLocation.reason === "paragraph_fallback"
              || cachedLocation.reason === "page_location_only"
              ? cachedLocation.reason
              : "missing",
            pageIndex: cachedLocation.page_idx,
            spanIndexes: [],
            score: cachedLocation.score
          } satisfies PdfSentenceMatch,
          cachedLocation,
          spans: [] as PdfTextSpan[]
        };
      }

      const fallbackTargetPage = estimatePdfTargetPage(
        cachedPdfLocationsRef.current,
        reader,
        pdfPageCount,
        pdfPageTextsRef.current
      );
      const targetPage = estimatePdfTargetPageForSentence(
        sentence,
        pdfPageTextsRef.current,
        pdfPageCount,
        cachedLocation?.page_idx ?? fallbackTargetPage
      ) ?? fallbackTargetPage;
      const candidatePageIndexes = estimateCandidatePdfPagesForSentence(
        sentence,
        pdfPageTextsRef.current,
        pdfPageCount,
        targetPage
      );

      let bestResult: DirectSentenceMatchResult = { match: null, spans: [] };
      for (const candidatePageIndex of candidatePageIndexes) {
        await ensureRenderedPageWindow(candidatePageIndex, 1);
        const candidatePages = renderedPagesRef.current.filter((page) => Math.abs(page.pageIndex - candidatePageIndex) <= 1);
        const result = findSentenceMatchInRenderedPages(candidatePages, sentence);
        if (scoreMatchConfidence(result.match) > scoreMatchConfidence(bestResult.match)) {
          bestResult = result;
        }
        if (result.match?.confidence === "exact") {
          break;
        }
      }
      logPdfDebug("currentSentenceResolve", {
        sentenceIdx,
        targetPage,
        candidatePages: candidatePageIndexes,
        matchedPage: bestResult.match?.pageIndex ?? null,
        confidence: bestResult.match?.confidence ?? "missing",
        reason: bestResult.match?.reason ?? "missing",
        score: bestResult.match?.score ?? 0
      });
      return {
        match: bestResult.match,
        cachedLocation: null,
        spans: bestResult.spans
      };
    }, [ensureRenderedPageWindow, globalSentenceStart, pdfPageCount, reader]);

    const applyHighlight = useCallback(
      async (behavior: ScrollBehavior, force = false) => {
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
        const resolved = await resolveCurrentSentenceHighlight(idx);
        const cachedLocation = resolved.cachedLocation;
        const match = resolved.match;
        const spans = resolved.spans;
        sentenceMatchesRef.current = reader.sentences.map((_, sentenceIndex) => (
          sentenceIndex === idx && match
            ? match
            : {
              confidence: "missing",
              reason: "missing",
              pageIndex: null,
              spanIndexes: [],
              score: 0
            } satisfies PdfSentenceMatch
        ));
        setMappingSummary(match
          ? {
              exact: match.confidence === "exact" ? 1 : 0,
              fallback: match.confidence === "fallback" ? 1 : 0,
              pageOnly: match.confidence === "page" ? 1 : 0,
              missing: match.confidence === "missing" ? 1 : 0
            }
          : {
              exact: 0,
              fallback: 0,
              pageOnly: 0,
              missing: 1
            });
        setActiveMatch(match ?? null);
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
      [canSyncHighlights, reader, resolveCurrentSentenceHighlight]
    );

    useImperativeHandle(ref, () => ({
      jumpToHighlightedSentence() {
        void applyHighlight("smooth", true);
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
        void applyHighlight("auto");
      });
      return () => {
        cancelled = true;
      };
    }, [applyHighlight, cachedSyncVersion, ensureTargetPagesRendered, globalSentenceStart, highlightedSentenceIdx, loading, pdfPageTextVersion, zoom]);

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
            Native PDF | class: {documentClassLabel} | geometry: {modeLabel} | sync: {strategyLabel} | policy: {highlightPolicyLabel}
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
            {reader.pdf_runtime_policy?.explanation
              ?? (reader.pdf_geometry_mode === "ocr_required"
                ? "This PDF is renderable now, but precise highlight sync and text playback will stay gated until OCR produces usable text."
                : "This PDF is render-only right now. Text-only/TTS can continue, but precise PDF highlight sync is unavailable.")}
          </Typography>
        ) : null}
        {!error && reader.pdf_classification ? (
          <Typography color="text.secondary" variant="caption" data-testid="reader-pretty-pdf-classification">
            OCR: {ocrRecommendationLabel} | Confidence: {reader.pdf_classification.confidence.toFixed(2)}
            {reader.pdf_classification.reasons.length > 0
              ? ` | Why: ${reader.pdf_classification.reasons.slice(0, 2).join("; ").replaceAll("_", " ")}`
              : ""}
          </Typography>
        ) : null}
        {!error && reader.pdf_classification ? (
          <Typography color="text.secondary" variant="caption">
            Trust: block {reader.pdf_classification.trust_diagnostics.block_coherence.toFixed(2)} | coordinates{" "}
            {reader.pdf_classification.trust_diagnostics.coordinate_sanity.toFixed(2)} | order{" "}
            {reader.pdf_classification.trust_diagnostics.reading_order_stability.toFixed(2)} | image pages{" "}
            {(reader.pdf_classification.feature_summary.image_page_ratio * 100).toFixed(1)}%
            {reader.pdf_classification.trust_diagnostics.hidden_text_layer_suspected
              ? " | Hidden text layer suspected"
              : ""}
            {reader.pdf_classification.trust_diagnostics.invisible_text_suspected
              ? " | Invisible text suspected"
              : ""}
            {reader.pdf_classification.trust_diagnostics.stacked_duplicate_text_suspected
              ? " | Stacked duplicate text suspected"
              : ""}
          </Typography>
        ) : null}
        {!error && reader.pdf_runtime_policy ? (
          <Typography color="text.secondary" variant="caption" data-testid="reader-pretty-pdf-policy">
            Text: {reader.pdf_runtime_policy.text_only_policy.replaceAll("_", " ")} | Search: {reader.pdf_runtime_policy.search_policy.replaceAll("_", " ")} | Bookmark: {reader.pdf_runtime_policy.bookmark_policy.replaceAll("_", " ")}
            {reader.pdf_runtime_policy.degraded_reasons.length > 0
              ? ` | Degraded: ${reader.pdf_runtime_policy.degraded_reasons.slice(0, 2).join("; ").replaceAll("_", " ")}`
              : ""}
          </Typography>
        ) : null}
        {!error && reader.pdf_ocr_alignment ? (
          <Typography color="text.secondary" variant="caption" data-testid="reader-pretty-pdf-ocr-alignment">
            OCR geometry: {reader.pdf_ocr_alignment.quality_class.replaceAll("_", " ")} | Source:{" "}
            {reader.pdf_ocr_alignment.source_kind.replaceAll("_", " ")} | Coverage:{" "}
            {(reader.pdf_ocr_alignment.coverage_ratio * 100).toFixed(1)}% | Rects:{" "}
            {reader.pdf_ocr_alignment.rect_mapped_sentence_count} | Line:{" "}
            {reader.pdf_ocr_alignment.line_mapped_sentence_count} | Block:{" "}
            {reader.pdf_ocr_alignment.block_mapped_sentence_count} | Page-only:{" "}
            {reader.pdf_ocr_alignment.page_only_sentence_count} | Missing:{" "}
            {reader.pdf_ocr_alignment.unmappable_sentence_count} | Reused:{" "}
            {reader.pdf_ocr_alignment.reused_alignment_count} | Rebuilt:{" "}
            {reader.pdf_ocr_alignment.rebuilt_alignment_count} | Build:{" "}
            {reader.pdf_ocr_alignment.alignment_build_ms} ms
          </Typography>
        ) : null}
        {!error && reader.pdf_ocr_pipeline ? (
          <Typography color="text.secondary" variant="caption" data-testid="reader-pretty-pdf-ocr-pipeline">
            OCR engine: {reader.pdf_ocr_pipeline.engine_policy.replaceAll("_", " ")} | OCR enabled:{" "}
            {reader.pdf_ocr_pipeline.ocr_enabled ? "yes" : "no"} | Chunks: {reader.pdf_ocr_pipeline.chunk_count} | Order:{" "}
            {reader.pdf_ocr_pipeline.reading_order_mode.replaceAll("_", " ")}
            {reader.pdf_ocr_pipeline.fallback_decisions.length > 0
              ? ` | Fallbacks: ${reader.pdf_ocr_pipeline.fallback_decisions.join("; ").replaceAll("_", " ")}`
              : ""}
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
