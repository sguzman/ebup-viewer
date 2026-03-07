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

interface SentenceAnchorDiagnostics {
  matchedSentences: number;
  totalSentences: number;
}

export interface NativeHtmlSentenceAnchorResult {
  diagnostics: SentenceAnchorDiagnostics;
  firstAnchors: Map<number, HTMLElement>;
  sentenceAnchors: Map<number, HTMLElement[]>;
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
        parent.closest("[data-ll-reader-overlay='1']")
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
  let current = walker.nextNode();
  while (current) {
    const textNode = current as Text;
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
  const wrapped: HTMLElement[] = [];
  if (start.node === end.node) {
    let node = start.node;
    const segmentEnd = end.offset + end.length;
    if (segmentEnd < node.data.length) {
      node.splitText(segmentEnd);
    }
    if (start.offset > 0) {
      node = node.splitText(start.offset);
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
  range.setStart(start.node, start.offset);
  range.setEnd(end.node, end.offset + end.length);
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
    if (node === start.node) {
      segmentStart = start.offset;
    }
    if (node === end.node) {
      segmentEnd = end.offset + end.length;
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
