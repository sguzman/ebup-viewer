import DarkModeOutlinedIcon from "@mui/icons-material/DarkModeOutlined";
import DeleteOutlineIcon from "@mui/icons-material/DeleteOutline";
import FolderOpenIcon from "@mui/icons-material/FolderOpen";
import LightModeOutlinedIcon from "@mui/icons-material/LightModeOutlined";
import RefreshIcon from "@mui/icons-material/Refresh";
import {
  Alert,
  Button,
  Card,
  CardContent,
  CircularProgress,
  Divider,
  FormControl,
  InputLabel,
  MenuItem,
  Select,
  Stack,
  TextField,
  Typography
} from "@mui/material";
import { useEffect, useMemo, useState } from "react";

import {
  computeVirtualWindow,
  filterAndSortCalibreBooks,
  type CalibreSort
} from "./calibreList";
import { useRenderDebugCounter } from "../perf/debug";
import { toThumbnailSrc } from "./starterShared";
import { useBrowserTabs } from "./useBrowserTabs";
import { useCalibreThumbnails } from "./useCalibreThumbnails";
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
  onDeleteRecent: (path: string) => Promise<void>;
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
  onDeleteRecent,
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
          <Card className="rounded-3xl border border-slate-200 shadow-sm">
            <CardContent>
              <Stack spacing={2.5}>
          <Stack direction={{ xs: "column", md: "row" }} spacing={1} alignItems={{ xs: "stretch", md: "center" }}>
            <Typography variant="caption" color="text.secondary">
              Runtime log level:{" "}
              <span data-testid="starter-runtime-log-level-value">{runtimeLogLevel}</span>
            </Typography>
            <FormControl size="small" className="md:min-w-44">
              <InputLabel id="runtime-log-level-label">Log Level</InputLabel>
              <Select
                labelId="runtime-log-level-label"
                label="Log Level"
                value={logLevelValue}
                onChange={(event) => setLogLevelValue(String(event.target.value))}
                disabled={busy}
                data-testid="starter-log-level-select"
              >
                <MenuItem value="trace">trace</MenuItem>
                <MenuItem value="debug">debug</MenuItem>
                <MenuItem value="info">info</MenuItem>
                <MenuItem value="warn">warn</MenuItem>
                <MenuItem value="error">error</MenuItem>
              </Select>
            </FormControl>
            <Button
              size="small"
              variant="outlined"
              onClick={() => void onSetRuntimeLogLevel(logLevelValue)}
              disabled={busy || runtimeLogLevel === logLevelValue}
              data-testid="starter-log-level-apply-button"
            >
              Apply Log Level
            </Button>
          </Stack>
          {sourceOpenStatus ? (
            <Typography variant="caption" color="text.secondary" data-testid="starter-open-status">
              {sourceOpenStatus}
            </Typography>
          ) : null}
          {calibreStatus ? (
            <Typography
              variant="caption"
              color="text.secondary"
              data-testid="starter-calibre-status"
            >
              {calibreStatus}
            </Typography>
          ) : null}
          {pdfStatus ? (
            <Typography variant="caption" color="text.secondary" data-testid="starter-pdf-status">
              {pdfStatus}
            </Typography>
          ) : null}

          <Stack direction={{ xs: "column", md: "row" }} spacing={1.5}>
            <TextField
              fullWidth
              size="small"
              label="Open Path (.epub/.pdf/.txt/.md/.markdown/.html/.doc/.docx)"
              value={path}
              inputProps={{ "data-testid": "starter-open-path-input" }}
              onChange={(event) => setPath(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  void handleOpenPath();
                }
              }}
              disabled={busy}
            />
            <Button
              variant="contained"
              startIcon={<FolderOpenIcon />}
              onClick={() => void handleOpenPath()}
              disabled={busy}
              data-testid="starter-open-path-button"
            >
              Open
            </Button>
            <Button
              variant="outlined"
              onClick={() => void handleClipboardOpen()}
              disabled={busy}
              data-testid="starter-open-clipboard-button"
            >
              Open Clipboard
            </Button>
            <Button
              variant="outlined"
              startIcon={
                currentTheme === "night" ? <LightModeOutlinedIcon /> : <DarkModeOutlinedIcon />
              }
              onClick={() => void onToggleTheme()}
              disabled={busy}
              data-testid="starter-theme-toggle-button"
            >
              {themeToggleLabel}
            </Button>
          </Stack>

          {clipboardError ? (
            <Typography variant="caption" color="error">
              {clipboardError}
            </Typography>
          ) : null}

          <Card variant="outlined">
            <CardContent>
              <Stack spacing={1.5}>
                <Stack
                  direction={{ xs: "column", md: "row" }}
                  spacing={1}
                  alignItems={{ xs: "stretch", md: "center" }}
                  justifyContent="space-between"
                >
                  <Typography variant="h6" component="h2" fontWeight={700}>
                    Browser Tabs
                  </Typography>
                  <Button
                    size="small"
                    variant="outlined"
                    onClick={() => void loadBrowserTabs(true)}
                    disabled={busy || browserTabsLoading}
                    data-testid="starter-browser-tabs-refresh-button"
                  >
                    Refresh Tabs
                  </Button>
                </Stack>

                <Typography
                  variant="caption"
                  color={browserHealth?.extension_connected ? "success.main" : "text.secondary"}
                  data-testid="starter-browser-tabs-health"
                >
                  {browserHealth
                    ? `Browsr ${browserHealth.ok ? "online" : "offline"} · extension ${browserHealth.extension_connected ? "connected" : "disconnected"}`
                    : browserHealthError ?? "Browsr status unavailable"}
                </Typography>

                {browserTabsError ? <Alert severity="error">{browserTabsError}</Alert> : null}

                <Stack direction={{ xs: "column", md: "row" }} spacing={1}>
                  <FormControl size="small" className="md:min-w-56">
                    <InputLabel id="starter-browser-window-label">Window</InputLabel>
                    <Select
                      labelId="starter-browser-window-label"
                      label="Window"
                      value={selectedBrowserWindowId}
                      onChange={(event) => {
                        const raw = event.target.value;
                        setSelectedBrowserWindowId(raw === "all" ? "all" : Number(raw));
                      }}
                      data-testid="starter-browser-window-select"
                    >
                      <MenuItem value="all">All Windows</MenuItem>
                      {browserWindows.map((window) => (
                        <MenuItem key={window.id} value={window.id}>
                          Window {window.id}
                          {window.focused ? " · Focused" : ""}
                          {window.state ? ` · ${window.state}` : ""}
                        </MenuItem>
                      ))}
                    </Select>
                  </FormControl>
                  <TextField
                    size="small"
                    fullWidth
                    label="Search tabs"
                    value={browserTabSearch}
                    onChange={(event) => setBrowserTabSearch(event.target.value)}
                    inputProps={{ "data-testid": "starter-browser-tabs-search-input" }}
                  />
                </Stack>

                {!browserTabsLoading && visibleBrowserTabs.length > 0 ? (
                  <Typography variant="caption" color="text.secondary">
                    Showing {visibleBrowserTabs.length.toLocaleString()} tab{visibleBrowserTabs.length === 1 ? "" : "s"}
                  </Typography>
                ) : null}

                <div
                  style={{ maxHeight: browserTabsViewportHeight }}
                  className="overflow-y-auto pr-1"
                  onScroll={(event) => {
                    const nextWindow = Math.floor(
                      event.currentTarget.scrollTop / browserTabsRowHeight
                    );
                    if (nextWindow === browserTabsWindowRef.current) {
                      return;
                    }
                    browserTabsWindowRef.current = nextWindow;
                    setBrowserTabsScrollTop(nextWindow * browserTabsRowHeight);
                  }}
                >
                  <div>
                    {browserTabsVirtualWindow.topSpacerPx > 0 ? (
                      <div style={{ height: browserTabsVirtualWindow.topSpacerPx }} />
                    ) : null}
                  {browserTabsLoading ? (
                    <Stack direction="row" spacing={1} alignItems="center">
                      <CircularProgress size={18} />
                      <Typography variant="body2">Loading browser tabs...</Typography>
                    </Stack>
                  ) : null}
                  {!browserTabsLoading && visibleBrowserTabs.length === 0 ? (
                    <Typography variant="body2" color="text.secondary">
                      {browserWindows.length === 0
                        ? "No browser windows found."
                        : "No tabs matched the current browser-tab filters."}
                    </Typography>
                  ) : null}
                  {browserTabsVirtualWindow.items.map((tab) => (
                    <div key={tab.id} style={{ height: browserTabsRowHeight }}>
                      <div className="flex h-full items-center justify-between rounded-2xl border border-slate-200 bg-white/70 px-4 py-3">
                        <Stack spacing={0.35} className="min-w-0 flex-1">
                          <Typography variant="subtitle2" noWrap title={tab.title}>
                            {tab.title}
                          </Typography>
                          <Typography variant="caption" color="text.secondary" noWrap title={tab.url}>
                            {tab.url}
                          </Typography>
                          <Typography variant="caption" color="text.secondary" noWrap>
                            Window {tab.windowId}
                            {tab.active ? " · Active" : ""}
                            {tab.audible ? " · Audible" : ""}
                            {tab.pinned ? " · Pinned" : ""}
                            {tab.status ? ` · ${tab.status}` : ""}
                          </Typography>
                        </Stack>
                        <Button
                          size="small"
                          variant="contained"
                          onClick={() => void onOpenBrowserTab(tab.id, tab.windowId)}
                          disabled={busy || browserTabsLoading}
                          data-testid={`starter-browser-tab-open-${tab.id}`}
                          sx={{ flexShrink: 0 }}
                        >
                          Import
                        </Button>
                      </div>
                    </div>
                  ))}
                    {browserTabsVirtualWindow.bottomSpacerPx > 0 ? (
                      <div style={{ height: browserTabsVirtualWindow.bottomSpacerPx }} />
                    ) : null}
                  </div>
                </div>
              </Stack>
            </CardContent>
          </Card>
              </Stack>
            </CardContent>
          </Card>
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
            <Stack spacing={2.5}>
              <Stack direction="row" alignItems="center" justifyContent="space-between">
                <Typography variant="h6" component="h2" fontWeight={700}>
                  Recent Books
                </Typography>
                <Button
                  size="small"
                  variant="text"
                  startIcon={<RefreshIcon />}
                  onClick={() => void onRefreshRecents()}
                  disabled={busy || loadingRecents}
                >
                  Refresh
                </Button>
              </Stack>

              <Stack spacing={1}>
                <Stack direction={{ xs: "column", md: "row" }} spacing={1}>
                  <TextField
                    size="small"
                    fullWidth
                    label="Search recents (title/snippet)"
                    value={recentsSearch}
                    inputProps={{ "data-testid": "starter-recents-search-input" }}
                    onChange={(event) => setRecentsSearch(event.target.value)}
                    disabled={busy || loadingRecents}
                  />
                  <FormControl size="small" className="md:min-w-56">
                    <InputLabel id="recents-sort-label">Sort</InputLabel>
                    <Select
                      labelId="recents-sort-label"
                      label="Sort"
                      value={recentsSort}
                      onChange={(event) =>
                        setRecentsSort(
                          event.target.value as
                            | "recent_first"
                            | "recent_last"
                            | "title_asc"
                            | "title_desc"
                            | "path_asc"
                            | "path_desc"
                        )
                      }
                      disabled={busy || loadingRecents}
                    >
                      <MenuItem value="recent_first">Recently Opened</MenuItem>
                      <MenuItem value="recent_last">Least Recently Opened</MenuItem>
                      <MenuItem value="title_asc">Title (A-Z)</MenuItem>
                      <MenuItem value="title_desc">Title (Z-A)</MenuItem>
                      <MenuItem value="path_asc">Path (A-Z)</MenuItem>
                      <MenuItem value="path_desc">Path (Z-A)</MenuItem>
                    </Select>
                  </FormControl>
                </Stack>

                {!loadingRecents && hasRecents ? (
                  <Typography variant="caption" color="text.secondary">
                    Showing {filteredRecents.length.toLocaleString()} of {recents.length.toLocaleString()} recent entries
                  </Typography>
                ) : null}
              </Stack>

              {loadingRecents ? (
                <Stack direction="row" spacing={1} alignItems="center">
                  <CircularProgress size={18} />
                  <Typography variant="body2" color="text.secondary">
                    Loading recent books...
                  </Typography>
                </Stack>
              ) : null}

              {!hasFilteredRecents && !loadingRecents ? (
                <Typography variant="body2" color="text.secondary">
                  {hasRecents ? "No recent books match the current filters." : "No recent books yet."}
                </Typography>
              ) : null}

              {hasFilteredRecents ? (
                <div
                  className="overflow-y-auto pr-1"
                  style={{ maxHeight: recentsViewportHeight }}
                  onScroll={(event) => {
                    setRecentsScrollTop(event.currentTarget.scrollTop);
                  }}
                >
                  <div>
                    {recentsVirtualWindow.topSpacerPx > 0 ? (
                      <div style={{ height: recentsVirtualWindow.topSpacerPx }} />
                    ) : null}
                    {recentsVirtualWindow.items.map((recent) => {
                      const recentThumbnailSrc = toThumbnailSrc(recent.thumbnail_path);
                      return (
                        <div key={recent.source_path} style={{ height: recentsRowHeight }}>
                          <div
                            className="flex h-full items-center justify-between rounded-2xl border border-slate-200 bg-white/70 px-4 py-3"
                            data-testid="starter-recent-card"
                            data-recent-path={recent.source_path}
                          >
                            <Stack direction="row" spacing={1.25} alignItems="center" className="min-w-0 flex-1">
                              {recentThumbnailSrc ? (
                                <img
                                  src={recentThumbnailSrc}
                                  alt={recent.display_title}
                                  className="h-11 w-9 shrink-0 rounded border border-slate-200 object-cover"
                                  loading="lazy"
                                />
                              ) : null}
                              <Stack spacing={0.75} className="min-w-0">
                                <Typography variant="subtitle1" fontWeight={700} noWrap>
                                  {recent.display_title}
                                </Typography>
                                <Typography
                                  variant="caption"
                                  color="text.secondary"
                                  noWrap
                                  className="truncate"
                                >
                                  {recent.snippet}
                                </Typography>
                              </Stack>
                            </Stack>
                            <Stack direction="row" spacing={1}>
                              <Button
                                size="small"
                                variant="contained"
                                onClick={() => void onOpenPath(recent.source_path)}
                                disabled={busy}
                                data-testid="starter-recent-open-button"
                                data-recent-path={recent.source_path}
                              >
                                Open
                              </Button>
                              <Button
                                size="small"
                                color="error"
                                variant="outlined"
                                startIcon={<DeleteOutlineIcon />}
                                onClick={() => void onDeleteRecent(recent.source_path)}
                                disabled={busy}
                                data-testid="starter-recent-delete-button"
                                data-recent-path={recent.source_path}
                              >
                                Delete
                              </Button>
                            </Stack>
                          </div>
                        </div>
                      );
                    })}
                    {recentsVirtualWindow.bottomSpacerPx > 0 ? (
                      <div style={{ height: recentsVirtualWindow.bottomSpacerPx }} />
                    ) : null}
                  </div>
                </div>
              ) : null}
            </Stack>
          </div>

          <div
            style={{
              contentVisibility: "auto",
              containIntrinsicSize: "900px",
              contain: "layout paint style"
            }}
          >
            <Stack spacing={2.5}>
              <Stack direction="row" alignItems="center" justifyContent="space-between">
                <Typography variant="h6" component="h2" fontWeight={700}>
                  Calibre Library
                </Typography>
                <Stack direction="row" spacing={1}>
                  <Button
                    size="small"
                    variant="outlined"
                    onClick={() => setShowCalibre((current) => !current)}
                    disabled={busy}
                    data-testid="starter-calibre-toggle-button"
                  >
                    {showCalibre ? "Hide" : "Show"}
                  </Button>
                  <Button
                    size="small"
                    variant="outlined"
                    onClick={() => void onLoadCalibre(false)}
                    disabled={busy || loadingCalibre}
                    data-testid="starter-calibre-load-button"
                  >
                    Load
                  </Button>
                  <Button
                    size="small"
                    variant="text"
                    startIcon={<RefreshIcon />}
                    onClick={() => void onLoadCalibre(true)}
                    disabled={busy || loadingCalibre}
                    data-testid="starter-calibre-refresh-button"
                  >
                    Refresh
                  </Button>
                </Stack>
              </Stack>

              {showCalibre ? (
                <Stack spacing={1}>
                  <Stack direction={{ xs: "column", md: "row" }} spacing={1}>
                    <TextField
                      size="small"
                      fullWidth
                      label="Search calibre (title/author/format)"
                      value={calibreSearch}
                      inputProps={{ "data-testid": "starter-calibre-search-input" }}
                      onChange={(event) => setCalibreSearch(event.target.value)}
                      disabled={busy || loadingCalibre}
                    />
                    <FormControl size="small" className="md:min-w-56">
                      <InputLabel id="calibre-sort-label">Sort</InputLabel>
                      <Select
                        labelId="calibre-sort-label"
                        label="Sort"
                        value={calibreSort}
                        onChange={(event) => setCalibreSort(event.target.value as CalibreSort)}
                        disabled={busy || loadingCalibre}
                      >
                        <MenuItem value="title_asc">Title (A-Z)</MenuItem>
                        <MenuItem value="title_desc">Title (Z-A)</MenuItem>
                        <MenuItem value="author_asc">Author (A-Z)</MenuItem>
                        <MenuItem value="author_desc">Author (Z-A)</MenuItem>
                        <MenuItem value="year_desc">Year (Newest)</MenuItem>
                        <MenuItem value="year_asc">Year (Oldest)</MenuItem>
                        <MenuItem value="id_asc">Book ID (Ascending)</MenuItem>
                        <MenuItem value="id_desc">Book ID (Descending)</MenuItem>
                      </Select>
                    </FormControl>
                  </Stack>

                  {!loadingCalibre && filteredCalibre.length > 0 ? (
                    <Typography variant="caption" color="text.secondary">
                      Showing {filteredCalibre.length.toLocaleString()} calibre entries
                    </Typography>
                  ) : null}
                </Stack>
              ) : null}

              {loadingCalibre ? (
                <Stack direction="row" spacing={1} alignItems="center">
                  <CircularProgress size={18} />
                  <Typography variant="body2" color="text.secondary">
                    Loading calibre books...
                  </Typography>
                </Stack>
              ) : null}

              {!loadingCalibre && filteredCalibre.length === 0 ? (
                <Typography variant="body2" color="text.secondary">
                  No calibre books loaded yet.
                </Typography>
              ) : null}

              {showCalibre && filteredCalibre.length > 0 ? (
                <div
                  className="overflow-y-auto rounded-2xl border border-slate-200"
                  style={{ maxHeight: calibreViewportHeight }}
                  onScroll={(event) => {
                    setCalibreScrollTop(event.currentTarget.scrollTop);
                  }}
                >
                  <div className="divide-y divide-slate-200">
                    {virtualWindow.topSpacerPx > 0 ? (
                      <div style={{ height: virtualWindow.topSpacerPx }} />
                    ) : null}
                    {virtualWindow.items.map((book) => {
                      const calibreThumbnailSrc = toThumbnailSrc(
                        calibreThumbOverrides[book.id] ?? book.cover_thumbnail
                      );
                      return (
                      <div key={book.id} className="flex items-center justify-between gap-3 px-4 py-2.5">
                        <div className="flex min-w-0 items-center gap-2.5">
                          {calibreThumbnailSrc ? (
                            <img
                              src={calibreThumbnailSrc}
                              alt={book.title}
                              className="h-11 w-9 shrink-0 rounded border border-slate-200 object-cover"
                              loading="lazy"
                            />
                          ) : null}
                          <Stack spacing={0.25} className="min-w-0">
                            <Typography variant="subtitle2" noWrap>
                              {book.title}
                            </Typography>
                            <Typography variant="caption" color="text.secondary" noWrap>
                              {book.authors || "Unknown author"} · {book.extension.toUpperCase()}
                              {book.year ? " · " + book.year : ""}
                            </Typography>
                          </Stack>
                        </div>
                        <Button
                          size="small"
                          variant="contained"
                          onClick={() => void onOpenCalibreBook(book.id)}
                          disabled={busy}
                          data-testid="starter-calibre-open-button"
                          data-book-id={book.id}
                        >
                          Open
                        </Button>
                      </div>
                    );
                    })}
                    {virtualWindow.bottomSpacerPx > 0 ? (
                      <div style={{ height: virtualWindow.bottomSpacerPx }} />
                    ) : null}
                  </div>
                </div>
              ) : null}
            </Stack>
          </div>
        </div>
      </Stack>
    </div>
  );
}
