import { create, type StateCreator } from "zustand";

import { backendApi, type BackendApi } from "../api/tauri";
import type {
  BootstrapState,
  CalibreBook,
  CalibreLoadEvent,
  LogLevelEvent,
  PdfTranscriptionEvent,
  ReaderPlaybackState,
  ReaderPlaybackStateEvent,
  ReaderSettingsPatch,
  ReaderSnapshot,
  RecentBook,
  SessionState,
  SourceOpenEvent,
  TtsStateEvent
} from "../types";
import { createCalibreSliceActions } from "./slices/calibreSlice";
import { createNotificationsSliceActions } from "./slices/notificationsSlice";
import { createReaderSliceActions } from "./slices/readerSlice";
import { createSessionSliceActions } from "./slices/sessionSlice";
import { createSettingsSliceActions } from "./slices/settingsSlice";
import { createStatsSliceActions } from "./slices/statsSlice";
import type { StoreGet, StoreSet } from "./slices/types";
import { createTtsSliceActions } from "./slices/ttsSlice";

type ToastSeverity = "info" | "success" | "error";

export interface ToastMessage {
  id: number;
  severity: ToastSeverity;
  message: string;
}

export interface ActionTelemetry {
  id: number;
  action: string;
  started_at_unix_ms: number;
  duration_ms: number;
  ok: boolean;
  error: string | null;
}

export interface OperationState {
  sourceOpen: boolean;
  starterCommand: boolean;
  readerCommand: boolean;
  readerTts: boolean;
  readerSettings: boolean;
  browserTabRefresh: boolean;
  calibreLoad: boolean;
  runtimeConfig: boolean;
}

export type OperationScope = keyof OperationState;

export interface AppShellState {
  bootstrapState: BootstrapState | null;
  runtimeLogLevel: string;
  operations: OperationState;
  loadingBootstrap: boolean;
  busy: boolean;
}

export interface SessionDomainState {
  session: SessionState | null;
}

export interface ReaderDocumentState {
  snapshot: ReaderSnapshot | null;
}

export interface ReaderPlaybackDomainState {
  playback: ReaderPlaybackState | null;
  ttsStateEvent: TtsStateEvent | null;
  playbackEvent: ReaderPlaybackStateEvent | null;
}

export interface ReaderUiState {
  sourcePath: string | null;
  currentPage: number | null;
  totalPages: number | null;
  textOnlyMode: boolean;
  prettyKind: ReaderSnapshot["pretty_kind"] | null;
  searchQuery: string;
  searchMatches: number[];
  selectedSearchMatch: number | null;
  panels: ReaderSnapshot["panels"] | null;
  settings: ReaderSnapshot["settings"] | null;
}

export interface StarterState {
  recents: RecentBook[];
  calibreBooks: CalibreBook[];
  loadingRecents: boolean;
  loadingCalibre: boolean;
}

export interface JobsState {
  sourceOpenEvent: SourceOpenEvent | null;
  calibreLoadEvent: CalibreLoadEvent | null;
  pdfTranscriptionEvent: PdfTranscriptionEvent | null;
  logLevelEvent: LogLevelEvent | null;
  sourceOpenSubscribed: boolean;
  calibreSubscribed: boolean;
  ttsStateSubscribed: boolean;
  pdfTranscriptionSubscribed: boolean;
  logLevelSubscribed: boolean;
  sessionStateSubscribed: boolean;
  readerStateSubscribed: boolean;
  readerPlaybackStateSubscribed: boolean;
  lastSessionEventRequestId: number;
  lastReaderEventRequestId: number;
  lastReaderPlaybackEventRequestId: number;
  lastSourceOpenEventRequestId: number;
  lastCalibreEventRequestId: number;
  lastTtsEventRequestId: number;
  lastPdfEventRequestId: number;
  lastLogLevelEventRequestId: number;
}

export interface NotificationsState {
  error: string | null;
  toast: ToastMessage | null;
  telemetry: ActionTelemetry[];
}

