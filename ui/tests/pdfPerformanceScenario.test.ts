import { describe, expect, it } from "vitest";

import {
  evaluatePdfPerformanceScenario,
  PDF_PERFORMANCE_SCENARIOS
} from "../src/components/pdfPerformanceScenario";

describe("pdfPerformanceScenario", () => {
  it("keeps each baseline scenario within the configured render budgets", () => {
    for (const scenario of PDF_PERFORMANCE_SCENARIOS) {
      const result = evaluatePdfPerformanceScenario(scenario);
      expect(result.openImmediatePages).toBe(1);
      expect(result.canvasPages).toBeLessThanOrEqual(result.profile.maxLiveCanvases);
      expect(result.textLayerPages).toBeLessThanOrEqual(
        Math.max(result.profile.maxLiveTextLayers, result.visiblePages + 1)
      );
      expect(result.canvasPages).toBeLessThan(result.totalPages);
      expect(result.deferredPages).toBeGreaterThan(0);
    }
  });

  it("keeps expensive work near the viewport instead of the whole document", () => {
    const results = PDF_PERFORMANCE_SCENARIOS.map(evaluatePdfPerformanceScenario);
    for (const result of results) {
      expect(result.canvasPages).toBeLessThanOrEqual(result.visiblePages + result.mediumPriorityPages + result.highPriorityPages);
      expect(result.lowPriorityPages).toBeLessThan(result.totalPages / 2);
    }
  });
});
