import type {
  CalibreLoadEvent,
  LogLevelEvent,
  PdfTranscriptionEvent,
  ReaderPlaybackStateEvent,
  ReaderStateEvent,
  SessionStateEvent,
  SourceOpenEvent,
  TtsStateEvent
} from "../../types";
import type { AppStore } from "../appStore";
import { buildToast, toReaderPlaybackState } from "./shared";

function samePanels(
  left: AppStore["session"] extends infer T
    ? T extends { panels: infer P }
      ? P
      : never
    : never,
  right: AppStore["session"] extends infer T
    ? T extends { panels: infer P }
      ? P
      : never
    : never
): boolean {
  return (
    left.show_settings === right.show_settings &&
    left.show_stats === right.show_stats &&
    left.show_tts === right.show_tts
  );
}

function sameSessionState(left: AppStore["session"], right: AppStore["session"]): boolean {
  if (!left || !right) {
    return left === right;
  }
  return (
    left.mode === right.mode &&
    left.active_source_path === right.active_source_path &&
    left.open_in_flight === right.open_in_flight &&
    samePanels(left.panels, right.panels)
  );
}

function sameSourceOpenEvent(left: AppStore["sourceOpenEvent"], right: SourceOpenEvent): boolean {
  if (!left) {
    return false;
  }
  return (
    left.phase === right.phase &&
    left.source_path === right.source_path &&
    left.message === right.message
  );
}

function sameCalibreEvent(left: AppStore["calibreLoadEvent"], right: CalibreLoadEvent): boolean {
  if (!left) {
    return false;
  }
  return left.phase === right.phase && left.count === right.count && left.message === right.message;
}

function sameTtsEvent(left: AppStore["ttsStateEvent"], right: TtsStateEvent): boolean {
  if (!left) {
    return false;
  }
  return (
    left.action === right.action &&
    left.tts.state === right.tts.state &&
    left.tts.current_sentence_idx === right.tts.current_sentence_idx &&
    left.tts.sentence_count === right.tts.sentence_count &&
    left.tts.can_seek_prev === right.tts.can_seek_prev &&
    left.tts.can_seek_next === right.tts.can_seek_next &&
    left.tts.progress_pct === right.tts.progress_pct
  );
}

function samePdfEvent(
  left: AppStore["pdfTranscriptionEvent"],
  right: PdfTranscriptionEvent
): boolean {
  if (!left) {
    return false;
  }
  return (
    left.phase === right.phase &&
    left.source_path === right.source_path &&
    left.message === right.message
  );
}

function sameLogLevelEvent(left: AppStore["logLevelEvent"], right: LogLevelEvent): boolean {
  if (!left) {
    return false;
  }
  return left.level === right.level;
}

function sameReaderSnapshot(
  left: AppStore["reader"],
  right: ReaderStateEvent["reader"]
): boolean {
  if (!left) {
    return false;
  }
  const leftSentences = left.sentences ?? [];
  const rightSentences = right.sentences ?? [];
  const leftSearchMatches = left.search_matches ?? [];
  const rightSearchMatches = right.search_matches ?? [];
  const leftSentenceAnchorMap = left.sentence_anchor_map ?? [];
  const rightSentenceAnchorMap = right.sentence_anchor_map ?? [];
  return (
    left.source_path === right.source_path &&
    left.current_page === right.current_page &&
    left.total_pages === right.total_pages &&
    left.text_only_mode === right.text_only_mode &&
    left.pretty_kind === right.pretty_kind &&
    left.reading_markdown_page === right.reading_markdown_page &&
    left.reading_html_page === right.reading_html_page &&
    left.page_text === right.page_text &&
    left.search_query === right.search_query &&
    left.selected_search_match === right.selected_search_match &&
    left.highlighted_sentence_idx === right.highlighted_sentence_idx &&
    left.tts.state === right.tts.state &&
    left.tts.current_sentence_idx === right.tts.current_sentence_idx &&
    left.tts.sentence_count === right.tts.sentence_count &&
    left.tts.can_seek_prev === right.tts.can_seek_prev &&
    left.tts.can_seek_next === right.tts.can_seek_next &&
    left.tts.progress_pct === right.tts.progress_pct &&
    left.panels.show_settings === right.panels.show_settings &&
    left.panels.show_stats === right.panels.show_stats &&
    left.panels.show_tts === right.panels.show_tts &&
    leftSentences.length === rightSentences.length &&
    leftSearchMatches.length === rightSearchMatches.length &&
    leftSentenceAnchorMap.length === rightSentenceAnchorMap.length &&
    leftSentences.every((value, idx) => value === rightSentences[idx]) &&
    leftSearchMatches.every((value, idx) => value === rightSearchMatches[idx]) &&
    leftSentenceAnchorMap.every((value, idx) => value === rightSentenceAnchorMap[idx])
  );
}

