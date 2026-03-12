import { describe, expect, it } from "vitest";

import { buildPdfOpenPlan } from "../src/components/pdfDocumentModel";

describe("pdfDocumentModel", () => {
  it("builds an initial open plan with current page first adjacent pages second", () => {
    const plan = buildPdfOpenPlan(8, 3);

    expect(plan.currentPageIndex).toBe(3);
    expect(plan.immediatePageIndexes).toEqual([3]);
    expect(plan.adjacentPageIndexes).toEqual([2, 4]);
    expect(plan.deferredPageIndexes).toEqual([0, 1, 5, 6, 7]);
  });

  it("clamps the open plan for single-page and out-of-range documents", () => {
    expect(buildPdfOpenPlan(1, 10)).toEqual({
      currentPageIndex: 0,
      immediatePageIndexes: [0],
      adjacentPageIndexes: [],
      deferredPageIndexes: []
    });
  });
});
