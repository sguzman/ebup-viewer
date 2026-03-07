import { renderMarkdownToHtml } from "./markdownRender";
import { renderNativePrettyHtml } from "./prettyHtml";

interface ContentRenderImageCandidate {
  rawPath: string;
  src: string;
}

export function renderPrettyMarkdownDocument(
  markdown: string,
  imageCandidates: ContentRenderImageCandidate[]
): string {
  return renderMarkdownToHtml(markdown, imageCandidates);
}

export function renderPrettyNativeHtmlDocument(
  html: string,
  imageCandidates: ContentRenderImageCandidate[]
): string {
  return renderNativePrettyHtml(html, imageCandidates);
}
