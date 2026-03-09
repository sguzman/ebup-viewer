import { describe, expect, it } from "vitest";

import {
  buildPdfSentenceSpanMap,
  findNearestSentenceForPageIndex,
  findNearestSentenceForSpanIndex
} from "../src/components/pdfTextSync";

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
      pageOnlyMatches: 0,
      missingMatches: 0,
      cappedLeaps: 0
    });
  });

  it("falls back to a single span when an exact normalized match is unavailable", () => {
    const spans = [
      createSpan(0, "Alpha heading"),
      createSpan(0, "Body text."),
      createSpan(1, "Appendix")
    ];

    const result = buildPdfSentenceSpanMap(spans, ["Alpha missing"]);

    expect(result.matches[0]).toMatchObject({
      confidence: "fallback",
      reason: "paragraph_fallback",
      pageIndex: 0,
      spanIndexes: [0]
    });
    expect(result.diagnostics.fallbackMatches).toBe(1);
  });

  it("uses fuzzy sentence geometry before paragraph fallback when local tokens align", () => {
    const spans = [
      createSpan(0, "Alpha heading"),
      createSpan(0, "The quick"),
      createSpan(0, "brown fox"),
      createSpan(0, "jumps today"),
      createSpan(1, "Appendix")
    ];

    const result = buildPdfSentenceSpanMap(spans, ["The quick brown fox leaps.", "Appendix"]);

    expect(result.matches[0]).toMatchObject({
      confidence: "fallback",
      reason: "fuzzy_sentence_geometry",
      pageIndex: 0,
      spanIndexes: [0, 1, 2]
    });
    expect(result.diagnostics.fallbackMatches).toBe(1);
  });

  it("degrades to page-only location when neither exact nor stable local fallback exists", () => {
    const spans = [
      createSpan(0, "Cover page"),
      createSpan(1, "Appendix"),
      createSpan(2, "References")
    ];

    const result = buildPdfSentenceSpanMap(spans, [
      "References",
      "A sentence that does not exist anywhere in the PDF text layer"
    ]);

    expect(result.matches[1]).toMatchObject({
      confidence: "page",
      reason: "page_location_only",
      pageIndex: 2,
      spanIndexes: []
    });
    expect(result.diagnostics.pageOnlyMatches).toBe(1);
  });

  it("normalizes ligatures and soft hyphenation before matching PDF spans", () => {
    const spans = [
      createSpan(0, "The of\uFB01ce co\u00AD"),
      createSpan(0, "operate plan.")
    ];

    const result = buildPdfSentenceSpanMap(spans, ["The office cooperate plan."]);

    expect(result.matches[0]).toMatchObject({
      confidence: "exact",
      reason: "exact_geometry",
      pageIndex: 0,
      spanIndexes: [0, 1]
    });
    expect(result.matches[0]?.score).toBe(1);
  });

  it("strips zero-width text-layer noise before matching", () => {
    const spans = [
      createSpan(0, "Alpha\u200B beta"),
      createSpan(0, "gamma.")
    ];

    const result = buildPdfSentenceSpanMap(spans, ["Alpha beta gamma."]);

    expect(result.matches[0]).toMatchObject({
      confidence: "exact",
      reason: "exact_geometry",
      pageIndex: 0,
      spanIndexes: [0, 1]
    });
  });

  it("ignores duplicated hidden text-layer spans on the same page", () => {
    const spans = [
      createSpan(0, "Alpha beta gamma."),
      createSpan(0, "Alpha beta gamma."),
      createSpan(0, "Delta epsilon.")
    ];

    const result = buildPdfSentenceSpanMap(spans, ["Alpha beta gamma.", "Delta epsilon."]);

    expect(result.matches[0]).toMatchObject({
      confidence: "exact",
      reason: "exact_geometry",
      pageIndex: 0,
      spanIndexes: [0]
    });
    expect(result.matches[1]).toMatchObject({
      confidence: "exact",
      reason: "exact_geometry",
      pageIndex: 0,
      spanIndexes: [2]
    });
  });

  it("suppresses repeated page-edge boilerplate during fallback matching", () => {
    const spans = [
      createSpan(0, "Journal of Testing"),
      createSpan(0, "Alpha body opening"),
      createSpan(0, "Footer 1"),
      createSpan(1, "Journal of Testing"),
      createSpan(1, "Beta body target"),
      createSpan(1, "Footer 2"),
      createSpan(2, "Journal of Testing"),
      createSpan(2, "Gamma appendix"),
      createSpan(2, "Footer 3")
    ];

    const result = buildPdfSentenceSpanMap(spans, ["Beta target missing exact body text"]);

    expect(result.matches[0]).toMatchObject({
      confidence: "fallback",
      reason: "paragraph_fallback",
      pageIndex: 1,
      spanIndexes: [4]
    });
    expect(result.matches[0]?.score).toBeGreaterThanOrEqual(0.34);
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

  it("finds the nearest sentence for a clicked PDF page", () => {
    const spans = [
      createSpan(0, "Alpha"),
      createSpan(0, "beta"),
      createSpan(1, "Gamma"),
      createSpan(1, "delta"),
      createSpan(2, "Epsilon")
    ];
    const { matches } = buildPdfSentenceSpanMap(spans, ["Alpha beta", "Gamma delta", "Epsilon"]);

    expect(findNearestSentenceForPageIndex(matches, 1)).toBe(1);
    expect(findNearestSentenceForPageIndex(matches, 2)).toBe(2);
    expect(findNearestSentenceForPageIndex(matches, 3)).toBe(2);
    expect(matches[1]?.score).toBe(1);
  });
});
