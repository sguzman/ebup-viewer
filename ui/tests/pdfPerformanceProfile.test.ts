import { describe, expect, it } from "vitest";

import {
  readPdfPerformanceProfileOverride,
  resolvePdfPerformanceProfile
} from "../src/components/pdfPerformanceProfile";

describe("pdfPerformanceProfile", () => {
  it("chooses low-memory budgets on constrained devices", () => {
    const profile = resolvePdfPerformanceProfile({
      deviceMemory: 4,
      hardwareConcurrency: 4
    });

    expect(profile.title).toBe("low_memory");
    expect(profile.maxLiveCanvases).toBe(4);
    expect(profile.overscan).toBe(0);
  });

  it("chooses high-memory budgets on larger devices", () => {
    const profile = resolvePdfPerformanceProfile({
      deviceMemory: 16,
      hardwareConcurrency: 8
    });

    expect(profile.title).toBe("high_memory");
    expect(profile.maxLiveCanvases).toBe(12);
    expect(profile.bitmapArtifacts).toBe(16);
  });

  it("honors explicit local override values", () => {
    const profile = resolvePdfPerformanceProfile({
      deviceMemory: 2,
      hardwareConcurrency: 2,
      override: "high_memory"
    });

    expect(profile.title).toBe("high_memory");
  });

  it("reads local storage overrides safely", () => {
    expect(readPdfPerformanceProfileOverride({
      getItem(key: string) {
        return key === "ll.pdfPerformanceProfile" ? "low_memory" : null;
      }
    })).toBe("low_memory");
    expect(readPdfPerformanceProfileOverride(null)).toBeNull();
  });
});
