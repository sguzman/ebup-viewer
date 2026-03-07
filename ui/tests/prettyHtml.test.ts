// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { renderNativePrettyHtml } from "../src/components/prettyHtml";

describe("renderNativePrettyHtml", () => {
  it("sanitizes unsafe markup and rewrites image/link targets", () => {
    const html = `
      <section>
        <h1 onclick="evil()">Title</h1>
        <p style="color:red">Body <a href="https://example.com/path">external</a></p>
        <img src="OPS/images/cover.jpg" />
        <script>alert('xss')</script>
      </section>
    `;
    const out = renderNativePrettyHtml(html, [
      {
        rawPath: "images/img-0001-aabbccddeeff-cover.jpg",
        src: "asset:/cache/images/img-0001-aabbccddeeff-cover.jpg",
      },
    ]);
    expect(out).toContain("<!doctype html>");
    expect(out).toContain('data-ll-html-anchor="0"');
    expect(out).toContain('src="asset:/cache/images/img-0001-aabbccddeeff-cover.jpg"');
    expect(out).not.toContain("<script");
    expect(out).not.toContain("onclick=");
    expect(out).toContain('style="color:red"');
  });

  it("preserves internal anchors and table/footnote-like content", () => {
    const html = `
      <article>
        <p id="fnref1"><a href="#fn1">[1]</a></p>
        <table><tbody><tr><td>row</td></tr></tbody></table>
        <p id="fn1">Footnote body</p>
      </article>
    `;
    const out = renderNativePrettyHtml(html, []);
    expect(out).toContain('href="#fn1"');
    expect(out).toContain("<table>");
    expect(out).toContain("Footnote body");
    expect(out).toContain('data-ll-html-anchor="0"');
  });

  it("does not transform markdown-style link/image syntax into HTML tags", () => {
    const html = `<p>Raw markdown [link](doc.md) and ![img](cover.png)</p>`;
    const out = renderNativePrettyHtml(html, []);
    expect(out).toContain("[link](doc.md)");
    expect(out).toContain("![img](cover.png)");
    expect(out).not.toContain('href="doc.md"');
    expect(out).not.toContain("<img");
  });

  it("rewrites svg image xlink references for epub cover pages", () => {
    const html = `
      <svg viewBox="0 0 100 100">
        <image width="100" height="100" xlink:href="images/00161.jpeg"></image>
      </svg>
    `;
    const out = renderNativePrettyHtml(html, [
      {
        rawPath: "images/img-0010-deadbeef0011-00161.jpeg",
        src: "asset:/cache/images/img-0010-deadbeef0011-00161.jpeg",
      },
    ]);
    expect(out).toContain('xlink:href="asset:/cache/images/img-0010-deadbeef0011-00161.jpeg"');
    expect(out).toContain('href="asset:/cache/images/img-0010-deadbeef0011-00161.jpeg"');
  });

  it("rewrites mixed cover refs for epub cover chapter markup", () => {
    const html = `
      <section>
        <svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
          <image width="600" height="909" xlink:href="images/00161.jpeg"></image>
        </svg>
        <p class="coverimage"><img src="../images/00001.jpeg" alt="img"/></p>
      </section>
    `;
    const out = renderNativePrettyHtml(html, [
      {
        rawPath: "/tmp/cache/images/images/00161.jpeg",
        src: "asset:/cache/images/images/00161.jpeg",
      },
      {
        rawPath: "/tmp/cache/images/images/00001.jpeg",
        src: "asset:/cache/images/images/00001.jpeg",
      },
    ]);
    expect(out).toContain('xlink:href="asset:/cache/images/images/00161.jpeg"');
    expect(out).toContain('href="asset:/cache/images/images/00161.jpeg"');
    expect(out).toContain('src="asset:/cache/images/images/00001.jpeg"');
  });

  it("does not assign block-level section/article anchors that over-highlight whole pages", () => {
    const html = `
      <section><article><p>Sentence one.</p><p>Sentence two.</p></article></section>
    `;
    const out = renderNativePrettyHtml(html, []);
    expect(out).not.toContain("<section data-ll-html-anchor=");
    expect(out).not.toContain("<article data-ll-html-anchor=");
    expect(out).not.toContain("<img data-ll-html-anchor=");
    expect(out).toContain("<p data-ll-html-anchor=");
  });

  it("anchors leaf div text blocks for browser-like article paragraphs without anchoring wrappers", () => {
    const html = `
      <article>
        <div class="wrapper">
          <div>First paragraph inside a div block with enough text to anchor meaningfully.</div>
          <div>Second paragraph inside another div block with enough text to anchor meaningfully.</div>
        </div>
      </article>
    `;
    const out = renderNativePrettyHtml(html, []);
    expect(out).not.toContain('<article data-ll-html-anchor=');
    expect(out).not.toContain('<div class="wrapper" data-ll-html-anchor=');
    expect(out).toContain('First paragraph inside a div block');
    expect(out).toContain('Second paragraph inside another div block');
    expect((out.match(/data-ll-html-anchor="/g) ?? []).length).toBe(2);
  });

  it("rewrites relative links and images against browser-tab base urls", () => {
    const html = `
      <div data-ll-base-url="https://example.com/articles/start">
        <p><a href="/docs/page-2">Next</a></p>
        <img src="./cover.jpg" alt="Cover"/>
      </div>
    `;
    const out = renderNativePrettyHtml(html, []);
    expect(out).toContain('<base href="https://example.com/articles/start">');
    expect(out).toContain('href="https://example.com/docs/page-2"');
    expect(out).toContain('src="https://example.com/articles/cover.jpg"');
    expect(out).not.toContain("data-ll-base-url");
  });

  it("rewrites browser-tab css asset urls to cached local assets", () => {
    const html = `
      <div data-ll-base-url="https://example.com/articles/start">
        <style>.hero{background-image:url("../img/hero.png")}</style>
        <p style="background-image:url('./inline.png')">Body</p>
        <img src="https://example.com/articles/cover.jpg" alt="Cover"/>
      </div>
    `;
    const out = renderNativePrettyHtml(html, [
      {
        rawPath: "https://example.com/img/hero.png",
        src: "asset:/cache/browser-tabs/assets/hero.png",
      },
      {
        rawPath: "https://example.com/articles/inline.png",
        src: "asset:/cache/browser-tabs/assets/inline.png",
      },
      {
        rawPath: "https://example.com/articles/cover.jpg",
        src: "asset:/cache/browser-tabs/assets/cover.jpg",
      },
    ]);
    expect(out).toContain('background-image:url("asset:/cache/browser-tabs/assets/hero.png")');
    expect(out).toContain('style="background-image:url(&quot;asset:/cache/browser-tabs/assets/inline.png&quot;)"');
    expect(out).toContain('src="asset:/cache/browser-tabs/assets/cover.jpg"');
  });

  it("preserves full browser-tab documents instead of extracting a focused subtree", () => {
    const html = `
      <div data-ll-base-url="https://en.wikipedia.org/wiki/Example" data-ll-browser-tab="1">
        <html class="skin-vector-2022">
          <head>
            <style>body.skin-vector-2022 .mw-parser-output p{font-size:18px}</style>
          </head>
          <body class="mediawiki">
            <header>Site header</header>
            <nav>Sidebar nav</nav>
            <main id="content">
              <div class="mw-body-content">
                <div class="mw-parser-output">
                  <p>${"Article body ".repeat(80)}</p>
                </div>
              </div>
            </main>
            <footer>Site footer</footer>
          </body>
        </html>
      </div>
    `;
    const out = renderNativePrettyHtml(html, []);
    expect(out).toContain("Article body");
    expect(out).toContain("Site header");
    expect(out).toContain("Sidebar nav");
    expect(out).toContain("Site footer");
    expect(out).toContain("skin-vector-2022");
  });

  it("keeps full imported browser-tab content without pruning site chrome", () => {
    const html = `
      <div data-ll-base-url="https://example.com/story" data-ll-browser-tab="1">
        <article id="story">
          <div id="top-wrapper">Advertisement SKIP ADVERTISEMENT</div>
          <header><h1>Story Title</h1><img src="/hero.jpg" alt="Hero"/></header>
          <section>
            <div>${"Primary body text ".repeat(80)}</div>
          </section>
          <div id="recirculation">Related content and promos</div>
        </article>
      </div>
    `;
    const out = renderNativePrettyHtml(html, [
      { rawPath: "https://example.com/hero.jpg", src: "asset:/cache/browser-tabs/assets/hero.jpg" }
    ]);
    expect(out).toContain("Primary body text");
    expect(out).toContain("Story Title");
    expect(out).toContain('src="asset:/cache/browser-tabs/assets/hero.jpg"');
    expect(out).toContain("Advertisement");
    expect(out).toContain("Related content and promos");
  });
});
