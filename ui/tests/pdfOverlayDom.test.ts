// @vitest-environment jsdom

import { describe, expect, it } from "vitest";

import { applyPdfLocationHighlightOverlays, clearPdfHighlightOverlays } from "../src/components/pdfOverlayDom";
import type { PdfOverlayRect } from "../src/components/pdfOverlayGeometry";

function createPage(pageIndex: number): { pageIndex: number; container: HTMLDivElement } {
  const container = document.createElement("div");
  container.dataset.pageIndex = String(pageIndex);
  return { pageIndex, container };
}

describe("applyPdfLocationHighlightOverlays", () => {
  it("renders percentage-based sentence overlays with sentence metadata", () => {
    const page = createPage(0);
    const result = applyPdfLocationHighlightOverlays([], [], [page], [{
      pageIndex: 0,
      sentenceIndex: 4,
      kind: "sentence",
      left: 0.1,
      top: 0.2,
      width: 0.3,
      height: 0.4
    } satisfies PdfOverlayRect]);

    expect(result.highlightedOverlays).toHaveLength(1);
    expect(result.highlightedOverlays[0]?.style.left).toBe("10%");
    expect(result.highlightedOverlays[0]?.style.top).toBe("20%");
    expect(result.highlightedOverlays[0]?.style.width).toBe("30%");
    expect(result.highlightedOverlays[0]?.style.height).toBe("40%");
    expect(result.highlightedOverlays[0]?.getAttribute("data-ll-pdf-overlay-sentence-idx")).toBe("4");
    expect(result.renderedOverlayPageIndexes).toEqual([0]);
    expect(result.skippedOverlayPageIndexes).toEqual([]);
  });

  it("marks page overlays active and clears them on teardown", () => {
    const page = createPage(2);
    const result = applyPdfLocationHighlightOverlays([], [], [page], [{
      pageIndex: 2,
      sentenceIndex: 1,
      kind: "page",
      left: 0,
      top: 0,
      width: 1,
      height: 1
    } satisfies PdfOverlayRect]);

    expect(result.highlightedPages).toHaveLength(1);
    expect(page.container.classList.contains("reader-pdf-page-active")).toBe(true);

    clearPdfHighlightOverlays(result.highlightedOverlays, result.highlightedPages);

    expect(page.container.classList.contains("reader-pdf-page-active")).toBe(false);
    expect(page.container.children).toHaveLength(0);
  });

  it("rebinds overlays onto replacement page nodes after rerender cycles", () => {
    const firstPage = createPage(0);
    const first = applyPdfLocationHighlightOverlays([], [], [firstPage], [{
      pageIndex: 0,
      sentenceIndex: 2,
      kind: "line",
      left: 0.15,
      top: 0.1,
      width: 0.5,
      height: 0.08
    } satisfies PdfOverlayRect]);
    const replacementPage = createPage(0);

    const rebound = applyPdfLocationHighlightOverlays(
      first.highlightedOverlays,
      first.highlightedPages,
      [replacementPage],
      [{
        pageIndex: 0,
        sentenceIndex: 2,
        kind: "line",
        left: 0.15,
        top: 0.1,
        width: 0.5,
        height: 0.08
      } satisfies PdfOverlayRect]
    );

    expect(firstPage.container.children).toHaveLength(0);
    expect(replacementPage.container.children).toHaveLength(1);
    expect(rebound.highlightedOverlays[0]?.style.left).toBe("15%");
  });

  it("recomputes only against rendered pages and reports skipped overlay pages", () => {
    const page = createPage(0);
    const result = applyPdfLocationHighlightOverlays([], [], [page], [
      {
        pageIndex: 0,
        sentenceIndex: 7,
        kind: "block",
        left: 0.2,
        top: 0.2,
        width: 0.4,
        height: 0.2
      } satisfies PdfOverlayRect,
      {
        pageIndex: 4,
        sentenceIndex: 7,
        kind: "block",
        left: 0.1,
        top: 0.3,
        width: 0.6,
        height: 0.2
      } satisfies PdfOverlayRect
    ]);

    expect(result.highlightedOverlays).toHaveLength(1);
    expect(result.renderedOverlayPageIndexes).toEqual([0]);
    expect(result.skippedOverlayPageIndexes).toEqual([4]);
  });

  it("rotates normalized overlay geometry with the rendered PDF page rotation", () => {
    const page = createPage(0);
    page.container.dataset.pdfRotation = "90";

    const result = applyPdfLocationHighlightOverlays([], [], [page], [{
      pageIndex: 0,
      sentenceIndex: 5,
      kind: "sentence",
      left: 0.1,
      top: 0.2,
      width: 0.3,
      height: 0.4
    } satisfies PdfOverlayRect]);

    expect(result.highlightedOverlays).toHaveLength(1);
    expect(result.highlightedOverlays[0]?.style.left).toBe("40%");
    expect(result.highlightedOverlays[0]?.style.top).toBe("10%");
    expect(result.highlightedOverlays[0]?.style.width).toBe("40%");
    expect(result.highlightedOverlays[0]?.style.height).toBe("30%");
  });

  it("keeps sentence-following overlay continuity across sentence transitions", () => {
    const firstPage = createPage(0);
    const secondPage = createPage(1);
    const first = applyPdfLocationHighlightOverlays([], [], [firstPage, secondPage], [{
      pageIndex: 0,
      sentenceIndex: 3,
      kind: "line",
      left: 0.15,
      top: 0.12,
      width: 0.6,
      height: 0.08
    } satisfies PdfOverlayRect]);

    expect(first.highlightedOverlays).toHaveLength(1);
    expect(first.overlaySentenceMap.get(0)).toBe(3);

    const second = applyPdfLocationHighlightOverlays(
      first.highlightedOverlays,
      first.highlightedPages,
      [firstPage, secondPage],
      [{
        pageIndex: 1,
        sentenceIndex: 4,
        kind: "line",
        left: 0.2,
        top: 0.22,
        width: 0.55,
        height: 0.08
      } satisfies PdfOverlayRect]
    );

    expect(firstPage.container.children).toHaveLength(0);
    expect(secondPage.container.children).toHaveLength(1);
    expect(second.highlightedOverlays).toHaveLength(1);
    expect(second.overlaySentenceMap.get(0)).toBeUndefined();
    expect(second.overlaySentenceMap.get(1)).toBe(4);
  });

  it("caps rendered overlay nodes and reports truncated overlays", () => {
    const page = createPage(0);
    const overlays = Array.from({ length: 5 }, (_, index) => ({
      pageIndex: 0,
      sentenceIndex: index,
      kind: "line" as const,
      left: 0.1,
      top: 0.1 + index * 0.05,
      width: 0.4,
      height: 0.03
    }));

    const result = applyPdfLocationHighlightOverlays([], [], [page], overlays, 3);

    expect(result.highlightedOverlays).toHaveLength(3);
    expect(result.truncatedOverlayCount).toBe(2);
  });
});
