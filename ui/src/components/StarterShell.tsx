import DarkModeOutlinedIcon from "@mui/icons-material/DarkModeOutlined";
import DeleteOutlineIcon from "@mui/icons-material/DeleteOutline";
import FolderOpenIcon from "@mui/icons-material/FolderOpen";
import LightModeOutlinedIcon from "@mui/icons-material/LightModeOutlined";
import RefreshIcon from "@mui/icons-material/Refresh";
import {
  Divider,
  Stack,
} from "@mui/material";
import { useEffect, useMemo, useState } from "react";

import {
  filterAndSortCalibreBooks,
  type CalibreSort
} from "./calibreList";
import { useRenderDebugCounter } from "../perf/debug";
import { computeVirtualWindow } from "./starterVirtualList";
import { useBrowserTabs } from "./useBrowserTabs";
import { useCalibreThumbnails } from "./useCalibreThumbnails";
import {
  StarterBrowserTabsPanel,
  StarterCalibrePanel,
  StarterOpenPanel,
  StarterRecentsPanel
} from "./starterPanels";
import type {
  BootstrapState,
  CalibreBook,
  CalibreLoadEvent,
  PdfTranscriptionEvent,
  RecentBook,
  SourceOpenEvent
} from "../types";

interface StarterShellProps {
  bootstrap: BootstrapState | null;
  recents: RecentBook[];
  calibreBooks: CalibreBook[];
  busy: boolean;
  loadingRecents: boolean;
  loadingCalibre: boolean;
  onOpenPath: (path: string) => Promise<void>;
  onOpenClipboardText: () => Promise<void>;
  onOpenBrowserTab: (tabId: number, windowId?: number) => Promise<void>;
  onOpenBrowserTabBundle: (tabId: number, windowId?: number) => Promise<void>;
  onDeleteRecent: (path: string, closeBrowserTab?: boolean) => Promise<void>;
  onCloseRecentBrowserTab: (path: string) => Promise<void>;
  onRefreshRecents: () => Promise<void>;
  onLoadCalibre: (forceRefresh?: boolean) => Promise<void>;
  onOpenCalibreBook: (bookId: number) => Promise<void>;
  onSetRuntimeLogLevel: (level: string) => Promise<void>;
  onToggleTheme: () => Promise<void>;
  sourceOpenEvent: SourceOpenEvent | null;
  calibreLoadEvent: CalibreLoadEvent | null;
  pdfTranscriptionEvent: PdfTranscriptionEvent | null;
  runtimeLogLevel: string;
}

