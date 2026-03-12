import {
  findNearestSentenceForPageIndex,
  findNearestSentenceForSpanIndex,
  type PdfSentenceMatch
} from "./pdfTextSync";

export function resolveSentenceFromPdfOverlayTarget(target: HTMLElement | null): number | null {
  const overlay = target?.closest("[data-ll-pdf-overlay-sentence-idx]") as HTMLElement | null;
  if (!overlay) {
    return null;
  }
  const raw = overlay.getAttribute("data-ll-pdf-overlay-sentence-idx");
  const sentenceIdx = raw === null ? Number.NaN : Number.parseInt(raw, 10);
  return Number.isFinite(sentenceIdx) ? sentenceIdx : null;
}

export function resolveSentenceFromPdfSpanTarget(
  target: HTMLElement | null,
  matches: PdfSentenceMatch[]
): { sentenceIdx: number | null; spanIdx: number | null } {
  const span = target?.closest("[data-ll-pdf-span-idx]") as HTMLElement | null;
  if (!span) {
    return { sentenceIdx: null, spanIdx: null };
  }
  const raw = span.getAttribute("data-ll-pdf-span-idx");
  const spanIdx = raw === null ? Number.NaN : Number.parseInt(raw, 10);
  if (!Number.isFinite(spanIdx)) {
    return { sentenceIdx: null, spanIdx: null };
  }
  return {
    sentenceIdx: findNearestSentenceForSpanIndex(matches, spanIdx),
    spanIdx
  };
}

export function resolveSentenceForPdfPageSelection(
  pageIndex: number,
  matches: PdfSentenceMatch[],
  overlaySentenceMap: Map<number, number>
): { sentenceIdx: number | null; reason: "page_click" | "page_overlay_fallback" | "missing" } {
  const sentenceIdx = findNearestSentenceForPageIndex(matches, pageIndex);
  if (sentenceIdx !== null) {
    return { sentenceIdx, reason: "page_click" };
  }
  return {
    sentenceIdx: overlaySentenceMap.get(pageIndex) ?? null,
    reason: overlaySentenceMap.has(pageIndex) ? "page_overlay_fallback" : "missing"
  };
}
