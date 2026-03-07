import { useEffect, useMemo, useRef, useState } from "react";

import type { ReaderSnapshot } from "../types";
import { clamp } from "./readerShared";

export interface ReaderSessionStatsState {
  estimatedReadPages: number;
  estimatedTotalSentences: number;
  estimatedTotalWords: number;
  pageFinishedPct: number;
  sentencesReadOnPage: number;
  sessionGlobalPercentFinished: number;
  sessionPagesFinished: number;
  sessionPercentPerMinute: number;
  sessionSecondsInApp: number;
  sessionSecondsListening: number;
  sessionSentencesPerMinute: number;
  sessionSentencesRead: number;
  sessionWordsPerMinute: number;
  sessionWordsRead: number;
  statsTab: "page" | "global" | "session";
  setStatsTab: (value: "page" | "global" | "session") => void;
  wordsReadOnPage: number;
}

export function useReaderSessionStats(reader: ReaderSnapshot): ReaderSessionStatsState {
  const [statsTab, setStatsTab] = useState<"page" | "global" | "session">("page");
  const [sessionNowMs, setSessionNowMs] = useState(Date.now());
  const sessionStartMsRef = useRef(Date.now());
  const listeningAccumulatedMsRef = useRef(0);
  const listeningStartedAtMsRef = useRef<number | null>(
    reader.tts.state === "playing" ? Date.now() : null
  );
  const sessionBaselineWordsRef = useRef(reader.stats.words_read_up_to_current_position);
  const sessionMaxWordsRef = useRef(reader.stats.words_read_up_to_current_position);
  const sessionBaselineSentencesRef = useRef(reader.stats.sentences_read_up_to_current_position);
  const sessionMaxSentencesRef = useRef(reader.stats.sentences_read_up_to_current_position);
  const sessionFinishedPagesRef = useRef<Set<number>>(new Set());
  const [sessionWordsRead, setSessionWordsRead] = useState(0);
  const [sessionSentencesRead, setSessionSentencesRead] = useState(0);
  const [sessionPagesFinished, setSessionPagesFinished] = useState(0);

  const estimatedTotalWords = useMemo(() => {
    if (reader.stats.page_end_percent <= 0) {
      return reader.stats.words_read_up_to_page_end;
    }
    return Math.max(
      reader.stats.words_read_up_to_page_end,
      Math.round((reader.stats.words_read_up_to_page_end * 100) / reader.stats.page_end_percent)
    );
  }, [reader.stats.page_end_percent, reader.stats.words_read_up_to_page_end]);

  const estimatedTotalSentences = useMemo(() => {
    if (reader.stats.page_end_percent <= 0) {
      return reader.stats.sentences_read_up_to_page_end;
    }
    return Math.max(
      reader.stats.sentences_read_up_to_page_end,
      Math.round(
        (reader.stats.sentences_read_up_to_page_end * 100) / reader.stats.page_end_percent
      )
    );
  }, [reader.stats.page_end_percent, reader.stats.sentences_read_up_to_page_end]);

  const wordsReadOnPage = useMemo(
    () =>
      Math.max(
        0,
        reader.stats.words_read_up_to_current_position - reader.stats.words_read_up_to_page_start
      ),
    [reader.stats.words_read_up_to_current_position, reader.stats.words_read_up_to_page_start]
  );

  const sentencesReadOnPage = useMemo(
    () =>
      Math.max(
        0,
        reader.stats.sentences_read_up_to_current_position -
          reader.stats.sentences_read_up_to_page_start
      ),
    [
      reader.stats.sentences_read_up_to_current_position,
      reader.stats.sentences_read_up_to_page_start
    ]
  );

  const pageFinishedPct = useMemo(() => {
    if (reader.stats.page_word_count <= 0) {
      return 0;
    }
    return clamp((wordsReadOnPage / reader.stats.page_word_count) * 100, 0, 100);
  }, [reader.stats.page_word_count, wordsReadOnPage]);

  useEffect(() => {
    if (!reader.panels.show_stats) {
      return;
    }
    const tick = window.setInterval(() => {
      setSessionNowMs(Date.now());
    }, 1000);
    return () => window.clearInterval(tick);
  }, [reader.panels.show_stats]);

  useEffect(() => {
    const now = Date.now();
    if (reader.tts.state === "playing") {
      if (listeningStartedAtMsRef.current === null) {
        listeningStartedAtMsRef.current = now;
      }
    } else if (listeningStartedAtMsRef.current !== null) {
      listeningAccumulatedMsRef.current += now - listeningStartedAtMsRef.current;
      listeningStartedAtMsRef.current = null;
    }
  }, [reader.tts.state]);

  useEffect(() => {
    sessionStartMsRef.current = Date.now();
    listeningAccumulatedMsRef.current = 0;
    listeningStartedAtMsRef.current = reader.tts.state === "playing" ? Date.now() : null;
    sessionBaselineWordsRef.current = reader.stats.words_read_up_to_current_position;
    sessionMaxWordsRef.current = reader.stats.words_read_up_to_current_position;
    sessionBaselineSentencesRef.current = reader.stats.sentences_read_up_to_current_position;
    sessionMaxSentencesRef.current = reader.stats.sentences_read_up_to_current_position;
    sessionFinishedPagesRef.current = new Set();
    setSessionWordsRead(0);
    setSessionSentencesRead(0);
    setSessionPagesFinished(0);
    setSessionNowMs(Date.now());
    setStatsTab("page");
  }, [reader.source_path]);

  useEffect(() => {
    sessionMaxWordsRef.current = Math.max(
      sessionMaxWordsRef.current,
      reader.stats.words_read_up_to_current_position
    );
    setSessionWordsRead(
      Math.max(0, sessionMaxWordsRef.current - sessionBaselineWordsRef.current)
    );
    sessionMaxSentencesRef.current = Math.max(
      sessionMaxSentencesRef.current,
      reader.stats.sentences_read_up_to_current_position
    );
    setSessionSentencesRead(
      Math.max(0, sessionMaxSentencesRef.current - sessionBaselineSentencesRef.current)
    );
    if (pageFinishedPct >= 99.9) {
      sessionFinishedPagesRef.current.add(reader.current_page);
      setSessionPagesFinished(sessionFinishedPagesRef.current.size);
    }
  }, [
    pageFinishedPct,
    reader.current_page,
    reader.stats.sentences_read_up_to_current_position,
    reader.stats.words_read_up_to_current_position
  ]);

  const sessionSecondsInApp = Math.floor((sessionNowMs - sessionStartMsRef.current) / 1000);
  const sessionSecondsListening = Math.floor(
    (listeningAccumulatedMsRef.current +
      (reader.tts.state === "playing" && listeningStartedAtMsRef.current !== null
        ? sessionNowMs - listeningStartedAtMsRef.current
        : 0)) /
      1000
  );

  const estimatedReadPages = useMemo(
    () =>
      Math.min(
        reader.stats.total_pages,
        Math.max(
          reader.stats.page_index,
          Math.floor((reader.stats.global_progress_pct / 100) * reader.stats.total_pages)
        )
      ),
    [reader.stats.global_progress_pct, reader.stats.page_index, reader.stats.total_pages]
  );

  const sessionGlobalPercentFinished = useMemo(() => {
    if (estimatedTotalWords <= 0) {
      return 0;
    }
    return clamp((sessionWordsRead / estimatedTotalWords) * 100, 0, 100);
  }, [estimatedTotalWords, sessionWordsRead]);

  const listeningMinutes = sessionSecondsListening / 60;
  const sessionWordsPerMinute = listeningMinutes > 0 ? sessionWordsRead / listeningMinutes : 0;
  const sessionSentencesPerMinute =
    listeningMinutes > 0 ? sessionSentencesRead / listeningMinutes : 0;
  const sessionPercentPerMinute =
    listeningMinutes > 0 ? sessionGlobalPercentFinished / listeningMinutes : 0;

  return {
    estimatedReadPages,
    estimatedTotalSentences,
    estimatedTotalWords,
    pageFinishedPct,
    sentencesReadOnPage,
    sessionGlobalPercentFinished,
    sessionPagesFinished,
    sessionPercentPerMinute,
    sessionSecondsInApp,
    sessionSecondsListening,
    sessionSentencesPerMinute,
    sessionSentencesRead,
    sessionWordsPerMinute,
    sessionWordsRead,
    statsTab,
    setStatsTab,
    wordsReadOnPage
  };
}
