import { describe, expect, it } from "vitest";

import type { PdfSentenceLocation } from "../src/api/tauri";
import {
  buildOverlayRectsFromLocation,
  rectsToOverlayRects
} from "../src/components/pdfOverlayGeometry";

describe("pdfOverlayGeometry", () => {
  it("prefers line and block rects before raw sentence rects", () => {
    const location: PdfSentenceLocation = {
      sentence_idx: 12,
      page_idx: 4,
      rects: [{ left: 0.1, top: 0.1, width: 0.2, height: 0.03 }],
      line_rects: [{ left: 0.15, top: 0.22, width: 0.45, height: 0.05 }],
      block_rects: [{ left: 0.12, top: 0.2, width: 0.5, height: 0.1 }],
      confidence: "fallback",
      reason: "line_window_fuzzy_alignment",
      score: 0.73
    };

    const overlays = buildOverlayRectsFromLocation(location, 3);

    expect(overlays).toEqual([{
      left: 0.15,
      top: 0.22,
      width: 0.45,
      height: 0.05,
      pageIndex: 4,
      sentenceIndex: 3,
      kind: "line"
    }]);
  });

  it("builds full-page fallback overlays for page-only locations", () => {
    const location: PdfSentenceLocation = {
      sentence_idx: 9,
      page_idx: 2,
      rects: [],
      line_rects: [],
      block_rects: [],
      confidence: "page",
      reason: "page_location_only",
      score: 0.18
    };

    const overlays = buildOverlayRectsFromLocation(location, 1);

    expect(overlays).toEqual([{
      left: 0,
      top: 0,
      width: 1,
      height: 1,
      pageIndex: 2,
      sentenceIndex: 1,
      kind: "page"
    }]);
  });

  it("normalizes DOM rects into page-relative overlays", () => {
    const overlays = rectsToOverlayRects(
      [{
        left: 110,
        top: 240,
        width: 90,
        height: 20,
        right: 200,
        bottom: 260
      }],
      {
        left: 100,
        top: 200,
        width: 200,
        height: 100,
        right: 300,
        bottom: 300
      },
      5,
      7
    );

    expect(overlays).toHaveLength(1);
    expect(overlays[0]).toMatchObject({
      left: 0.05,
      top: 0.4,
      width: 0.45,
      pageIndex: 5,
      sentenceIndex: 7,
      kind: "sentence"
    });
    expect(overlays[0]?.height ?? 0).toBeCloseTo(0.2, 6);
  });
});
