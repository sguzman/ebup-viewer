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
const TABLE_ROW_ALIGNMENT_TOLERANCE = 2;
const FOOTNOTE_BAND_START = 0.78;
const FULL_WIDTH_BAND_GAP_FRACTION = 0.3;
const FULL_WIDTH_BAND_WIDTH_FRACTION = 0.6;

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

interface TwoColumnLayout {
  leftColumn: MeasuredPdfSpan[];
  rightColumn: MeasuredPdfSpan[];
  bands: MeasuredPdfSpan[];
}

function hasStrongRowAlignment(leftColumn: MeasuredPdfSpan[], rightColumn: MeasuredPdfSpan[]): boolean {
  let alignedRows = 0;
  for (const left of leftColumn) {
    if (rightColumn.some((right) => Math.abs(right.top - left.top) <= TABLE_ROW_ALIGNMENT_TOLERANCE)) {
      alignedRows += 1;
    }
  }
  const comparableRows = Math.min(leftColumn.length, rightColumn.length);
  return comparableRows >= 3 && alignedRows / comparableRows >= 0.6;
}

function tryBuildTwoColumnLayout(spans: MeasuredPdfSpan[]): TwoColumnLayout | null {
  const columns = bucketByColumn(spans);
  if (columns.length < 2) {
    return null;
  }
  const leftAnchor = columns[0]?.[0]?.left;
  const rightAnchor = columns.at(-1)?.[0]?.left;
  if (leftAnchor === undefined || rightAnchor === undefined) {
    return null;
  }
  const gap = rightAnchor - leftAnchor;
  if (gap < MIN_COLUMN_GAP) {
    return null;
  }

  const leftColumn: MeasuredPdfSpan[] = [];
  const rightColumn: MeasuredPdfSpan[] = [];
  const bands: MeasuredPdfSpan[] = [];

  for (const span of spans) {
    const spansGap = span.left > leftAnchor + gap * FULL_WIDTH_BAND_GAP_FRACTION
      && span.left < rightAnchor - gap * FULL_WIDTH_BAND_GAP_FRACTION;
    const spansWidth = span.width >= gap * FULL_WIDTH_BAND_WIDTH_FRACTION;
    if (spansGap || spansWidth) {
      bands.push(span);
      continue;
    }
    if (Math.abs(span.left - leftAnchor) <= Math.abs(span.left - rightAnchor)) {
      leftColumn.push(span);
    } else {
      rightColumn.push(span);
    }
  }

  if (leftColumn.length < MIN_COLUMN_SPAN_COUNT || rightColumn.length < MIN_COLUMN_SPAN_COUNT) {
    return null;
  }
  if (hasStrongRowAlignment(leftColumn, rightColumn)) {
    return null;
  }

  return {
    leftColumn,
    rightColumn,
    bands: sortSingleColumn(bands)
  };
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

function orderTwoColumnLayout(layout: TwoColumnLayout): MeasuredPdfSpan[] {
  const orderedLeft = sortSingleColumn(layout.leftColumn);
  const orderedRight = sortSingleColumn(layout.rightColumn);
  if (layout.bands.length === 0) {
    const leftSplit = splitBottomNoteBand(orderedLeft);
    const rightSplit = splitBottomNoteBand(orderedRight);
    return [
      ...sortSingleColumn(leftSplit.main),
      ...sortSingleColumn(leftSplit.bottomBand),
      ...sortSingleColumn(rightSplit.main),
      ...sortSingleColumn(rightSplit.bottomBand)
    ];
  }

  const ordered: MeasuredPdfSpan[] = [];
  let previousBandTop = Number.NEGATIVE_INFINITY;

  for (const band of layout.bands) {
    const leftSegment = orderedLeft.filter((span) => span.top < band.top && span.top >= previousBandTop);
    const rightSegment = orderedRight.filter((span) => span.top < band.top && span.top >= previousBandTop);
    const leftSplit = splitBottomNoteBand(leftSegment);
    const rightSplit = splitBottomNoteBand(rightSegment);
    ordered.push(
      ...sortSingleColumn(leftSplit.main),
      ...sortSingleColumn(leftSplit.bottomBand),
      ...sortSingleColumn(rightSplit.main),
      ...sortSingleColumn(rightSplit.bottomBand),
      band
    );
    previousBandTop = band.top;
  }

  const remainingLeft = orderedLeft.filter((span) => span.top >= previousBandTop);
  const remainingRight = orderedRight.filter((span) => span.top >= previousBandTop);
  const leftSplit = splitBottomNoteBand(remainingLeft);
  const rightSplit = splitBottomNoteBand(remainingRight);
  ordered.push(
    ...sortSingleColumn(leftSplit.main),
    ...sortSingleColumn(leftSplit.bottomBand),
    ...sortSingleColumn(rightSplit.main),
    ...sortSingleColumn(rightSplit.bottomBand)
  );

  return ordered;
}

function sortSingleColumn(spans: MeasuredPdfSpan[]): MeasuredPdfSpan[] {
  const sorted = [...spans].sort((left, right) => {
    if (left.top !== right.top) {
      return left.top - right.top;
    }
    if (left.left !== right.left) {
      return left.left - right.left;
    }
    return left.index - right.index;
  });
  const rows: MeasuredPdfSpan[][] = [];
  for (const span of sorted) {
    const current = rows.at(-1);
    if (!current) {
      rows.push([span]);
      continue;
    }
    const rowTop = current[0]?.top ?? span.top;
    if (Math.abs(span.top - rowTop) <= ROW_TOLERANCE) {
      current.push(span);
      continue;
    }
    rows.push([span]);
  }
  return rows.flatMap((row) =>
    [...row].sort((left, right) => {
      if (left.left !== right.left) {
        return left.left - right.left;
      }
      if (left.top !== right.top) {
        return left.top - right.top;
      }
      return left.index - right.index;
    })
  );
}

export function orderPdfTextLayerSpans(
  elements: HTMLElement[],
  pageIndex: number,
  rotationDegrees = 0
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

  const normalizedRotation = ((rotationDegrees % 360) + 360) % 360;
  const normalizedMeasured =
    normalizedRotation === 90 || normalizedRotation === 270
      ? measured.map((span) => ({
        ...span,
        top: span.left,
        left: span.top
      }))
      : measured;

  const twoColumnLayout = tryBuildTwoColumnLayout(normalizedMeasured);
  const ordered = twoColumnLayout
    ? orderTwoColumnLayout(twoColumnLayout)
    : sortSingleColumn(normalizedMeasured);

  return ordered.map((span) => ({
    pageIndex: span.pageIndex,
    text: span.text,
    element: span.element
  }));
}
