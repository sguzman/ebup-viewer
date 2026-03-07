import {
  Card,
  CardContent,
  Divider,
  Stack,
  Typography,
} from "@mui/material";
import { memo, useEffect, useMemo, useRef, useState } from "react";

import { useRenderDebugCounter } from "../perf/debug";
import { useReaderPlaybackState } from "../store/selectors";
import type { ReaderSettingsPatch, ReaderSnapshot, TtsStateEvent } from "../types";
import {
  renderPrettyMarkdownDocument,
  renderPrettyNativeHtmlDocument
} from "./contentRender";
import {
  ReaderPrettyHtmlPane,
  ReaderPrettyMarkdownPane,
  ReaderPrettyUnavailableNotice,
  ReaderTextOnlyPane
} from "./readerContentPanes";
import {
  ReaderSearchBar,
  ReaderSettingsPanel,
  ReaderStatsPanel,
  ReaderTopBar,
  ReaderTtsPanel
} from "./readerPanels";
import { toReaderImageSrc } from "./readerDom";
import { computeReaderTypographyLayout } from "./readerTypography";
import { TtsPlayerWidget } from "./TtsPlayerWidget";
import { useReaderHighlightSync } from "./useReaderHighlightSync";
import { useReaderSessionStats } from "./useReaderSessionStats";

interface ReaderShellProps {
  reader: ReaderSnapshot;
  busy: boolean;
  onCloseSession: () => Promise<void>;
  onPrevPage: () => Promise<void>;
  onNextPage: () => Promise<void>;
  onPrevSentence: () => Promise<void>;
  onNextSentence: () => Promise<void>;
  onSetPage: (page: number) => Promise<void>;
  onSentenceClick: (sentenceIdx: number) => Promise<void>;
  onToggleTextOnly: () => Promise<void>;
  onSearchQuery: (query: string) => Promise<void>;
  onSearchNext: () => Promise<void>;
  onSearchPrev: () => Promise<void>;
  onToggleTheme: () => Promise<void>;
  onToggleSettingsPanel: () => Promise<void>;
  onToggleStatsPanel: () => Promise<void>;
  onToggleTtsPanel: () => Promise<void>;
  onTtsPlay: () => Promise<void>;
  onTtsPause: () => Promise<void>;
  onTtsTogglePlayPause: () => Promise<void>;
  onTtsPlayFromPageStart: () => Promise<void>;
  onTtsPlayFromHighlight: () => Promise<void>;
  onTtsSeekNext: () => Promise<void>;
  onTtsSeekPrev: () => Promise<void>;
  onTtsRepeatSentence: () => Promise<void>;
  onTtsPrecomputePage: () => Promise<void>;
  onApplySettings: (patch: ReaderSettingsPatch) => Promise<void>;
  ttsStateEvent: TtsStateEvent | null;
}

