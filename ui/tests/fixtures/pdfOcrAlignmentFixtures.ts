import type { PdfSentenceMatch } from "../../src/components/pdfTextSync";

export interface PdfOcrAlignmentFixture {
  name: string;
  pageIndex: number;
  expectedSentenceIdx: number;
  expectedReason: "page_click" | "page_overlay_fallback";
  matches: PdfSentenceMatch[];
  overlaySentenceMap?: Array<[number, number]>;
}

export const PDF_OCR_ALIGNMENT_FIXTURES: PdfOcrAlignmentFixture[] = [
  {
    name: "clean book scans",
    pageIndex: 2,
    expectedSentenceIdx: 2,
    expectedReason: "page_click",
    matches: [
      { confidence: "exact", reason: "exact_token_chain_alignment", pageIndex: 0, spanIndexes: [0], score: 1 },
      { confidence: "exact", reason: "exact_token_chain_alignment", pageIndex: 1, spanIndexes: [1], score: 1 },
      { confidence: "exact", reason: "exact_token_chain_alignment", pageIndex: 2, spanIndexes: [2], score: 1 }
    ]
  },
  {
    name: "low-contrast scans",
    pageIndex: 1,
    expectedSentenceIdx: 1,
    expectedReason: "page_click",
    matches: [
      { confidence: "fallback", reason: "line_window_fuzzy_alignment", pageIndex: 0, spanIndexes: [0], score: 0.74 },
      { confidence: "fallback", reason: "line_window_fuzzy_alignment", pageIndex: 1, spanIndexes: [1], score: 0.71 }
    ]
  },
  {
    name: "skewed pages",
    pageIndex: 3,
    expectedSentenceIdx: 1,
    expectedReason: "page_click",
    matches: [
      { confidence: "fallback", reason: "block_fallback_alignment", pageIndex: 2, spanIndexes: [0], score: 0.62 },
      { confidence: "fallback", reason: "block_fallback_alignment", pageIndex: 3, spanIndexes: [1], score: 0.6 }
    ]
  },
  {
    name: "noisy photocopies",
    pageIndex: 4,
    expectedSentenceIdx: 1,
    expectedReason: "page_overlay_fallback",
    matches: [
      { confidence: "missing", reason: "missing", pageIndex: null, spanIndexes: [], score: 0 },
      { confidence: "missing", reason: "missing", pageIndex: null, spanIndexes: [], score: 0 }
    ],
    overlaySentenceMap: [[4, 1]]
  },
  {
    name: "two-column scans",
    pageIndex: 5,
    expectedSentenceIdx: 1,
    expectedReason: "page_click",
    matches: [
      { confidence: "fallback", reason: "normalized_sentence_alignment", pageIndex: 4, spanIndexes: [0, 1], score: 0.83 },
      { confidence: "fallback", reason: "line_window_fuzzy_alignment", pageIndex: 5, spanIndexes: [2, 3], score: 0.79 },
      { confidence: "fallback", reason: "line_window_fuzzy_alignment", pageIndex: 5, spanIndexes: [4, 5], score: 0.76 }
    ]
  },
  {
    name: "scans with captions and figures",
    pageIndex: 6,
    expectedSentenceIdx: 1,
    expectedReason: "page_click",
    matches: [
      { confidence: "exact", reason: "exact_token_chain_alignment", pageIndex: 5, spanIndexes: [0], score: 1 },
      { confidence: "fallback", reason: "block_fallback_alignment", pageIndex: 6, spanIndexes: [1], score: 0.64 },
      { confidence: "fallback", reason: "block_fallback_alignment", pageIndex: 6, spanIndexes: [2], score: 0.6 }
    ]
  },
  {
    name: "scans with tables",
    pageIndex: 7,
    expectedSentenceIdx: 1,
    expectedReason: "page_overlay_fallback",
    matches: [
      { confidence: "missing", reason: "missing", pageIndex: null, spanIndexes: [], score: 0 },
      { confidence: "missing", reason: "missing", pageIndex: null, spanIndexes: [], score: 0 }
    ],
    overlaySentenceMap: [[7, 1]]
  },
  {
    name: "scans with footnotes and sidenotes",
    pageIndex: 8,
    expectedSentenceIdx: 0,
    expectedReason: "page_click",
    matches: [
      { confidence: "fallback", reason: "line_window_fuzzy_alignment", pageIndex: 8, spanIndexes: [0, 1], score: 0.7 },
      { confidence: "fallback", reason: "block_fallback_alignment", pageIndex: 8, spanIndexes: [2], score: 0.59 }
    ]
  },
  {
    name: "rotated scans",
    pageIndex: 9,
    expectedSentenceIdx: 0,
    expectedReason: "page_click",
    matches: [
      { confidence: "fallback", reason: "normalized_sentence_alignment", pageIndex: 9, spanIndexes: [0, 1], score: 0.8 }
    ]
  },
  {
    name: "scans with marginal annotations",
    pageIndex: 10,
    expectedSentenceIdx: 1,
    expectedReason: "page_overlay_fallback",
    matches: [
      { confidence: "missing", reason: "missing", pageIndex: null, spanIndexes: [], score: 0 },
      { confidence: "missing", reason: "missing", pageIndex: null, spanIndexes: [], score: 0 }
    ],
    overlaySentenceMap: [[10, 1]]
  },
  {
    name: "mixed embedded-text + image PDFs",
    pageIndex: 11,
    expectedSentenceIdx: 1,
    expectedReason: "page_click",
    matches: [
      { confidence: "exact", reason: "exact_token_chain_alignment", pageIndex: 10, spanIndexes: [0], score: 1 },
      { confidence: "fallback", reason: "normalized_sentence_alignment", pageIndex: 11, spanIndexes: [1, 2], score: 0.84 },
      { confidence: "fallback", reason: "line_window_fuzzy_alignment", pageIndex: 11, spanIndexes: [3, 4], score: 0.75 }
    ]
  }
];
