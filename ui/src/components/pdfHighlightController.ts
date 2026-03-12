import type { PdfOverlayRect } from "./pdfOverlayGeometry";
import type { PdfSentenceMatch } from "./pdfTextSync";

export interface CachedPdfHighlightTarget {
  sentenceIndex: number;
  pageIndex: number | null;
  match: PdfSentenceMatch;
  overlayRects: PdfOverlayRect[];
  useOverlay: boolean;
}

export function canReuseCachedPdfHighlightTarget(
  cached: CachedPdfHighlightTarget | undefined,
  sentenceIndex: number
): cached is CachedPdfHighlightTarget {
  return (
    cached !== undefined
    && cached.sentenceIndex === sentenceIndex
    && cached.useOverlay
    && cached.match.reason !== "page_location_only"
  );
}

export function buildCachedPdfHighlightTarget(
  sentenceIndex: number,
  match: PdfSentenceMatch,
  overlayRects: PdfOverlayRect[],
  useOverlay: boolean
): CachedPdfHighlightTarget {
  return {
    sentenceIndex,
    pageIndex: match.pageIndex,
    match,
    overlayRects,
    useOverlay
  };
}