export const ReaderShell = memo(function ReaderShell({
  reader: documentReader,
  busy,
  onCloseSession,
  onPrevPage,
  onNextPage,
  onPrevSentence,
  onNextSentence,
  onSetPage,
  onSentenceClick,
  onToggleTextOnly,
  onSearchQuery,
  onSearchNext,
  onSearchPrev,
  onToggleTheme,
  onToggleSettingsPanel,
  onToggleStatsPanel,
  onToggleTtsPanel,
  onTtsPlay,
  onTtsPause,
  onTtsTogglePlayPause,
  onTtsPlayFromPageStart,
  onTtsPlayFromHighlight,
  onTtsSeekNext,
  onTtsSeekPrev,
  onTtsRepeatSentence,
  onTtsPrecomputePage,
  onApplySettings,
  ttsStateEvent
}: ReaderShellProps) {
  void onToggleTextOnly;
  void onToggleSettingsPanel;
  void onToggleStatsPanel;
  void onToggleTtsPanel;
  void onTtsPlay;
  void onTtsPause;
  void onTtsPlayFromPageStart;
  void onTtsPlayFromHighlight;
  void onTtsRepeatSentence;
  void onTtsPrecomputePage;

  useRenderDebugCounter("ReaderShell");
  const playback = useReaderPlaybackState(documentReader.source_path, documentReader.current_page);
  const reader = useMemo(
    () =>
      playback
        ? {
            ...documentReader,
            highlighted_sentence_idx: playback.highlighted_sentence_idx,
            tts: playback.tts,
            stats: playback.stats
          }
        : documentReader,
    [documentReader, playback]
  );
  const [pageInput, setPageInput] = useState(String(reader.current_page + 1));
  const [searchInput, setSearchInput] = useState(reader.search_query);
  const sentenceRefs = useRef<Record<number, HTMLButtonElement | null>>({});
  const sentenceScrollRef = useRef<HTMLDivElement | null>(null);
  const nativeHtmlCacheRef = useRef<{ key: string; html: string }>({ key: "", html: "" });

  useEffect(() => {
    setPageInput(String(reader.current_page + 1));
  }, [reader.current_page]);

  useEffect(() => {
    setSearchInput(reader.search_query);
  }, [reader.search_query]);

  const searchMatchSet = useMemo(() => new Set(reader.search_matches), [reader.search_matches]);
  const panelTitle = useMemo(() => {
    if (reader.panels.show_settings) {
      return "Settings";
    }
    if (reader.panels.show_stats) {
      return "Stats";
    }
    if (reader.panels.show_tts) {
      return "TTS Options";
    }
    return null;
  }, [reader.panels.show_settings, reader.panels.show_stats, reader.panels.show_tts]);
  const readerTypography = useMemo(
    () => computeReaderTypographyLayout(reader.settings),
    [reader.settings]
  );
  const sessionStats = useReaderSessionStats(reader);
  const imageCandidatesKey = useMemo(
    () => reader.images.map((image) => `${image.raw_path}\t${image.local_path}`).join("\n"),
    [reader.images]
  );
  const readerImageCandidates = useMemo(
    () =>
      reader.images.map((image) => ({
        rawPath: image.raw_path,
        src: toReaderImageSrc(image.local_path)
      })),
    [reader.images]
  );
  const hasHighlightSentence = reader.highlighted_sentence_idx !== null;
  const isPrettyTextMode = !reader.text_only_mode;
  const hasPrettyMarkdown =
    isPrettyTextMode && reader.pretty_kind === "markdown" && Boolean(reader.reading_markdown_page);
  const hasPrettyHtml =
    isPrettyTextMode && reader.pretty_kind === "html" && Boolean(reader.reading_html_page);
  const isBrowserTabPrettyHtml =
    hasPrettyHtml && reader.source_path.toLowerCase().endsWith(".lltab");
  const prettyUnavailable = isPrettyTextMode && !hasPrettyMarkdown && !hasPrettyHtml;
  const showSentenceList = reader.text_only_mode || prettyUnavailable;
  const ttsSentenceLabel = useMemo(
    () =>
      `Sentence ${reader.tts.current_sentence_idx !== null ? reader.tts.current_sentence_idx + 1 : 0}/${Math.max(1, reader.tts.sentence_count)}`,
    [reader.tts.current_sentence_idx, reader.tts.sentence_count]
  );
  const ttsProgressLabel = useMemo(
    () => `Progress ${reader.tts.progress_pct.toFixed(3)}% | ${reader.tts.state}`,
    [reader.tts.progress_pct, reader.tts.state]
  );
  const renderedMarkdownHtml = useMemo(() => {
    if (!hasPrettyMarkdown || !reader.reading_markdown_page) {
      return "";
    }
    return renderPrettyMarkdownDocument(reader.reading_markdown_page, readerImageCandidates);
  }, [hasPrettyMarkdown, reader.reading_markdown_page, readerImageCandidates]);
  const renderedNativeHtml = useMemo(() => {
    if (!hasPrettyHtml || !reader.reading_html_page) {
      return "";
    }
    const key = `${reader.source_path}\n${imageCandidatesKey}\n${reader.reading_html_page}`;
    if (nativeHtmlCacheRef.current.key === key) {
      return nativeHtmlCacheRef.current.html;
    }
    const rendered = renderPrettyNativeHtmlDocument(reader.reading_html_page, readerImageCandidates);
    nativeHtmlCacheRef.current = { key, html: rendered };
    return rendered;
  }, [
    hasPrettyHtml,
    imageCandidatesKey,
    reader.reading_html_page,
    reader.source_path,
    readerImageCandidates
  ]);
  const { handlePrettyContentClick, jumpToHighlightedSentence, nativeHtmlFrameRef } =
    useReaderHighlightSync({
      hasPrettyHtml,
      hasPrettyMarkdown,
      reader,
      renderedMarkdownHtml,
      renderedNativeHtml,
      sentenceRefs,
      sentenceScrollRef
    });
  const themeLabel = reader.settings.theme === "night" ? "Day" : "Night";

  return (
    <Card className="w-full max-w-[1700px] min-h-0 rounded-3xl border border-slate-200 shadow-sm lg:h-full">
      <CardContent className="h-full p-4 md:p-6" sx={{ position: "relative" }}>
        <Stack spacing={2} sx={{ height: "100%", minHeight: 0 }}>
          <ReaderTopBar
            busy={busy}
            hasHighlightSentence={hasHighlightSentence}
            jumpToHighlightedSentence={jumpToHighlightedSentence}
            onCloseSession={onCloseSession}
            onNextPage={onNextPage}
            onNextSentence={onNextSentence}
            onPrevPage={onPrevPage}
            onPrevSentence={onPrevSentence}
            onSetPage={onSetPage}
            onToggleTheme={onToggleTheme}
            pageInput={pageInput}
            reader={reader}
            setPageInput={setPageInput}
            themeLabel={themeLabel}
          />

          <ReaderSearchBar
            onSearchNext={onSearchNext}
            onSearchPrev={onSearchPrev}
            onSearchQuery={onSearchQuery}
            searchInput={searchInput}
            setSearchInput={setSearchInput}
          />

          <Stack direction={{ xs: "column", lg: "row" }} spacing={2} sx={{ flex: 1, minHeight: 0 }}>
            <div className="min-h-0 flex flex-1 flex-col overflow-hidden rounded-2xl border border-slate-200">
              <div
                ref={sentenceScrollRef}
                className={
                  hasPrettyHtml && !hasPrettyMarkdown && !showSentenceList
                    ? "overflow-hidden overscroll-contain"
                    : "overflow-y-auto overscroll-contain"
                }
                data-testid="reader-sentence-scroll-container"
                style={{
                  height: reader.panels.show_tts ? "calc(100% - 118px)" : "100%",
                  paddingLeft: `${readerTypography.horizontalMarginPx}px`,
                  paddingRight: `${readerTypography.horizontalMarginPx}px`,
                  paddingTop: `${readerTypography.verticalMarginPx}px`,
                  paddingBottom: `${readerTypography.verticalMarginPx}px`,
                  scrollbarGutter: "stable",
                  background: isPrettyTextMode
                    ? "linear-gradient(180deg, rgba(255,255,255,0.7) 0%, rgba(248,250,252,0.8) 100%)"
                    : "transparent"
                }}
              >
                <Stack
                  spacing={0.75}
                  sx={{
                    minHeight: hasPrettyHtml && !hasPrettyMarkdown && !showSentenceList ? "100%" : undefined,
                    height: hasPrettyHtml && !hasPrettyMarkdown && !showSentenceList ? "100%" : undefined,
                  }}
                >
                  {hasPrettyHtml ? (
                    <ReaderPrettyHtmlPane
                      currentPage={reader.current_page}
                      hasPrettyMarkdown={hasPrettyMarkdown}
                      isBrowserTabPrettyHtml={isBrowserTabPrettyHtml}
                      nativeHtmlFrameRef={nativeHtmlFrameRef}
                      renderedNativeHtml={renderedNativeHtml}
                      showSentenceList={showSentenceList}
                    />
                  ) : null}
                  {hasPrettyMarkdown ? (
                    <ReaderPrettyMarkdownPane
                      handlePrettyContentClick={handlePrettyContentClick}
                      renderedMarkdownHtml={renderedMarkdownHtml}
                    />
                  ) : null}
                  {prettyUnavailable ? <ReaderPrettyUnavailableNotice /> : null}
                  {showSentenceList ? (
                    <ReaderTextOnlyPane
                      isPrettyTextMode={isPrettyTextMode}
                      onSentenceClick={onSentenceClick}
                      reader={reader}
                      readerTypography={readerTypography}
                      searchMatchSet={searchMatchSet}
                      sentenceRefs={sentenceRefs}
                    />
                  ) : null}
                </Stack>
              </div>
              <TtsPlayerWidget
                visible={reader.panels.show_tts}
                busy={busy}
                isPlaying={reader.tts.state === "playing"}
                canPrevPage={reader.current_page > 0}
                canNextPage={reader.current_page + 1 < reader.total_pages}
                canPrevSentence={reader.tts.can_seek_prev}
                canNextSentence={reader.tts.can_seek_next}
                currentSentenceLabel={ttsSentenceLabel}
                progressLabel={ttsProgressLabel}
                onPrevPage={onPrevPage}
                onPrevSentence={onTtsSeekPrev}
                onTogglePlayPause={onTtsTogglePlayPause}
                onNextSentence={onTtsSeekNext}
                onNextPage={onNextPage}
              />
            </div>

            {panelTitle ? (
              <div className="w-full min-h-0 shrink-0 rounded-2xl border border-slate-200 p-3 lg:h-full lg:w-[380px]">
                <Stack spacing={1.25} sx={{ height: "100%", minHeight: 0 }}>
                  <Typography variant="subtitle1" fontWeight={700} sx={{ flexShrink: 0 }}>
                    <span data-testid="reader-panel-title">{panelTitle}</span>
                  </Typography>
                  <Divider sx={{ flexShrink: 0 }} />

                  <div
                    style={{
                      overflowY: "auto",
                      minHeight: 0,
                      overscrollBehavior: "contain",
                      paddingTop: 6,
                      paddingRight: 8,
                      scrollbarGutter: "stable"
                    }}
                  >
                    {reader.panels.show_settings ? (
                      <ReaderSettingsPanel onApplySettings={onApplySettings} reader={reader} />
                    ) : null}
                    {reader.panels.show_stats ? (
                      <ReaderStatsPanel reader={reader} stats={sessionStats} />
                    ) : null}
                    {reader.panels.show_tts ? (
                      <ReaderTtsPanel
                        onApplySettings={onApplySettings}
                        reader={reader}
                        ttsStateEvent={ttsStateEvent}
                      />
                    ) : null}
                  </div>
                </Stack>
              </div>
            ) : null}
          </Stack>
        </Stack>
      </CardContent>
    </Card>
  );
});
