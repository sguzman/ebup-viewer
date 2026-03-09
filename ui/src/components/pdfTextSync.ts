import { buildHtmlSentenceAnchorMap, normalizeSyncText } from "./htmlSync";

export interface PdfTextSpan {
  pageIndex: number;
  text: string;
  element: HTMLElement;
}

export interface PdfSentenceMatch {
  confidence: "exact" | "fallback" | "page" | "missing";
  reason:
    | "exact_geometry"
    | "fuzzy_sentence_geometry"
    | "paragraph_fallback"
    | "page_location_only"
    | "missing";
  pageIndex: number | null;
  spanIndexes: number[];
}

export interface PdfSentenceMatchDiagnostics {
  exactMatches: number;
  fallbackMatches: number;
  pageOnlyMatches: number;
  missingMatches: number;
  cappedLeaps: number;
}

interface NormalizedRange {
  start: number;
  end: number;
}

interface FuzzyCandidate {
  end: number;
  pageIndex: number | null;
  score: number;
  spanIndexes: number[];
}

const MIN_FUZZY_TOKENS = 4;
const MIN_FUZZY_SCORE = 0.58;
const MAX_LOCAL_LEAP_SPANS = 40;
const MIN_REPEATABLE_BOILERPLATE_LENGTH = 3;
const MAX_REPEATABLE_BOILERPLATE_LENGTH = 72;

const PDF_LIGATURES: Record<string, string> = {
  "\uFB00": "ff",
  "\uFB01": "fi",
  "\uFB02": "fl",
  "\uFB03": "ffi",
  "\uFB04": "ffl",
  "\uFB05": "ft",
  "\uFB06": "st"
};

function needsJoiner(left: string, right: string): boolean {
  if (!left || !right) {
    return false;
  }
  const leftChar = left[left.length - 1] ?? "";
  const rightChar = right[0] ?? "";
  return /\p{L}|\p{N}/u.test(leftChar) && /\p{L}|\p{N}/u.test(rightChar);
}

function hasWordJoinAcrossBoundary(leftRaw: string, rightRaw: string): boolean {
  if (!leftRaw || !rightRaw) {
    return false;
  }
  const lastLeft = leftRaw[leftRaw.length - 1] ?? "";
  const firstRight = rightRaw[0] ?? "";
  return /[\u00AD\u2010\u2011-]/u.test(lastLeft) && /\p{Ll}/u.test(firstRight);
}

function normalizePdfSpanText(value: string): string {
  let normalized = value;
  for (const [ligature, replacement] of Object.entries(PDF_LIGATURES)) {
    normalized = normalized.replaceAll(ligature, replacement);
  }
  normalized = normalized
    .replace(/[\u00AD\u200B\u200C\u200D\u2060\uFEFF]/g, "")
    .replace(/[\u2010\u2011\u2012\u2013\u2014\u2015]/g, "-");
  return normalizeSyncText(normalized);
}

function buildPageSpanIndexes(spans: PdfTextSpan[]): Map<number, number[]> {
  const byPage = new Map<number, number[]>();
  for (let idx = 0; idx < spans.length; idx += 1) {
    const pageIndex = spans[idx]?.pageIndex ?? -1;
    const page = byPage.get(pageIndex);
    if (page) {
      page.push(idx);
      continue;
    }
    byPage.set(pageIndex, [idx]);
  }
  return byPage;
}

function buildRepeatedBoilerplateIndexes(
  normalizedSpanTexts: string[],
  spans: PdfTextSpan[]
): Set<number> {
  const byPage = buildPageSpanIndexes(spans);
  const pageEdgeEntries = new Map<string, number[]>();
  for (const pageIndexes of byPage.values()) {
    const edgeIndexes = new Set<number>([
      ...pageIndexes.slice(0, 3),
      ...pageIndexes.slice(Math.max(0, pageIndexes.length - 3))
    ]);
    for (const spanIdx of edgeIndexes) {
      const text = normalizedSpanTexts[spanIdx] ?? "";
      if (
        text.length < MIN_REPEATABLE_BOILERPLATE_LENGTH
        || text.length > MAX_REPEATABLE_BOILERPLATE_LENGTH
      ) {
        continue;
      }
      const hits = pageEdgeEntries.get(text);
      if (hits) {
        hits.push(spanIdx);
        continue;
      }
      pageEdgeEntries.set(text, [spanIdx]);
    }
  }

  const suppressed = new Set<number>();
  for (const indexes of pageEdgeEntries.values()) {
    const pageHits = new Set(indexes.map((idx) => spans[idx]?.pageIndex ?? -1));
    if (pageHits.size < 3) {
      continue;
    }
    for (const idx of indexes) {
      suppressed.add(idx);
    }
  }
  return suppressed;
}

