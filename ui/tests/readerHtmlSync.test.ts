// @vitest-environment jsdom

import { describe, expect, it } from "vitest";

import { buildReaderHtmlSyncMap, collectIndexedAnchors } from "../src/components/readerHtmlSync";

describe("readerHtmlSync", () => {
  it("collects numeric indexed anchors from a document root", () => {
    const root = document.createElement("div");
    root.innerHTML = `
      <p data-ll-html-anchor="0">Alpha.</p>
      <p data-ll-html-anchor="1">Beta.</p>
      <p data-ll-html-anchor="oops">Ignored.</p>
    `;

    const anchors = collectIndexedAnchors(root, "data-ll-html-anchor");

    expect(Array.from(anchors.keys())).toEqual([0, 1]);
    expect(anchors.get(0)?.textContent).toContain("Alpha");
  });

  it("builds end-to-end html sync mappings from indexed anchors and sentences", () => {
    const root = document.createElement("div");
    root.innerHTML = `
      <p data-ll-html-anchor="0">Alpha sentence.</p>
      <p data-ll-html-anchor="1">Beta sentence.</p>
      <p data-ll-html-anchor="2">Gamma sentence.</p>
    `;

    const anchors = collectIndexedAnchors(root, "data-ll-html-anchor");
    const result = buildReaderHtmlSyncMap(
      anchors.values(),
      ["Alpha sentence.", "Gamma sentence."],
      [0, 2]
    );

    expect(result.map).toEqual([0, 2]);
    expect(result.anchors.get(2)?.textContent).toContain("Gamma");
  });
});
