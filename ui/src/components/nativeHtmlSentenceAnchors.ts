import { recordPerfMeasure } from "../perf/debug";

interface NormalizedCharPosition {
  length: number;
  node: Text;
  offset: number;
}

interface SentenceRange {
  end: number;
  start: number;
}

interface SentenceMatchCandidate {
  end: number;
  score: number;
  start: number;
}

interface SafeTextBoundary {
  node: Text;
  offset: number;
}

const STRUCTURAL_BOUNDARY_TAGS = new Set([
  "article",
  "aside",
  "blockquote",
  "br",
  "div",
  "figcaption",
  "figure",
  "footer",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "header",
  "hr",
  "img",
  "li",
  "main",
  "nav",
  "ol",
  "p",
  "picture",
  "pre",
  "section",
  "svg",
  "table",
  "tbody",
  "td",
  "th",
  "thead",
  "tr",
  "ul"
]);

interface SentenceAnchorDiagnostics {
  matchedSentences: number;
  totalSentences: number;
}

export interface NativeHtmlSentenceAnchorResult {
  diagnostics: SentenceAnchorDiagnostics;
  firstAnchors: Map<number, HTMLElement>;
  sentenceAnchors: Map<number, HTMLElement[]>;
}

function isSuperscriptCitationText(node: Text): boolean {
  const parent = node.parentElement;
  if (!parent) {
    return false;
  }
  const sup = parent.closest("sup");
  if (!sup) {
    return false;
  }
  const value = (node.textContent ?? "").trim();
  if (!value) {
    return false;
  }
  return /^[\[(]?(?:\d+|[ivxlcdm]+|[a-z])[\]).,;:]*$/i.test(value);
}

function isStructuralBoundaryElement(node: Node | null): node is Element {
  return node instanceof Element && STRUCTURAL_BOUNDARY_TAGS.has(node.tagName.toLowerCase());
}

function nearestStructuralAncestor(node: Node | null): Element | null {
  let cursor: Node | null = node instanceof Text ? node.parentElement : node;
  while (cursor) {
    if (cursor instanceof Element && isStructuralBoundaryElement(cursor)) {
      return cursor;
    }
    cursor = cursor.parentNode;
  }
  return null;
}

function lowestCommonAncestor(left: Node, right: Node): Node | null {
  const lineage = new Set<Node>();
  let current: Node | null = left;
  while (current) {
    lineage.add(current);
    current = current.parentNode;
  }
  current = right;
  while (current) {
    if (lineage.has(current)) {
      return current;
    }
    current = current.parentNode;
  }
  return null;
}

function childUnderAncestor(ancestor: Node, node: Node): Node | null {
  let current: Node | null = node;
  let child: Node | null = node;
  while (current && current !== ancestor) {
    child = current;
    current = current.parentNode;
  }
  return current === ancestor ? child : null;
}

function hasStructuralBoundaryBetween(previousNode: Text | null, currentNode: Text): boolean {
  if (!previousNode) {
    return false;
  }
  const previousBoundary = nearestStructuralAncestor(previousNode);
  const currentBoundary = nearestStructuralAncestor(currentNode);
  if (previousBoundary && currentBoundary && previousBoundary !== currentBoundary) {
    return true;
  }

  const commonAncestor = lowestCommonAncestor(previousNode, currentNode);
  if (!commonAncestor) {
    return false;
  }
  const previousBranch = childUnderAncestor(commonAncestor, previousNode);
  const currentBranch = childUnderAncestor(commonAncestor, currentNode);
  if (!previousBranch || !currentBranch || previousBranch === currentBranch) {
    return false;
  }
  if (isStructuralBoundaryElement(previousBranch) || isStructuralBoundaryElement(currentBranch)) {
    return true;
  }
  let cursor = previousBranch.nextSibling;
  while (cursor && cursor !== currentBranch) {
    if (cursor.nodeType === Node.TEXT_NODE && (cursor.textContent ?? "").trim().length > 0) {
      return true;
    }
    if (isStructuralBoundaryElement(cursor)) {
      return true;
    }
    cursor = cursor.nextSibling;
  }
  return false;
}

