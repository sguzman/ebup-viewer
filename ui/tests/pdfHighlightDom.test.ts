import { describe, expect, it } from "vitest";

import { applyPdfHighlightDom } from "../src/components/pdfHighlightDom";
import type { PdfSentenceMatch, PdfTextSpan } from "../src/components/pdfTextSync";

function createSpan(text: string): PdfTextSpan {
  return {
    pageIndex: 0,
    text,
    element: {
      classList: {
        values: new Set<string>(),
        add(value: string) {
          this.values.add(value);
        },
        remove(value: string) {
          this.values.delete(value);
        },
        contains(value: string) {
          return this.values.has(value);
        }
      }
    } as unknown as HTMLElement
  };
}

function createPage(pageIndex: number): { pageIndex: number; container: HTMLDivElement } {
  return {
    pageIndex,
    container: {
      classList: {
        values: new Set<string>(),
        add(value: string) {
          this.values.add(value);
        },
        remove(value: string) {
          this.values.delete(value);
        },
        contains(value: string) {
          return this.values.has(value);
        }
      }
    } as unknown as HTMLDivElement
  };
}

function exactMatch(spanIndexes: number[]): PdfSentenceMatch {
  return {
    confidence: "exact",
    reason: "exact_geometry",
    pageIndex: 0,
    spanIndexes,
    score: 1
  };
}

describe("applyPdfHighlightDom", () => {
  it("applies sentence highlight classes to the matched spans", () => {
    const spans = [createSpan("Alpha"), createSpan("Beta"), createSpan("Gamma")];

    const result = applyPdfHighlightDom([], [], spans, [createPage(0)], exactMatch([0, 1]));

    expect(result.highlightedNodes).toHaveLength(2);
    expect((spans[0].element.classList as any).contains("reader-pdf-highlight")).toBe(true);
    expect((spans[1].element.classList as any).contains("reader-pdf-highlight")).toBe(true);
    expect((spans[2].element.classList as any).contains("reader-pdf-highlight")).toBe(false);
  });

  it("clears stale sentence highlights before applying the next target", () => {
    const spans = [createSpan("Alpha"), createSpan("Beta"), createSpan("Gamma")];
    const first = applyPdfHighlightDom([], [], spans, [createPage(0)], exactMatch([0, 1]));

    const second = applyPdfHighlightDom(first.highlightedNodes, first.highlightedPages, spans, [createPage(0)], exactMatch([2]));

    expect((spans[0].element.classList as any).contains("reader-pdf-highlight")).toBe(false);
    expect((spans[1].element.classList as any).contains("reader-pdf-highlight")).toBe(false);
    expect((spans[2].element.classList as any).contains("reader-pdf-highlight")).toBe(true);
    expect(second.highlightedNodes).toHaveLength(1);
  });

  it("highlights the page container for page-only fallback and clears prior span highlights", () => {
    const spans = [createSpan("Alpha"), createSpan("Beta")];
    const pages = [createPage(0), createPage(1)];
    const first = applyPdfHighlightDom([], [], spans, pages, exactMatch([0]));

    const second = applyPdfHighlightDom(first.highlightedNodes, first.highlightedPages, spans, pages, {
      confidence: "page",
      reason: "page_location_only",
      pageIndex: 1,
      spanIndexes: [],
      score: 0.2
    });

    expect((spans[0].element.classList as any).contains("reader-pdf-highlight")).toBe(false);
    expect((pages[1].container.classList as any).contains("reader-pdf-page-active")).toBe(true);
    expect(second.highlightedNodes).toHaveLength(0);
    expect(second.highlightedPages).toHaveLength(1);
  });

  it("removes stale page and span classes when highlight disappears", () => {
    const spans = [createSpan("Alpha")];
    const pages = [createPage(0)];
    const highlighted = applyPdfHighlightDom([], [], spans, pages, {
      confidence: "page",
      reason: "page_location_only",
      pageIndex: 0,
      spanIndexes: [],
      score: 0.2
    });

    const cleared = applyPdfHighlightDom(highlighted.highlightedNodes, highlighted.highlightedPages, spans, pages, null);

    expect((pages[0].container.classList as any).contains("reader-pdf-page-active")).toBe(false);
    expect(cleared.highlightedNodes).toHaveLength(0);
    expect(cleared.highlightedPages).toHaveLength(0);
  });
});
