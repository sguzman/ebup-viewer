import { describe, expect, it } from "vitest";

import { createLruCache, pdfBitmapArtifactKey, pdfSpanArtifactKey } from "../src/components/pdfArtifactCache";

describe("pdfArtifactCache", () => {
  it("evicts least-recently-used entries when capacity is exceeded", () => {
    const cache = createLruCache<string, number>(2);

    cache.set("a", 1);
    cache.set("b", 2);
    expect(cache.get("a")).toBe(1);
    cache.set("c", 3);

    expect(cache.has("a")).toBe(true);
    expect(cache.has("b")).toBe(false);
    expect(cache.has("c")).toBe(true);
  });

  it("builds stable page span artifact keys by page and zoom bucket", () => {
    expect(pdfSpanArtifactKey(4, 1.23456)).toBe("4:1.235");
    expect(pdfSpanArtifactKey(4, 1.23411)).toBe("4:1.234");
    expect(pdfBitmapArtifactKey(4, 1.23456)).toBe("bitmap:4:1.235");
  });
});