function buildDuplicateSpanIndexes(normalizedSpanTexts: string[], spans: PdfTextSpan[]): Set<number> {
  const suppressed = new Set<number>();
  for (let idx = 1; idx < spans.length; idx += 1) {
    const current = normalizedSpanTexts[idx] ?? "";
    const previous = normalizedSpanTexts[idx - 1] ?? "";
    if (!current || current !== previous) {
      continue;
    }
    if (spans[idx]?.pageIndex !== spans[idx - 1]?.pageIndex) {
      continue;
    }
    suppressed.add(idx);
  }
  return suppressed;
}

function buildNormalizedDocument(spans: PdfTextSpan[]): {
  normalized: string;
  ranges: NormalizedRange[];
  normalizedSpanTexts: string[];
  suppressedSpanIndexes: Set<number>;
} {
  let normalized = "";
  const ranges: NormalizedRange[] = [];
  const normalizedSpanTexts = spans.map((span) => normalizePdfSpanText(span.text));
  const suppressedSpanIndexes = new Set<number>([
    ...buildRepeatedBoilerplateIndexes(normalizedSpanTexts, spans),
    ...buildDuplicateSpanIndexes(normalizedSpanTexts, spans)
  ]);

  for (let idx = 0; idx < spans.length; idx += 1) {
    const text = normalizedSpanTexts[idx] ?? "";
    if (!text || suppressedSpanIndexes.has(idx)) {
      ranges.push({ start: -1, end: -1 });
      continue;
    }
    const previousRaw = spans[idx - 1]?.text ?? "";
    const currentRaw = spans[idx]?.text ?? "";
    if (normalized && needsJoiner(normalized, text) && !hasWordJoinAcrossBoundary(previousRaw, currentRaw)) {
      normalized += " ";
    }
    const start = normalized.length;
    normalized += text;
    const end = normalized.length;
    ranges.push({ start, end });
  }

  return {
    normalized,
    ranges,
    normalizedSpanTexts,
    suppressedSpanIndexes
  };
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

function buildSentenceTokens(sentence: string): string[] {
  return normalizePdfSpanText(sentence)
    .split(" ")
    .filter((token) => token.length > 0);
}

function scoreFuzzyWindow(
  windowText: string,
  sentenceTokens: string[]
): number {
  if (sentenceTokens.length === 0) {
    return 0;
  }
  const normalizedWindow = normalizePdfSpanText(windowText);
  let hits = 0;
  for (const token of sentenceTokens) {
    if (normalizedWindow.includes(token)) {
      hits += 1;
    }
  }
  return hits / sentenceTokens.length;
}

function buildPageFallbackMap(spans: PdfTextSpan[]): number[] {
  const firstByPage = new Map<number, number>();
  for (let idx = 0; idx < spans.length; idx += 1) {
    const pageIndex = spans[idx]?.pageIndex;
    if (pageIndex === undefined || firstByPage.has(pageIndex)) {
      continue;
    }
    firstByPage.set(pageIndex, idx);
  }
  return Array.from(firstByPage.entries())
    .sort((left, right) => left[0] - right[0])
    .map(([, spanIdx]) => spanIdx);
}

function findParagraphFallbackSpan(
  spans: PdfTextSpan[],
  sentence: string,
  fallbackHintIdx: number | null | undefined,
  previousSpanIdx: number,
  normalizedSpanTexts: string[],
  suppressedSpanIndexes: Set<number>
): number | null {
  const tokens = buildSentenceTokens(sentence);
  if (tokens.length === 0) {
    return null;
  }
  const startIdx = Math.max(0, previousSpanIdx, fallbackHintIdx ?? 0);
  const endIdx = Math.min(spans.length - 1, startIdx + 8);
  let bestIdx: number | null = null;
  let bestScore = 0;
  for (let idx = startIdx; idx <= endIdx; idx += 1) {
    if (suppressedSpanIndexes.has(idx)) {
      continue;
    }
    const score = scoreFuzzyWindow(normalizedSpanTexts[idx] ?? "", tokens);
    if (score > bestScore) {
      bestScore = score;
      bestIdx = idx;
    }
  }
  return bestScore >= 0.34 ? bestIdx : null;
}

function findFuzzySentenceCandidate(
  spanTexts: string[],
  ranges: NormalizedRange[],
  spans: PdfTextSpan[],
  sentence: string,
  scanStart: number,
  previousPageIndex: number | null,
  suppressedSpanIndexes: Set<number>
): FuzzyCandidate | null {
  const tokens = buildSentenceTokens(sentence);
  if (tokens.length < MIN_FUZZY_TOKENS) {
    return null;
  }

  const scanSpanStart = Math.max(
    0,
    ranges.findIndex((range) => range.start >= Math.max(0, scanStart - 64))
  );
  const localSpanIndexes = spans
    .map((_, idx) => idx)
    .slice(scanSpanStart, scanSpanStart + MAX_LOCAL_LEAP_SPANS)
    .filter((idx) => !suppressedSpanIndexes.has(idx));
  if (localSpanIndexes.length === 0) {
    return null;
  }

  let best: FuzzyCandidate | null = null;
  for (let startOffset = 0; startOffset < localSpanIndexes.length; startOffset += 1) {
    const startSpanIdx = localSpanIndexes[startOffset];
    const startRange = ranges[startSpanIdx];
    if (!startRange || startRange.start < 0) {
      continue;
    }
    const maxWindowEnd = Math.min(localSpanIndexes.length, startOffset + 12);
    for (let endOffset = startOffset; endOffset < maxWindowEnd; endOffset += 1) {
      const endSpanIdx = localSpanIndexes[endOffset];
      const endRange = ranges[endSpanIdx];
      if (!endRange || endRange.end <= startRange.start) {
        continue;
      }
      const windowText = spanTexts
        .slice(startSpanIdx, endSpanIdx + 1)
        .filter((value) => value.length > 0)
        .join(" ");
      const tokenScore = scoreFuzzyWindow(windowText, tokens);
      if (tokenScore < MIN_FUZZY_SCORE) {
        continue;
      }
      const windowLengthPenalty = Math.min(
        0.2,
        Math.abs(windowText.length - sentence.length) / Math.max(sentence.length, 1)
      );
      const pageIndex = spans[startSpanIdx]?.pageIndex ?? null;
      const pagePenalty =
        previousPageIndex !== null && pageIndex !== null && pageIndex > previousPageIndex + 1
          ? 0.15
          : 0;
      const score = tokenScore - windowLengthPenalty - pagePenalty;
      if (score < MIN_FUZZY_SCORE) {
        continue;
      }
      const spanIndexes = localSpanIndexes.slice(startOffset, endOffset + 1);
      if (!best || score > best.score) {
        best = {
          end: endRange.end,
          pageIndex,
          score,
          spanIndexes
        };
      }
    }
  }

  return best;
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
        reason: "missing",
        pageIndex: null,
        spanIndexes: []
      })),
      diagnostics: {
        exactMatches: 0,
        fallbackMatches: 0,
        pageOnlyMatches: 0,
        missingMatches: sentences.length,
        cappedLeaps: 0
      }
    };
  }

  const { normalized, ranges, normalizedSpanTexts, suppressedSpanIndexes } = buildNormalizedDocument(spans);
  const spanTexts = normalizedSpanTexts;
  const fallbackMap = buildHtmlSentenceAnchorMap(
    spans.map((span) => span.text),
    sentences,
    sentences.map((_, idx) => idx)
  ).map;
  const pageFallbackMap = buildPageFallbackMap(spans);

  const matches: PdfSentenceMatch[] = [];
  let scanStart = 0;
  let previousPageIndex: number | null = null;
  let exactMatches = 0;
  let fallbackMatches = 0;
  let pageOnlyMatches = 0;
  let missingMatches = 0;
  let cappedLeaps = 0;

  for (let sentenceIdx = 0; sentenceIdx < sentences.length; sentenceIdx += 1) {
    const sentence = normalizePdfSpanText(sentences[sentenceIdx] ?? "");
    if (!sentence) {
      matches.push({
        confidence: "missing",
        reason: "missing",
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
          reason: "exact_geometry",
          pageIndex,
          spanIndexes
        });
        scanStart = Math.max(scanStart, end);
        previousPageIndex = pageIndex;
        exactMatches += 1;
        continue;
      }
    }

    const fuzzyCandidate = findFuzzySentenceCandidate(
      spanTexts,
      ranges,
      spans,
      sentence,
      scanStart,
      previousPageIndex,
      suppressedSpanIndexes
    );
    if (fuzzyCandidate) {
      const leapStart = fuzzyCandidate.spanIndexes[0] ?? 0;
      const leapDistance = Math.max(0, leapStart - (matches.at(-1)?.spanIndexes.at(-1) ?? leapStart));
      if (leapDistance > MAX_LOCAL_LEAP_SPANS) {
        cappedLeaps += 1;
      } else {
        matches.push({
          confidence: "fallback",
          reason: "fuzzy_sentence_geometry",
          pageIndex: fuzzyCandidate.pageIndex,
          spanIndexes: fuzzyCandidate.spanIndexes
        });
        scanStart = Math.max(scanStart, fuzzyCandidate.end);
        previousPageIndex = fuzzyCandidate.pageIndex;
        fallbackMatches += 1;
        continue;
      }
    }

    const previousSpanIdx = matches.at(-1)?.spanIndexes.at(-1) ?? -1;
    const fallbackIdx = findParagraphFallbackSpan(
      spans,
      sentence,
      fallbackMap[sentenceIdx],
      previousSpanIdx,
      normalizedSpanTexts,
      suppressedSpanIndexes
    );
    if (fallbackIdx !== null && fallbackIdx !== undefined && spans[fallbackIdx]) {
      const fallbackPageIndex = spans[fallbackIdx].pageIndex;
      const previousSpanIdx = matches.at(-1)?.spanIndexes.at(-1) ?? -1;
      const leapDistance = fallbackIdx - previousSpanIdx;
      const allowFallback =
        (
          previousSpanIdx < 0
            || leapDistance <= MAX_LOCAL_LEAP_SPANS
            || (previousPageIndex !== null && fallbackPageIndex <= previousPageIndex + 1)
        );
      if (!allowFallback) {
        cappedLeaps += 1;
      } else {
        matches.push({
          confidence: "fallback",
          reason: "paragraph_fallback",
          pageIndex: fallbackPageIndex,
          spanIndexes: [fallbackIdx]
        });
        previousPageIndex = fallbackPageIndex;
        scanStart = Math.max(scanStart, ranges[fallbackIdx]?.end ?? scanStart);
        fallbackMatches += 1;
        continue;
      }
    }

    const pageFallbackIdx: number | undefined =
      previousPageIndex === null
        ? pageFallbackMap[0]
        : pageFallbackMap[Math.min(pageFallbackMap.length - 1, previousPageIndex + 1)];
    if (pageFallbackIdx !== undefined && spans[pageFallbackIdx]) {
      matches.push({
        confidence: "page",
        reason: "page_location_only",
        pageIndex: spans[pageFallbackIdx].pageIndex,
        spanIndexes: []
      });
      previousPageIndex = spans[pageFallbackIdx].pageIndex;
      pageOnlyMatches += 1;
      continue;
    }

    matches.push({
      confidence: "missing",
      reason: "missing",
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
      pageOnlyMatches,
      missingMatches,
      cappedLeaps
    }
  };
}

export function findNearestSentenceForSpanIndex(
  matches: PdfSentenceMatch[],
  spanIndex: number
): number | null {
  let bestSentenceIdx: number | null = null;
  let bestDistance = Number.POSITIVE_INFINITY;

  for (let sentenceIdx = 0; sentenceIdx < matches.length; sentenceIdx += 1) {
    const match = matches[sentenceIdx];
    if (!match || match.spanIndexes.length === 0) {
      continue;
    }
    if (match.spanIndexes.includes(spanIndex)) {
      return sentenceIdx;
    }
    for (const candidate of match.spanIndexes) {
      const distance = Math.abs(candidate - spanIndex);
      if (distance < bestDistance) {
        bestDistance = distance;
        bestSentenceIdx = sentenceIdx;
      }
    }
  }

  return bestSentenceIdx;
}
