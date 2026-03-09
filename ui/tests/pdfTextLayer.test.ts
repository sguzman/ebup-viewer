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
      createElement("right-1", { top: 10, left: 260 }),
      createElement("left-1", { top: 10, left: 24 }),
      createElement("right-2", { top: 30, left: 260 }),
      createElement("left-2", { top: 30, left: 24 }),
      createElement("right-3", { top: 50, left: 260 }),
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
});
