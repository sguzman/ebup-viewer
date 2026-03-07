import { buildHtmlSentenceAnchorMap } from "./htmlSync";

export type ReaderAnchorAttribute = "data-ll-html-anchor" | "data-ll-md-anchor";

export interface ReaderHtmlSyncResult {
  anchors: Map<number, HTMLElement>;
  diagnostics: ReturnType<typeof buildHtmlSentenceAnchorMap>["diagnostics"];
  map: number[];
}

export function collectIndexedAnchors(
  root: ParentNode,
  attribute: ReaderAnchorAttribute
): Map<number, HTMLElement> {
  const anchors = Array.from(root.querySelectorAll(`[${attribute}]`)) as HTMLElement[];
  const nextMap = new Map<number, HTMLElement>();
  for (const element of anchors) {
    const raw = element.getAttribute(attribute);
    const parsed = raw === null ? Number.NaN : Number.parseInt(raw, 10);
    if (Number.isFinite(parsed)) {
      nextMap.set(parsed, element);
    }
  }
  return nextMap;
}

export function buildReaderHtmlSyncMap(
  anchors: Iterable<HTMLElement>,
  sentences: string[],
  sentenceAnchorMap: Array<number | null>
): ReaderHtmlSyncResult {
  const anchorList = Array.from(anchors);
  const anchorTexts = anchorList.map((element) => element.textContent ?? "");
  const { map, diagnostics } = buildHtmlSentenceAnchorMap(anchorTexts, sentences, sentenceAnchorMap);
  const indexedAnchors = new Map<number, HTMLElement>();
  for (const element of anchorList) {
    const raw = element.getAttribute("data-ll-html-anchor");
    const parsed = raw === null ? Number.NaN : Number.parseInt(raw, 10);
    if (Number.isFinite(parsed)) {
      indexedAnchors.set(parsed, element);
    }
  }
  return {
    anchors: indexedAnchors,
    diagnostics,
    map
  };
}
