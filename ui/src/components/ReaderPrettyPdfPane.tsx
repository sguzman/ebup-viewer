import { Button, ButtonGroup, CircularProgress, Stack, Typography } from "@mui/material";
import type { PDFDocumentProxy } from "pdfjs-dist/types/src/display/api";
import { forwardRef, useCallback, useEffect, useImperativeHandle, useMemo, useRef, useState } from "react";

import type { ReaderSnapshot } from "../types";
import { recordPerfMeasure } from "../perf/debug";
import { toReaderImageSrc } from "./readerDom";
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
import pdfWorkerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

let pdfJsWorkerConfigured = false;

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
    const highlightedNodesRef = useRef<HTMLElement[]>([]);
    const highlightedPagesRef = useRef<HTMLDivElement[]>([]);
    const highlightedSentenceRef = useRef<number | null>(null);
    const lastScrollTargetRef = useRef<string | null>(null);
    const sentenceMatchesRef = useRef<PdfSentenceMatch[]>([]);
    const matchCacheRef = useRef<CachedPdfMatchResult | null>(null);
    const [zoom, setZoom] = useState(1.2);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [renderVersion, setRenderVersion] = useState(0);
    const [mappingSummary, setMappingSummary] = useState<{
      exact: number;
      fallback: number;
      pageOnly: number;
      missing: number;
    } | null>(null);
    const [activeMatch, setActiveMatch] = useState<PdfSentenceMatch | null>(null);

    const pdfUrl = useMemo(() => toReaderImageSrc(sourcePath), [sourcePath]);
    const canSyncHighlights = reader.pdf_sync_strategy !== "render_only";
    const modeLabel = reader.pdf_geometry_mode ? reader.pdf_geometry_mode.replaceAll("_", " ") : "unknown";
    const strategyLabel = reader.pdf_sync_strategy ? reader.pdf_sync_strategy.replaceAll("_", " ") : "unknown";

    const resolveSentenceMatches = useCallback(() => {
      const spans = renderedPagesRef.current.flatMap((page) => page.spans);
      const cacheKey = [
        reader.source_path,
        String(renderVersion),
        String(spans.length),
        reader.sentences.join("\n")
      ].join("\u241f");
      if (matchCacheRef.current?.key === cacheKey) {
        logPdfDebug("mappingCacheHit", {
          sourcePath: reader.source_path,
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

      const { matches, diagnostics } = buildPdfSentenceSpanMap(spans, reader.sentences);
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
        renderVersion,
        spanCount: spans.length,
        sentenceCount: reader.sentences.length
      });
      return { spans, matches, summary };
    }, [reader.sentences, reader.source_path, renderVersion]);

    const applyHighlight = useCallback(
      (behavior: ScrollBehavior, force = false) => {
        const idx = reader.highlighted_sentence_idx;
        const startedAt = typeof performance !== "undefined" ? performance.now() : 0;
        let resolvedScrollTarget: string | null = null;

        if (idx === null || idx === undefined) {
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
          highlightedSentenceRef.current = idx;
          recordPerfMeasure("ReaderPrettyPdfPane.resolveHighlight", startedAt);
          return;
        }

        const { spans, matches, summary } = resolveSentenceMatches();
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
          cappedLeaps: summary.cappedLeaps,
          lowConfidenceMatches: matches.filter((candidate) => candidate.score > 0 && candidate.score < 0.88).length
        });
        if (!match) {
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
        const highlighted = applyPdfHighlightDom(
          highlightedNodesRef.current,
          highlightedPagesRef.current,
          spans,
          renderedPagesRef.current,
          match
        );
        highlightedNodesRef.current = highlighted.highlightedNodes;
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
        if (match.spanIndexes.length === 0) {
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
        resolvedScrollTarget = match.spanIndexes.join(",");
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
      [canSyncHighlights, reader.highlighted_sentence_idx, reader.sentences, reader.settings.auto_scroll_tts, reader.settings.center_spoken_sentence, reader.tts.state, resolveSentenceMatches]
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
      let cancelled = false;
      const root = containerRef.current;
      if (!root) {
        return;
      }

      const render = async (): Promise<void> => {
        setLoading(true);
        setError(null);
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
        matchCacheRef.current = null;
        lastScrollTargetRef.current = null;
        root.innerHTML = "";
        applyPdfHighlightColor(root, reader);
        const startedAt = typeof performance !== "undefined" ? performance.now() : 0;
        logPdfDebug("renderStart", {
          sourcePath,
          zoom,
          mode: reader.pdf_geometry_mode,
          strategy: reader.pdf_sync_strategy
        });

        try {
          const pdfjs = await import("pdfjs-dist");
          if (!pdfJsWorkerConfigured) {
            pdfjs.GlobalWorkerOptions.workerSrc = pdfWorkerUrl;
            pdfJsWorkerConfigured = true;
          }
          const loadingTask = pdfjs.getDocument({
            url: pdfUrl,
            isEvalSupported: false
          });
          const pdf = await loadingTask.promise;
          logPdfDebug("documentLoaded", {
            sourcePath,
            numPages: pdf.numPages,
            zoom
          });
          if (cancelled) {
            void pdf.destroy();
            return;
          }
          await renderPdfPages(pdfjs.TextLayer, pdf, root, zoom, cancelled, renderedPagesRef);
          if (cancelled) {
            void pdf.destroy();
            return;
          }
          recordPerfMeasure("ReaderPrettyPdfPane.renderDocument", startedAt);
          logPdfDebug("renderComplete", {
            sourcePath,
            zoom,
            numPages: pdf.numPages
          });
          setRenderVersion((value) => value + 1);
        } catch (cause) {
          if (!cancelled) {
            setError(cause instanceof Error ? cause.message : String(cause));
            logPdfDebug("renderError", {
              sourcePath,
              zoom,
              error: cause instanceof Error ? cause.message : String(cause)
            });
          }
        } finally {
          if (!cancelled) {
            setLoading(false);
          }
        }
      };

      void render();

      return () => {
        cancelled = true;
      };
    }, [pdfUrl, reader, zoom]);

    useEffect(() => {
      if (loading) {
        return;
      }
      applyHighlight("auto");
    }, [applyHighlight, loading, reader.current_page, renderVersion]);

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

async function renderPdfPages(
  TextLayerImpl: typeof import("pdfjs-dist")["TextLayer"],
  pdf: PDFDocumentProxy,
  root: HTMLDivElement,
  zoom: number,
  cancelled: boolean,
  renderedPagesRef: React.MutableRefObject<RenderedPdfPage[]>
): Promise<void> {
  let globalSpanIndex = 0;
  for (let pageNumber = 1; pageNumber <= pdf.numPages; pageNumber += 1) {
    if (cancelled) {
      return;
    }
    const startedAt = typeof performance !== "undefined" ? performance.now() : 0;
    const page = await pdf.getPage(pageNumber);
    const viewport = page.getViewport({ scale: clamp(zoom, 0.7, 2.5) });

    const pageContainer = document.createElement("div");
    pageContainer.className = "reader-pdf-page";
    pageContainer.dataset.pageIndex = String(pageNumber - 1);
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
      pageNumber - 1,
      viewport.rotation
    ).map((span) => {
      span.element.setAttribute("data-ll-pdf-span-idx", String(globalSpanIndex));
      globalSpanIndex += 1;
      return span;
    });

    renderedPagesRef.current.push({
      container: pageContainer,
      pageIndex: pageNumber - 1,
      spans
    });
    recordPerfMeasure("ReaderPrettyPdfPane.renderPage", startedAt);
    logPdfDebug("renderPage", {
      pageIndex: pageNumber - 1,
      zoom,
      width: Math.round(viewport.width),
      height: Math.round(viewport.height),
      textSpanCount: spans.length
    });
  }
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
