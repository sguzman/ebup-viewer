import { useEffect, useRef, useState } from "react";

import { backendApi } from "../api/tauri";
import { recordPerfMeasure } from "../perf/debug";
import type { CalibreBook } from "../types";

export function useCalibreThumbnails(visibleBooks: CalibreBook[]) {
  const [calibreThumbOverrides, setCalibreThumbOverrides] = useState<Record<number, string>>({});
  const calibreThumbInFlightRef = useRef<Set<number>>(new Set());
  const calibreThumbFailedRef = useRef<Set<number>>(new Set());

  useEffect(() => {
    let cancelled = false;
    const candidates = visibleBooks.filter((book) => {
      if (book.cover_thumbnail || calibreThumbOverrides[book.id]) {
        return false;
      }
      if (calibreThumbInFlightRef.current.has(book.id) || calibreThumbFailedRef.current.has(book.id)) {
        return false;
      }
      return true;
    });
    if (candidates.length === 0) {
      return () => {
        cancelled = true;
      };
    }

    const run = async (): Promise<void> => {
      const startedAt = typeof performance !== "undefined" ? performance.now() : 0;
      const pending: Array<[number, string]> = [];
      for (const book of candidates.slice(0, 18)) {
        calibreThumbInFlightRef.current.add(book.id);
        try {
          const thumbnail = await backendApi.calibreEnsureThumbnail(book.id);
          if (!thumbnail) {
            calibreThumbFailedRef.current.add(book.id);
            continue;
          }
          if (cancelled) {
            continue;
          }
          pending.push([book.id, thumbnail]);
        } catch {
          calibreThumbFailedRef.current.add(book.id);
        } finally {
          calibreThumbInFlightRef.current.delete(book.id);
        }
      }
      if (cancelled || pending.length === 0) {
        return;
      }
      setCalibreThumbOverrides((current) => {
        let changed = false;
        const next = { ...current };
        for (const [bookId, thumbnail] of pending) {
          if (next[bookId] === thumbnail) {
            continue;
          }
          next[bookId] = thumbnail;
          changed = true;
        }
        return changed ? next : current;
      });
      recordPerfMeasure("StarterShell.thumbnailHydrationBatch", startedAt);
    };

    void run();
    return () => {
      cancelled = true;
    };
  }, [calibreThumbOverrides, visibleBooks]);

  return calibreThumbOverrides;
}
