import { describe, expect, it } from "vitest";

import { buildPdfSentenceSpanMap, findNearestSentenceForSpanIndex } from "../src/components/pdfTextSync";

function createSpan(pageIndex: number, text: string): { pageIndex: number; text: string; element: HTMLElement } {
  const element = {
    textContent: text
  } as HTMLElement;
  return { pageIndex, text, element };
}

describe("buildPdfSentenceSpanMap", () => {
  it("maps a sentence across multiple adjacent spans", () => {
    const spans = [
      createSpan(0, "Alpha"),
      createSpan(0, "beta"),
      createSpan(0, "gamma."),
      createSpan(1, "Delta"),
      createSpan(1, "epsilon.")
    ];

    const result = buildPdfSentenceSpanMap(spans, ["Alpha beta gamma.", "Delta epsilon."]);

    expect(result.matches[0]).toMatchObject({
      confidence: "exact",
      reason: "exact_geometry",
      pageIndex: 0,
      spanIndexes: [0, 1, 2]
    });
    expect(result.matches[1]).toMatchObject({
      confidence: "exact",
      reason: "exact_geometry",
      pageIndex: 1,
      spanIndexes: [3, 4]
    });
    expect(result.diagnostics).toEqual({
      exactMatches: 2,
      fallbackMatches: 0,
      missingMatches: 0
    });
  });

  it("falls back to a single span when an exact normalized match is unavailable", () => {
    const spans = [
      createSpan(0, "Alpha heading"),
      createSpan(0, "Body text."),
      createSpan(1, "Appendix")
    ];

    const result = buildPdfSentenceSpanMap(spans, ["Missing sentence", "Appendix"]);

    expect(result.matches[0]).toMatchObject({
      confidence: "fallback",
      reason: "paragraph_fallback",
      pageIndex: 0,
      spanIndexes: [0]
    });
    expect(result.matches[1]).toMatchObject({
      confidence: "exact",
      pageIndex: 1,
      spanIndexes: [2]
    });
    expect(result.diagnostics.fallbackMatches).toBe(1);
  });

  it("finds the nearest sentence for a clicked span index", () => {
    const spans = [
      createSpan(0, "Alpha"),
      createSpan(0, "beta"),
      createSpan(0, "gamma."),
      createSpan(0, "Delta")
    ];
    const { matches } = buildPdfSentenceSpanMap(spans, ["Alpha beta gamma.", "Delta"]);
    expect(findNearestSentenceForSpanIndex(matches, 1)).toBe(0);
    expect(findNearestSentenceForSpanIndex(matches, 3)).toBe(1);
  });
});