function normalizeWithPositions(root: ParentNode): {
  positions: NormalizedCharPosition[];
  text: string;
} {
  const doc = root instanceof Document ? root : root.ownerDocument;
  if (!doc) {
    return { positions: [], text: "" };
  }
  const walker = doc.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      const parent = node.parentElement;
      if (!parent) {
        return NodeFilter.FILTER_REJECT;
      }
      const tag = parent.tagName.toLowerCase();
      if (
        tag === "script" ||
        tag === "style" ||
        tag === "noscript" ||
        parent.closest("[data-ll-reader-overlay='1']") ||
        isSuperscriptCitationText(node as Text)
      ) {
        return NodeFilter.FILTER_REJECT;
      }
      if (!(node.textContent ?? "").trim()) {
        return NodeFilter.FILTER_REJECT;
      }
      return NodeFilter.FILTER_ACCEPT;
    }
  });

  const chars: string[] = [];
  const positions: NormalizedCharPosition[] = [];
  let lastWasSpace = true;
  let previousTextNode: Text | null = null;
  let current = walker.nextNode();
  while (current) {
    const textNode = current as Text;
    if (
      previousTextNode
      && chars.length > 0
      && !lastWasSpace
      && hasStructuralBoundaryBetween(previousTextNode, textNode)
    ) {
      chars.push(" ");
      positions.push({ node: textNode, offset: 0, length: 0 });
      lastWasSpace = true;
    }
    const value = textNode.data;
    for (let offset = 0; offset < value.length; ) {
      const char = value.slice(offset, offset + 2).match(/^[\uD800-\uDBFF][\uDC00-\uDFFF]/)
        ? value.slice(offset, offset + 2)
        : value[offset] ?? "";
      const lower = char.toLocaleLowerCase();
      const isWord = /[\p{L}\p{N}]/u.test(lower);
      const isSpace = /\s/u.test(lower);
      if (isWord) {
        chars.push(lower);
        positions.push({ node: textNode, offset, length: char.length });
        lastWasSpace = false;
      } else if ((isSpace || lower) && !lastWasSpace && chars.length > 0) {
        chars.push(" ");
        positions.push({ node: textNode, offset, length: char.length });
        lastWasSpace = true;
      }
      offset += char.length || 1;
    }
    previousTextNode = textNode;
    current = walker.nextNode();
  }

  while (chars.length > 0 && chars[0] === " ") {
    chars.shift();
    positions.shift();
  }
  while (chars.length > 0 && chars[chars.length - 1] === " ") {
    chars.pop();
    positions.pop();
  }

  return {
    positions,
    text: chars.join("")
  };
}

