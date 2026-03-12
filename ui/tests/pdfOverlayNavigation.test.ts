// @vitest-environment jsdom

import { describe, expect, it } from "vitest";

import {
  resolveSentenceForPdfPageSelection,
  resolveSentenceFromPdfOverlayTarget,
  resolveSentenceFromPdfSpanTarget
} from "../src/components/pdfOverlayNavigation";
import { PDF_OCR_ALIGNMENT_FIXTURES } from "./fixtures/pdfOcrAlignmentFixtures";

describe("pdfOverlayNavigation", () => {
  it("resolves sentence index directly from overlay targets", () => {
    const overlay = document.createElement("div");
    overlay.setAttribute("data-ll-pdf-overlay-sentence-idx", "12");
    const child = document.createElement("span");
    overlay.appendChild(child);

    expect(resolveSentenceFromPdfOverlayTarget(child)).toBe(12);
  });

  it("resolves nearest sentence from text-layer span targets", () => {
    const span = document.createElement("span");
    span.setAttribute("data-ll-pdf-span-idx", "4");
    const child = document.createElement("em");
    span.appendChild(child);

    const result = resolveSentenceFromPdfSpanTarget(child, [
      { confidence: "exact", reason: "exact_token_chain_alignment", pageIndex: 0, spanIndexes: [0, 1], score: 1 },
      { confidence: "fallback", reason: "line_window_fuzzy_alignment", pageIndex: 0, spanIndexes: [4, 5], score: 0.71 }
    ]);

    expect(result).toEqual({ sentenceIdx: 1, spanIdx: 4 });
  });

  it("uses named OCR fixtures for page-click resolution regression coverage", () => {
    for (const fixture of PDF_OCR_ALIGNMENT_FIXTURES) {
      const resolution = resolveSentenceForPdfPageSelection(
        fixture.pageIndex,
        fixture.matches,
        new Map(fixture.overlaySentenceMap ?? [])
      );
      expect(resolution.sentenceIdx, fixture.name).toBe(fixture.expectedSentenceIdx);
      expect(resolution.reason, fixture.name).toBe(fixture.expectedReason);
    }
  });
});
