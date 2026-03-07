import { useEffect } from "react";

const counters = new Map<string, number>();
const measures = new Map<string, number[]>();
const selectorCounters = new Map<string, number>();
let flushHandle: number | null = null;

function scheduleFlush(): void {
  if (!import.meta.env.DEV || typeof window === "undefined" || flushHandle !== null) {
    return;
  }
  flushHandle = window.setTimeout(() => {
    flushHandle = null;
    if (counters.size === 0 && measures.size === 0) {
      return;
    }
    const renderSummary = Object.fromEntries(counters.entries());
    const selectorSummary = Object.fromEntries(selectorCounters.entries());
    const measureSummary = Object.fromEntries(
      Array.from(measures.entries()).map(([name, values]) => [
        name,
        {
          count: values.length,
          avgMs:
            values.length > 0
              ? Number((values.reduce((sum, value) => sum + value, 0) / values.length).toFixed(2))
              : 0
        }
      ])
    );
    console.debug("ui perf summary", {
      renders: renderSummary,
      selectors: selectorSummary,
      measures: measureSummary
    });
    counters.clear();
    selectorCounters.clear();
    measures.clear();
  }, 5000);
}

export function useRenderDebugCounter(name: string): void {
  useEffect(() => {
    if (!import.meta.env.DEV) {
      return;
    }
    counters.set(name, (counters.get(name) ?? 0) + 1);
    scheduleFlush();
  });
}

export function recordPerfMeasure(name: string, startedAt: number): void {
  if (!import.meta.env.DEV || typeof performance === "undefined") {
    return;
  }
  const duration = performance.now() - startedAt;
  const values = measures.get(name) ?? [];
  values.push(duration);
  measures.set(name, values);
  scheduleFlush();
}

export function recordSelectorInvalidation(name: string): void {
  if (!import.meta.env.DEV) {
    return;
  }
  selectorCounters.set(name, (selectorCounters.get(name) ?? 0) + 1);
  scheduleFlush();
}
