import { describe, expect, it } from "vitest";

import { orderPdfTextLayerSpans } from "../src/components/pdfTextLayer";

function createElement(
  text: string,
  rect: { top: number; left: number; width?: number; height?: number }
): HTMLElement {
  return {
    textContent: text,
    getBoundingClientRect() {
      return {
        top: rect.top,
        left: rect.left,
        width: rect.width ?? 40,
        height: rect.height ?? 12
      } as DOMRect;
    }
  } as HTMLElement;
}

describe("orderPdfTextLayerSpans", () => {
  it("sorts one-column spans by top then left", () => {
    const spans = orderPdfTextLayerSpans([
      createElement("gamma", { top: 48, left: 20 }),
      createElement("alpha", { top: 12, left: 20 }),
      createElement("beta", { top: 28, left: 22 })
    ], 0);

    expect(spans.map((span) => span.text)).toEqual(["alpha", "beta", "gamma"]);
  });

  it("prefers full left-column then right-column order for strong two-column layouts", () => {
    const spans = orderPdfTextLayerSpans([
      createElement("right-1", { top: 14, left: 260 }),
      createElement("left-1", { top: 10, left: 24 }),
      createElement("right-2", { top: 36, left: 260 }),
      createElement("left-2", { top: 30, left: 24 }),
      createElement("right-3", { top: 58, left: 260 }),
      createElement("left-3", { top: 50, left: 24 })
    ], 0);

    expect(spans.map((span) => span.text)).toEqual([
      "left-1",
      "left-2",
      "left-3",
      "right-1",
      "right-2",
      "right-3"
    ]);
  });

  it("pushes bottom note bands after each column body in strong two-column layouts", () => {
    const spans = orderPdfTextLayerSpans([
      createElement("right-body-1", { top: 14, left: 260 }),
      createElement("left-body-1", { top: 10, left: 24 }),
      createElement("right-body-2", { top: 36, left: 260 }),
      createElement("left-body-2", { top: 32, left: 24 }),
      createElement("right-body-3", { top: 58, left: 260 }),
      createElement("left-body-3", { top: 54, left: 24 }),
      createElement("left-footnote", { top: 120, left: 24 }),
      createElement("left-footnote-2", { top: 132, left: 24 }),
      createElement("right-footnote", { top: 124, left: 260 }),
      createElement("right-footnote-2", { top: 136, left: 260 })
    ], 0);

    expect(spans.map((span) => span.text)).toEqual([
      "left-body-1",
      "left-body-2",
      "left-body-3",
      "left-footnote",
      "left-footnote-2",
      "right-body-1",
      "right-body-2",
      "right-body-3",
      "right-footnote",
      "right-footnote-2"
    ]);
  });

  it("keeps full-width caption bands between two-column body sections", () => {
    const spans = orderPdfTextLayerSpans([
      createElement("right-top-1", { top: 14, left: 260 }),
      createElement("left-top-1", { top: 10, left: 24 }),
      createElement("right-top-2", { top: 36, left: 260 }),
      createElement("left-top-2", { top: 30, left: 24 }),
      createElement("right-top-3", { top: 58, left: 260 }),
      createElement("left-top-3", { top: 50, left: 24 }),
      createElement("Figure 1. Full width caption", { top: 72, left: 120, width: 160 }),
      createElement("right-bottom-1", { top: 96, left: 260 }),
      createElement("left-bottom-1", { top: 92, left: 24 }),
      createElement("right-bottom-2", { top: 116, left: 260 }),
      createElement("left-bottom-2", { top: 112, left: 24 }),
      createElement("right-bottom-3", { top: 136, left: 260 }),
      createElement("left-bottom-3", { top: 132, left: 24 })
    ], 0);

    expect(spans.map((span) => span.text)).toEqual([
      "left-top-1",
      "left-top-2",
      "left-top-3",
      "right-top-1",
      "right-top-2",
      "right-top-3",
      "Figure 1. Full width caption",
      "left-bottom-1",
      "left-bottom-2",
      "left-bottom-3",
      "right-bottom-1",
      "right-bottom-2",
      "right-bottom-3"
    ]);
  });

  it("reorders interleaved table cells into row-major reading order", () => {
    const spans = orderPdfTextLayerSpans([
      createElement("r2c2", { top: 40, left: 160 }),
      createElement("r1c1", { top: 10, left: 24 }),
      createElement("r2c1", { top: 40, left: 24 }),
      createElement("r1c2", { top: 10, left: 160 }),
      createElement("r3c2", { top: 70, left: 160 }),
      createElement("r3c1", { top: 70, left: 24 })
    ], 0);

    expect(spans.map((span) => span.text)).toEqual([
      "r1c1",
      "r1c2",
      "r2c1",
      "r2c2",
      "r3c1",
      "r3c2"
    ]);
  });

  it("preserves reading order for rotated pages", () => {
    const spans = orderPdfTextLayerSpans([
      createElement("alpha", { top: 140, left: 20 }),
      createElement("beta", { top: 120, left: 20 }),
      createElement("gamma", { top: 100, left: 20 })
    ], 0, 90);

    expect(spans.map((span) => span.text)).toEqual(["gamma", "beta", "alpha"]);
  });
});
