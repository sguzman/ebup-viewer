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

  it("keeps the first sentence after headings and images alignable", () => {
    const doc = document.implementation.createHTMLDocument("");
    doc.body.innerHTML = `
      <article>
        <h2><em>Chapter One</em></h2>
        <p>First body sentence. Second body sentence.</p>
        <figure><img src="cover.jpg" alt="cover"></figure>
        <p>Sentence after image. Final sentence.</p>
      </article>
    `;

    const result = annotateNativeHtmlSentences(doc, [
      "Chapter One",
      "First body sentence.",
      "Second body sentence.",
      "Sentence after image.",
      "Final sentence."
    ]);

    expect(result.diagnostics.matchedSentences).toBe(5);
    expect(result.sentenceAnchors.get(1)?.map((node) => node.textContent).join("")).toBe(
      "First body sentence"
    );
    expect(result.sentenceAnchors.get(3)?.map((node) => node.textContent).join("")).toBe(
      "Sentence after image"
    );
  });

  it("tolerates minor canonical-vs-dom drift at heading and image boundaries", () => {
    const doc = document.implementation.createHTMLDocument("");
    doc.body.innerHTML = `
      <article>
        <h2><span>Chapter One</span></h2>
        <p>First sentence after heading now begins. Second sentence stays stable.</p>
        <figure><img src="cover.jpg" alt="cover"></figure>
        <p>Sentence after image now resumes here. Final sentence stays stable.</p>
      </article>
    `;

    const result = annotateNativeHtmlSentences(doc, [
      "Chapter One",
      "First sentence after heading begins.",
      "Second sentence stays stable.",
      "Sentence after image resumes here.",
      "Final sentence stays stable."
    ]);

    expect(result.diagnostics.matchedSentences).toBe(5);
    expect(result.sentenceAnchors.get(1)?.map((node) => node.textContent).join("")).toContain(
      "First sentence after heading"
    );
    expect(result.sentenceAnchors.get(3)?.map((node) => node.textContent).join("")).toContain(
      "Sentence after image"
    );
  });

  it("ignores superscript citations when matching sentence highlights", () => {
    const doc = document.implementation.createHTMLDocument("");
    doc.body.innerHTML = `
      <article>
        <p>
          This claim remains disputed<sup><a href="#fn1">1</a></sup>.
          The next sentence should still align.
        </p>
      </article>
    `;

    const result = annotateNativeHtmlSentences(doc, [
      "This claim remains disputed.",
      "The next sentence should still align."
    ]);

    expect(result.diagnostics.matchedSentences).toBe(2);
    expect(result.sentenceAnchors.get(0)?.map((node) => node.textContent).join("")).toContain(
      "This claim remains disputed"
    );
    expect(result.sentenceAnchors.get(1)?.map((node) => node.textContent).join("")).toContain(
      "The next sentence should still align"
    );
  });

  it("keeps short chapter titles alignable when the canonical title drifts slightly", () => {
    const doc = document.implementation.createHTMLDocument("");
    doc.body.innerHTML = `
      <article>
        <h2><span>Chapter One</span></h2>
        <p>Body sentence follows.</p>
      </article>
    `;

    const result = annotateNativeHtmlSentences(doc, [
      "Chapter 1",
      "Body sentence follows."
    ]);

    expect(result.diagnostics.matchedSentences).toBe(2);
    expect(result.sentenceAnchors.get(0)?.map((node) => node.textContent).join("")).toContain(
      "Chapter One"
    );
  });
});
