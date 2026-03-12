export type PdfPerformanceProfileName = "low_memory" | "balanced" | "high_memory";

export interface PdfPerformanceProfile {
  bitmapArtifacts: number;
  highlightOverlays: number;
  lowPriorityPrefetchDelayMs: number;
  maxLiveCanvases: number;
  maxLiveTextLayers: number;
  maxLiveTextSpans: number;
  overscan: number;
  sentenceTargets: number;
  spanArtifacts: number;
  textLayerSelectionRadius: number;
  title: PdfPerformanceProfileName;
}

export interface PdfPerformanceProfileEnv {
  deviceMemory?: number | null;
  hardwareConcurrency?: number | null;
  override?: string | null;
}

const PDF_PERFORMANCE_PROFILES: Record<PdfPerformanceProfileName, PdfPerformanceProfile> = {
  low_memory: {
    bitmapArtifacts: 4,
    highlightOverlays: 20,
    lowPriorityPrefetchDelayMs: 64,
    maxLiveCanvases: 4,
    maxLiveTextLayers: 2,
    maxLiveTextSpans: 900,
    overscan: 0,
    sentenceTargets: 128,
    spanArtifacts: 8,
    textLayerSelectionRadius: 0,
    title: "low_memory"
  },
  balanced: {
    bitmapArtifacts: 10,
    highlightOverlays: 48,
    lowPriorityPrefetchDelayMs: 32,
    maxLiveCanvases: 8,
    maxLiveTextLayers: 4,
    maxLiveTextSpans: 2200,
    overscan: 1,
    sentenceTargets: 256,
    spanArtifacts: 12,
    textLayerSelectionRadius: 1,
    title: "balanced"
  },
  high_memory: {
    bitmapArtifacts: 16,
    highlightOverlays: 72,
    lowPriorityPrefetchDelayMs: 16,
    maxLiveCanvases: 12,
    maxLiveTextLayers: 6,
    maxLiveTextSpans: 4200,
    overscan: 2,
    sentenceTargets: 512,
    spanArtifacts: 20,
    textLayerSelectionRadius: 2,
    title: "high_memory"
  }
};

function normalizeProfileOverride(value: string | null | undefined): PdfPerformanceProfileName | null {
  switch ((value ?? "").trim()) {
    case "low":
    case "low_memory":
      return "low_memory";
    case "balanced":
    case "default":
      return "balanced";
    case "high":
    case "high_memory":
      return "high_memory";
    default:
      return null;
  }
}

export function resolvePdfPerformanceProfile(
  env: PdfPerformanceProfileEnv = {}
): PdfPerformanceProfile {
  const override = normalizeProfileOverride(env.override);
  if (override) {
    return PDF_PERFORMANCE_PROFILES[override];
  }
  const deviceMemory = env.deviceMemory ?? null;
  const hardwareConcurrency = env.hardwareConcurrency ?? null;
  if ((deviceMemory !== null && deviceMemory <= 4) || (hardwareConcurrency !== null && hardwareConcurrency <= 4)) {
    return PDF_PERFORMANCE_PROFILES.low_memory;
  }
  if ((deviceMemory !== null && deviceMemory >= 16) && (hardwareConcurrency !== null && hardwareConcurrency >= 8)) {
    return PDF_PERFORMANCE_PROFILES.high_memory;
  }
  return PDF_PERFORMANCE_PROFILES.balanced;
}

export function readPdfPerformanceProfileOverride(storage: Pick<Storage, "getItem"> | null | undefined): string | null {
  if (!storage) {
    return null;
  }
  return storage.getItem("ll.pdfPerformanceProfile");
}