function normalizeSentence(value: string): string {
  return value
    .toLocaleLowerCase()
    .replace(/[^\p{L}\p{N}\s]+/gu, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function expandNumericToken(token: string): string {
  const parsed = Number.parseInt(token, 10);
  if (!Number.isFinite(parsed)) {
    return token;
  }
  const numericWords: Record<number, string> = {
    0: "zero",
    1: "one",
    2: "two",
    3: "three",
    4: "four",
    5: "five",
    6: "six",
    7: "seven",
    8: "eight",
    9: "nine",
    10: "ten",
    11: "eleven",
    12: "twelve",
    13: "thirteen",
    14: "fourteen",
    15: "fifteen",
    16: "sixteen",
    17: "seventeen",
    18: "eighteen",
    19: "nineteen",
    20: "twenty"
  };
  return numericWords[parsed] ?? token;
}

function buildTokenSequences(normalizedSentence: string): string[][] {
  const tokens = normalizedSentence.split(/\s+/).filter(Boolean);
  if (tokens.length === 0) {
    return [];
  }
  const sequences = [tokens];
  const expanded = tokens.map((token) => expandNumericToken(token));
  if (expanded.join(" ") !== tokens.join(" ")) {
    sequences.push(expanded);
  }
  return sequences;
}

function buildTokenFragments(
  normalizedSentence: string,
  fromEnd = false
): string[] {
  const fragments: string[] = [];
  for (const tokens of buildTokenSequences(normalizedSentence)) {
    for (const tokenCount of [8, 6, 5, 4, 3]) {
      if (tokens.length < tokenCount) {
        continue;
      }
      const fragmentTokens = fromEnd ? tokens.slice(-tokenCount) : tokens.slice(0, tokenCount);
      const fragment = fragmentTokens.join(" ").trim();
      if (fragment.length >= 12 && !fragments.includes(fragment)) {
        fragments.push(fragment);
      }
    }
    for (const tokenCount of [2, 1]) {
      if (tokens.length < tokenCount) {
        continue;
      }
      const fragmentTokens = fromEnd ? tokens.slice(-tokenCount) : tokens.slice(0, tokenCount);
      const fragment = fragmentTokens.join(" ").trim();
      if (fragment.length >= 4 && !fragments.includes(fragment)) {
        fragments.push(fragment);
      }
    }
  }
  if (fragments.length === 0 && normalizedSentence.length >= 12) {
    fragments.push(normalizedSentence);
  } else if (fragments.length === 0 && normalizedSentence.length >= 4) {
    fragments.push(normalizedSentence);
  }
  return fragments;
}

function findApproximateSentenceRange(
  normalizedText: string,
  normalizedSentence: string,
  cursor: number
): SentenceRange | null {
  if (!normalizedSentence) {
    return null;
  }
  const prefixFragments = buildTokenFragments(normalizedSentence);
  const suffixFragments = buildTokenFragments(normalizedSentence, true);
  const searchStart = Math.max(0, cursor - 48);
  const searchEnd = Math.min(
    normalizedText.length,
    Math.max(cursor + Math.max(normalizedSentence.length * 8, 2048), searchStart + normalizedSentence.length)
  );
  let best: SentenceMatchCandidate | null = null;

  for (const prefix of prefixFragments) {
    if (!prefix) {
      continue;
    }
    let hits = 0;
    let start = normalizedText.indexOf(prefix, searchStart);
    while (start >= 0 && start < searchEnd && hits < 12) {
      hits += 1;
      const estimatedEnd = Math.min(normalizedText.length, start + normalizedSentence.length);
      let end = estimatedEnd;
      let score = prefix.length / normalizedSentence.length;
      for (const suffix of suffixFragments) {
        if (!suffix) {
          continue;
        }
        const suffixWindowStart = Math.max(start, estimatedEnd - Math.max(48, suffix.length * 2));
        const suffixWindowEnd = Math.min(searchEnd, estimatedEnd + Math.max(96, suffix.length * 3));
        const suffixIdx = normalizedText.indexOf(suffix, suffixWindowStart);
        if (suffixIdx >= 0 && suffixIdx < suffixWindowEnd) {
          end = suffixIdx + suffix.length;
          score += suffix.length / normalizedSentence.length;
          break;
        }
      }
      const distancePenalty = Math.abs(start - cursor) / Math.max(normalizedSentence.length * 2, 160);
      const candidateScore = score - distancePenalty;
      if (!best || candidateScore > best.score) {
        best = { start, end, score: candidateScore };
      }
      start = normalizedText.indexOf(prefix, start + 1);
    }
  }

  if (!best || best.score < 0.45) {
    return null;
  }
  return {
    start: best.start,
    end: Math.max(best.start + 1, best.end)
  };
}

function locateSentenceRanges(
  normalizedText: string,
  sentences: string[]
): Map<number, SentenceRange> {
  const ranges = new Map<number, SentenceRange>();
  let cursor = 0;
  for (let idx = 0; idx < sentences.length; idx += 1) {
    const normalizedSentence = normalizeSentence(sentences[idx] ?? "");
    if (!normalizedSentence) {
      continue;
    }
    let start = normalizedText.indexOf(normalizedSentence, cursor);
    if (start < 0 && cursor > 0) {
      start = normalizedText.indexOf(normalizedSentence, Math.max(0, cursor - 16));
    }
    if (start < 0) {
      const approximate = findApproximateSentenceRange(normalizedText, normalizedSentence, cursor);
      if (approximate) {
        ranges.set(idx, approximate);
        cursor = approximate.end;
      }
      continue;
    }
    const end = start + normalizedSentence.length;
    ranges.set(idx, { start, end });
    cursor = end;
  }
  return ranges;
}

function wrapTextRange(
  doc: Document,
  start: NormalizedCharPosition,
  end: NormalizedCharPosition,
  sentenceIdx: number
): HTMLElement[] {
  const clampBoundary = (position: NormalizedCharPosition, inclusiveEnd: boolean): SafeTextBoundary | null => {
    const node = position.node;
    if (!node.isConnected) {
      return null;
    }
    const maxOffset = node.data.length;
    const rawOffset = inclusiveEnd ? position.offset + position.length : position.offset;
    return {
      node,
      offset: Math.max(0, Math.min(maxOffset, rawOffset))
    };
  };

  const wrapped: HTMLElement[] = [];
  const safeStart = clampBoundary(start, false);
  const safeEnd = clampBoundary(end, true);
  if (!safeStart || !safeEnd) {
    return wrapped;
  }
  if (
    safeStart.node === safeEnd.node
    && safeStart.offset <= safeEnd.offset
  ) {
    let node = safeStart.node;
    const segmentEnd = safeEnd.offset;
    if (segmentEnd < node.data.length) {
      node.splitText(segmentEnd);
    }
    if (safeStart.offset > 0) {
      node = node.splitText(safeStart.offset);
    }
    if (!(node.data ?? "").trim()) {
      return wrapped;
    }
    const span = doc.createElement("span");
    span.setAttribute("data-ll-html-sentence", String(sentenceIdx));
    span.className = "ll-native-html-sentence";
    node.parentNode?.insertBefore(span, node);
    span.appendChild(node);
    wrapped.push(span);
    return wrapped;
  }
  const range = doc.createRange();
  try {
    range.setStart(safeStart.node, safeStart.offset);
    range.setEnd(safeEnd.node, safeEnd.offset);
  } catch {
    return wrapped;
  }
  const textNodes: Text[] = [];
  const walker = doc.createTreeWalker(range.commonAncestorContainer, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      if (!range.intersectsNode(node)) {
        return NodeFilter.FILTER_REJECT;
      }
      const parent = node.parentElement;
      if (!parent) {
        return NodeFilter.FILTER_REJECT;
      }
      if (parent.hasAttribute("data-ll-html-sentence")) {
        return NodeFilter.FILTER_REJECT;
      }
      return NodeFilter.FILTER_ACCEPT;
    }
  });
  let current = walker.nextNode();
  while (current) {
    textNodes.push(current as Text);
    current = walker.nextNode();
  }

  for (const originalNode of textNodes.reverse()) {
    let node = originalNode;
    let segmentStart = 0;
    let segmentEnd = node.data.length;
    if (node === safeStart.node) {
      segmentStart = Math.max(0, Math.min(node.data.length, safeStart.offset));
    }
    if (node === safeEnd.node) {
      segmentEnd = Math.max(0, Math.min(node.data.length, safeEnd.offset));
    }
    if (segmentStart >= segmentEnd) {
      continue;
    }
    if (segmentEnd < node.data.length) {
      node.splitText(segmentEnd);
    }
    if (segmentStart > 0) {
      node = node.splitText(segmentStart);
    }
    if (!(node.data ?? "").trim()) {
      continue;
    }
    const span = doc.createElement("span");
    span.setAttribute("data-ll-html-sentence", String(sentenceIdx));
    span.className = "ll-native-html-sentence";
    node.parentNode?.insertBefore(span, node);
    span.appendChild(node);
    wrapped.unshift(span);
  }

  return wrapped;
}

