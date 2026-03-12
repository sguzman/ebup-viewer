export interface PdfOpenPlan {
  currentPageIndex: number;
  immediatePageIndexes: number[];
  adjacentPageIndexes: number[];
  deferredPageIndexes: number[];
}

function clampPage(pageIndex: number, totalPages: number): number {
  return Math.max(0, Math.min(totalPages - 1, pageIndex));
}

export function buildPdfOpenPlan(totalPages: number, currentPageIndex: number): PdfOpenPlan {
  if (totalPages <= 0) {
    return {
      currentPageIndex: 0,
      immediatePageIndexes: [],
      adjacentPageIndexes: [],
      deferredPageIndexes: []
    };
  }
  const target = clampPage(currentPageIndex, totalPages);
  const immediatePageIndexes = [target];
  const adjacentPageIndexes = Array.from(new Set([
    clampPage(target - 1, totalPages),
    clampPage(target + 1, totalPages)
  ])).filter((pageIndex) => pageIndex !== target);
  const deferredPageIndexes: number[] = [];
  for (let pageIndex = 0; pageIndex < totalPages; pageIndex += 1) {
    if (pageIndex === target || adjacentPageIndexes.includes(pageIndex)) {
      continue;
    }
    deferredPageIndexes.push(pageIndex);
  }
  return {
    currentPageIndex: target,
    immediatePageIndexes,
    adjacentPageIndexes,
    deferredPageIndexes
  };
}
