// @vitest-environment jsdom

import { describe, expect, it } from "vitest";

import { annotateNativeHtmlSentences } from "../src/components/nativeHtmlSentenceAnchors";

describe("nativeHtmlSentenceAnchors", () => {
  it("wraps exact sentence spans in native html order", () => {
    const doc = document.implementation.createHTMLDocument("");
    doc.body.innerHTML = `
      <article>
        <p>Alpha sentence. Beta sentence.</p>
        <p><strong>Gamma</strong> sentence.</p>
      </article>
    `;

    const result = annotateNativeHtmlSentences(doc, [
      "Alpha sentence.",
      "Beta sentence.",
      "Gamma sentence."
    ]);

    expect(result.diagnostics.matchedSentences).toBe(3);
    expect(result.sentenceAnchors.get(0)?.map((node) => node.textContent).join("")).toBe(
      "Alpha sentence"
    );
    expect(result.sentenceAnchors.get(1)?.map((node) => node.textContent).join("")).toBe(
      "Beta sentence"
    );
    expect(result.sentenceAnchors.get(2)?.map((node) => node.textContent).join("")).toBe(
      "Gamma sentence"
    );
    expect(doc.querySelectorAll("[data-ll-html-sentence='2']").length).toBe(2);
  });

  it("returns existing sentence anchors without rewrapping", () => {
    const doc = document.implementation.createHTMLDocument("");
    doc.body.innerHTML = `
      <p><span data-ll-html-sentence="0">Alpha sentence.</span></p>
      <p><span data-ll-html-sentence="1">Beta sentence.</span></p>
    `;

    const result = annotateNativeHtmlSentences(doc, ["Alpha sentence.", "Beta sentence."]);

    expect(result.firstAnchors.get(0)?.textContent).toContain("Alpha");
    expect(doc.querySelectorAll("[data-ll-html-sentence]").length).toBe(2);
  });
});
