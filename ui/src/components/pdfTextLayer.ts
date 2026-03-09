import type { PdfTextSpan } from "./pdfTextSync";

interface MeasuredPdfSpan {
  element: HTMLElement;
  text: string;
  pageIndex: number;
  index: number;
  top: number;
  left: number;
  width: number;
  height: number;
}

const MIN_COLUMN_SPAN_COUNT = 3;
const MIN_COLUMN_GAP = 72;
const ROW_TOLERANCE = 8;
const FOOTNOTE_BAND_START = 0.78;

function measurePdfSpan(
  element: HTMLElement,
  text: string,
  pageIndex: number,
  index: number
): MeasuredPdfSpan {
  const rect = element.getBoundingClientRect();
  return {
    element,
    text,
    pageIndex,
    index,
    top: rect.top,
    left: rect.left,
    width: rect.width,
    height: rect.height
  };
}

function bucketByColumn(spans: MeasuredPdfSpan[]): MeasuredPdfSpan[][] {
  const sorted = [...spans].sort((left, right) => {
    if (left.left !== right.left) {
      return left.left - right.left;
    }
    return left.top - right.top;
  });
  const columns: MeasuredPdfSpan[][] = [];
  for (const span of sorted) {
    const current = columns.at(-1);
    if (!current) {
      columns.push([span]);
      continue;
    }
    const anchorLeft = current[0]?.left ?? span.left;
    if (Math.abs(span.left - anchorLeft) <= MIN_COLUMN_GAP) {
      current.push(span);
      continue;
    }
    columns.push([span]);
  }
  return columns;
}

function hasStrongTwoColumnLayout(columns: MeasuredPdfSpan[][]): boolean {
  if (columns.length !== 2) {
    return false;
  }
  const [left, right] = columns;
  if (!left || !right) {
    return false;
  }
  if (left.length < MIN_COLUMN_SPAN_COUNT || right.length < MIN_COLUMN_SPAN_COUNT) {
    return false;
  }
  const gap = (right[0]?.left ?? 0) - (left.at(-1)?.left ?? 0);
  return gap >= MIN_COLUMN_GAP;
}

function splitBottomNoteBand(spans: MeasuredPdfSpan[]): {
  main: MeasuredPdfSpan[];
  bottomBand: MeasuredPdfSpan[];
} {
  if (spans.length === 0) {
    return { main: [], bottomBand: [] };
  }
  const tops = spans.map((span) => span.top);
  const minTop = Math.min(...tops);
  const maxTop = Math.max(...tops);
  const heightRange = maxTop - minTop;
  if (heightRange <= 0) {
    return { main: spans, bottomBand: [] };
  }
  const threshold = minTop + heightRange * FOOTNOTE_BAND_START;
  const bottomBand = spans.filter((span) => span.top >= threshold);
  const main = spans.filter((span) => span.top < threshold);
  if (bottomBand.length < 2 || main.length < 2) {
    return { main: spans, bottomBand: [] };
  }
  return { main, bottomBand };
}

function sortSingleColumn(spans: MeasuredPdfSpan[]): MeasuredPdfSpan[] {
  return [...spans].sort((left, right) => {
    const topDelta = left.top - right.top;
    if (Math.abs(topDelta) > ROW_TOLERANCE) {
      return topDelta;
    }
    if (left.left !== right.left) {
      return left.left - right.left;
    }
    return left.index - right.index;
  });
}

export function orderPdfTextLayerSpans(
  elements: HTMLElement[],
  pageIndex: number
): PdfTextSpan[] {
  const measured = elements
    .map((element, index) => ({
      element,
      text: element.textContent ?? "",
      pageIndex,
      index
    }))
    .filter((span) => span.text.trim().length > 0)
    .map((span) => measurePdfSpan(span.element, span.text, span.pageIndex, span.index));

  const columns = bucketByColumn(measured);
  const ordered = hasStrongTwoColumnLayout(columns)
    ? columns.flatMap((column) => {
      const { main, bottomBand } = splitBottomNoteBand(column);
      return [...sortSingleColumn(main), ...sortSingleColumn(bottomBand)];
    })
    : sortSingleColumn(measured);

  return ordered.map((span) => ({
    pageIndex: span.pageIndex,
    text: span.text,
    element: span.element
  }));
}
