import { buildHtmlSentenceAnchorMap, normalizeSyncText } from "./htmlSync";

export interface PdfTextSpan {
  pageIndex: number;
  text: string;
  element: HTMLElement;
}

export interface PdfSentenceMatch {
  confidence: "exact" | "fallback" | "missing";
  pageIndex: number | null;
  spanIndexes: number[];
}

export interface PdfSentenceMatchDiagnostics {
  exactMatches: number;
  fallbackMatches: number;
  missingMatches: number;
}

interface NormalizedRange {
  start: number;
  end: number;
}

function needsJoiner(left: string, right: string): boolean {
  if (!left || !right) {
    return false;
  }
  const leftChar = left[left.length - 1] ?? "";
  const rightChar = right[0] ?? "";
  return /\p{L}|\p{N}/u.test(leftChar) && /\p{L}|\p{N}/u.test(rightChar);
}

function buildNormalizedDocument(spans: PdfTextSpan[]): {
  normalized: string;
  ranges: NormalizedRange[];
} {
  let normalized = "";
  const ranges: NormalizedRange[] = [];

  for (const span of spans) {
    const text = normalizeSyncText(span.text);
    if (!text) {
      ranges.push({ start: -1, end: -1 });
      continue;
    }
    if (normalized && needsJoiner(normalized, text)) {
      normalized += " ";
    }
    const start = normalized.length;
    normalized += text;
    const end = normalized.length;
    ranges.push({ start, end });
  }

  return { normalized, ranges };
}

function collectSpanIndexesForRange(
  ranges: NormalizedRange[],
  start: number,
  end: number
): number[] {
  const indexes: number[] = [];
  for (let idx = 0; idx < ranges.length; idx += 1) {
    const range = ranges[idx];
    if (range.start < 0 || range.end <= start || range.start >= end) {
      continue;
    }
    indexes.push(idx);
  }
  return indexes;
}

export function buildPdfSentenceSpanMap(
  spans: PdfTextSpan[],
  sentences: string[]
): {
  matches: PdfSentenceMatch[];
  diagnostics: PdfSentenceMatchDiagnostics;
} {
  if (spans.length === 0 || sentences.length === 0) {
    return {
      matches: sentences.map(() => ({
        confidence: "missing",
        pageIndex: null,
        spanIndexes: []
      })),
      diagnostics: {
        exactMatches: 0,
        fallbackMatches: 0,
        missingMatches: sentences.length
      }
    };
  }

  const { normalized, ranges } = buildNormalizedDocument(spans);
  const fallbackMap = buildHtmlSentenceAnchorMap(
    spans.map((span) => span.text),
    sentences,
    sentences.map((_, idx) => idx)
  ).map;

  const matches: PdfSentenceMatch[] = [];
  let scanStart = 0;
  let exactMatches = 0;
  let fallbackMatches = 0;
  let missingMatches = 0;

  for (let sentenceIdx = 0; sentenceIdx < sentences.length; sentenceIdx += 1) {
    const sentence = normalizeSyncText(sentences[sentenceIdx] ?? "");
    if (!sentence) {
      matches.push({
        confidence: "missing",
        pageIndex: null,
        spanIndexes: []
      });
      missingMatches += 1;
      continue;
    }

    let start = normalized.indexOf(sentence, scanStart);
    if (start < 0) {
      const prefix = sentence.slice(0, Math.min(sentence.length, 64));
      if (prefix.length >= 16) {
        start = normalized.indexOf(prefix, Math.max(0, scanStart - 32));
      }
    }

    if (start >= 0) {
      const end = start + sentence.length;
      const spanIndexes = collectSpanIndexesForRange(ranges, start, end);
      if (spanIndexes.length > 0) {
        const pageIndex = spans[spanIndexes[0]]?.pageIndex ?? null;
        matches.push({
          confidence: "exact",
          pageIndex,
          spanIndexes
        });
        scanStart = Math.max(scanStart, end);
        exactMatches += 1;
        continue;
      }
    }

    const fallbackIdx = fallbackMap[sentenceIdx];
    if (fallbackIdx !== null && fallbackIdx !== undefined && spans[fallbackIdx]) {
      matches.push({
        confidence: "fallback",
        pageIndex: spans[fallbackIdx].pageIndex,
        spanIndexes: [fallbackIdx]
      });
      fallbackMatches += 1;
      continue;
    }

    matches.push({
      confidence: "missing",
      pageIndex: null,
      spanIndexes: []
    });
    missingMatches += 1;
  }

  return {
    matches,
    diagnostics: {
      exactMatches,
      fallbackMatches,
      missingMatches
    }
  };
}
