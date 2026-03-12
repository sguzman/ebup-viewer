import { Button, ButtonGroup, CircularProgress, Stack, Typography } from "@mui/material";
import type { PDFDocumentProxy } from "pdfjs-dist/legacy/build/pdf.mjs";
import { forwardRef, useCallback, useEffect, useImperativeHandle, useRef, useState } from "react";

import { backendApi, type PdfSentenceLocation } from "../api/tauri";
import type { ReaderPlaybackState, ReaderSnapshot } from "../types";
import { recordPerfCounter, recordPerfGauge, recordPerfMeasure } from "../perf/debug";
import { clamp, normalizeNumber } from "./readerShared";
import {
  buildPdfSentenceSpanMap,
  findNearestSentenceForPageIndex,
  findNearestSentenceForSpanIndex,
  type PdfSentenceMatch,
  type PdfTextSpan,
  scorePdfSentenceMatch
} from "./pdfTextSync";
import { applyPdfHighlightDom } from "./pdfHighlightDom";
import { orderPdfTextLayerSpans, type PdfTextLayerGeometry } from "./pdfTextLayer";
import { normalizeSyncText } from "./htmlSync";
import {
  buildOverlayRectsFromLocation,
  buildOverlayRectsFromMatch,
  type PdfOverlayRect
} from "./pdfOverlayGeometry";
import { applyPdfLocationHighlightOverlays, clearPdfHighlightOverlays } from "./pdfOverlayDom";
import {
  buildPdfViewportRenderPlan,
  choosePdfViewportEvictions,
  computePdfPreviewScale,
  type PdfPageLifecycleState,
  type PdfPageRegistryEntry
} from "./pdfViewportScheduler";
import {
  readPdfPerformanceProfileOverride,
  resolvePdfPerformanceProfile
} from "./pdfPerformanceProfile";
import {
  createLruCache,
  pdfBitmapArtifactKey,
  pdfSpanArtifactKey,
  type PdfBitmapArtifact
} from "./pdfArtifactCache";
import {
  buildCachedPdfHighlightTarget,
  canReuseCachedPdfHighlightTarget,
  type CachedPdfHighlightTarget
} from "./pdfHighlightController";
import { buildPdfOpenPlan } from "./pdfDocumentModel";
let pdfJsImportPromise: Promise<typeof import("pdfjs-dist/legacy/build/pdf.mjs")> | null = null;
let pdfJsWorkerImportPromise: Promise<typeof import("pdfjs-dist/legacy/build/pdf.worker.mjs")> | null = null;
type PdfJsModule = typeof import("pdfjs-dist/legacy/build/pdf.mjs");
type PdfPageTextContent =
  Awaited<ReturnType<Awaited<ReturnType<PDFDocumentProxy["getPage"]>>["getTextContent"]>>;

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
  playback: ReaderPlaybackState;
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
  renderedZoom: number;
  textLayerZoom: number | null;
  textLayerDiv: HTMLDivElement | null;
}

interface DirectSentenceMatchResult {
  match: PdfSentenceMatch | null;
  spans: PdfTextSpan[];
}

function normalizeCachedPdfMatchReason(reason: string): PdfSentenceMatch["reason"] {
  switch (reason) {
    case "exact_token_chain_alignment":
    case "normalized_sentence_alignment":
    case "line_window_fuzzy_alignment":
    case "block_fallback_alignment":
    case "page_location_only":
    case "missing":
      return reason;
    case "exact_geometry":
      return "exact_token_chain_alignment";
    case "fuzzy_sentence_geometry":
      return "line_window_fuzzy_alignment";
    case "paragraph_fallback":
      return "block_fallback_alignment";
    default:
      return "missing";
  }
}

function normalizeCachedPdfMatchConfidence(
  confidence: string,
  reason: PdfSentenceMatch["reason"]
): PdfSentenceMatch["confidence"] {
  if (confidence === "exact" || confidence === "fallback" || confidence === "page" || confidence === "missing") {
    return confidence;
  }
  switch (reason) {
    case "exact_token_chain_alignment":
      return "exact";
    case "normalized_sentence_alignment":
    case "line_window_fuzzy_alignment":
    case "block_fallback_alignment":
      return "fallback";
    case "page_location_only":
      return "page";
    default:
      return "missing";
  }
}

interface HighlightOverlayResult {
  highlightedNodes: HTMLElement[];
  highlightedPages: HTMLDivElement[];
  highlightedOverlays: HTMLDivElement[];
}

interface ActivePdfHighlightState {
  sentenceIdx: number;
  match: PdfSentenceMatch;
  overlayRects: PdfOverlayRect[];
  useOverlay: boolean;
}

interface PdfShellMetric {
  width: number;
  height: number;
}

const PDF_ZOOM_SETTLE_MS = 140;

function resolvePdfVisibleOverscan(): number {
  const profile = resolvePdfPerformanceProfile({
    deviceMemory: typeof navigator !== "undefined" && "deviceMemory" in navigator
      ? Number((navigator as Navigator & { deviceMemory?: number }).deviceMemory)
      : null,
    hardwareConcurrency: typeof navigator !== "undefined" ? navigator.hardwareConcurrency : null,
    override: typeof window !== "undefined" ? readPdfPerformanceProfileOverride(window.localStorage) : null
  });
  if (typeof window === "undefined") {
    return profile.overscan;
  }
  const raw = window.localStorage.getItem("ll.pdfOverscan");
  const parsed = Number.parseInt(raw ?? "", 10);
  if (!Number.isFinite(parsed)) {
    return profile.overscan;
  }
  return clamp(parsed, 0, 4);
}

function buildPdfTextLayerGeometry(
  textContent: PdfPageTextContent,
  elementCount?: number
): PdfTextLayerGeometry[] {
  const items = Array.isArray(textContent.items) ? textContent.items : [];
  const geometry = items.map((item) => {
    const candidate = item as {
      height?: number;
      str?: string;
      transform?: [number, number, number, number, number, number];
      width?: number;
    };
    return {
      height: candidate.height,
      left: candidate.transform?.[4],
      text: candidate.str ?? "",
      top: candidate.transform?.[5],
      width: candidate.width
    };
  });
  const filtered = geometry.filter((entry) => entry.text.trim().length > 0);
  if (elementCount !== undefined && filtered.length >= elementCount) {
    return filtered.slice(0, elementCount);
  }
  return filtered;
}

