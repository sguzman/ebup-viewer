import { describe, expect, it } from "vitest";

import {
  filterAndSortCalibreBooks,
  type CalibreSort
} from "../src/components/calibreList";
import { computeVirtualWindow } from "../src/components/starterVirtualList";
import type { CalibreBook } from "../src/types";

function makeBook(id: number, title: string, authors: string, year: number): CalibreBook {
  return {
    id,
    title,
    extension: "epub",
    authors,
    year,
    file_size_bytes: null,
    source_path: null,
    cover_thumbnail: null
  };
}

function makeLargeCalibreFixture(count: number): CalibreBook[] {
  return Array.from({ length: count }, (_, index) => {
    const id = index + 1;
    return makeBook(
      id,
      `Book ${String(id).padStart(6, "0")}`,
      `Author ${String((id % 997) + 1).padStart(4, "0")}`,
      1900 + (id % 120)
    );
  });
}

describe("calibre list helpers", () => {
  it("filters and sorts by requested strategy", () => {
    const books = [
      makeBook(1, "Gamma", "Author B", 1999),
      makeBook(2, "Alpha", "Author C", 2001),
      makeBook(3, "Beta", "Author A", 1990)
    ];

    const byTitleAsc = filterAndSortCalibreBooks(books, "", "title_asc");
    const byAuthorDesc = filterAndSortCalibreBooks(books, "", "author_desc");
    const byYearDesc = filterAndSortCalibreBooks(books, "", "year_desc");
    const filtered = filterAndSortCalibreBooks(books, "alp", "title_asc");

    expect(byTitleAsc.map((book) => book.title)).toEqual(["Alpha", "Beta", "Gamma"]);
    expect(byAuthorDesc.map((book) => book.authors)).toEqual([
      "Author C",
      "Author B",
      "Author A"
    ]);
    expect(byYearDesc.map((book) => book.year)).toEqual([2001, 1999, 1990]);
    expect(filtered.map((book) => book.title)).toEqual(["Alpha"]);
  });

  it("keeps virtualized window bounded for very large calibre catalogs", () => {
    const books = makeLargeCalibreFixture(100_000);
    const sorted = filterAndSortCalibreBooks(books, "", "title_asc");

    const rowHeight = 58;
    const viewportHeight = 384;
    const overscan = 10;
    const scrollTop = 58 * 12_345;
    const windowed = computeVirtualWindow(sorted, scrollTop, rowHeight, viewportHeight, overscan);
    const maxVisible = Math.ceil(viewportHeight / rowHeight) + overscan * 2;

    expect(windowed.totalCount).toBe(100_000);
    expect(windowed.items.length).toBeLessThanOrEqual(maxVisible);
    expect(windowed.startIndex).toBeGreaterThan(0);
    expect(windowed.endIndex).toBeLessThanOrEqual(100_000);
    expect(windowed.topSpacerPx + windowed.bottomSpacerPx + windowed.items.length * rowHeight).toBe(
      100_000 * rowHeight
    );
  });

  it("handles invalid virtualization geometry defensively", () => {
    const books = [makeBook(1, "A", "AA", 2000), makeBook(2, "B", "BB", 2001)];
    const sorts: CalibreSort[] = ["title_asc", "id_desc"];
    const filtered = filterAndSortCalibreBooks(books, "", sorts[0]);
    const windowed = computeVirtualWindow(filtered, -200, 0, 0, -4);

    expect(windowed.items.length).toBeGreaterThan(0);
    expect(windowed.topSpacerPx).toBe(0);
    expect(windowed.bottomSpacerPx).toBeGreaterThanOrEqual(0);
  });
});