export interface AppStore {
  appShell: AppShellState;
  sessionDomain: SessionDomainState;
  readerDocument: ReaderDocumentState;
  readerPlaybackDomain: ReaderPlaybackDomainState;
  readerUi: ReaderUiState;
  starter: StarterState;
  jobs: JobsState;
  notifications: NotificationsState;
  bootstrapState: BootstrapState | null;
  session: SessionState | null;
  reader: ReaderSnapshot | null;
  readerPlayback: ReaderPlaybackState | null;
  recents: RecentBook[];
  calibreBooks: CalibreBook[];
  telemetry: ActionTelemetry[];
  operations: OperationState;
  loadingBootstrap: boolean;
  loadingRecents: boolean;
  loadingCalibre: boolean;
  busy: boolean;
  error: string | null;
  toast: ToastMessage | null;
  sourceOpenEvent: SourceOpenEvent | null;
  calibreLoadEvent: CalibreLoadEvent | null;
  ttsStateEvent: TtsStateEvent | null;
  readerPlaybackStateEvent: ReaderPlaybackStateEvent | null;
  pdfTranscriptionEvent: PdfTranscriptionEvent | null;
  logLevelEvent: LogLevelEvent | null;
  runtimeLogLevel: string;
  sourceOpenSubscribed: boolean;
  calibreSubscribed: boolean;
  ttsStateSubscribed: boolean;
  pdfTranscriptionSubscribed: boolean;
  logLevelSubscribed: boolean;
  sessionStateSubscribed: boolean;
  readerStateSubscribed: boolean;
  readerPlaybackStateSubscribed: boolean;
  lastSessionEventRequestId: number;
  lastReaderEventRequestId: number;
  lastReaderPlaybackEventRequestId: number;
  lastSourceOpenEventRequestId: number;
  lastCalibreEventRequestId: number;
  lastTtsEventRequestId: number;
  lastPdfEventRequestId: number;
  lastLogLevelEventRequestId: number;
  appSafeQuit: () => Promise<void>;
  bootstrap: () => Promise<void>;
  refreshRecents: () => Promise<void>;
  openSourcePath: (path: string) => Promise<void>;
  openClipboardText: () => Promise<void>;
  openBrowserTab: (tabId: number, windowId?: number) => Promise<void>;
  refreshCurrentBrowserTab: () => Promise<void>;
  deleteRecent: (path: string, closeBrowserTab?: boolean) => Promise<void>;
  closeRecentBrowserTab: (path: string) => Promise<void>;
  returnToStarter: () => Promise<void>;
  closeReaderSession: () => Promise<void>;
  refreshReaderSnapshot: () => Promise<void>;
  readerNextPage: () => Promise<void>;
  readerPrevPage: () => Promise<void>;
  readerSetPage: (page: number) => Promise<void>;
  readerSentenceClick: (sentenceIdx: number) => Promise<void>;
  readerNextSentence: () => Promise<void>;
  readerPrevSentence: () => Promise<void>;
  readerToggleTextOnly: () => Promise<void>;
  readerApplySettings: (patch: ReaderSettingsPatch) => Promise<void>;
  readerSearchSetQuery: (query: string) => Promise<void>;
  readerSearchNext: () => Promise<void>;
  readerSearchPrev: () => Promise<void>;
  readerTtsPlay: () => Promise<void>;
  readerTtsPause: () => Promise<void>;
  readerTtsTogglePlayPause: () => Promise<void>;
  readerTtsPlayFromPageStart: () => Promise<void>;
  readerTtsPlayFromHighlight: () => Promise<void>;
  readerTtsSeekNext: () => Promise<void>;
  readerTtsSeekPrev: () => Promise<void>;
  readerTtsRepeatSentence: () => Promise<void>;
  readerTtsPrecomputePage: () => Promise<void>;
  toggleSettingsPanel: () => Promise<void>;
  toggleStatsPanel: () => Promise<void>;
  toggleTtsPanel: () => Promise<void>;
  loadCalibreBooks: (forceRefresh?: boolean) => Promise<void>;
  openCalibreBook: (bookId: number) => Promise<void>;
  setRuntimeLogLevel: (level: string) => Promise<void>;
  toggleTheme: () => Promise<void>;
  clearError: () => void;
  dismissToast: () => void;
  clearTelemetry: () => void;
}

