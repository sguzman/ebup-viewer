import type { PdfOverlayRect } from "./pdfOverlayGeometry";

export interface PdfOverlayDomResult {
  highlightedOverlays: HTMLDivElement[];
  highlightedPages: HTMLDivElement[];
  overlaySentenceMap: Map<number, number>;
  renderedOverlayPageIndexes: number[];
  skippedOverlayPageIndexes: number[];
  truncatedOverlayCount: number;
}

export function clearPdfHighlightOverlays(
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

export function applyPdfLocationHighlightOverlays(
  previousOverlays: HTMLDivElement[],
  previousPages: HTMLDivElement[],
  pages: Array<{ pageIndex: number; container: HTMLDivElement }>,
  overlayRects: PdfOverlayRect[],
  maxOverlayCount = Number.POSITIVE_INFINITY
): PdfOverlayDomResult {
  clearPdfHighlightOverlays(previousOverlays, previousPages);
  if (overlayRects.length === 0) {
    return {
      highlightedOverlays: [],
      highlightedPages: [],
      overlaySentenceMap: new Map(),
      renderedOverlayPageIndexes: [],
      skippedOverlayPageIndexes: [],
      truncatedOverlayCount: 0
    };
  }
  const overlays: HTMLDivElement[] = [];
  const highlightedPages: HTMLDivElement[] = [];
  const overlaySentenceMap = new Map<number, number>();
  const renderedOverlayPageIndexes: number[] = [];
  const skippedOverlayPageIndexes: number[] = [];
  const groupedByPage = new Map<number, PdfOverlayRect[]>();
  let truncatedOverlayCount = 0;
  for (const overlayRect of overlayRects) {
    const pageRects = groupedByPage.get(overlayRect.pageIndex);
    if (pageRects) {
      pageRects.push(overlayRect);
    } else {
      groupedByPage.set(overlayRect.pageIndex, [overlayRect]);
    }
  }
  let renderedCount = 0;
  for (const [pageIndex, pageOverlays] of groupedByPage.entries()) {
    const page = pages.find((candidate) => candidate.pageIndex === pageIndex)?.container;
    if (!page) {
      skippedOverlayPageIndexes.push(pageIndex);
      continue;
    }
    renderedOverlayPageIndexes.push(pageIndex);
    const rotation = Number.parseInt(page.dataset.pdfRotation ?? "0", 10);
    for (const overlayRect of pageOverlays) {
      if (renderedCount >= maxOverlayCount) {
        truncatedOverlayCount += 1;
        continue;
      }
      const normalizedRect = rotateOverlayRect(overlayRect, rotation);
      const overlay = document.createElement("div");
      overlay.className = `reader-pdf-highlight-overlay reader-pdf-highlight-overlay-${normalizedRect.kind}`;
      overlay.style.left = formatPercentStyle(normalizedRect.left);
      overlay.style.top = formatPercentStyle(normalizedRect.top);
      overlay.style.width = formatPercentStyle(normalizedRect.width);
      overlay.style.height = formatPercentStyle(normalizedRect.height);
      overlay.setAttribute("data-ll-pdf-overlay-sentence-idx", String(normalizedRect.sentenceIndex));
      overlay.setAttribute("data-ll-pdf-overlay-page-idx", String(normalizedRect.pageIndex));
      overlay.setAttribute("data-ll-pdf-overlay-kind", normalizedRect.kind);
      page.appendChild(overlay);
      overlays.push(overlay);
      renderedCount += 1;
      overlaySentenceMap.set(normalizedRect.pageIndex, normalizedRect.sentenceIndex);
    }
    if (pageOverlays.some((overlayRect) => overlayRect.kind === "page")) {
      page.classList.add("reader-pdf-page-active");
      highlightedPages.push(page);
    }
  }
  return {
    highlightedOverlays: overlays,
    highlightedPages,
    overlaySentenceMap,
    renderedOverlayPageIndexes,
    skippedOverlayPageIndexes,
    truncatedOverlayCount
  };
}

function formatPercentStyle(value: number): string {
  return `${Number((value * 100).toFixed(4))}%`;
}

function rotateOverlayRect(rect: PdfOverlayRect, rotationDegrees: number): PdfOverlayRect {
  const normalizedRotation = ((rotationDegrees % 360) + 360) % 360;
  switch (normalizedRotation) {
    case 90:
      return {
        ...rect,
        left: 1 - (rect.top + rect.height),
        top: rect.left,
        width: rect.height,
        height: rect.width
      };
    case 180:
      return {
        ...rect,
        left: 1 - (rect.left + rect.width),
        top: 1 - (rect.top + rect.height)
      };
    case 270:
      return {
        ...rect,
        left: rect.top,
        top: 1 - (rect.left + rect.width),
        width: rect.height,
        height: rect.width
      };
    default:
      return rect;
  }
}