function sameReaderPlaybackState(
  left: AppStore["readerPlayback"],
  right: ReaderPlaybackStateEvent["playback"]
): boolean {
  if (!left) {
    return false;
  }
  return (
    left.source_path === right.source_path &&
    left.current_page === right.current_page &&
    left.highlighted_sentence_idx === right.highlighted_sentence_idx &&
    left.tts.state === right.tts.state &&
    left.tts.current_sentence_idx === right.tts.current_sentence_idx &&
    left.tts.sentence_count === right.tts.sentence_count &&
    left.tts.can_seek_prev === right.tts.can_seek_prev &&
    left.tts.can_seek_next === right.tts.can_seek_next &&
    left.tts.progress_pct === right.tts.progress_pct &&
    left.stats.page_index === right.stats.page_index &&
    left.stats.total_pages === right.stats.total_pages &&
    left.stats.tts_progress_pct === right.stats.tts_progress_pct &&
    left.stats.global_progress_pct === right.stats.global_progress_pct &&
    left.stats.page_time_remaining_secs === right.stats.page_time_remaining_secs &&
    left.stats.book_time_remaining_secs === right.stats.book_time_remaining_secs
  );
}

export function reduceSourceOpenEvent(
  current: AppStore,
  event: SourceOpenEvent
): Partial<AppStore> {
  if (event.request_id < current.lastSourceOpenEventRequestId) {
    return {};
  }
  const next: Partial<AppStore> = {
    lastSourceOpenEventRequestId: event.request_id
  };
  if (!sameSourceOpenEvent(current.sourceOpenEvent, event)) {
    next.sourceOpenEvent = event;
  }
  if (event.phase === "cancelled") {
    const suffix = event.request_id > 0 ? ` (request ${event.request_id})` : "";
    next.toast = buildToast("info", `${event.message ?? "Source open cancelled"}${suffix}`);
  } else if (event.phase === "failed") {
    const suffix = event.request_id > 0 ? ` (request ${event.request_id})` : "";
    next.toast = buildToast("error", `${event.message ?? "Source open failed"}${suffix}`);
  }
  return next;
}

export function reduceCalibreLoadEvent(
  current: AppStore,
  event: CalibreLoadEvent
): Partial<AppStore> {
  if (event.request_id < current.lastCalibreEventRequestId) {
    return {};
  }
  const next: Partial<AppStore> = {
    lastCalibreEventRequestId: event.request_id
  };
  if (!sameCalibreEvent(current.calibreLoadEvent, event)) {
    next.calibreLoadEvent = event;
  }
  if (event.phase === "failed") {
    const suffix = event.request_id > 0 ? ` (request ${event.request_id})` : "";
    next.toast = buildToast("error", `${event.message ?? "Calibre load failed"}${suffix}`);
  }
  return next;
}

