import { useEffect } from "react";

const counters = new Map<string, number>();
const measures = new Map<string, number[]>();
const gauges = new Map<string, number>();
const selectorCounters = new Map<string, number>();
const eventCounters = new Map<string, { count: number; bytes: number }>();
let flushHandle: number | null = null;

function scheduleFlush(): void {
  if (!import.meta.env.DEV || typeof window === "undefined" || flushHandle !== null) {
    return;
  }
  flushHandle = window.setTimeout(() => {
    flushHandle = null;
    if (counters.size === 0 && measures.size === 0 && gauges.size === 0) {
      return;
    }
    const renderSummary = Object.fromEntries(counters.entries());
    const gaugeSummary = Object.fromEntries(gauges.entries());
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
    const eventSummary = Object.fromEntries(
      Array.from(eventCounters.entries()).map(([name, summary]) => [
        name,
        {
          count: summary.count,
          avgBytes: summary.count > 0 ? Math.round(summary.bytes / summary.count) : 0
        }
      ])
    );
    console.debug("ui perf summary", {
      renders: renderSummary,
      gauges: gaugeSummary,
      selectors: selectorSummary,
      events: eventSummary,
      measures: measureSummary
    });
    counters.clear();
    gauges.clear();
    selectorCounters.clear();
    eventCounters.clear();
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

export function recordPerfCounter(name: string, delta = 1): void {
  if (!import.meta.env.DEV) {
    return;
  }
  counters.set(name, (counters.get(name) ?? 0) + delta);
  scheduleFlush();
}

export function recordPerfGauge(name: string, value: number): void {
  if (!import.meta.env.DEV) {
    return;
  }
  gauges.set(name, value);
  scheduleFlush();
}

export function recordSelectorInvalidation(name: string): void {
  if (!import.meta.env.DEV) {
    return;
  }
  selectorCounters.set(name, (selectorCounters.get(name) ?? 0) + 1);
  scheduleFlush();
}

export function recordEventIngestion(name: string, payload: unknown): void {
  if (!import.meta.env.DEV) {
    return;
  }
  let bytes = 0;
  try {
    bytes = JSON.stringify(payload)?.length ?? 0;
  } catch {
    bytes = 0;
  }
  const current = eventCounters.get(name) ?? { count: 0, bytes: 0 };
  current.count += 1;
  current.bytes += bytes;
  eventCounters.set(name, current);
  scheduleFlush();
}

export interface PerfSnapshot {
  counters: Record<string, number>;
  eventCounters: Record<string, { avgBytes: number; count: number }>;
  gauges: Record<string, number>;
  measures: Record<string, { avgMs: number; count: number }>;
  selectorCounters: Record<string, number>;
}

export function getPerfSnapshot(): PerfSnapshot {
  return {
    counters: Object.fromEntries(counters.entries()),
    eventCounters: Object.fromEntries(
      Array.from(eventCounters.entries()).map(([name, summary]) => [
        name,
        {
          avgBytes: summary.count > 0 ? Math.round(summary.bytes / summary.count) : 0,
          count: summary.count
        }
      ])
    ),
    gauges: Object.fromEntries(gauges.entries()),
    measures: Object.fromEntries(
      Array.from(measures.entries()).map(([name, values]) => [
        name,
        {
          avgMs:
            values.length > 0
              ? Number((values.reduce((sum, value) => sum + value, 0) / values.length).toFixed(2))
              : 0,
          count: values.length
        }
      ])
    ),
    selectorCounters: Object.fromEntries(selectorCounters.entries())
  };
}

export function resetPerfSnapshot(): void {
  counters.clear();
  measures.clear();
  gauges.clear();
  selectorCounters.clear();
  eventCounters.clear();
}
