import { create, type StateCreator } from "zustand";

import { backendApi, type BackendApi } from "../api/tauri";
import type {
  BootstrapState,
  CalibreBook,
  CalibreLoadEvent,
  LogLevelEvent,
  PdfTranscriptionEvent,
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

export interface AppStore {
  bootstrapState: BootstrapState | null;
  session: SessionState | null;
  reader: ReaderSnapshot | null;
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
  lastSessionEventRequestId: number;
  lastReaderEventRequestId: number;
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
  deleteRecent: (path: string) => Promise<void>;
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
  | "bootstrapState"
  | "session"
  | "reader"
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
  | "lastSessionEventRequestId"
  | "lastReaderEventRequestId"
  | "lastSourceOpenEventRequestId"
  | "lastCalibreEventRequestId"
  | "lastTtsEventRequestId"
  | "lastPdfEventRequestId"
  | "lastLogLevelEventRequestId"
> = {
  bootstrapState: null,
  session: null,
  reader: null,
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
  lastSessionEventRequestId: 0,
  lastReaderEventRequestId: 0,
  lastSourceOpenEventRequestId: 0,
  lastCalibreEventRequestId: 0,
  lastTtsEventRequestId: 0,
  lastPdfEventRequestId: 0,
  lastLogLevelEventRequestId: 0
};

export function createAppStoreState(backend: BackendApi): StateCreator<AppStore> {
  return (set, get) => {
    const context = {
      set: set as StoreSet,
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
