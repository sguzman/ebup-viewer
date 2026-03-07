import { describe, expect, it } from "vitest";

import { renderPrettyMarkdownDocument } from "../src/components/contentRender";

describe("renderMarkdownToHtml", () => {
  it("renders headings, paragraphs, lists, links, and image anchors", () => {
    const out = renderPrettyMarkdownDocument(
      [
        "# Heading",
        "",
        "Paragraph with a [link](https://example.com).",
        "",
        "- First bullet",
        "- Second bullet",
        "",
        "![Cover](images/cover.jpg)"
      ].join("\n"),
      [{ rawPath: "images/cover.jpg", src: "asset:/cache/images/cover.jpg" }]
    );

    expect(out).toContain("<h1 data-ll-md-anchor=\"0\">Heading</h1>");
    expect(out).toContain("<p data-ll-md-anchor=\"1\">Paragraph with a ");
    expect(out).toContain('href="https://example.com"');
    expect(out).toContain("<ul>");
    expect(out).toContain("<li data-ll-md-anchor=\"2\">First bullet</li>");
    expect(out).toContain('src="asset:/cache/images/cover.jpg"');
  });

  it("falls back gracefully when markdown image targets cannot be resolved", () => {
    const out = renderPrettyMarkdownDocument("![Missing](missing.png)", []);

    expect(out).toContain("reader-md-missing-image");
    expect(out).toContain("[image: Missing]");
  });
});
