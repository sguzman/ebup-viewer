import type { PdfSentenceLocation, PdfSentenceRect } from "../api/tauri";
import type { PdfSentenceMatch, PdfTextSpan } from "./pdfTextSync";

export interface PdfOverlayRect {
  left: number;
  top: number;
  width: number;
  height: number;
  pageIndex: number;
  sentenceIndex: number;
  kind: "sentence" | "line" | "block" | "page";
}

interface RectLike {
  left: number;
  top: number;
  width: number;
  height: number;
  right: number;
  bottom: number;
}

function clamp01(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }
  return Math.max(0, Math.min(1, value));
}

function normalizeRect(rect: PdfSentenceRect, pageIndex: number, sentenceIndex: number, kind: PdfOverlayRect["kind"]): PdfOverlayRect {
  return {
    left: clamp01(rect.left),
    top: clamp01(rect.top),
    width: clamp01(rect.width),
    height: clamp01(rect.height),
    pageIndex,
    sentenceIndex,
    kind
  };
}

export function buildOverlayRectsFromLocation(
  location: PdfSentenceLocation,
  sentenceIndex: number
): PdfOverlayRect[] {
  if (location.page_idx === null || location.page_idx === undefined) {
    return [];
  }
  if (location.reason === "page_location_only") {
    return [{
      left: 0,
      top: 0,
      width: 1,
      height: 1,
      pageIndex: location.page_idx,
      sentenceIndex,
      kind: "page"
    }];
  }
  if (location.line_rects.length > 0) {
    return location.line_rects.map((rect) => normalizeRect(rect, location.page_idx!, sentenceIndex, "line"));
  }
  if (location.block_rects.length > 0) {
    return location.block_rects.map((rect) => normalizeRect(rect, location.page_idx!, sentenceIndex, "block"));
  }
  return location.rects.map((rect) => normalizeRect(rect, location.page_idx!, sentenceIndex, "sentence"));
}

export function rectsToOverlayRects(
  rects: RectLike[],
  pageRect: RectLike,
  pageIndex: number,
  sentenceIndex: number
): PdfOverlayRect[] {
  if (pageRect.width <= 0 || pageRect.height <= 0) {
    return [];
  }
  return rects
    .filter((rect) => rect.width > 0 && rect.height > 0)
    .map((rect) => {
      const left = clamp01((rect.left - pageRect.left) / pageRect.width);
      const top = clamp01((rect.top - pageRect.top) / pageRect.height);
      const right = clamp01((rect.right - pageRect.left) / pageRect.width);
      const bottom = clamp01((rect.bottom - pageRect.top) / pageRect.height);
      return {
        left,
        top,
        width: Math.max(0, right - left),
        height: Math.max(0, bottom - top),
        pageIndex,
        sentenceIndex,
        kind: "sentence" as const
      };
    })
    .filter((rect) => rect.width > 0 && rect.height > 0);
}

export function buildOverlayRectsFromMatch(
  spans: PdfTextSpan[],
  match: PdfSentenceMatch,
  sentenceIndex: number
): PdfOverlayRect[] {
  if (match.pageIndex === null) {
    return [];
  }
  if (match.reason === "page_location_only") {
    return [{
      left: 0,
      top: 0,
      width: 1,
      height: 1,
      pageIndex: match.pageIndex,
      sentenceIndex,
      kind: "page"
    }];
  }
  const spanElements = match.spanIndexes
    .map((spanIndex) => spans[spanIndex]?.element)
    .filter((value): value is HTMLElement => Boolean(value));
  if (spanElements.length === 0) {
    return [];
  }
  const pageElement = spanElements[0]?.closest(".reader-pdf-page") as HTMLElement | null;
  if (!pageElement) {
    return [];
  }
  const pageRect = pageElement.getBoundingClientRect();
  const rects = spanElements.flatMap((element) => Array.from(element.getClientRects()));
  return rectsToOverlayRects(rects, pageRect, match.pageIndex, sentenceIndex);
}
