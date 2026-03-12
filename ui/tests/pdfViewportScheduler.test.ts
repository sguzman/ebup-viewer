import { describe, expect, it } from "vitest";

import {
  buildPdfViewportRenderPlan,
  choosePdfViewportEvictions,
  computePdfPreviewScale,
  type PdfPageRegistryEntry
} from "../src/components/pdfViewportScheduler";

describe("pdfViewportScheduler", () => {
  it("prioritizes visible overscan plus active and jump targets", () => {
    const plan = buildPdfViewportRenderPlan({
      totalPages: 20,
      visiblePageIndexes: [5, 6],
      overscan: 1,
      activeTtsPageIndex: 12,
      jumpTargetPageIndex: 18
    });

    expect(plan.canvasPageIndexes).toEqual([4, 5, 6, 7, 12, 18]);
    expect(plan.textLayerPageIndexes).toEqual([5, 6, 12, 18]);
    expect(plan.priorityPageIndexes).toEqual([6, 12, 18]);
    expect(plan.mediumPriorityPageIndexes).toEqual([4, 5, 7]);
    expect(plan.lowPriorityPageIndexes).toEqual([2, 3, 8, 9]);
  });

  it("evicts stale canvases and text layers outside the keep window", () => {
    const entries: PdfPageRegistryEntry[] = [
      { pageIndex: 0, state: "canvas_ready", lastTouchedAt: 1, renderedZoom: 1.2, textLayerZoom: null },
      { pageIndex: 1, state: "text_ready", lastTouchedAt: 2, renderedZoom: 1.2, textLayerZoom: 1.2 },
      { pageIndex: 2, state: "text_ready", lastTouchedAt: 3, renderedZoom: 1.2, textLayerZoom: 1.2 },
      { pageIndex: 3, state: "canvas_ready", lastTouchedAt: 4, renderedZoom: 1.2, textLayerZoom: null },
      { pageIndex: 4, state: "text_ready", lastTouchedAt: 5, renderedZoom: 1.2, textLayerZoom: 1.2 }
    ];

    const decision = choosePdfViewportEvictions({
      entries,
      keepCanvasPageIndexes: [2, 3, 4],
      keepTextLayerPageIndexes: [3, 4],
      maxCanvasPages: 3,
      maxTextLayerPages: 2
    });

    expect(decision.evictCanvasPageIndexes).toEqual([0, 1]);
    expect(decision.evictTextLayerPageIndexes).toEqual([1]);
  });

  it("computes preview scale from display zoom against rendered zoom", () => {
    expect(computePdfPreviewScale(1.8, 1.2)).toBeCloseTo(1.5);
    expect(computePdfPreviewScale(1.2, 1.2)).toBeCloseTo(1);
    expect(computePdfPreviewScale(1.2, null)).toBe(1);
  });
});
