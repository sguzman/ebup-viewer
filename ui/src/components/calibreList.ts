import type { CalibreBook } from "../types";

export type CalibreSort =
  | "title_asc"
  | "title_desc"
  | "author_asc"
  | "author_desc"
  | "year_desc"
  | "year_asc"
  | "id_asc"
  | "id_desc";

export function filterAndSortCalibreBooks(
  calibreBooks: CalibreBook[],
  query: string,
  sort: CalibreSort
): CalibreBook[] {
  const normalized = query.trim().toLowerCase();
  const filtered = calibreBooks.filter((book) => {
    if (!normalized) {
      return true;
    }
    return (
      book.title.toLowerCase().includes(normalized) ||
      book.authors.toLowerCase().includes(normalized) ||
      book.extension.toLowerCase().includes(normalized)
    );
  });

  const sorted = [...filtered];
  sorted.sort((left, right) => {
    switch (sort) {
      case "title_desc":
        return right.title.localeCompare(left.title);
      case "author_asc":
        return left.authors.localeCompare(right.authors);
      case "author_desc":
        return right.authors.localeCompare(left.authors);
      case "year_desc":
        return (right.year ?? 0) - (left.year ?? 0);
      case "year_asc":
        return (left.year ?? 0) - (right.year ?? 0);
      case "id_asc":
        return left.id - right.id;
      case "id_desc":
        return right.id - left.id;
      case "title_asc":
      default:
        return left.title.localeCompare(right.title);
    }
  });
  return sorted;
}