export function annotateNativeHtmlSentences(
  doc: Document,
  sentences: string[]
): NativeHtmlSentenceAnchorResult {
  const startedAt = typeof performance !== "undefined" ? performance.now() : 0;
  const existing = doc.querySelectorAll<HTMLElement>("[data-ll-html-sentence]");
  if (existing.length > 0) {
    const firstAnchors = new Map<number, HTMLElement>();
    const sentenceAnchors = new Map<number, HTMLElement[]>();
    for (const element of existing) {
      const raw = element.getAttribute("data-ll-html-sentence");
      const idx = raw === null ? Number.NaN : Number.parseInt(raw, 10);
      if (!Number.isFinite(idx)) {
        continue;
      }
      const current = sentenceAnchors.get(idx) ?? [];
      current.push(element);
      sentenceAnchors.set(idx, current);
      if (!firstAnchors.has(idx)) {
        firstAnchors.set(idx, element);
      }
    }
    return {
      diagnostics: {
        matchedSentences: firstAnchors.size,
        totalSentences: sentences.length
      },
      firstAnchors,
      sentenceAnchors
    };
  }

  const body = doc.body ?? doc.documentElement;
  const { positions, text } = normalizeWithPositions(body);
  const ranges = locateSentenceRanges(text, sentences);
  const firstAnchors = new Map<number, HTMLElement>();
  const sentenceAnchors = new Map<number, HTMLElement[]>();

  const orderedRanges = Array.from(ranges.entries()).sort((left, right) => right[0] - left[0]);
  for (const [sentenceIdx, range] of orderedRanges) {
    const start = positions[range.start];
    const end = positions[range.end - 1];
    if (!start || !end) {
      continue;
    }
    const wrapped = wrapTextRange(doc, start, end, sentenceIdx);
    if (wrapped.length === 0) {
      continue;
    }
    firstAnchors.set(sentenceIdx, wrapped[0]);
    sentenceAnchors.set(sentenceIdx, wrapped);
  }

  recordPerfMeasure("ReaderShell.annotateNativeHtmlSentences", startedAt);
  return {
    diagnostics: {
      matchedSentences: firstAnchors.size,
      totalSentences: sentences.length
    },
    firstAnchors,
    sentenceAnchors
  };
}
