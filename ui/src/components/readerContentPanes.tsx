import { memo } from "react";
import { Stack, Typography } from "@mui/material";

import type { ReaderSnapshot } from "../types";
import type { ReturnTypeTypographyLayout } from "./readerTypographyTypes";

interface ReaderPrettyHtmlPaneProps {
  currentPage: number;
  onFrameLoad: () => void;
  hasPrettyMarkdown: boolean;
  isBrowserTabPrettyHtml: boolean;
  nativeHtmlFrameRef: React.MutableRefObject<HTMLIFrameElement | null>;
  renderedNativeHtml: string;
  showSentenceList: boolean;
}

interface ReaderPrettyMarkdownPaneProps {
  handlePrettyContentClick: (event: React.MouseEvent<HTMLDivElement>) => void;
  renderedMarkdownHtml: string;
}

interface ReaderTextOnlyPaneProps {
  isPrettyTextMode: boolean;
  onSentenceClick: (idx: number) => Promise<void>;
  reader: ReaderSnapshot;
  readerTypography: ReturnTypeTypographyLayout;
  searchMatchSet: Set<number>;
  sentenceRefs: React.MutableRefObject<Record<number, HTMLButtonElement | null>>;
}

export const ReaderPdfTextUnavailableNotice = memo(function ReaderPdfTextUnavailableNotice({
  reader
}: {
  reader: ReaderSnapshot;
}) {
  const message = reader.pdf_runtime_policy?.explanation
    ?? (reader.pdf_geometry_mode === "ocr_required"
      ? "Text-only content is unavailable for this PDF until OCR produces usable text."
      : "Text-only content is unavailable for this PDF because no usable extracted text is available.");
  const why = reader.pdf_classification?.reasons?.[0]?.replaceAll("_", " ") ?? null;
  return (
    <Stack spacing={0.25}>
      <Typography
        variant="caption"
        color="text.secondary"
        data-testid="reader-pdf-text-unavailable"
      >
        {message}
      </Typography>
      {why ? (
        <Typography variant="caption" color="text.secondary">
          Reason: {why}
        </Typography>
      ) : null}
    </Stack>
  );
});

export const ReaderPrettyHtmlPane = memo(function ReaderPrettyHtmlPane({
  currentPage,
  onFrameLoad,
  hasPrettyMarkdown,
  isBrowserTabPrettyHtml,
  nativeHtmlFrameRef,
  renderedNativeHtml,
  showSentenceList
}: ReaderPrettyHtmlPaneProps) {
  return (
    <div
      style={{
        width: "100%",
        flex: showSentenceList || hasPrettyMarkdown ? undefined : 1,
        minHeight: showSentenceList || hasPrettyMarkdown ? undefined : 0,
      }}
    >
      <iframe
        ref={nativeHtmlFrameRef}
        className="reader-native-html-frame"
        data-testid="reader-pretty-native-html"
        data-reader-browser-tab={isBrowserTabPrettyHtml ? "1" : "0"}
        onLoad={onFrameLoad}
        sandbox="allow-same-origin allow-popups allow-popups-to-escape-sandbox"
        srcDoc={renderedNativeHtml}
        title={`Native HTML reader page ${currentPage + 1}`}
      />
    </div>
  );
});

export const ReaderPrettyMarkdownPane = memo(function ReaderPrettyMarkdownPane({
  handlePrettyContentClick,
  renderedMarkdownHtml
}: ReaderPrettyMarkdownPaneProps) {
  return (
    <div
      style={{
        maxWidth: "72ch",
        marginInline: "auto",
        padding: "10px 12px",
        border: "1px solid rgba(148, 163, 184, 0.36)",
        borderRadius: 12,
        background: "rgba(255, 255, 255, 0.82)",
        boxShadow: "0 1px 2px rgba(15, 23, 42, 0.06)",
        color: "#1f2937"
      }}
    >
      <div
        className="reader-markdown-content"
        data-testid="reader-pretty-markdown"
        onClick={handlePrettyContentClick}
        dangerouslySetInnerHTML={{ __html: renderedMarkdownHtml }}
      />
    </div>
  );
});

const ReaderSentenceRow = memo(function ReaderSentenceRow({
  currentPage,
  highlighted,
  idx,
  isPrettyTextMode,
  onClick,
  readerTypography,
  searchMatch,
  sentence,
  sentenceRef
}: {
  currentPage: number;
  highlighted: boolean;
  idx: number;
  isPrettyTextMode: boolean;
  onClick: () => void;
  readerTypography: ReturnTypeTypographyLayout;
  searchMatch: boolean;
  sentence: string;
  sentenceRef: (element: HTMLButtonElement | null) => void;
}) {
  const baseBorderColor = isPrettyTextMode ? "rgba(148, 163, 184, 0.36)" : "transparent";
  const baseBackground = isPrettyTextMode ? "rgba(255, 255, 255, 0.78)" : "transparent";
  return (
    <button
      key={`${currentPage}:${idx}`}
      ref={sentenceRef}
      type="button"
      onClick={onClick}
      className="w-full rounded-lg border px-3 py-1.5 text-left transition-colors"
      data-testid={`reader-sentence-${idx}`}
      data-highlighted={highlighted ? "1" : "0"}
      style={{
        fontSize: `${readerTypography.fontSizePx}px`,
        lineHeight: isPrettyTextMode
          ? Math.max(readerTypography.lineSpacing, 1.55)
          : readerTypography.lineSpacing,
        wordSpacing: `${readerTypography.wordSpacingPx}px`,
        letterSpacing: `${readerTypography.letterSpacingPx}px`,
        borderColor: highlighted
          ? "var(--reader-highlight-border)"
          : searchMatch
            ? "var(--reader-search-border)"
            : baseBorderColor,
        background: highlighted
          ? "var(--reader-highlight-bg)"
          : searchMatch
            ? "var(--reader-search-bg)"
            : baseBackground,
        maxWidth: isPrettyTextMode ? "72ch" : "100%",
        marginInline: isPrettyTextMode ? "auto" : undefined,
        boxShadow: isPrettyTextMode ? "0 1px 2px rgba(15, 23, 42, 0.06)" : "none",
        borderRadius: isPrettyTextMode ? 12 : 8,
        color: isPrettyTextMode ? "#1f2937" : undefined
      }}
    >
      {sentence}
    </button>
  );
});

export const ReaderTextOnlyPane = memo(function ReaderTextOnlyPane({
  isPrettyTextMode,
  onSentenceClick,
  reader,
  readerTypography,
  searchMatchSet,
  sentenceRefs
}: ReaderTextOnlyPaneProps) {
  if (reader.sentences.length === 0) {
    return null;
  }
  return (
    <Stack spacing={0.75}>
      {reader.sentences.map((sentence, idx) => (
        <ReaderSentenceRow
          key={`${reader.current_page}:${idx}`}
          currentPage={reader.current_page}
          highlighted={reader.highlighted_sentence_idx === idx}
          idx={idx}
          isPrettyTextMode={isPrettyTextMode}
          onClick={() => void onSentenceClick(idx)}
          readerTypography={readerTypography}
          searchMatch={searchMatchSet.has(idx)}
          sentence={sentence}
          sentenceRef={(element) => {
            sentenceRefs.current[idx] = element;
          }}
        />
      ))}
    </Stack>
  );
});

export const ReaderPrettyUnavailableNotice = memo(function ReaderPrettyUnavailableNotice() {
  return (
    <Typography
      variant="caption"
      color="text.secondary"
      data-testid="reader-pretty-markdown-fallback"
    >
      Pretty view unavailable for this source. Showing text fallback.
    </Typography>
  );
});
