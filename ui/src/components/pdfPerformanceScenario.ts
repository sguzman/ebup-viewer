import { buildPdfOpenPlan } from "./pdfDocumentModel";
import { resolvePdfPerformanceProfile, type PdfPerformanceProfile, type PdfPerformanceProfileName } from "./pdfPerformanceProfile";
import { buildPdfViewportRenderPlan } from "./pdfViewportScheduler";

export interface PdfPerformanceScenario {
  activeTtsPageIndex: number | null;
  currentPageIndex: number;
  jumpTargetPageIndex: number | null;
  name: string;
  profile: PdfPerformanceProfileName;
  totalPages: number;
  visiblePageIndexes: number[];
}

export interface PdfPerformanceScenarioResult {
  canvasPages: number;
  deferredPages: number;
  highPriorityPages: number;
  lowPriorityPages: number;
  mediumPriorityPages: number;
  name: string;
  openImmediatePages: number;
  profile: PdfPerformanceProfile;
  textLayerPages: number;
  totalPages: number;
  visiblePages: number;
}

export const PDF_PERFORMANCE_SCENARIOS: PdfPerformanceScenario[] = [
  {
    activeTtsPageIndex: null,
    currentPageIndex: 0,
    jumpTargetPageIndex: null,
    name: "small_text_pdf",
    profile: "balanced",
    totalPages: 12,
    visiblePageIndexes: [0]
  },
  {
    activeTtsPageIndex: 88,
    currentPageIndex: 84,
    jumpTargetPageIndex: null,
    name: "large_academic_pdf",
    profile: "balanced",
    totalPages: 240,
    visiblePageIndexes: [83, 84, 85]
  },
  {
    activeTtsPageIndex: null,
    currentPageIndex: 12,
    jumpTargetPageIndex: 14,
    name: "image_heavy_pdf",
    profile: "low_memory",
    totalPages: 80,
    visiblePageIndexes: [12, 13]
  },
  {
    activeTtsPageIndex: 31,
    currentPageIndex: 30,
    jumpTargetPageIndex: null,
    name: "two_column_pdf",
    profile: "balanced",
    totalPages: 120,
    visiblePageIndexes: [29, 30, 31]
  },
  {
    activeTtsPageIndex: 175,
    currentPageIndex: 174,
    jumpTargetPageIndex: 176,
    name: "tts_playback_long_pdf",
    profile: "high_memory",
    totalPages: 320,
    visiblePageIndexes: [173, 174, 175]
  }
];

export function evaluatePdfPerformanceScenario(
  scenario: PdfPerformanceScenario
): PdfPerformanceScenarioResult {
  const profile = resolvePdfPerformanceProfile({ override: scenario.profile });
  const openPlan = buildPdfOpenPlan(scenario.totalPages, scenario.currentPageIndex);
  const renderPlan = buildPdfViewportRenderPlan({
    activeTtsPageIndex: scenario.activeTtsPageIndex,
    jumpTargetPageIndex: scenario.jumpTargetPageIndex,
    overscan: profile.overscan,
    totalPages: scenario.totalPages,
    visiblePageIndexes: scenario.visiblePageIndexes
  });
  return {
    canvasPages: renderPlan.canvasPageIndexes.length,
    deferredPages: openPlan.deferredPageIndexes.length,
    highPriorityPages: renderPlan.priorityPageIndexes.length,
    lowPriorityPages: renderPlan.lowPriorityPageIndexes.length,
    mediumPriorityPages: renderPlan.mediumPriorityPageIndexes.length,
    name: scenario.name,
    openImmediatePages: openPlan.immediatePageIndexes.length,
    profile,
    textLayerPages: renderPlan.textLayerPageIndexes.length,
    totalPages: scenario.totalPages,
    visiblePages: scenario.visiblePageIndexes.length
  };
}
