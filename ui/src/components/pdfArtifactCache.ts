import type { PdfSentenceMatch } from "./pdfTextSync";
import type { PdfOverlayRect } from "./pdfOverlayGeometry";

export interface PdfSentenceTargetArtifact {
  sentenceIndex: number;
  pageIndex: number | null;
  match: PdfSentenceMatch;
  overlayRects: PdfOverlayRect[];
  useOverlay: boolean;
}

export interface PdfSpanArtifact<TSpan> {
  pageIndex: number;
  zoom: number;
  spans: TSpan[];
}

export interface LruCache<K, V> {
  clear(): void;
  delete(key: K): boolean;
  entries(): Array<[K, V]>;
  get(key: K): V | undefined;
  has(key: K): boolean;
  set(key: K, value: V): K | undefined;
  size(): number;
}

export function createLruCache<K, V>(capacity: number): LruCache<K, V> {
  const store = new Map<K, V>();

  return {
    clear() {
      store.clear();
    },
    delete(key) {
      return store.delete(key);
    },
    entries() {
      return Array.from(store.entries());
    },
    get(key) {
      const value = store.get(key);
      if (value === undefined) {
        return undefined;
      }
      store.delete(key);
      store.set(key, value);
      return value;
    },
    has(key) {
      return store.has(key);
    },
    set(key, value) {
      if (store.has(key)) {
        store.delete(key);
      }
      store.set(key, value);
      let evicted: K | undefined;
      while (store.size > capacity) {
        const oldest = store.keys().next().value as K | undefined;
        if (oldest === undefined) {
          break;
        }
        store.delete(oldest);
        evicted = oldest;
      }
      return evicted;
    },
    size() {
      return store.size;
    }
  };
}

export function pdfSpanArtifactKey(pageIndex: number, zoom: number): string {
  return `${pageIndex}:${zoom.toFixed(3)}`;
}