function clonePdfCanvas(canvas: HTMLCanvasElement): HTMLCanvasElement {
  const clone = document.createElement("canvas");
  clone.className = canvas.className;
  clone.width = canvas.width;
  clone.height = canvas.height;
  clone.style.width = canvas.style.width;
  clone.style.height = canvas.style.height;
  const context = clone.getContext("2d");
  if (context) {
    context.drawImage(canvas, 0, 0);
  }
  return clone;
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

function markPdfPageLifecycle(
  registry: Map<number, PdfPageRegistryEntry>,
  pageIndex: number,
  state: PdfPageLifecycleState,
  patch?: Partial<PdfPageRegistryEntry>
): void {
  const existing = registry.get(pageIndex);
  registry.set(pageIndex, {
    pageIndex,
    ...existing,
    renderedZoom: existing?.renderedZoom ?? null,
    textLayerZoom: existing?.textLayerZoom ?? null,
    ...patch,
    state,
    lastTouchedAt: Date.now()
  });
}

function bumpPageRenderVersion(map: Map<number, number>, pageIndex: number): number {
  const next = (map.get(pageIndex) ?? 0) + 1;
  map.set(pageIndex, next);
  return next;
}

function hasCurrentPageRenderVersion(
  map: Map<number, number>,
  pageIndex: number,
  version: number
): boolean {
  return (map.get(pageIndex) ?? 0) === version;
}

function estimatePdfTargetPage(
  locations: PdfSentenceLocation[] | null,
  totalPages: number,
  options: {
    globalSentenceStart: number;
    highlightedSentenceIdx: number | null;
    pageStartPercent: number;
    pageText: string;
    sentences: string[];
  },
  pageTexts?: Map<number, string>
): number {
  if (totalPages <= 1) {
    return 0;
  }
  const globalSentenceIdx = options.globalSentenceStart + (options.highlightedSentenceIdx ?? 0);
  const direct = locations?.find((location) => location.sentence_idx === globalSentenceIdx);
  if (direct?.page_idx !== null && direct?.page_idx !== undefined) {
    return clamp(direct.page_idx, 0, totalPages - 1);
  }
  const normalizedQuery = normalizeSyncText(options.pageText || options.sentences.join(" "));
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
  const startPct = clamp(options.pageStartPercent, 0, 100) / 100;
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

function findSentenceMatchInRenderedPages(
  pages: RenderedPdfPage[],
  sentence: string
): DirectSentenceMatchResult {
  const normalizedSentence = normalizePdfMatchText(sentence);
  if (!normalizedSentence || pages.length === 0) {
    return { match: null, spans: [] };
  }

  const spans = flattenRenderedPdfSpans(pages);
  const result = buildPdfSentenceSpanMap(spans, [sentence]);
  return {
    match: result.matches[0] ?? null,
    spans
  };
}

function findCachedSentenceMatchInRenderedPages(
  pages: RenderedPdfPage[],
  sentence: string,
  spanCache: ReturnType<typeof createLruCache<string, PdfTextSpan[]>>
): DirectSentenceMatchResult {
  const normalizedSentence = normalizePdfMatchText(sentence);
  if (!normalizedSentence || pages.length === 0) {
    return { match: null, spans: [] };
  }

  const spans: PdfTextSpan[] = [];
  for (const page of pages) {
    const cacheKey = pdfSpanArtifactKey(page.pageIndex, page.textLayerZoom ?? page.renderedZoom);
    const cached = spanCache.get(cacheKey);
    if (cached) {
      spans.push(...cached);
      continue;
    }
    spanCache.set(cacheKey, page.spans);
    spans.push(...page.spans);
  }
  const result = buildPdfSentenceSpanMap(spans, [sentence]);
  return {
    match: result.matches[0] ?? null,
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

function isPdfScrollTargetMostlyVisible(target: HTMLElement): boolean {
  if (typeof window === "undefined") {
    return true;
  }
  const rect = target.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) {
    return false;
  }
  const viewportWidth = window.innerWidth || document.documentElement.clientWidth || 0;
  const viewportHeight = window.innerHeight || document.documentElement.clientHeight || 0;
  const visibleWidth = Math.max(0, Math.min(rect.right, viewportWidth) - Math.max(rect.left, 0));
  const visibleHeight = Math.max(0, Math.min(rect.bottom, viewportHeight) - Math.max(rect.top, 0));
  const visibleArea = visibleWidth * visibleHeight;
  const totalArea = rect.width * rect.height;
  if (totalArea <= 0) {
    return false;
  }
  return (visibleArea / totalArea) >= 0.55;
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
  function ReaderPrettyPdfPane({ onSentenceClick: _onSentenceClick, playback, reader, sourcePath }, ref) {
    const performanceProfileRef = useRef(resolvePdfPerformanceProfile({
      deviceMemory: typeof navigator !== "undefined" && "deviceMemory" in navigator
        ? Number((navigator as Navigator & { deviceMemory?: number }).deviceMemory)
        : null,
      hardwareConcurrency: typeof navigator !== "undefined" ? navigator.hardwareConcurrency : null,
      override: typeof window !== "undefined" ? readPdfPerformanceProfileOverride(window.localStorage) : null
    }));
    const performanceProfile = performanceProfileRef.current;
    const containerRef = useRef<HTMLDivElement | null>(null);
    const renderedPagesRef = useRef<RenderedPdfPage[]>([]);
    const pageRegistryRef = useRef<Map<number, PdfPageRegistryEntry>>(new Map());
    const pageShellsRef = useRef<Map<number, HTMLDivElement>>(new Map());
    const pageMetricsRef = useRef<Map<string, PdfShellMetric>>(new Map());
    const pdfPageTextsRef = useRef<Map<number, string>>(new Map());
    const pdfTextContentCacheRef = useRef<Map<number, PdfPageTextContent>>(new Map());
    const bitmapArtifactCacheRef = useRef(createLruCache<string, PdfBitmapArtifact<HTMLCanvasElement>>(performanceProfile.bitmapArtifacts));
    const spanArtifactCacheRef = useRef(createLruCache<string, PdfTextSpan[]>(performanceProfile.spanArtifacts));
    const sentenceTargetCacheRef = useRef(createLruCache<number, CachedPdfHighlightTarget>(performanceProfile.sentenceTargets));
    const sentencePageTargetCacheRef = useRef(createLruCache<number, number>(performanceProfile.sentenceTargets));
    const renderedPageZoomRef = useRef<Map<number, number>>(new Map());
    const visiblePageIndexesRef = useRef<Set<number>>(new Set());
    const selectionPageIndexRef = useRef<number | null>(null);
    const canvasRenderVersionRef = useRef<Map<number, number>>(new Map());
    const textLayerRenderVersionRef = useRef<Map<number, number>>(new Map());
    const zoomSettleTimerRef = useRef<number | null>(null);
    const pendingJumpTargetPageRef = useRef<number | null>(null);
    const defaultShellMetricRef = useRef<PdfShellMetric>({ width: 720, height: 980 });
    const lowPriorityPrefetchTimerRef = useRef<number | null>(null);
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
    const overlaySentenceMapRef = useRef<Map<number, number>>(new Map());
    const activeHighlightStateRef = useRef<ActivePdfHighlightState | null>(null);
    const documentOpenStartedAtRef = useRef(0);
    const [zoom, setZoom] = useState(1.2);
    const [renderZoom, setRenderZoom] = useState(1.2);
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
    const preferOverlayHighlights = Boolean(
      reader.pdf_ocr_alignment
      && reader.pdf_ocr_alignment.quality_class !== "ocr_failed_or_unusable"
      && reader.pdf_ocr_pipeline?.engine_policy !== "embedded_text_only"
    );
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
    const globalSentenceStart = playback.stats.sentences_read_up_to_page_start;
    const highlightedSentenceIdx = playback.highlighted_sentence_idx ?? null;

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
      overlaySentenceMapRef.current.clear();
      renderedPagesRef.current = [];
      pageRegistryRef.current.clear();
      pageShellsRef.current.clear();
      pageMetricsRef.current.clear();
      pdfTextContentCacheRef.current.clear();
      bitmapArtifactCacheRef.current.clear();
      spanArtifactCacheRef.current.clear();
      sentenceTargetCacheRef.current.clear();
      sentencePageTargetCacheRef.current.clear();
      canvasRenderVersionRef.current.clear();
      textLayerRenderVersionRef.current.clear();
      renderedPageZoomRef.current.clear();
      visiblePageIndexesRef.current.clear();
      if (lowPriorityPrefetchTimerRef.current !== null) {
        window.clearTimeout(lowPriorityPrefetchTimerRef.current);
        lowPriorityPrefetchTimerRef.current = null;
      }
      lastScrollTargetRef.current = null;
      const root = containerRef.current;
      if (root) {
        root.innerHTML = "";
      }
    }, []);

    const ensurePdfDocumentLoaded = useCallback(async (): Promise<{ pdfjs: PdfJsModule; pdf: PDFDocumentProxy }> => {
      const startedAt = typeof performance !== "undefined" ? performance.now() : 0;
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
      recordPerfMeasure("ReaderPrettyPdfPane.loadDocument", startedAt);
      return { pdfjs, pdf };
    }, [sourcePath]);

    const ensurePageShells = useCallback(async (pdf: PDFDocumentProxy, activeZoom: number) => {
      const startedAt = typeof performance !== "undefined" ? performance.now() : 0;
      const root = containerRef.current;
      if (!root) {
        return;
      }
      const firstPage = await pdf.getPage(1);
      const firstViewport = firstPage.getViewport({ scale: clamp(activeZoom, 0.7, 2.5) });
      const baseWidth = firstViewport.width;
      const baseHeight = firstViewport.height;
      defaultShellMetricRef.current = { width: baseWidth, height: baseHeight };
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
        shell.dataset.pageLifecycle = "placeholder";
        shell.style.width = `${baseWidth}px`;
        shell.style.minHeight = `${baseHeight}px`;
        shell.style.display = "flex";
        shell.style.justifyContent = "center";
        shell.style.alignItems = "flex-start";
        shell.style.contain = "layout paint style";
        root.appendChild(shell);
        pageShellsRef.current.set(pageIndex, shell);
        markPdfPageLifecycle(pageRegistryRef.current, pageIndex, "placeholder");
      }
      setPdfPageCount(pdf.numPages);
      logPdfDebug("pageShellsReady", {
        sourcePath,
        pageCount: pdf.numPages,
        baseWidth: Math.round(baseWidth),
        baseHeight: Math.round(baseHeight)
      });
      recordPerfMeasure("ReaderPrettyPdfPane.mountPageShells", startedAt);
    }, [sourcePath]);

    const syncPdfPreviewZoom = useCallback((displayZoom: number) => {
      const fallbackMetric = defaultShellMetricRef.current;
      for (const [pageIndex, shell] of pageShellsRef.current.entries()) {
        const renderedZoomForPage = renderedPageZoomRef.current.get(pageIndex) ?? renderZoom;
        const metric = pageMetricsRef.current.get(`${pageIndex}:${renderedZoomForPage}`) ?? fallbackMetric;
        const previewScale = computePdfPreviewScale(displayZoom, renderedZoomForPage);
        shell.style.width = `${metric.width * previewScale}px`;
        shell.style.minHeight = `${metric.height * previewScale}px`;
        const pageContainer = shell.firstElementChild as HTMLDivElement | null;
        if (pageContainer) {
          pageContainer.style.transformOrigin = "top center";
          pageContainer.style.transform = previewScale === 1 ? "" : `scale(${previewScale})`;
        }
      }
      recordPerfGauge("ReaderPrettyPdfPane.liveRenderedPages", renderedPagesRef.current.length);
    }, [renderZoom]);

    const ensurePageCanvasRendered = useCallback(async (
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
        recordPerfCounter("ReaderPrettyPdfPane.canceledPageRenders");
        return;
      }
      const existingEntry = renderedPagesRef.current.find((entry) => entry.pageIndex === pageIndex);
      if (existingEntry && existingEntry.renderedZoom === activeZoom) {
        markPdfPageLifecycle(pageRegistryRef.current, pageIndex, existingEntry.textLayerZoom === activeZoom ? "text_ready" : "canvas_ready");
        recordPerfCounter("ReaderPrettyPdfPane.pageCanvasCacheHit");
        return;
      }
      recordPerfCounter("ReaderPrettyPdfPane.pageCanvasCacheMiss");
      const bitmapCacheKey = pdfBitmapArtifactKey(pageIndex, activeZoom);
      const cachedBitmap = bitmapArtifactCacheRef.current.get(bitmapCacheKey);
      if (cachedBitmap) {
        shell.style.width = `${cachedBitmap.width}px`;
        shell.style.minHeight = `${cachedBitmap.height}px`;
        shell.innerHTML = "";
        const pageContainer = document.createElement("div");
        pageContainer.className = "reader-pdf-page";
        pageContainer.dataset.pageIndex = String(pageIndex);
        pageContainer.dataset.pdfRotation = String(cachedBitmap.rotation);
        pageContainer.style.width = `${cachedBitmap.width}px`;
        pageContainer.style.height = `${cachedBitmap.height}px`;
        pageContainer.appendChild(clonePdfCanvas(cachedBitmap.bitmap));
        shell.appendChild(pageContainer);
        renderedPageZoomRef.current.set(pageIndex, activeZoom);
        renderedPagesRef.current = [
          ...renderedPagesRef.current.filter((entry) => entry.pageIndex !== pageIndex),
          { container: pageContainer, pageIndex, spans: [], renderedZoom: activeZoom, textLayerZoom: null, textLayerDiv: null }
        ].sort((left, right) => left.pageIndex - right.pageIndex);
        markPdfPageLifecycle(pageRegistryRef.current, pageIndex, "canvas_ready", { renderedZoom: activeZoom, textLayerZoom: null });
        shell.dataset.pageLifecycle = "canvas_ready";
        recordPerfCounter("ReaderPrettyPdfPane.pageBitmapCacheHit");
        recordPerfGauge("ReaderPrettyPdfPane.bitmapArtifactCacheSize", bitmapArtifactCacheRef.current.size());
        setRenderVersion((value) => value + 1);
        syncPdfPreviewZoom(zoom);
        return;
      }
      recordPerfCounter("ReaderPrettyPdfPane.pageBitmapCacheMiss");
      const requestVersion = bumpPageRenderVersion(canvasRenderVersionRef.current, pageIndex);
      markPdfPageLifecycle(pageRegistryRef.current, pageIndex, "scheduled");
      shell.dataset.pageLifecycle = "scheduled";
      const page = await pdf.getPage(pageIndex + 1);
      if (
        renderGenerationRef.current !== generation
        || !hasCurrentPageRenderVersion(canvasRenderVersionRef.current, pageIndex, requestVersion)
      ) {
        recordPerfCounter("ReaderPrettyPdfPane.canceledPageRenders");
        return;
      }
      const startedAt = typeof performance !== "undefined" ? performance.now() : 0;
      markPdfPageLifecycle(pageRegistryRef.current, pageIndex, "rendering_canvas");
      shell.dataset.pageLifecycle = "rendering_canvas";
      const viewport = page.getViewport({ scale: clamp(activeZoom, 0.7, 2.5) });
      const metricKey = `${pageIndex}:${activeZoom}`;
      pageMetricsRef.current.set(metricKey, { width: viewport.width, height: viewport.height });
      shell.style.width = `${viewport.width}px`;
      shell.style.minHeight = `${viewport.height}px`;
      shell.innerHTML = "";

      const pageContainer = document.createElement("div");
      pageContainer.className = "reader-pdf-page";
      pageContainer.dataset.pageIndex = String(pageIndex);
      pageContainer.dataset.pdfRotation = String(viewport.rotation);
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
      shell.appendChild(pageContainer);

      await page.render({ canvas, canvasContext: context, viewport }).promise;
      if (
        renderGenerationRef.current !== generation
        || !hasCurrentPageRenderVersion(canvasRenderVersionRef.current, pageIndex, requestVersion)
      ) {
        recordPerfCounter("ReaderPrettyPdfPane.canceledPageRenders");
        return;
      }

      renderedPageZoomRef.current.set(pageIndex, activeZoom);
      renderedPagesRef.current = [
        ...renderedPagesRef.current.filter((entry) => entry.pageIndex !== pageIndex),
        { container: pageContainer, pageIndex, spans: [], renderedZoom: activeZoom, textLayerZoom: null, textLayerDiv: null }
      ].sort((left, right) => left.pageIndex - right.pageIndex);
      markPdfPageLifecycle(pageRegistryRef.current, pageIndex, "canvas_ready", { renderedZoom: activeZoom, textLayerZoom: null });
      shell.dataset.pageLifecycle = "canvas_ready";
      recordPerfMeasure("ReaderPrettyPdfPane.renderCanvas", startedAt);
      if (visiblePageIndexesRef.current.has(pageIndex)) {
        recordPerfMeasure("ReaderPrettyPdfPane.visiblePageRender", startedAt);
      }
      recordPerfGauge("ReaderPrettyPdfPane.liveRenderedPages", renderedPagesRef.current.length);
      setRenderVersion((value) => value + 1);
      syncPdfPreviewZoom(zoom);
    }, [syncPdfPreviewZoom, zoom]);

    const ensurePageTextLayerRendered = useCallback(async (
      pdfjs: PdfJsModule,
      pdf: PDFDocumentProxy,
      pageIndex: number,
      activeZoom: number,
      generation: number
    ) => {
      const renderedPage = renderedPagesRef.current.find((entry) => entry.pageIndex === pageIndex);
      if (!renderedPage || renderedPage.renderedZoom !== activeZoom) {
        return;
      }
      if (renderGenerationRef.current !== generation) {
        recordPerfCounter("ReaderPrettyPdfPane.canceledPageRenders");
        return;
      }
      if (renderedPage.textLayerZoom === activeZoom && renderedPage.textLayerDiv) {
        markPdfPageLifecycle(pageRegistryRef.current, pageIndex, "text_ready");
        recordPerfCounter("ReaderPrettyPdfPane.textLayerCacheHit");
        return;
      }
      recordPerfCounter("ReaderPrettyPdfPane.textLayerCacheMiss");
      const requestVersion = bumpPageRenderVersion(textLayerRenderVersionRef.current, pageIndex);
      const page = await pdf.getPage(pageIndex + 1);
      const viewport = page.getViewport({ scale: clamp(activeZoom, 0.7, 2.5) });
      const pageContainer = renderedPage.container;
      let textLayerDiv = renderedPage.textLayerDiv;
      if (!textLayerDiv) {
        textLayerDiv = document.createElement("div");
        textLayerDiv.className = "reader-pdf-text-layer";
        pageContainer.appendChild(textLayerDiv);
      }
      textLayerDiv.innerHTML = "";
      const extractStartedAt = typeof performance !== "undefined" ? performance.now() : 0;
      const textContent = pdfTextContentCacheRef.current.get(pageIndex) ?? await page.getTextContent();
      if (
        renderGenerationRef.current !== generation
        || !hasCurrentPageRenderVersion(textLayerRenderVersionRef.current, pageIndex, requestVersion)
      ) {
        recordPerfCounter("ReaderPrettyPdfPane.canceledPageRenders");
        return;
      }
      pdfTextContentCacheRef.current.set(pageIndex, textContent);
      recordPerfMeasure("ReaderPrettyPdfPane.extractTextLayer", extractStartedAt);
      const textLayerStartedAt = typeof performance !== "undefined" ? performance.now() : 0;
      const textLayer = new pdfjs.TextLayer({
        textContentSource: textContent,
        container: textLayerDiv,
        viewport
      });
      await textLayer.render();
      if (
        renderGenerationRef.current !== generation
        || !hasCurrentPageRenderVersion(textLayerRenderVersionRef.current, pageIndex, requestVersion)
      ) {
        recordPerfCounter("ReaderPrettyPdfPane.canceledPageRenders");
        return;
      }
      const spanElements = Array.from(textLayerDiv.querySelectorAll("span")) as HTMLElement[];
      const visibleSpanElements = spanElements.filter((element) => isVisiblePdfTextSpan(element));
      const postprocessStartedAt = typeof performance !== "undefined" ? performance.now() : 0;
      const spans: PdfTextSpan[] = orderPdfTextLayerSpans(
        visibleSpanElements,
        pageIndex,
        viewport.rotation,
        buildPdfTextLayerGeometry(textContent, visibleSpanElements.length)
      );
      recordPerfMeasure("ReaderPrettyPdfPane.textLayerPostprocess", postprocessStartedAt);
      const nextEntry = renderedPagesRef.current.find((entry) => entry.pageIndex === pageIndex);
      if (!nextEntry) {
        return;
      }
      nextEntry.spans = spans;
      nextEntry.textLayerZoom = activeZoom;
      nextEntry.textLayerDiv = textLayerDiv;
      markPdfPageLifecycle(pageRegistryRef.current, pageIndex, "text_ready", { renderedZoom: activeZoom, textLayerZoom: activeZoom });
      const shell = pageShellsRef.current.get(pageIndex);
      if (shell) {
        shell.dataset.pageLifecycle = "text_ready";
      }
      setRenderedPdfSpanIndexes(renderedPagesRef.current);
      recordPerfMeasure("ReaderPrettyPdfPane.mountTextLayer", textLayerStartedAt);
      if (visiblePageIndexesRef.current.has(pageIndex) && documentOpenStartedAtRef.current > 0) {
        recordPerfMeasure("ReaderPrettyPdfPane.firstVisibleTextLayer", documentOpenStartedAtRef.current);
        documentOpenStartedAtRef.current = 0;
      }
      recordPerfGauge("ReaderPrettyPdfPane.liveTextLayers", renderedPagesRef.current.filter((entry) => entry.textLayerDiv !== null).length);
      setRenderVersion((value) => value + 1);
    }, []);

    const evictPageArtifacts = useCallback((pageIndex: number, mode: "canvas" | "text") => {
      const shell = pageShellsRef.current.get(pageIndex);
      const entry = renderedPagesRef.current.find((candidate) => candidate.pageIndex === pageIndex);
      if (!shell || !entry) {
        return;
      }
      if (mode === "text") {
        bumpPageRenderVersion(textLayerRenderVersionRef.current, pageIndex);
        entry.spans = [];
        entry.textLayerZoom = null;
        entry.textLayerDiv?.remove();
        entry.textLayerDiv = null;
        markPdfPageLifecycle(pageRegistryRef.current, pageIndex, "canvas_ready", { textLayerZoom: null });
        shell.dataset.pageLifecycle = "canvas_ready";
        recordPerfCounter("ReaderPrettyPdfPane.evictedTextLayers");
        logPdfDebug("evictArtifacts", { pageIndex, mode, reason: "budget" });
        return;
      }
      bumpPageRenderVersion(canvasRenderVersionRef.current, pageIndex);
      bumpPageRenderVersion(textLayerRenderVersionRef.current, pageIndex);
      const canvas = entry.container.querySelector("canvas");
      if (canvas instanceof HTMLCanvasElement && entry.renderedZoom !== null) {
        const width = Number.parseFloat(canvas.style.width || "0");
        const height = Number.parseFloat(canvas.style.height || "0");
        bitmapArtifactCacheRef.current.set(pdfBitmapArtifactKey(pageIndex, entry.renderedZoom), {
          bitmap: clonePdfCanvas(canvas),
          height: Number.isFinite(height) && height > 0 ? height : canvas.height,
          pageIndex,
          rotation: Number.parseInt(entry.container.dataset.pdfRotation ?? "0", 10) || 0,
          width: Number.isFinite(width) && width > 0 ? width : canvas.width,
          zoom: entry.renderedZoom
        });
        recordPerfGauge("ReaderPrettyPdfPane.bitmapArtifactCacheSize", bitmapArtifactCacheRef.current.size());
      }
      shell.innerHTML = "";
      renderedPagesRef.current = renderedPagesRef.current.filter((candidate) => candidate.pageIndex !== pageIndex);
      renderedPageZoomRef.current.delete(pageIndex);
      markPdfPageLifecycle(pageRegistryRef.current, pageIndex, "evicted", { renderedZoom: null, textLayerZoom: null });
      shell.dataset.pageLifecycle = "evicted";
      recordPerfCounter("ReaderPrettyPdfPane.evictedCanvases");
      logPdfDebug("evictArtifacts", { pageIndex, mode, reason: "budget_or_visibility" });
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
        pdfPageCount,
        {
          globalSentenceStart,
          highlightedSentenceIdx,
          pageStartPercent: playback.stats.page_start_percent,
          pageText: reader.page_text,
          sentences: reader.sentences
        },
        pdfPageTextsRef.current
      );
      pendingJumpTargetPageRef.current = targetPage;
      for (const pageIndex of pageIndexesAround(targetPage, pdfPageCount, 1)) {
        await ensurePageCanvasRendered(pdf, pageIndex, renderZoom, generation);
        await ensurePageTextLayerRendered(pdfjs, pdf, pageIndex, renderZoom, generation);
      }
    }, [
      ensurePageCanvasRendered,
      ensurePageTextLayerRendered,
      globalSentenceStart,
      highlightedSentenceIdx,
      pdfPageCount,
      playback.stats.page_start_percent,
      reader.page_text,
      reader.sentences,
      renderZoom
    ]);

    const ensureRenderedPageWindow = useCallback(async (targetPage: number, radius: number) => {
      const pdf = pdfDocRef.current;
      const pdfjs = pdfJsModuleRef.current;
      if (!pdf || !pdfjs || pdfPageCount <= 0) {
        return;
      }
      const generation = renderGenerationRef.current;
      for (const pageIndex of pageIndexesAround(targetPage, pdfPageCount, radius)) {
        await ensurePageCanvasRendered(pdf, pageIndex, renderZoom, generation);
        await ensurePageTextLayerRendered(pdfjs, pdf, pageIndex, renderZoom, generation);
      }
    }, [ensurePageCanvasRendered, ensurePageTextLayerRendered, pdfPageCount, renderZoom]);

    const rebindActiveHighlightForRenderedPages = useCallback(() => {
      const activeHighlight = activeHighlightStateRef.current;
      if (!activeHighlight) {
        return;
      }
      const startedAt = typeof performance !== "undefined" ? performance.now() : 0;
      const renderedPages = renderedPagesRef.current;
      if (activeHighlight.useOverlay) {
        const overlays = applyPdfLocationHighlightOverlays(
          highlightedOverlayNodesRef.current,
          highlightedPagesRef.current,
          renderedPages,
          activeHighlight.overlayRects,
          performanceProfile.highlightOverlays
        );
        highlightedOverlayNodesRef.current = overlays.highlightedOverlays;
        highlightedPagesRef.current = overlays.highlightedPages;
        overlaySentenceMapRef.current = overlays.overlaySentenceMap;
        const clearedSpans = applyPdfHighlightDom(
          highlightedNodesRef.current,
          [],
          [],
          [],
          null
        );
        highlightedNodesRef.current = clearedSpans.highlightedNodes;
        logPdfDebug("rebindActiveOverlay", {
          sentenceIdx: activeHighlight.sentenceIdx,
          renderedOverlayPages: overlays.renderedOverlayPageIndexes,
          skippedOverlayPages: overlays.skippedOverlayPageIndexes,
          reason: activeHighlight.match.reason
        });
      } else {
        clearPdfHighlightOverlays(highlightedOverlayNodesRef.current, highlightedPagesRef.current);
        highlightedOverlayNodesRef.current = [];
        overlaySentenceMapRef.current.clear();
        const domHighlight = applyPdfHighlightDom(
          highlightedNodesRef.current,
          highlightedPagesRef.current,
          flattenRenderedPdfSpans(renderedPages),
          renderedPages,
          activeHighlight.match
        );
        highlightedNodesRef.current = domHighlight.highlightedNodes;
        highlightedPagesRef.current = domHighlight.highlightedPages;
        logPdfDebug("rebindActiveSpanHighlight", {
          sentenceIdx: activeHighlight.sentenceIdx,
          renderedPageCount: renderedPages.length,
          reason: activeHighlight.match.reason
        });
      }
      recordPerfMeasure("ReaderPrettyPdfPane.rebindOverlay", startedAt);
    }, []);

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
        const startedAt = typeof performance !== "undefined" ? performance.now() : 0;
        const page = await pdf.getPage(pageIndex + 1);
        const textContent = pdfTextContentCacheRef.current.get(pageIndex) ?? await page.getTextContent();
        pdfTextContentCacheRef.current.set(pageIndex, textContent);
        pdfPageTextsRef.current.set(pageIndex, extractNormalizedPdfPageText(textContent));
        recordPerfMeasure("ReaderPrettyPdfPane.extractPageText", startedAt);
        if ((pageIndex + 1) % 12 === 0 || pageIndex + 1 === pdf.numPages) {
          setPdfPageTextVersion((value) => value + 1);
          await new Promise((resolve) => window.setTimeout(resolve, 0));
        }
      }
    }, []);

    const enforceTextSpanBudget = useCallback((keepPageIndexes: Set<number>) => {
      let liveTextSpanCount = renderedPagesRef.current.reduce((total, page) => total + page.spans.length, 0);
      if (liveTextSpanCount <= performanceProfile.maxLiveTextSpans) {
        return;
      }
      const evictionCandidates = renderedPagesRef.current
        .filter((page) => page.textLayerDiv !== null && !keepPageIndexes.has(page.pageIndex))
        .map((page) => ({
          lastTouchedAt: pageRegistryRef.current.get(page.pageIndex)?.lastTouchedAt ?? 0,
          pageIndex: page.pageIndex,
          spanCount: page.spans.length
        }))
        .sort((left, right) => left.lastTouchedAt - right.lastTouchedAt);
      for (const candidate of evictionCandidates) {
        if (liveTextSpanCount <= performanceProfile.maxLiveTextSpans) {
          break;
        }
        liveTextSpanCount -= candidate.spanCount;
        recordPerfCounter("ReaderPrettyPdfPane.evictedTextSpans", candidate.spanCount);
        evictPageArtifacts(candidate.pageIndex, "text");
      }
      recordPerfGauge("ReaderPrettyPdfPane.liveTextSpans", liveTextSpanCount);
    }, [evictPageArtifacts]);

    const scheduleVisiblePdfWork = useCallback(async () => {
      const pdf = pdfDocRef.current;
      const pdfjs = pdfJsModuleRef.current;
      if (!pdf || !pdfjs || pdfPageCount <= 0) {
        return;
      }
      const targetPage = estimatePdfTargetPage(
        cachedPdfLocationsRef.current,
        pdfPageCount,
        {
          globalSentenceStart,
          highlightedSentenceIdx,
          pageStartPercent: playback.stats.page_start_percent,
          pageText: reader.page_text,
          sentences: reader.sentences
        },
        pdfPageTextsRef.current
      );
      const overscan = resolvePdfVisibleOverscan();
      const renderPlan = buildPdfViewportRenderPlan({
        totalPages: pdfPageCount,
        visiblePageIndexes: Array.from(visiblePageIndexesRef.current),
        overscan,
        activeTtsPageIndex: targetPage,
        jumpTargetPageIndex: pendingJumpTargetPageRef.current
      });
      const selectionTextLayerPages = selectionPageIndexRef.current === null
        ? []
        : pageIndexesAround(selectionPageIndexRef.current, pdfPageCount, performanceProfile.textLayerSelectionRadius);
      const textLayerPageIndexes = Array.from(new Set([
        ...renderPlan.textLayerPageIndexes,
        ...selectionTextLayerPages
      ])).sort((left, right) => left - right);
      const startedAt = typeof performance !== "undefined" ? performance.now() : 0;
      logPdfDebug("scheduleVisiblePages", {
        visiblePages: Array.from(visiblePageIndexesRef.current).sort((left, right) => left - right),
        canvasPages: renderPlan.canvasPageIndexes,
        textLayerPages: textLayerPageIndexes,
        priorityPages: renderPlan.priorityPageIndexes,
        mediumPages: renderPlan.mediumPriorityPageIndexes,
        lowPages: renderPlan.lowPriorityPageIndexes,
        overscan,
        profile: performanceProfile.title,
        selectionPageIndex: selectionPageIndexRef.current
      });
      recordPerfGauge("ReaderPrettyPdfPane.visiblePages", visiblePageIndexesRef.current.size);
      recordPerfGauge("ReaderPrettyPdfPane.overscan", overscan);
      const generation = renderGenerationRef.current;
      const keepCanvasPages = new Set(renderPlan.canvasPageIndexes);
      const keepTextPages = new Set(textLayerPageIndexes);
      for (const [pageIndex, entry] of pageRegistryRef.current.entries()) {
        if (!keepCanvasPages.has(pageIndex) && (entry.state === "scheduled" || entry.state === "rendering_canvas")) {
          bumpPageRenderVersion(canvasRenderVersionRef.current, pageIndex);
          recordPerfCounter("ReaderPrettyPdfPane.canceledPageRenders");
        }
        if (!keepTextPages.has(pageIndex) && entry.textLayerZoom !== null) {
          bumpPageRenderVersion(textLayerRenderVersionRef.current, pageIndex);
        }
      }
      for (const pageIndex of renderPlan.priorityPageIndexes) {
        await ensurePageCanvasRendered(pdf, pageIndex, renderZoom, generation);
      }
      for (const pageIndex of renderPlan.mediumPriorityPageIndexes) {
        await ensurePageCanvasRendered(pdf, pageIndex, renderZoom, generation);
      }
      for (const pageIndex of textLayerPageIndexes) {
        await ensurePageTextLayerRendered(pdfjs, pdf, pageIndex, renderZoom, generation);
      }
      if (lowPriorityPrefetchTimerRef.current !== null) {
        window.clearTimeout(lowPriorityPrefetchTimerRef.current);
      }
      lowPriorityPrefetchTimerRef.current = window.setTimeout(() => {
        if (renderGenerationRef.current !== generation) {
          recordPerfCounter("ReaderPrettyPdfPane.canceledPageRenders");
          return;
        }
        void (async () => {
          for (const pageIndex of renderPlan.lowPriorityPageIndexes) {
            await ensurePageCanvasRendered(pdf, pageIndex, renderZoom, generation);
          }
        })();
      }, performanceProfile.lowPriorityPrefetchDelayMs);
      const eviction = choosePdfViewportEvictions({
        entries: Array.from(pageRegistryRef.current.values()),
        keepCanvasPageIndexes: renderPlan.canvasPageIndexes,
        keepTextLayerPageIndexes: renderPlan.textLayerPageIndexes,
        maxCanvasPages: performanceProfile.maxLiveCanvases,
        maxTextLayerPages: performanceProfile.maxLiveTextLayers
      });
      for (const pageIndex of eviction.evictTextLayerPageIndexes) {
        evictPageArtifacts(pageIndex, "text");
      }
      for (const pageIndex of eviction.evictCanvasPageIndexes) {
        evictPageArtifacts(pageIndex, "canvas");
      }
      enforceTextSpanBudget(keepTextPages);
      recordPerfMeasure("ReaderPrettyPdfPane.scheduleVisiblePages", startedAt);
      recordPerfGauge("ReaderPrettyPdfPane.liveRenderedPages", renderedPagesRef.current.length);
      recordPerfGauge(
        "ReaderPrettyPdfPane.liveTextLayers",
        renderedPagesRef.current.filter((entry) => entry.textLayerDiv !== null).length
      );
      recordPerfGauge("ReaderPrettyPdfPane.liveTextSpans", flattenRenderedPdfSpans(renderedPagesRef.current).length);
      recordPerfGauge("ReaderPrettyPdfPane.sentenceTargetCacheSize", sentenceTargetCacheRef.current.size());
      recordPerfGauge("ReaderPrettyPdfPane.sentencePageTargetCacheSize", sentencePageTargetCacheRef.current.size());
      recordPerfGauge("ReaderPrettyPdfPane.spanArtifactCacheSize", spanArtifactCacheRef.current.size());
      recordPerfGauge("ReaderPrettyPdfPane.bitmapArtifactCacheSize", bitmapArtifactCacheRef.current.size());
    }, [
      enforceTextSpanBudget,
      ensurePageCanvasRendered,
      ensurePageTextLayerRendered,
      evictPageArtifacts,
      globalSentenceStart,
      highlightedSentenceIdx,
      pdfPageCount,
      playback.stats.page_start_percent,
      reader.page_text,
      reader.sentences,
      renderZoom
    ]);

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
      const cachedTarget = sentenceTargetCacheRef.current.get(globalSentenceIdx);
      if (canReuseCachedPdfHighlightTarget(cachedTarget, globalSentenceIdx)) {
        logPdfDebug("currentSentenceResolveCacheHit", {
          sentenceIdx,
          pageIndex: cachedTarget.pageIndex,
          confidence: cachedTarget.match.confidence,
          reason: cachedTarget.match.reason
        });
        return {
          match: cachedTarget.match,
          cachedLocation: null,
          spans: cachedTarget.useOverlay ? [] : flattenRenderedPdfSpans(renderedPagesRef.current.filter(
            (page) => page.pageIndex === cachedTarget.pageIndex || cachedTarget.pageIndex === null
          ))
        };
      }
      const cachedLocation = cachedPdfLocationsRef.current?.find(
        (location) => location.sentence_idx === globalSentenceIdx
      ) ?? null;
      if (cachedLocation && hasPdfLocationGeometry(cachedLocation)) {
        const normalizedReason = normalizeCachedPdfMatchReason(cachedLocation.reason);
        const cachedMatch = {
          confidence: normalizeCachedPdfMatchConfidence(cachedLocation.confidence, normalizedReason),
          reason: normalizedReason,
          pageIndex: cachedLocation.page_idx,
          spanIndexes: [],
          score: cachedLocation.score
        } satisfies PdfSentenceMatch;
        sentenceTargetCacheRef.current.set(globalSentenceIdx, buildCachedPdfHighlightTarget(
          globalSentenceIdx,
          cachedMatch,
          buildOverlayRectsFromLocation(cachedLocation, sentenceIdx),
          true
        ));
        if (cachedLocation.page_idx !== null && cachedLocation.page_idx !== undefined) {
          sentencePageTargetCacheRef.current.set(globalSentenceIdx, cachedLocation.page_idx);
        }
        return {
          match: cachedMatch,
          cachedLocation,
          spans: [] as PdfTextSpan[]
        };
      }

      const fallbackTargetPage = estimatePdfTargetPage(
        cachedPdfLocationsRef.current,
        pdfPageCount,
        {
          globalSentenceStart,
          highlightedSentenceIdx,
          pageStartPercent: playback.stats.page_start_percent,
          pageText: reader.page_text,
          sentences: reader.sentences
        },
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
        sentencePageTargetCacheRef.current.get(globalSentenceIdx) ?? targetPage
      );

      let bestResult: DirectSentenceMatchResult = { match: null, spans: [] };
      for (const candidatePageIndex of candidatePageIndexes) {
        await ensureRenderedPageWindow(candidatePageIndex, 1);
        const candidatePages = renderedPagesRef.current.filter((page) => Math.abs(page.pageIndex - candidatePageIndex) <= 1);
        const result = findCachedSentenceMatchInRenderedPages(
          candidatePages,
          sentence,
          spanArtifactCacheRef.current
        );
        if (scorePdfSentenceMatch(result.match) > scorePdfSentenceMatch(bestResult.match)) {
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
      if (bestResult.match) {
        if (bestResult.match.pageIndex !== null && bestResult.match.pageIndex !== undefined) {
          sentencePageTargetCacheRef.current.set(globalSentenceIdx, bestResult.match.pageIndex);
        }
        sentenceTargetCacheRef.current.set(globalSentenceIdx, buildCachedPdfHighlightTarget(
          globalSentenceIdx,
          bestResult.match,
          [],
          false
        ));
      }
      return {
        match: bestResult.match,
        cachedLocation: null,
        spans: bestResult.spans
      };
    }, [ensureRenderedPageWindow, globalSentenceStart, pdfPageCount, reader.sentences]);

    const jumpToResolvedSentenceTarget = useCallback(async () => {
      const idx = reader.highlighted_sentence_idx;
      if (idx === null || idx === undefined) {
        return false;
      }
      const pdf = pdfDocRef.current;
      const pdfjs = pdfJsModuleRef.current;
      if (!pdf || !pdfjs) {
        return false;
      }
      const globalIdx = globalSentenceStart + idx;
      const cachedTarget = sentenceTargetCacheRef.current.get(globalIdx);
      const activeHighlight = activeHighlightStateRef.current?.sentenceIdx === idx
        ? activeHighlightStateRef.current
        : null;
      const targetPage =
        cachedTarget?.pageIndex
        ?? sentencePageTargetCacheRef.current.get(globalIdx)
        ?? activeHighlight?.match.pageIndex
        ?? null;
      if (targetPage === null || targetPage === undefined) {
        return false;
      }
      pendingJumpTargetPageRef.current = targetPage;
      const generation = renderGenerationRef.current;
      await ensurePageCanvasRendered(pdf, targetPage, renderZoom, generation);
      if (!cachedTarget?.useOverlay && !activeHighlight?.useOverlay) {
        await ensurePageTextLayerRendered(pdfjs, pdf, targetPage, renderZoom, generation);
      }
      rebindActiveHighlightForRenderedPages();
      const page = renderedPagesRef.current.find((entry) => entry.pageIndex === targetPage)?.container ?? null;
      const scrollTarget = highlightedOverlayNodesRef.current[0] ?? highlightedNodesRef.current[0] ?? page;
      if (!scrollTarget) {
        return false;
      }
      const scrollStartedAt = typeof performance !== "undefined" ? performance.now() : 0;
      scrollTarget.scrollIntoView({
        behavior: "smooth",
        block: reader.settings.center_spoken_sentence ? "center" : "nearest",
        inline: "nearest"
      });
      recordPerfMeasure("ReaderPrettyPdfPane.autoScroll", scrollStartedAt);
      recordPerfCounter("ReaderPrettyPdfPane.manualJumpFastPath");
      lastScrollTargetRef.current = `manual_jump:${globalIdx}:${targetPage}`;
      logPdfDebug("scrollTarget", {
        reason: "manual_jump_fast_path",
        target: lastScrollTargetRef.current
      });
      return true;
    }, [
      ensurePageCanvasRendered,
      ensurePageTextLayerRendered,
      globalSentenceStart,
      reader,
      rebindActiveHighlightForRenderedPages,
      renderZoom
    ]);

    const maybeAutoScrollToRenderedHighlight = useCallback((
      behavior: ScrollBehavior,
      force = false,
      reason: string
    ) => {
      const idx = reader.highlighted_sentence_idx;
      if (idx === null || idx === undefined) {
        return false;
      }
      if (!force && (!reader.settings.auto_scroll_tts || playback.tts.state !== "playing")) {
        return false;
      }
      const activeHighlight = activeHighlightStateRef.current;
      const target = highlightedOverlayNodesRef.current[0]
        ?? highlightedNodesRef.current[0]
        ?? highlightedPagesRef.current[0]
        ?? (activeHighlight?.match.pageIndex !== null && activeHighlight?.match.pageIndex !== undefined
          ? renderedPagesRef.current.find((entry) => entry.pageIndex === activeHighlight.match.pageIndex)?.container ?? null
          : null);
      if (!target) {
        return false;
      }
      const globalIdx = globalSentenceStart + idx;
      const pageIndex = activeHighlight?.match.pageIndex ?? target.closest(".reader-pdf-page")?.getAttribute("data-page-index") ?? "none";
      const targetKey = `sentence:${globalIdx}:page:${pageIndex}:${activeHighlight?.useOverlay ? "overlay" : "dom"}`;
      const shouldScroll = force
        || lastScrollTargetRef.current !== targetKey
        || !isPdfScrollTargetMostlyVisible(target);
      if (!shouldScroll) {
        lastScrollTargetRef.current = targetKey;
        return false;
      }
      const scrollStartedAt = typeof performance !== "undefined" ? performance.now() : 0;
      target.scrollIntoView({
        behavior,
        block: reader.settings.center_spoken_sentence ? "center" : "nearest",
        inline: "nearest"
      });
      recordPerfMeasure("ReaderPrettyPdfPane.autoScroll", scrollStartedAt);
      lastScrollTargetRef.current = targetKey;
      logPdfDebug("scrollTarget", {
        reason,
        target: targetKey,
        force
      });
      return true;
    }, [globalSentenceStart, playback.tts.state, reader.settings.auto_scroll_tts, reader.settings.center_spoken_sentence]);

    const applyHighlight = useCallback(
      async (behavior: ScrollBehavior, force = false) => {
        const idx = reader.highlighted_sentence_idx;
        const startedAt = typeof performance !== "undefined" ? performance.now() : 0;

        if (idx === null || idx === undefined) {
          activeHighlightStateRef.current = null;
          clearPdfHighlightOverlays(highlightedOverlayNodesRef.current, highlightedPagesRef.current);
          highlightedOverlayNodesRef.current = [];
          overlaySentenceMapRef.current.clear();
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
          activeHighlightStateRef.current = null;
          clearPdfHighlightOverlays(highlightedOverlayNodesRef.current, highlightedPagesRef.current);
          highlightedOverlayNodesRef.current = [];
          overlaySentenceMapRef.current.clear();
          highlightedSentenceRef.current = idx;
          recordPerfMeasure("ReaderPrettyPdfPane.resolveHighlight", startedAt);
          return;
        }
        const globalIdx = globalSentenceStart + idx;
        const cachedTarget = sentenceTargetCacheRef.current.get(globalIdx);
        const resolved = await resolveCurrentSentenceHighlight(idx);
        const cachedLocation = resolved.cachedLocation;
        const match = resolved.match;
        const spans = resolved.spans;
        const overlayStartedAt = typeof performance !== "undefined" ? performance.now() : 0;
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
          activeHighlightStateRef.current = null;
          clearPdfHighlightOverlays(highlightedOverlayNodesRef.current, highlightedPagesRef.current);
          highlightedOverlayNodesRef.current = [];
          overlaySentenceMapRef.current.clear();
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
        let highlighted: HighlightOverlayResult;
        let activeHighlightState: ActivePdfHighlightState | null = null;
        if (cachedTarget?.useOverlay && cachedTarget.overlayRects.length > 0) {
          const overlays = applyPdfLocationHighlightOverlays(
            highlightedOverlayNodesRef.current,
            highlightedPagesRef.current,
            renderedPagesRef.current,
            cachedTarget.overlayRects,
            performanceProfile.highlightOverlays
          );
          highlightedOverlayNodesRef.current = overlays.highlightedOverlays;
          overlaySentenceMapRef.current = overlays.overlaySentenceMap;
          highlighted = {
            highlightedNodes: overlays.highlightedOverlays,
            highlightedPages: overlays.highlightedPages,
            highlightedOverlays: overlays.highlightedOverlays
          };
          activeHighlightState = {
            sentenceIdx: idx,
            match,
            overlayRects: cachedTarget.overlayRects,
            useOverlay: true
          };
          const clearedSpans = applyPdfHighlightDom(
            highlightedNodesRef.current,
            [],
            [],
            [],
            null
          );
          highlightedNodesRef.current = clearedSpans.highlightedNodes;
        } else if (cachedLocation) {
          const overlayRects = buildOverlayRectsFromLocation(cachedLocation, idx);
          const overlays = applyPdfLocationHighlightOverlays(
            highlightedOverlayNodesRef.current,
            highlightedPagesRef.current,
            renderedPagesRef.current,
            overlayRects,
            performanceProfile.highlightOverlays
          );
          highlightedOverlayNodesRef.current = overlays.highlightedOverlays;
          overlaySentenceMapRef.current = overlays.overlaySentenceMap;
          highlighted = {
            highlightedNodes: overlays.highlightedOverlays,
            highlightedPages: overlays.highlightedPages,
            highlightedOverlays: overlays.highlightedOverlays
          };
          activeHighlightState = {
            sentenceIdx: idx,
            match,
            overlayRects,
            useOverlay: true
          };
          sentenceTargetCacheRef.current.set(globalIdx, buildCachedPdfHighlightTarget(globalIdx, match, overlayRects, true));
          const clearedSpans = applyPdfHighlightDom(
            highlightedNodesRef.current,
            [],
            [],
            [],
            null
          );
          highlightedNodesRef.current = clearedSpans.highlightedNodes;
        } else if (preferOverlayHighlights) {
          const overlayRects = buildOverlayRectsFromMatch(spans, match, idx);
          if (overlayRects.length > 0) {
            const overlays = applyPdfLocationHighlightOverlays(
              highlightedOverlayNodesRef.current,
              highlightedPagesRef.current,
              renderedPagesRef.current,
              overlayRects,
              performanceProfile.highlightOverlays
            );
            highlightedOverlayNodesRef.current = overlays.highlightedOverlays;
            overlaySentenceMapRef.current = overlays.overlaySentenceMap;
            const clearedSpans = applyPdfHighlightDom(
              highlightedNodesRef.current,
              [],
              [],
              [],
              null
            );
            highlightedNodesRef.current = clearedSpans.highlightedNodes;
            highlighted = {
              highlightedNodes: overlays.highlightedOverlays,
              highlightedPages: overlays.highlightedPages,
              highlightedOverlays: overlays.highlightedOverlays
            };
            activeHighlightState = {
              sentenceIdx: idx,
              match,
              overlayRects,
              useOverlay: true
            };
            sentenceTargetCacheRef.current.set(globalIdx, buildCachedPdfHighlightTarget(globalIdx, match, overlayRects, true));
          } else {
            clearPdfHighlightOverlays(highlightedOverlayNodesRef.current, highlightedPagesRef.current);
            highlightedOverlayNodesRef.current = [];
            overlaySentenceMapRef.current.clear();
            const domHighlight = applyPdfHighlightDom(
              highlightedNodesRef.current,
              highlightedPagesRef.current,
              spans,
              renderedPagesRef.current,
              match
            );
            highlightedNodesRef.current = domHighlight.highlightedNodes;
            highlighted = {
              highlightedNodes: domHighlight.highlightedNodes,
              highlightedPages: domHighlight.highlightedPages,
              highlightedOverlays: []
            };
            activeHighlightState = {
              sentenceIdx: idx,
              match,
              overlayRects: [],
              useOverlay: false
            };
            sentenceTargetCacheRef.current.set(globalIdx, buildCachedPdfHighlightTarget(globalIdx, match, [], false));
          }
        } else {
          clearPdfHighlightOverlays(highlightedOverlayNodesRef.current, highlightedPagesRef.current);
          highlightedOverlayNodesRef.current = [];
          overlaySentenceMapRef.current.clear();
          const domHighlight = applyPdfHighlightDom(
            highlightedNodesRef.current,
            highlightedPagesRef.current,
            spans,
            renderedPagesRef.current,
            match
          );
          highlightedNodesRef.current = domHighlight.highlightedNodes;
          highlighted = {
            highlightedNodes: domHighlight.highlightedNodes,
            highlightedPages: domHighlight.highlightedPages,
            highlightedOverlays: []
          };
          activeHighlightState = {
            sentenceIdx: idx,
            match,
            overlayRects: [],
            useOverlay: false
          };
          sentenceTargetCacheRef.current.set(globalIdx, buildCachedPdfHighlightTarget(globalIdx, match, [], false));
        }
        activeHighlightStateRef.current = activeHighlightState;
        highlightedPagesRef.current = highlighted.highlightedPages;
        if (match.reason === "page_location_only" && match.pageIndex !== null) {
          const page = highlighted.highlightedPages[0];
          if (page) {
            highlightedSentenceRef.current = idx;
            void page;
            maybeAutoScrollToRenderedHighlight(
              behavior,
              force,
              force ? "manual_jump" : "page_location_change"
            );
            recordPerfMeasure("ReaderPrettyPdfPane.updateOverlay", overlayStartedAt);
            recordPerfMeasure("ReaderPrettyPdfPane.resolveHighlight", startedAt);
            return;
          }
        }
        if (match.spanIndexes.length === 0 && !cachedLocation) {
          highlightedSentenceRef.current = idx;
          lastScrollTargetRef.current = null;
          recordPerfMeasure("ReaderPrettyPdfPane.updateOverlay", overlayStartedAt);
          recordPerfMeasure("ReaderPrettyPdfPane.resolveHighlight", startedAt);
          return;
        }

        const elements = highlighted.highlightedNodes;
        highlightedSentenceRef.current = idx;

        const shouldAutoScroll =
          force || (reader.settings.auto_scroll_tts && playback.tts.state === "playing");
        const anchor = elements[0];
        if (!anchor || !shouldAutoScroll) {
          recordPerfMeasure("ReaderPrettyPdfPane.updateOverlay", overlayStartedAt);
          recordPerfMeasure("ReaderPrettyPdfPane.resolveHighlight", startedAt);
          return;
        }
        maybeAutoScrollToRenderedHighlight(
          behavior,
          force,
          force ? "manual_jump" : "sentence_location_change"
        );
        recordPerfMeasure("ReaderPrettyPdfPane.updateOverlay", overlayStartedAt);
        recordPerfMeasure("ReaderPrettyPdfPane.resolveHighlight", startedAt);
      },
      [
        canSyncHighlights,
        maybeAutoScrollToRenderedHighlight,
        playback.tts.state,
        preferOverlayHighlights,
        reader.sentences,
        reader.settings.auto_scroll_tts,
        resolveCurrentSentenceHighlight
      ]
    );

    useImperativeHandle(ref, () => ({
      jumpToHighlightedSentence() {
        void (async () => {
          const usedFastPath = await jumpToResolvedSentenceTarget();
          if (!usedFastPath) {
            await applyHighlight("smooth", true);
          }
        })();
      }
    }), [applyHighlight, jumpToResolvedSentenceTarget]);

    useEffect(() => {
      const root = containerRef.current;
      if (!root) {
        return;
      }
      const updateSelectionPageIndex = () => {
        const selection = window.getSelection();
        if (!selection || selection.isCollapsed) {
        if (selectionPageIndexRef.current !== null) {
          selectionPageIndexRef.current = null;
          }
          return;
        }
        const anchorNode = selection.anchorNode;
        const page = (anchorNode instanceof Element
          ? anchorNode.closest(".reader-pdf-page")
          : anchorNode?.parentElement?.closest(".reader-pdf-page")) as HTMLDivElement | null;
        const nextPageIndex = page
          ? Number.parseInt(page.dataset.pageIndex ?? "", 10)
          : null;
        const normalizedPageIndex = Number.isFinite(nextPageIndex ?? NaN) ? nextPageIndex : null;
        if (selectionPageIndexRef.current === normalizedPageIndex) {
          return;
        }
        selectionPageIndexRef.current = normalizedPageIndex;
      };
      document.addEventListener("selectionchange", updateSelectionPageIndex);
      root.addEventListener("mouseup", updateSelectionPageIndex);
      return () => {
        document.removeEventListener("selectionchange", updateSelectionPageIndex);
        root.removeEventListener("mouseup", updateSelectionPageIndex);
      };
    }, [scheduleVisiblePdfWork]);

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
            currentPage: playback.current_page
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
      pdfPageTextsRef.current.clear();
      sentencePageTargetCacheRef.current.clear();
      setPdfPageTextVersion((value) => value + 1);
      void backendApi.readerLoadPdfRenderPrecomputed(sourcePath)
        .then((artifact) => {
          if (cancelled) {
            return;
          }
          artifact.page_texts.forEach((pageText, pageIndex) => {
            pdfPageTextsRef.current.set(pageIndex, pageText);
          });
          artifact.sentence_page_hints.forEach((pageIndex, sentenceIndex) => {
            if (pageIndex === null || pageIndex === undefined) {
              return;
            }
            sentencePageTargetCacheRef.current.set(sentenceIndex, pageIndex);
          });
          setPdfPageTextVersion((value) => value + 1);
          logPdfDebug("loadedBackendRenderPrecompute", {
            sourcePath,
            pageTextCount: artifact.page_texts.length,
            sentenceHintCount: artifact.sentence_page_hints.filter((value) => value !== null).length,
            source: artifact.source
          });
        })
        .catch((cause: unknown) => {
          if (cancelled) {
            return;
          }
          logPdfDebug("loadBackendRenderPrecomputeError", {
            sourcePath,
            error: cause instanceof Error ? cause.message : String(cause)
          });
        });
      return () => {
        cancelled = true;
      };
    }, [sourcePath]);

    useEffect(() => {
      syncPdfPreviewZoom(zoom);
    }, [renderVersion, syncPdfPreviewZoom, zoom]);

    useEffect(() => {
      if (zoomSettleTimerRef.current !== null) {
        window.clearTimeout(zoomSettleTimerRef.current);
      }
      const startedAt = typeof performance !== "undefined" ? performance.now() : 0;
      logPdfDebug("zoomStart", {
        displayZoom: zoom,
        renderZoom
      });
      zoomSettleTimerRef.current = window.setTimeout(() => {
        zoomSettleTimerRef.current = null;
        setRenderZoom((current) => {
          if (current === zoom) {
            return current;
          }
          logPdfDebug("zoomSettle", {
            from: current,
            to: zoom
          });
          recordPerfMeasure("ReaderPrettyPdfPane.zoomSettle", startedAt);
          return zoom;
        });
      }, PDF_ZOOM_SETTLE_MS);
      return () => {
        if (zoomSettleTimerRef.current !== null) {
          window.clearTimeout(zoomSettleTimerRef.current);
          zoomSettleTimerRef.current = null;
        }
      };
    }, [renderZoom, zoom]);

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
      documentOpenStartedAtRef.current = startedAt;
      let firstUsefulPagePainted = false;
        logPdfDebug("renderInit", {
          sourcePath,
          zoom: renderZoom,
          viewportVersion
        });
        try {
          const { pdf } = await ensurePdfDocumentLoaded();
          if (cancelled || renderGenerationRef.current !== generation) {
            return;
          }
          renderedPagesRef.current = [];
          renderedPageZoomRef.current.clear();
          await ensurePageShells(pdf, renderZoom);
          for (const shell of pageShellsRef.current.values()) {
            shell.innerHTML = "";
          }
          const openPlan = buildPdfOpenPlan(pdf.numPages, estimatePdfTargetPage(
            cachedPdfLocationsRef.current,
            pdf.numPages,
            {
              globalSentenceStart,
              highlightedSentenceIdx,
              pageStartPercent: playback.stats.page_start_percent,
              pageText: reader.page_text,
              sentences: reader.sentences
            },
            pdfPageTextsRef.current
          ));
          logPdfDebug("openPlan", {
            currentPageIndex: openPlan.currentPageIndex,
            immediatePageIndexes: openPlan.immediatePageIndexes,
            adjacentPageIndexes: openPlan.adjacentPageIndexes,
            deferredPageIndexes: openPlan.deferredPageIndexes
          });
          void ensurePdfPageTexts(pdf, generation);
          for (const pageIndex of openPlan.immediatePageIndexes) {
            await ensurePageCanvasRendered(pdf, pageIndex, renderZoom, generation);
            if (!firstUsefulPagePainted) {
              firstUsefulPagePainted = true;
              recordPerfMeasure("ReaderPrettyPdfPane.firstUsefulPage", startedAt);
            }
          }
          for (const pageIndex of openPlan.adjacentPageIndexes) {
            await ensurePageCanvasRendered(pdf, pageIndex, renderZoom, generation);
          }
          await scheduleVisiblePdfWork();
          recordPerfMeasure("ReaderPrettyPdfPane.renderDocument", startedAt);
          logPdfDebug("renderInitComplete", {
            sourcePath,
            zoom: renderZoom,
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
    }, [
      ensurePageShells,
      ensurePdfDocumentLoaded,
      ensurePdfPageTexts,
      globalSentenceStart,
      highlightedSentenceIdx,
      playback.stats.page_start_percent,
      reader.page_text,
      reader.sentences,
      renderZoom,
      resetRenderedPdfDocument,
      scheduleVisiblePdfWork,
      sourcePath,
      viewportVersion
    ]);

    useEffect(() => {
      const root = containerRef.current;
      const pdf = pdfDocRef.current;
      if (!root || !pdf || pageShellsRef.current.size === 0) {
        return;
      }
      const observer = new IntersectionObserver((entries) => {
        for (const entry of entries) {
          const target = entry.target as HTMLElement;
          const rawPageIndex = target.dataset.pageIndex;
          const pageIndex = rawPageIndex ? Number.parseInt(rawPageIndex, 10) : Number.NaN;
          if (!Number.isFinite(pageIndex)) {
            continue;
          }
          if (entry.isIntersecting) {
            visiblePageIndexesRef.current.add(pageIndex);
          } else {
            visiblePageIndexesRef.current.delete(pageIndex);
          }
        }
        void scheduleVisiblePdfWork();
      }, {
        root: null,
        rootMargin: "100% 0px 100% 0px",
        threshold: 0.01
      });
      for (const shell of pageShellsRef.current.values()) {
        observer.observe(shell);
      }
      return () => {
        observer.disconnect();
      };
    }, [pdfPageCount, renderVersion, scheduleVisiblePdfWork]);

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
    }, [applyHighlight, cachedSyncVersion, ensureTargetPagesRendered, globalSentenceStart, highlightedSentenceIdx, loading, pdfPageTextVersion, renderZoom]);

    useEffect(() => {
      if (loading) {
        return;
      }
      const activeHighlight = activeHighlightStateRef.current;
      if (!activeHighlight) {
        return;
      }
      if (playback.highlighted_sentence_idx !== activeHighlight.sentenceIdx) {
        return;
      }
      rebindActiveHighlightForRenderedPages();
      maybeAutoScrollToRenderedHighlight("auto", false, "highlight_rebind");
    }, [loading, maybeAutoScrollToRenderedHighlight, playback.highlighted_sentence_idx, renderVersion, rebindActiveHighlightForRenderedPages]);

    useEffect(() => {
      recordPerfGauge("ReaderPrettyPdfPane.liveRenderedPages", renderedPagesRef.current.length);
      recordPerfGauge(
        "ReaderPrettyPdfPane.liveTextLayers",
        renderedPagesRef.current.filter((entry) => entry.textLayerDiv !== null).length
      );
      recordPerfGauge("ReaderPrettyPdfPane.liveHighlightOverlays", highlightedOverlayNodesRef.current.length);
    }, [renderVersion, highlightedSentenceIdx, loading]);

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
        {!error ? (
          <Typography color="text.secondary" variant="caption" data-testid="reader-pretty-pdf-profile">
            Profile: {performanceProfile.title.replaceAll("_", " ")} | Canvas budget: {performanceProfile.maxLiveCanvases} | Text budget: {performanceProfile.maxLiveTextLayers} | Span budget: {performanceProfile.maxLiveTextSpans} | Overscan: {resolvePdfVisibleOverscan()}
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
            {reader.pdf_ocr_alignment.alignment_build_ms} ms | Exact rate:{" "}
            {(reader.pdf_ocr_alignment.exact_sentence_rate * 100).toFixed(1)}% | Fallback rate:{" "}
            {(reader.pdf_ocr_alignment.degraded_fallback_rate * 100).toFixed(1)}% | Page-only rate:{" "}
            {(reader.pdf_ocr_alignment.page_only_rate * 100).toFixed(1)}% | Blocks:{" "}
            {reader.pdf_ocr_alignment.geometry_block_count} | Lines:{" "}
            {reader.pdf_ocr_alignment.geometry_line_count} | Tokens:{" "}
            {reader.pdf_ocr_alignment.geometry_token_count} | Cross-column:{" "}
            {reader.pdf_ocr_alignment.cross_column_alignment_count}/{reader.pdf_ocr_alignment.cross_column_confident_alignment_count} | Page max:{" "}
            {reader.pdf_ocr_alignment.max_page_build_ms} ms | Chunk max:{" "}
            {reader.pdf_ocr_alignment.max_chunk_build_ms} ms
          </Typography>
        ) : null}
        {!error && reader.pdf_ocr_pipeline ? (
          <Typography color="text.secondary" variant="caption" data-testid="reader-pretty-pdf-ocr-pipeline">
            OCR engine: {reader.pdf_ocr_pipeline.engine_policy.replaceAll("_", " ")} | OCR enabled:{" "}
            {reader.pdf_ocr_pipeline.ocr_enabled ? "yes" : "no"} | Chunks: {reader.pdf_ocr_pipeline.chunk_count} | Order:{" "}
            {reader.pdf_ocr_pipeline.reading_order_mode.replaceAll("_", " ")}
            {" | "}Joins: {reader.pdf_ocr_pipeline.normalization_summary.broken_line_join_count}
            {" | "}Hyphen: {reader.pdf_ocr_pipeline.normalization_summary.hyphen_recovery_count}
            {" | "}Headers: {reader.pdf_ocr_pipeline.normalization_summary.repeated_header_suppression_count}
            {" | "}Footers: {reader.pdf_ocr_pipeline.normalization_summary.repeated_footer_suppression_count}
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
          style={{ opacity: loading ? 0.35 : 1 }}
        />
      </div>
    );
  }
);

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
  pageContainer.dataset.pdfRotation = String(viewport.rotation);
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
    spans,
    renderedZoom: zoom,
    textLayerZoom: zoom,
    textLayerDiv
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
