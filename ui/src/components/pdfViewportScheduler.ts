export type PdfPageLifecycleState =
  | "placeholder"
  | "scheduled"
  | "rendering_canvas"
  | "canvas_ready"
  | "text_ready"
  | "evicted";

export interface PdfPageRegistryEntry {
  pageIndex: number;
  state: PdfPageLifecycleState;
  lastTouchedAt: number;
  renderedZoom: number | null;
  textLayerZoom: number | null;
}

export interface PdfViewportRenderPlan {
  canvasPageIndexes: number[];
  textLayerPageIndexes: number[];
  priorityPageIndexes: number[];
}

export interface PdfViewportPlanInput {
  totalPages: number;
  visiblePageIndexes: number[];
  overscan: number;
  activeTtsPageIndex: number | null;
  jumpTargetPageIndex: number | null;
}

export interface PdfViewportBudgetInput {
  entries: PdfPageRegistryEntry[];
  keepCanvasPageIndexes: number[];
  keepTextLayerPageIndexes: number[];
  maxCanvasPages: number;
  maxTextLayerPages: number;
}

export interface PdfViewportBudgetDecision {
  evictCanvasPageIndexes: number[];
  evictTextLayerPageIndexes: number[];
}

function clampPage(pageIndex: number, totalPages: number): number {
  return Math.max(0, Math.min(totalPages - 1, pageIndex));
}

function sortUnique(values: number[]): number[] {
  return Array.from(new Set(values)).sort((left, right) => left - right);
}

function pageWindow(pageIndex: number, totalPages: number, radius: number): number[] {
  const out: number[] = [];
  for (let delta = -radius; delta <= radius; delta += 1) {
    out.push(clampPage(pageIndex + delta, totalPages));
  }
  return sortUnique(out);
}

export function buildPdfViewportRenderPlan(input: PdfViewportPlanInput): PdfViewportRenderPlan {
  const { totalPages, visiblePageIndexes, overscan, activeTtsPageIndex, jumpTargetPageIndex } = input;
  if (totalPages <= 0) {
    return {
      canvasPageIndexes: [],
      textLayerPageIndexes: [],
      priorityPageIndexes: []
    };
  }

  const visible = sortUnique(
    visiblePageIndexes
      .filter((pageIndex) => Number.isFinite(pageIndex))
      .map((pageIndex) => clampPage(pageIndex, totalPages))
  );
  const priority: number[] = [];
  if (jumpTargetPageIndex !== null && jumpTargetPageIndex !== undefined) {
    priority.push(clampPage(jumpTargetPageIndex, totalPages));
  }
  if (activeTtsPageIndex !== null && activeTtsPageIndex !== undefined) {
    priority.push(clampPage(activeTtsPageIndex, totalPages));
  }
  if (visible.length > 0) {
    priority.push(visible[Math.floor(visible.length / 2)] ?? visible[0]!);
  }
  const priorityPageIndexes = sortUnique(priority);

  const canvasPageIndexes = sortUnique([
    ...visible.flatMap((pageIndex) => pageWindow(pageIndex, totalPages, overscan)),
    ...priorityPageIndexes
  ]);

  const textLayerPageIndexes = sortUnique([
    ...visible,
    ...(activeTtsPageIndex !== null && activeTtsPageIndex !== undefined
      ? [clampPage(activeTtsPageIndex, totalPages)]
      : []),
    ...(jumpTargetPageIndex !== null && jumpTargetPageIndex !== undefined
      ? [clampPage(jumpTargetPageIndex, totalPages)]
      : [])
  ]);

  return {
    canvasPageIndexes,
    textLayerPageIndexes,
    priorityPageIndexes
  };
}

function evictionPriority(
  entry: PdfPageRegistryEntry,
  keepPageIndexes: Set<number>,
  mode: "canvas" | "text"
): number {
  if (keepPageIndexes.has(entry.pageIndex)) {
    return Number.NEGATIVE_INFINITY;
  }
  const hasArtifact = mode === "canvas" ? entry.renderedZoom !== null : entry.textLayerZoom !== null;
  if (!hasArtifact) {
    return Number.NEGATIVE_INFINITY;
  }
  return entry.lastTouchedAt;
}

export function choosePdfViewportEvictions(input: PdfViewportBudgetInput): PdfViewportBudgetDecision {
  const keepCanvasPageIndexes = new Set(input.keepCanvasPageIndexes);
  const keepTextLayerPageIndexes = new Set(input.keepTextLayerPageIndexes);

  const liveCanvasEntries = input.entries.filter((entry) => entry.renderedZoom !== null);
  const liveTextEntries = input.entries.filter((entry) => entry.textLayerZoom !== null);

  const canvasOverflow = Math.max(0, liveCanvasEntries.length - input.maxCanvasPages);
  const textOverflow = Math.max(0, liveTextEntries.length - input.maxTextLayerPages);

  const evictCanvasPageIndexes = [...liveCanvasEntries]
    .sort((left, right) => evictionPriority(left, keepCanvasPageIndexes, "canvas") - evictionPriority(right, keepCanvasPageIndexes, "canvas"))
    .filter((entry) => !keepCanvasPageIndexes.has(entry.pageIndex))
    .slice(0, canvasOverflow)
    .map((entry) => entry.pageIndex);

  const evictTextLayerPageIndexes = [...liveTextEntries]
    .sort((left, right) => evictionPriority(left, keepTextLayerPageIndexes, "text") - evictionPriority(right, keepTextLayerPageIndexes, "text"))
    .filter((entry) => !keepTextLayerPageIndexes.has(entry.pageIndex))
    .slice(0, textOverflow)
    .map((entry) => entry.pageIndex);

  return {
    evictCanvasPageIndexes: sortUnique(evictCanvasPageIndexes),
    evictTextLayerPageIndexes: sortUnique(evictTextLayerPageIndexes)
  };
}

export function computePdfPreviewScale(displayZoom: number, renderedZoom: number | null): number {
  if (!Number.isFinite(displayZoom) || displayZoom <= 0) {
    return 1;
  }
  if (!Number.isFinite(renderedZoom ?? NaN) || !renderedZoom || renderedZoom <= 0) {
    return 1;
  }
  return displayZoom / renderedZoom;
}