export function reduceTtsStateEvent(current: AppStore, event: TtsStateEvent): Partial<AppStore> {
  if (event.request_id < current.lastTtsEventRequestId) {
    return {};
  }
  const next: Partial<AppStore> = {
    lastTtsEventRequestId: event.request_id
  };
  if (!sameTtsEvent(current.ttsStateEvent, event)) {
    next.ttsStateEvent = event;
  }
  return next;
}

export function reducePdfTranscriptionEvent(
  current: AppStore,
  event: PdfTranscriptionEvent
): Partial<AppStore> {
  if (event.request_id < current.lastPdfEventRequestId) {
    return {};
  }
  const next: Partial<AppStore> = {
    lastPdfEventRequestId: event.request_id
  };
  if (!samePdfEvent(current.pdfTranscriptionEvent, event)) {
    next.pdfTranscriptionEvent = event;
  }
  if (event.phase === "failed") {
    const suffix = event.request_id > 0 ? ` (request ${event.request_id})` : "";
    next.toast = buildToast("error", `${event.message ?? "PDF transcription failed"}${suffix}`);
  }
  return next;
}

export function reduceLogLevelEvent(current: AppStore, event: LogLevelEvent): Partial<AppStore> {
  if (event.request_id < current.lastLogLevelEventRequestId) {
    return {};
  }
  const next: Partial<AppStore> = {
    runtimeLogLevel: event.level,
    lastLogLevelEventRequestId: event.request_id
  };
  if (!sameLogLevelEvent(current.logLevelEvent, event)) {
    next.logLevelEvent = event;
  }
  return next;
}

export function reduceSessionStateEvent(
  current: AppStore,
  event: SessionStateEvent
): Partial<AppStore> {
  if (event.request_id < current.lastSessionEventRequestId) {
    return {};
  }
  const next: Partial<AppStore> = {
    lastSessionEventRequestId: event.request_id
  };
  if (!sameSessionState(current.session, event.session)) {
    next.session = event.session;
  }
  if (event.session.mode !== "reader") {
    if (current.reader !== null) {
      next.reader = null;
    }
    next.lastReaderEventRequestId = Math.max(current.lastReaderEventRequestId, event.request_id);
  }
  return next;
}

export function reduceReaderStateEvent(
  current: AppStore,
  event: ReaderStateEvent
): Partial<AppStore> {
  if (event.request_id < current.lastReaderEventRequestId) {
    return {};
  }
  const nextSession = current.session
    ? {
        ...current.session,
        mode: "reader" as const,
        active_source_path: event.reader.source_path,
        open_in_flight: false,
        panels: event.reader.panels
      }
    : {
        mode: "reader" as const,
        active_source_path: event.reader.source_path,
        open_in_flight: false,
        panels: event.reader.panels
      };

  const next: Partial<AppStore> = {
    lastReaderEventRequestId: event.request_id,
    lastSessionEventRequestId: Math.max(current.lastSessionEventRequestId, event.request_id)
  };
  if (!sameReaderSnapshot(current.reader, event.reader)) {
    next.reader = event.reader;
  }
  const nextPlayback = toReaderPlaybackState(event.reader);
  if (!sameReaderPlaybackState(current.readerPlayback, nextPlayback!)) {
    next.readerPlayback = nextPlayback;
  }
  if (!sameSessionState(current.session, nextSession)) {
    next.session = nextSession;
  }
  return next;
}

export function reduceReaderPlaybackStateEvent(
  current: AppStore,
  event: ReaderPlaybackStateEvent
): Partial<AppStore> {
  if (event.request_id < current.lastReaderPlaybackEventRequestId) {
    return {};
  }
  const next: Partial<AppStore> = {
    lastReaderPlaybackEventRequestId: event.request_id
  };
  if (!sameReaderPlaybackState(current.readerPlayback, event.playback)) {
    next.readerPlayback = event.playback;
  }
  if (current.readerPlaybackStateEvent !== event) {
    next.readerPlaybackStateEvent = event;
  }
  return next;
}
