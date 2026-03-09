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
  for (const node of previousNodes) {
    node.classList.remove("reader-pdf-highlight");
  }
  for (const page of previousPages) {
    page.classList.remove("reader-pdf-page-active");
  }

  if (!match) {
    return { highlightedNodes: [], highlightedPages: [] };
  }

  if (match.reason === "page_location_only" && match.pageIndex !== null) {
    const page = pages.find((candidate) => candidate.pageIndex === match.pageIndex)?.container;
    if (!page) {
      return { highlightedNodes: [], highlightedPages: [] };
    }
    page.classList.add("reader-pdf-page-active");
    return { highlightedNodes: [], highlightedPages: [page] };
  }

  const highlightedNodes = match.spanIndexes
    .map((spanIdx) => spans[spanIdx]?.element)
    .filter((value): value is HTMLElement => Boolean(value));
  for (const element of highlightedNodes) {
    element.classList.add("reader-pdf-highlight");
  }

  return { highlightedNodes, highlightedPages: [] };
}
