import { describe, expect, it } from "vitest";

import { canReuseCachedPdfHighlightTarget } from "../src/components/pdfHighlightController";

describe("pdfHighlightController", () => {
  it("reuses stable overlay targets for the same sentence", () => {
    expect(canReuseCachedPdfHighlightTarget({
      sentenceIndex: 12,
      pageIndex: 3,
      match: {
        confidence: "fallback",
        reason: "line_window_fuzzy_alignment",
        pageIndex: 3,
        spanIndexes: [4, 5],
        score: 0.78
      },
      overlayRects: [{
        left: 0.1,
        top: 0.2,
        width: 0.4,
        height: 0.08,
        pageIndex: 3,
        sentenceIndex: 12,
        kind: "line"
      }],
      useOverlay: true
    }, 12)).toBe(true);
  });

  it("does not reuse page-only fallback targets", () => {
    expect(canReuseCachedPdfHighlightTarget({
      sentenceIndex: 12,
      pageIndex: 3,
      match: {
        confidence: "page",
        reason: "page_location_only",
        pageIndex: 3,
        spanIndexes: [],
        score: 0.2
      },
      overlayRects: [{
        left: 0,
        top: 0,
        width: 1,
        height: 1,
        pageIndex: 3,
        sentenceIndex: 12,
        kind: "page"
      }],
      useOverlay: true
    }, 12)).toBe(false);
  });
});
