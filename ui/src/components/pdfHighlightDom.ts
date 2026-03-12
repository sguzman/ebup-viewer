import type { PdfSentenceMatch, PdfTextSpan } from "./pdfTextSync";

export interface PdfHighlightDomResult {
  highlightedNodes: HTMLElement[];
  highlightedPages: HTMLDivElement[];
}

export function applyPdfHighlightDom(
  previousNodes: HTMLElement[],
  previousPages: HTMLDivElement[],
  spans: PdfTextSpan[],
  pages: Array<{ pageIndex: number; container: HTMLDivElement }>,
  match: PdfSentenceMatch | null | undefined
): PdfHighlightDomResult {
  let highlightedNodes: HTMLElement[] = [];
  let highlightedPages: HTMLDivElement[] = [];
  if (!match) {
    syncHighlightedElements(previousNodes, []);
    syncHighlightedPages(previousPages, []);
    return { highlightedNodes: [], highlightedPages: [] };
  }

  if (match.reason === "page_location_only" && match.pageIndex !== null) {
    const page = pages.find((candidate) => candidate.pageIndex === match.pageIndex)?.container;
    if (!page) {
      syncHighlightedElements(previousNodes, []);
      syncHighlightedPages(previousPages, []);
      return { highlightedNodes: [], highlightedPages: [] };
    }
    highlightedPages = [page];
    syncHighlightedElements(previousNodes, highlightedNodes);
    syncHighlightedPages(previousPages, highlightedPages);
    return { highlightedNodes, highlightedPages };
  }

  highlightedNodes = match.spanIndexes
    .map((spanIdx) => spans[spanIdx]?.element)
    .filter((value): value is HTMLElement => Boolean(value));
  syncHighlightedElements(previousNodes, highlightedNodes);
  syncHighlightedPages(previousPages, highlightedPages);

  return { highlightedNodes, highlightedPages };
}

function syncHighlightedElements(previousNodes: HTMLElement[], nextNodes: HTMLElement[]): void {
  const nextNodeSet = new Set(nextNodes);
  for (const node of previousNodes) {
    if (!nextNodeSet.has(node)) {
      node.classList.remove("reader-pdf-highlight");
    }
  }
  for (const node of nextNodes) {
    node.classList.add("reader-pdf-highlight");
  }
}

function syncHighlightedPages(previousPages: HTMLDivElement[], nextPages: HTMLDivElement[]): void {
  const nextPageSet = new Set(nextPages);
  for (const page of previousPages) {
    if (!nextPageSet.has(page)) {
      page.classList.remove("reader-pdf-page-active");
    }
  }
  for (const page of nextPages) {
    page.classList.add("reader-pdf-page-active");
  }
}
