import type { PdfOverlayRect } from "./pdfOverlayGeometry";

export interface PdfOverlayDomResult {
  highlightedOverlays: HTMLDivElement[];
  highlightedPages: HTMLDivElement[];
  overlaySentenceMap: Map<number, number>;
  renderedOverlayPageIndexes: number[];
  skippedOverlayPageIndexes: number[];
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
  overlayRects: PdfOverlayRect[]
): PdfOverlayDomResult {
  clearPdfHighlightOverlays(previousOverlays, previousPages);
  if (overlayRects.length === 0) {
    return {
      highlightedOverlays: [],
      highlightedPages: [],
      overlaySentenceMap: new Map(),
      renderedOverlayPageIndexes: [],
      skippedOverlayPageIndexes: []
    };
  }
  const overlays: HTMLDivElement[] = [];
  const highlightedPages: HTMLDivElement[] = [];
  const overlaySentenceMap = new Map<number, number>();
  const renderedOverlayPageIndexes: number[] = [];
  const skippedOverlayPageIndexes: number[] = [];
  const groupedByPage = new Map<number, PdfOverlayRect[]>();
  for (const overlayRect of overlayRects) {
    const pageRects = groupedByPage.get(overlayRect.pageIndex);
    if (pageRects) {
      pageRects.push(overlayRect);
    } else {
      groupedByPage.set(overlayRect.pageIndex, [overlayRect]);
    }
  }
  for (const [pageIndex, pageOverlays] of groupedByPage.entries()) {
    const page = pages.find((candidate) => candidate.pageIndex === pageIndex)?.container;
    if (!page) {
      skippedOverlayPageIndexes.push(pageIndex);
      continue;
    }
    renderedOverlayPageIndexes.push(pageIndex);
    for (const overlayRect of pageOverlays) {
      const overlay = document.createElement("div");
      overlay.className = `reader-pdf-highlight-overlay reader-pdf-highlight-overlay-${overlayRect.kind}`;
      overlay.style.left = `${overlayRect.left * 100}%`;
      overlay.style.top = `${overlayRect.top * 100}%`;
      overlay.style.width = `${overlayRect.width * 100}%`;
      overlay.style.height = `${overlayRect.height * 100}%`;
      overlay.setAttribute("data-ll-pdf-overlay-sentence-idx", String(overlayRect.sentenceIndex));
      overlay.setAttribute("data-ll-pdf-overlay-page-idx", String(overlayRect.pageIndex));
      overlay.setAttribute("data-ll-pdf-overlay-kind", overlayRect.kind);
      page.appendChild(overlay);
      overlays.push(overlay);
      overlaySentenceMap.set(overlayRect.pageIndex, overlayRect.sentenceIndex);
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
    skippedOverlayPageIndexes
  };
}
