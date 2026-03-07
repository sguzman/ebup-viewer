export interface VirtualWindow<T> {
  items: T[];
  topSpacerPx: number;
  bottomSpacerPx: number;
  totalCount: number;
  startIndex: number;
  endIndex: number;
}

export function computeVirtualWindow<T>(
  items: T[],
  scrollTop: number,
  rowHeight: number,
  viewportHeight: number,
  overscan: number
): VirtualWindow<T> {
  const totalCount = items.length;
  const safeRowHeight = Math.max(1, Math.floor(rowHeight));
  const safeViewportHeight = Math.max(1, Math.floor(viewportHeight));
  const safeOverscan = Math.max(0, Math.floor(overscan));

  if (totalCount === 0) {
    return {
      items: [],
      topSpacerPx: 0,
      bottomSpacerPx: 0,
      totalCount: 0,
      startIndex: 0,
      endIndex: 0
    };
  }

  const start = Math.max(0, Math.floor(scrollTop / safeRowHeight) - safeOverscan);
  const maxVisible = Math.ceil(safeViewportHeight / safeRowHeight) + safeOverscan * 2;
  const end = Math.min(totalCount, start + maxVisible);

  return {
    items: items.slice(start, end),
    topSpacerPx: start * safeRowHeight,
    bottomSpacerPx: Math.max(0, (totalCount - end) * safeRowHeight),
    totalCount,
    startIndex: start,
    endIndex: end
  };
}