const initialStoreState: Pick<
  AppStore,
  | "appShell"
  | "sessionDomain"
  | "readerDocument"
  | "readerPlaybackDomain"
  | "readerUi"
  | "starter"
  | "jobs"
  | "notifications"
  | "bootstrapState"
  | "session"
  | "reader"
  | "readerPlayback"
  | "recents"
  | "calibreBooks"
  | "telemetry"
  | "operations"
  | "loadingBootstrap"
  | "loadingRecents"
  | "loadingCalibre"
  | "busy"
  | "error"
  | "toast"
  | "sourceOpenEvent"
  | "calibreLoadEvent"
  | "ttsStateEvent"
  | "readerPlaybackStateEvent"
  | "pdfTranscriptionEvent"
  | "logLevelEvent"
  | "runtimeLogLevel"
  | "sourceOpenSubscribed"
  | "calibreSubscribed"
  | "ttsStateSubscribed"
  | "pdfTranscriptionSubscribed"
  | "logLevelSubscribed"
  | "sessionStateSubscribed"
  | "readerStateSubscribed"
  | "readerPlaybackStateSubscribed"
  | "lastSessionEventRequestId"
  | "lastReaderEventRequestId"
  | "lastReaderPlaybackEventRequestId"
  | "lastSourceOpenEventRequestId"
  | "lastCalibreEventRequestId"
  | "lastTtsEventRequestId"
  | "lastPdfEventRequestId"
  | "lastLogLevelEventRequestId"
> = {
  appShell: {
    bootstrapState: null,
    runtimeLogLevel: "info",
    operations: {
      sourceOpen: false,
      starterCommand: false,
      readerCommand: false,
      readerTts: false,
      readerSettings: false,
      browserTabRefresh: false,
      calibreLoad: false,
      runtimeConfig: false
    },
    loadingBootstrap: false,
    busy: false
  },
  sessionDomain: {
    session: null
  },
  readerDocument: {
    snapshot: null
  },
  readerPlaybackDomain: {
    playback: null,
    ttsStateEvent: null,
    playbackEvent: null
  },
  readerUi: {
    sourcePath: null,
    currentPage: null,
    totalPages: null,
    textOnlyMode: false,
    prettyKind: null,
    searchQuery: "",
    searchMatches: [],
    selectedSearchMatch: null,
    panels: null,
    settings: null
  },
  starter: {
    recents: [],
    calibreBooks: [],
    loadingRecents: false,
    loadingCalibre: false
  },
  jobs: {
    sourceOpenEvent: null,
    calibreLoadEvent: null,
    pdfTranscriptionEvent: null,
    logLevelEvent: null,
    sourceOpenSubscribed: false,
    calibreSubscribed: false,
    ttsStateSubscribed: false,
    pdfTranscriptionSubscribed: false,
    logLevelSubscribed: false,
    sessionStateSubscribed: false,
    readerStateSubscribed: false,
    readerPlaybackStateSubscribed: false,
    lastSessionEventRequestId: 0,
    lastReaderEventRequestId: 0,
    lastReaderPlaybackEventRequestId: 0,
    lastSourceOpenEventRequestId: 0,
    lastCalibreEventRequestId: 0,
    lastTtsEventRequestId: 0,
    lastPdfEventRequestId: 0,
    lastLogLevelEventRequestId: 0
  },
  notifications: {
    error: null,
    toast: null,
    telemetry: []
  },
  bootstrapState: null,
  session: null,
  reader: null,
  readerPlayback: null,
  recents: [],
  calibreBooks: [],
  telemetry: [],
  operations: {
    sourceOpen: false,
    starterCommand: false,
    readerCommand: false,
    readerTts: false,
    readerSettings: false,
    browserTabRefresh: false,
    calibreLoad: false,
    runtimeConfig: false
  },
  loadingBootstrap: false,
  loadingRecents: false,
  loadingCalibre: false,
  busy: false,
  error: null,
  toast: null,
  sourceOpenEvent: null,
  calibreLoadEvent: null,
  ttsStateEvent: null,
  readerPlaybackStateEvent: null,
  pdfTranscriptionEvent: null,
  logLevelEvent: null,
  runtimeLogLevel: "info",
  sourceOpenSubscribed: false,
  calibreSubscribed: false,
  ttsStateSubscribed: false,
  pdfTranscriptionSubscribed: false,
  logLevelSubscribed: false,
  sessionStateSubscribed: false,
  readerStateSubscribed: false,
  readerPlaybackStateSubscribed: false,
  lastSessionEventRequestId: 0,
  lastReaderEventRequestId: 0,
  lastReaderPlaybackEventRequestId: 0,
  lastSourceOpenEventRequestId: 0,
  lastCalibreEventRequestId: 0,
  lastTtsEventRequestId: 0,
  lastPdfEventRequestId: 0,
  lastLogLevelEventRequestId: 0
};

