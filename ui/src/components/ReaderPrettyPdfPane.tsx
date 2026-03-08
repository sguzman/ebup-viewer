import { Button, ButtonGroup, CircularProgress, Stack, Typography } from "@mui/material";
import type { PDFDocumentProxy } from "pdfjs-dist/types/src/display/api";
import { forwardRef, useCallback, useEffect, useImperativeHandle, useMemo, useRef, useState } from "react";

import type { ReaderSnapshot } from "../types";
import { recordPerfMeasure } from "../perf/debug";
import { toReaderImageSrc } from "./readerDom";
import { clamp, normalizeNumber } from "./readerShared";
import { buildPdfSentenceSpanMap, type PdfTextSpan } from "./pdfTextSync";
import pdfWorkerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

let pdfJsWorkerConfigured = false;

interface ReaderPrettyPdfPaneProps {
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
  function ReaderPrettyPdfPane({ reader, sourcePath }, ref) {
    const containerRef = useRef<HTMLDivElement | null>(null);
    const renderedPagesRef = useRef<RenderedPdfPage[]>([]);
    const highlightedNodesRef = useRef<HTMLElement[]>([]);
    const highlightedSentenceRef = useRef<number | null>(null);
    const [zoom, setZoom] = useState(1.2);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [renderVersion, setRenderVersion] = useState(0);

    const pdfUrl = useMemo(() => toReaderImageSrc(sourcePath), [sourcePath]);

    const applyHighlight = useCallback(
      (behavior: ScrollBehavior, force = false) => {
        const idx = reader.highlighted_sentence_idx;
        for (const node of highlightedNodesRef.current) {
          node.classList.remove("reader-pdf-highlight");
        }
        highlightedNodesRef.current = [];

        if (idx === null || idx === undefined) {
          highlightedSentenceRef.current = null;
          return;
        }

        const spans = renderedPagesRef.current.flatMap((page) => page.spans);
        const { matches } = buildPdfSentenceSpanMap(spans, reader.sentences);
        const match = matches[idx];
        if (!match || match.spanIndexes.length === 0) {
          highlightedSentenceRef.current = idx;
          return;
        }

        const elements = match.spanIndexes
          .map((spanIdx) => spans[spanIdx]?.element)
          .filter((value): value is HTMLElement => Boolean(value));
        for (const element of elements) {
          element.classList.add("reader-pdf-highlight");
        }
        highlightedNodesRef.current = elements;
        highlightedSentenceRef.current = idx;

        const shouldAutoScroll =
          force || (reader.settings.auto_scroll_tts && reader.tts.state === "playing");
        const anchor = elements[0];
        if (!anchor || !shouldAutoScroll) {
          return;
        }
        anchor.scrollIntoView({
          behavior,
          block: reader.settings.center_spoken_sentence ? "center" : "nearest",
          inline: "nearest"
        });
      },
      [reader.highlighted_sentence_idx, reader.sentences, reader.settings.auto_scroll_tts, reader.settings.center_spoken_sentence, reader.tts.state]
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
        renderedPagesRef.current = [];
        root.innerHTML = "";
        applyPdfHighlightColor(root, reader);
        const startedAt = typeof performance !== "undefined" ? performance.now() : 0;

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
          setRenderVersion((value) => value + 1);
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
            Native PDF
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

async function renderPdfPages(
  TextLayerImpl: typeof import("pdfjs-dist")["TextLayer"],
  pdf: PDFDocumentProxy,
  root: HTMLDivElement,
  zoom: number,
  cancelled: boolean,
  renderedPagesRef: React.MutableRefObject<RenderedPdfPage[]>
): Promise<void> {
  for (let pageNumber = 1; pageNumber <= pdf.numPages; pageNumber += 1) {
    if (cancelled) {
      return;
    }
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
    const spans: PdfTextSpan[] = spanElements
      .map((element) => ({
        pageIndex: pageNumber - 1,
        text: element.textContent ?? "",
        element
      }))
      .filter((span) => span.text.trim().length > 0);

    renderedPagesRef.current.push({
      container: pageContainer,
      pageIndex: pageNumber - 1,
      spans
    });
  }
}