export function StarterShell({
  bootstrap,
  recents,
  calibreBooks,
  busy,
  loadingRecents,
  loadingCalibre,
  onOpenPath,
  onOpenClipboardText,
  onOpenBrowserTab,
  onOpenBrowserTabBundle,
  onDeleteRecent,
  onCloseRecentBrowserTab,
  onRefreshRecents,
  onLoadCalibre,
  onOpenCalibreBook,
  onSetRuntimeLogLevel,
  onToggleTheme,
  sourceOpenEvent,
  calibreLoadEvent,
  pdfTranscriptionEvent,
  runtimeLogLevel
}: StarterShellProps) {
  useRenderDebugCounter("StarterShell");
  const [path, setPath] = useState("");
  const [clipboardError, setClipboardError] = useState<string | null>(null);
  const [calibreSearch, setCalibreSearch] = useState("");
  const [recentsSearch, setRecentsSearch] = useState("");
  const [showCalibre, setShowCalibre] = useState(true);
  const [calibreSort, setCalibreSort] = useState<CalibreSort>("title_asc");
  const [recentsSort, setRecentsSort] = useState<"recent_first" | "recent_last" | "title_asc" | "title_desc" | "path_asc" | "path_desc">("recent_first");
  const [recentsScrollTop, setRecentsScrollTop] = useState(0);
  const [calibreScrollTop, setCalibreScrollTop] = useState(0);
  const [logLevelValue, setLogLevelValue] = useState(runtimeLogLevel);
  const {
    browserHealth,
    browserHealthError,
    browserTabSearch,
    browserTabsError,
    browserTabsLoading,
    browserTabsRowHeight,
    browserTabsViewportHeight,
    browserTabsVirtualWindow,
    browserWindows,
    loadBrowserTabs,
    selectedBrowserWindowId,
    setBrowserTabSearch,
    setBrowserTabsScrollTop,
    setSelectedBrowserWindowId,
    visibleBrowserTabs,
    windowRef: browserTabsWindowRef
  } = useBrowserTabs();

  const recentsRowHeight = 132;
  const recentsOverscan = 8;
  const calibreRowHeight = 58;
  const calibreViewportHeight = 384;
  const calibreOverscan = 10;
  const recentsViewportHeight = 384;
  const currentTheme = bootstrap?.config.theme ?? "day";
  const themeToggleLabel = currentTheme === "night" ? "Switch to Day" : "Switch to Night";

  const filteredCalibre = useMemo(() => {
    return filterAndSortCalibreBooks(calibreBooks, calibreSearch, calibreSort);
  }, [calibreBooks, calibreSearch, calibreSort]);

  const filteredRecents = useMemo(() => {
    const needle = recentsSearch.trim().toLowerCase();
    const matches = needle.length === 0
      ? recents
      : recents.filter((recent) => {
          const title = recent.display_title.toLowerCase();
          const snippet = recent.snippet.toLowerCase();
          return title.includes(needle) || snippet.includes(needle);
        });

    const sorted = [...matches];
    sorted.sort((a, b) => {
      if (recentsSort === "recent_first") {
        return b.last_opened_unix_secs - a.last_opened_unix_secs;
      }
      if (recentsSort === "recent_last") {
        return a.last_opened_unix_secs - b.last_opened_unix_secs;
      }
      if (recentsSort === "title_asc") {
        return a.display_title.localeCompare(b.display_title);
      }
      if (recentsSort === "title_desc") {
        return b.display_title.localeCompare(a.display_title);
      }
      if (recentsSort === "path_asc") {
        return a.source_path.localeCompare(b.source_path);
      }
      return b.source_path.localeCompare(a.source_path);
    });
    return sorted;
  }, [recents, recentsSearch, recentsSort]);

  const virtualWindow = useMemo(() => {
    return computeVirtualWindow(
      filteredCalibre,
      calibreScrollTop,
      calibreRowHeight,
      calibreViewportHeight,
      calibreOverscan
    );
  }, [
    calibreOverscan,
    calibreRowHeight,
    calibreScrollTop,
    calibreViewportHeight,
    filteredCalibre
  ]);

  const recentsVirtualWindow = useMemo(() => {
    return computeVirtualWindow(
      filteredRecents,
      recentsScrollTop,
      recentsRowHeight,
      recentsViewportHeight,
      recentsOverscan
    );
  }, [
    filteredRecents,
    recentsOverscan,
    recentsRowHeight,
    recentsScrollTop,
    recentsViewportHeight
  ]);

  useEffect(() => {
    setCalibreScrollTop(0);
  }, [calibreSearch, calibreSort, showCalibre]);

  useEffect(() => {
    setRecentsScrollTop(0);
  }, [recentsSearch, recentsSort]);

  useEffect(() => {
    setLogLevelValue(runtimeLogLevel);
  }, [runtimeLogLevel]);
  const calibreThumbOverrides = useCalibreThumbnails(virtualWindow.items);

  const handleOpenPath = async () => {
    await onOpenPath(path);
  };

  const handleClipboardOpen = async () => {
    setClipboardError(null);
    try {
      await onOpenClipboardText();
    } catch (error) {
      const message = error instanceof Error
        ? `[starter-open-clipboard] ${error.message}`
        : `[starter-open-clipboard] ${String(error)}`;
      setClipboardError(message);
    }
  };

  const hasRecents = recents.length > 0;
  const hasFilteredRecents = filteredRecents.length > 0;
  const sourceOpenStatus =
    sourceOpenEvent && sourceOpenEvent.phase !== "ready"
      ? `Open #${sourceOpenEvent.request_id}: ${sourceOpenEvent.phase}${
          sourceOpenEvent.source_path ? ` · ${sourceOpenEvent.source_path}` : ""
        }${sourceOpenEvent.message ? ` · ${sourceOpenEvent.message}` : ""}`
      : null;
  const calibreStatus =
    calibreLoadEvent && calibreLoadEvent.phase !== "ready"
      ? `Calibre #${calibreLoadEvent.request_id}: ${calibreLoadEvent.phase}${
          calibreLoadEvent.count !== null ? ` · ${calibreLoadEvent.count.toLocaleString()} books` : ""
        }${calibreLoadEvent.message ? ` · ${calibreLoadEvent.message}` : ""}`
      : null;
  const pdfStatus =
    pdfTranscriptionEvent && pdfTranscriptionEvent.phase !== "ready"
      ? `PDF #${pdfTranscriptionEvent.request_id}: ${pdfTranscriptionEvent.phase}${
          pdfTranscriptionEvent.source_path ? ` · ${pdfTranscriptionEvent.source_path}` : ""
        }${pdfTranscriptionEvent.message ? ` · ${pdfTranscriptionEvent.message}` : ""}`
      : null;

  return (
    <div className="w-full max-w-7xl">
      <Stack spacing={2.5}>
        <div
          style={{
            contentVisibility: "auto",
            containIntrinsicSize: "720px",
            contain: "layout paint style"
          }}
        >
          <Stack spacing={2.5}>
            <StarterOpenPanel
              busy={busy}
              calibreStatus={calibreStatus}
              clipboardError={clipboardError}
              currentTheme={currentTheme}
              handleClipboardOpen={handleClipboardOpen}
              handleOpenPath={handleOpenPath}
              logLevelValue={logLevelValue}
              onSetRuntimeLogLevel={onSetRuntimeLogLevel}
              onToggleTheme={onToggleTheme}
              path={path}
              pdfStatus={pdfStatus}
              runtimeLogLevel={runtimeLogLevel}
              setLogLevelValue={setLogLevelValue}
              setPath={setPath}
              sourceOpenStatus={sourceOpenStatus}
              themeToggleLabel={themeToggleLabel}
            />

            <StarterBrowserTabsPanel
              browserHealth={browserHealth}
              browserHealthError={browserHealthError}
              browserTabSearch={browserTabSearch}
              browserTabsError={browserTabsError}
              browserTabsLoading={browserTabsLoading}
              browserTabsRowHeight={browserTabsRowHeight}
              browserTabsViewportHeight={browserTabsViewportHeight}
              browserTabsVirtualWindow={browserTabsVirtualWindow}
              browserWindows={browserWindows}
              busy={busy}
              loadBrowserTabs={loadBrowserTabs}
              onOpenBrowserTab={onOpenBrowserTab}
              onOpenBrowserTabBundle={onOpenBrowserTabBundle}
              selectedBrowserWindowId={selectedBrowserWindowId}
              setBrowserTabSearch={setBrowserTabSearch}
              setBrowserTabsScrollTop={setBrowserTabsScrollTop}
              setSelectedBrowserWindowId={setSelectedBrowserWindowId}
              visibleBrowserTabs={visibleBrowserTabs}
              windowRef={browserTabsWindowRef}
            />
          </Stack>
        </div>

        <Divider />

        <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
          <div
            style={{
              contentVisibility: "auto",
              containIntrinsicSize: "900px",
              contain: "layout paint style"
            }}
          >
            <StarterRecentsPanel
              busy={busy}
              filteredRecents={filteredRecents}
              hasFilteredRecents={hasFilteredRecents}
              hasRecents={hasRecents}
              loadingRecents={loadingRecents}
              defaultCloseBrowserTabOnDelete={
                bootstrap?.config.close_browser_tab_on_recent_delete ?? true
              }
              onCloseRecentBrowserTab={onCloseRecentBrowserTab}
              onDeleteRecent={onDeleteRecent}
              onOpenPath={onOpenPath}
              onRefreshRecents={onRefreshRecents}
              recents={recents}
              recentsScrollTop={recentsScrollTop}
              recentsSearch={recentsSearch}
              recentsSort={recentsSort}
              recentsViewportHeight={recentsViewportHeight}
              recentsVirtualWindow={recentsVirtualWindow}
              setRecentsScrollTop={setRecentsScrollTop}
              setRecentsSearch={setRecentsSearch}
              setRecentsSort={setRecentsSort}
            />
          </div>

          <div
            style={{
              contentVisibility: "auto",
              containIntrinsicSize: "900px",
              contain: "layout paint style"
            }}
          >
            <StarterCalibrePanel
              busy={busy}
              calibreSearch={calibreSearch}
              calibreSort={calibreSort}
              calibreThumbOverrides={calibreThumbOverrides}
              calibreViewportHeight={calibreViewportHeight}
              filteredCalibre={filteredCalibre}
              loadingCalibre={loadingCalibre}
              onLoadCalibre={onLoadCalibre}
              onOpenCalibreBook={onOpenCalibreBook}
              setCalibreScrollTop={setCalibreScrollTop}
              setCalibreSearch={setCalibreSearch}
              setCalibreSort={setCalibreSort}
              setShowCalibre={setShowCalibre}
              showCalibre={showCalibre}
              virtualWindow={virtualWindow}
            />
          </div>
        </div>
      </Stack>
    </div>
  );
}