function deriveReaderUi(reader: ReaderSnapshot | null): ReaderUiState {
  return {
    sourcePath: reader?.source_path ?? null,
    currentPage: reader?.current_page ?? null,
    totalPages: reader?.total_pages ?? null,
    textOnlyMode: reader?.text_only_mode ?? false,
    prettyKind: reader?.pretty_kind ?? null,
    searchQuery: reader?.search_query ?? "",
    searchMatches: reader?.search_matches ?? [],
    selectedSearchMatch: reader?.selected_search_match ?? null,
    panels: reader?.panels ?? null,
    settings: reader?.settings ?? null
  };
}

function normalizeDomainPatch(current: AppStore, partial: Partial<AppStore>): Partial<AppStore> {
  const next = { ...partial };
  const has = (key: keyof AppStore) => Object.prototype.hasOwnProperty.call(partial, key);
  const nextBootstrapState = has("bootstrapState") ? partial.bootstrapState! : current.bootstrapState;
  const nextRuntimeLogLevel = has("runtimeLogLevel")
    ? partial.runtimeLogLevel!
    : current.runtimeLogLevel;
  const nextOperations = has("operations") ? partial.operations! : current.operations;
  const nextLoadingBootstrap = has("loadingBootstrap")
    ? partial.loadingBootstrap!
    : current.loadingBootstrap;
  const nextBusy = has("busy") ? partial.busy! : current.busy;
  if (
    has("bootstrapState") ||
    has("runtimeLogLevel") ||
    has("operations") ||
    has("loadingBootstrap") ||
    has("busy")
  ) {
    next.appShell = {
      bootstrapState: nextBootstrapState,
      runtimeLogLevel: nextRuntimeLogLevel,
      operations: nextOperations,
      loadingBootstrap: nextLoadingBootstrap,
      busy: nextBusy
    };
  }

  const nextSession = has("session") ? partial.session! : current.session;
  if (has("session")) {
    next.sessionDomain = { session: nextSession };
  }

  const nextReader = has("reader") ? partial.reader! : current.reader;
  if (has("reader")) {
    next.readerDocument = { snapshot: nextReader };
    next.readerUi = deriveReaderUi(nextReader);
  }

  const nextReaderPlayback = has("readerPlayback") ? partial.readerPlayback! : current.readerPlayback;
  const nextTtsStateEvent = has("ttsStateEvent") ? partial.ttsStateEvent! : current.ttsStateEvent;
  const nextReaderPlaybackStateEvent = has("readerPlaybackStateEvent")
    ? partial.readerPlaybackStateEvent!
    : current.readerPlaybackStateEvent;
  if (has("readerPlayback") || has("ttsStateEvent") || has("readerPlaybackStateEvent")) {
    next.readerPlaybackDomain = {
      playback: nextReaderPlayback,
      ttsStateEvent: nextTtsStateEvent,
      playbackEvent: nextReaderPlaybackStateEvent
    };
  }

  const nextRecents = has("recents") ? partial.recents! : current.recents;
  const nextCalibreBooks = has("calibreBooks") ? partial.calibreBooks! : current.calibreBooks;
  const nextLoadingRecents = has("loadingRecents") ? partial.loadingRecents! : current.loadingRecents;
  const nextLoadingCalibre = has("loadingCalibre")
    ? partial.loadingCalibre!
    : current.loadingCalibre;
  if (has("recents") || has("calibreBooks") || has("loadingRecents") || has("loadingCalibre")) {
    next.starter = {
      recents: nextRecents,
      calibreBooks: nextCalibreBooks,
      loadingRecents: nextLoadingRecents,
      loadingCalibre: nextLoadingCalibre
    };
  }

  if (
    has("sourceOpenEvent") ||
    has("calibreLoadEvent") ||
    has("pdfTranscriptionEvent") ||
    has("logLevelEvent") ||
    has("sourceOpenSubscribed") ||
    has("calibreSubscribed") ||
    has("ttsStateSubscribed") ||
    has("pdfTranscriptionSubscribed") ||
    has("logLevelSubscribed") ||
    has("sessionStateSubscribed") ||
    has("readerStateSubscribed") ||
    has("readerPlaybackStateSubscribed") ||
    has("lastSessionEventRequestId") ||
    has("lastReaderEventRequestId") ||
    has("lastReaderPlaybackEventRequestId") ||
    has("lastSourceOpenEventRequestId") ||
    has("lastCalibreEventRequestId") ||
    has("lastTtsEventRequestId") ||
    has("lastPdfEventRequestId") ||
    has("lastLogLevelEventRequestId")
  ) {
    next.jobs = {
      sourceOpenEvent: has("sourceOpenEvent") ? partial.sourceOpenEvent! : current.sourceOpenEvent,
      calibreLoadEvent: has("calibreLoadEvent")
        ? partial.calibreLoadEvent!
        : current.calibreLoadEvent,
      pdfTranscriptionEvent: has("pdfTranscriptionEvent")
        ? partial.pdfTranscriptionEvent!
        : current.pdfTranscriptionEvent,
      logLevelEvent: has("logLevelEvent") ? partial.logLevelEvent! : current.logLevelEvent,
      sourceOpenSubscribed: has("sourceOpenSubscribed")
        ? partial.sourceOpenSubscribed!
        : current.sourceOpenSubscribed,
      calibreSubscribed: has("calibreSubscribed")
        ? partial.calibreSubscribed!
        : current.calibreSubscribed,
      ttsStateSubscribed: has("ttsStateSubscribed")
        ? partial.ttsStateSubscribed!
        : current.ttsStateSubscribed,
      pdfTranscriptionSubscribed: has("pdfTranscriptionSubscribed")
        ? partial.pdfTranscriptionSubscribed!
        : current.pdfTranscriptionSubscribed,
      logLevelSubscribed: has("logLevelSubscribed")
        ? partial.logLevelSubscribed!
        : current.logLevelSubscribed,
      sessionStateSubscribed: has("sessionStateSubscribed")
        ? partial.sessionStateSubscribed!
        : current.sessionStateSubscribed,
      readerStateSubscribed: has("readerStateSubscribed")
        ? partial.readerStateSubscribed!
        : current.readerStateSubscribed,
      readerPlaybackStateSubscribed: has("readerPlaybackStateSubscribed")
        ? partial.readerPlaybackStateSubscribed!
        : current.readerPlaybackStateSubscribed,
      lastSessionEventRequestId: has("lastSessionEventRequestId")
        ? partial.lastSessionEventRequestId!
        : current.lastSessionEventRequestId,
      lastReaderEventRequestId: has("lastReaderEventRequestId")
        ? partial.lastReaderEventRequestId!
        : current.lastReaderEventRequestId,
      lastReaderPlaybackEventRequestId: has("lastReaderPlaybackEventRequestId")
        ? partial.lastReaderPlaybackEventRequestId!
        : current.lastReaderPlaybackEventRequestId,
      lastSourceOpenEventRequestId: has("lastSourceOpenEventRequestId")
        ? partial.lastSourceOpenEventRequestId!
        : current.lastSourceOpenEventRequestId,
      lastCalibreEventRequestId: has("lastCalibreEventRequestId")
        ? partial.lastCalibreEventRequestId!
        : current.lastCalibreEventRequestId,
      lastTtsEventRequestId: has("lastTtsEventRequestId")
        ? partial.lastTtsEventRequestId!
        : current.lastTtsEventRequestId,
      lastPdfEventRequestId: has("lastPdfEventRequestId")
        ? partial.lastPdfEventRequestId!
        : current.lastPdfEventRequestId,
      lastLogLevelEventRequestId: has("lastLogLevelEventRequestId")
        ? partial.lastLogLevelEventRequestId!
        : current.lastLogLevelEventRequestId
    };
  }

  if (has("error") || has("toast") || has("telemetry")) {
    next.notifications = {
      error: has("error") ? partial.error! : current.error,
      toast: has("toast") ? partial.toast! : current.toast,
      telemetry: has("telemetry") ? partial.telemetry! : current.telemetry
    };
  }

  return next;
}

export function createAppStoreState(backend: BackendApi): StateCreator<AppStore> {
  return (set, get) => {
    const setWithDomains: StoreSet = (partial) => {
      const resolved =
        typeof partial === "function" ? partial(get()) : (partial as Partial<AppStore>);
      const normalized = normalizeDomainPatch(get(), resolved);
      set(normalized as Partial<AppStore>);
    };
    const context = {
      set: setWithDomains,
      get: get as StoreGet,
      backend
    };

    return {
      ...initialStoreState,
      ...createSessionSliceActions(context),
      ...createReaderSliceActions(context),
      ...createTtsSliceActions(context),
      ...createSettingsSliceActions(context),
      ...createCalibreSliceActions(context),
      ...createNotificationsSliceActions(context),
      ...createStatsSliceActions(context)
    };
  };
}

export const useAppStore = create<AppStore>(createAppStoreState(backendApi));
