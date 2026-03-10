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
  FormControlLabel,
  FormControl,
  InputLabel,
  MenuItem,
  Select,
  Stack,
  Switch,
  TextField,
  Typography
} from "@mui/material";
import { memo, useEffect, useState } from "react";

import type { BrowserTabInfo, BrowserWindowInfo, BrowsrHealth } from "../api/tauri";
import type { CalibreBook, RecentBook } from "../types";
import type { CalibreSort } from "./calibreList";
import type { VirtualWindow } from "./starterVirtualList";
import { toThumbnailSrc } from "./starterShared";

export function StarterOpenPanel({
  busy,
  clipboardError,
  currentTheme,
  handleClipboardOpen,
  handleOpenPath,
  logLevelValue,
  onSetRuntimeLogLevel,
  onToggleTheme,
  path,
  pdfStatus,
  runtimeLogLevel,
  setLogLevelValue,
  setPath,
  sourceOpenStatus,
  calibreStatus,
  themeToggleLabel
}: {
  busy: boolean;
  clipboardError: string | null;
  currentTheme: "day" | "night";
  handleClipboardOpen: () => Promise<void>;
  handleOpenPath: () => Promise<void>;
  logLevelValue: string;
  onSetRuntimeLogLevel: (level: string) => Promise<void>;
  onToggleTheme: () => Promise<void>;
  path: string;
  pdfStatus: string | null;
  runtimeLogLevel: string;
  setLogLevelValue: (value: string) => void;
  setPath: (value: string) => void;
  sourceOpenStatus: string | null;
  calibreStatus: string | null;
  themeToggleLabel: string;
}) {
  return (
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
            <Typography variant="caption" color="text.secondary" data-testid="starter-calibre-status">
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
              startIcon={currentTheme === "night" ? <LightModeOutlinedIcon /> : <DarkModeOutlinedIcon />}
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
        </Stack>
      </CardContent>
    </Card>
  );
}

const StarterBrowserTabRow = memo(function StarterBrowserTabRow({
  busy,
  browserTabsLoading,
  onOpenBrowserTab,
  onOpenBrowserTabBundle,
  tab
}: {
  busy: boolean;
  browserTabsLoading: boolean;
  onOpenBrowserTab: (tabId: number, windowId?: number) => Promise<void>;
  onOpenBrowserTabBundle: (tabId: number, windowId?: number) => Promise<void>;
  tab: BrowserTabInfo;
}) {
  return (
    <div style={{ height: 92 }}>
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
        <Stack direction={{ xs: "column", sm: "row" }} spacing={1} sx={{ flexShrink: 0 }}>
          <Button
            size="small"
            variant="outlined"
            onClick={() => void onOpenBrowserTab(tab.id, tab.windowId)}
            disabled={busy || browserTabsLoading}
            data-testid={`starter-browser-tab-open-${tab.id}`}
          >
            Quick Snapshot
          </Button>
          <Button
            size="small"
            variant="contained"
            onClick={() => void onOpenBrowserTabBundle(tab.id, tab.windowId)}
            disabled={busy || browserTabsLoading}
            data-testid={`starter-browser-tab-open-bundle-${tab.id}`}
          >
            Import Bundle
          </Button>
        </Stack>
      </div>
    </div>
  );
});

export function StarterBrowserTabsPanel({
  browserHealth,
  browserHealthError,
  browserTabSearch,
  browserTabsError,
  browserTabsLoading,
  browserTabsRowHeight,
  browserTabsViewportHeight,
  browserTabsVirtualWindow,
  browserWindows,
  busy,
  loadBrowserTabs,
  onOpenBrowserTab,
  onOpenBrowserTabBundle,
  selectedBrowserWindowId,
  setBrowserTabSearch,
  setBrowserTabsScrollTop,
  setSelectedBrowserWindowId,
  visibleBrowserTabs,
  windowRef
}: {
  browserHealth: BrowsrHealth | null;
  browserHealthError: string | null;
  browserTabSearch: string;
  browserTabsError: string | null;
  browserTabsLoading: boolean;
  browserTabsRowHeight: number;
  browserTabsViewportHeight: number;
  browserTabsVirtualWindow: VirtualWindow<BrowserTabInfo>;
  browserWindows: BrowserWindowInfo[];
  busy: boolean;
  loadBrowserTabs: (refresh?: boolean) => Promise<void>;
  onOpenBrowserTab: (tabId: number, windowId?: number) => Promise<void>;
  onOpenBrowserTabBundle: (tabId: number, windowId?: number) => Promise<void>;
  selectedBrowserWindowId: number | "all";
  setBrowserTabSearch: (value: string) => void;
  setBrowserTabsScrollTop: (value: number) => void;
  setSelectedBrowserWindowId: (value: number | "all") => void;
  visibleBrowserTabs: BrowserTabInfo[];
  windowRef: React.MutableRefObject<number>;
}) {
  return (
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
              const nextWindow = Math.floor(event.currentTarget.scrollTop / browserTabsRowHeight);
              if (nextWindow === windowRef.current) {
                return;
              }
              windowRef.current = nextWindow;
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
                  <StarterBrowserTabRow
                    key={tab.id}
                    busy={busy}
                    browserTabsLoading={browserTabsLoading}
                    onOpenBrowserTab={onOpenBrowserTab}
                    onOpenBrowserTabBundle={onOpenBrowserTabBundle}
                    tab={tab}
                  />
              ))}
              {browserTabsVirtualWindow.bottomSpacerPx > 0 ? (
                <div style={{ height: browserTabsVirtualWindow.bottomSpacerPx }} />
              ) : null}
            </div>
          </div>
        </Stack>
      </CardContent>
    </Card>
  );
}

const StarterRecentRow = memo(function StarterRecentRow({
  busy,
  defaultCloseBrowserTabOnDelete,
  onCloseRecentBrowserTab,
  onDeleteRecent,
  onOpenPath,
  recent
}: {
  busy: boolean;
  defaultCloseBrowserTabOnDelete: boolean;
  onCloseRecentBrowserTab: (path: string) => Promise<void>;
  onDeleteRecent: (path: string, closeBrowserTab?: boolean) => Promise<void>;
  onOpenPath: (path: string) => Promise<void>;
  recent: RecentBook;
}) {
  const recentThumbnailSrc = toThumbnailSrc(recent.thumbnail_path);
  const isBrowserTab = recent.browser_tab_id !== null;
  const [closeBrowserTabOnDelete, setCloseBrowserTabOnDelete] = useState(
    defaultCloseBrowserTabOnDelete
  );

  useEffect(() => {
    setCloseBrowserTabOnDelete(defaultCloseBrowserTabOnDelete);
  }, [defaultCloseBrowserTabOnDelete, recent.source_path]);

  return (
    <div style={{ height: 132 }}>
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
            <Typography variant="caption" color="text.secondary" noWrap className="truncate">
              {recent.snippet}
            </Typography>
            {isBrowserTab ? (
              <FormControlLabel
                control={
                  <Switch
                    size="small"
                    checked={closeBrowserTabOnDelete}
                    onChange={(event) => setCloseBrowserTabOnDelete(event.target.checked)}
                    disabled={busy}
                  />
                }
                label={
                  <Typography variant="caption" color="text.secondary">
                    Close tab on delete
                  </Typography>
                }
                sx={{ m: 0 }}
              />
            ) : null}
          </Stack>
        </Stack>
        <Stack direction="row" spacing={1}>
          {isBrowserTab ? (
            <Button
              size="small"
              variant="outlined"
              onClick={() => void onCloseRecentBrowserTab(recent.source_path)}
              disabled={busy}
              data-testid="starter-recent-close-tab-button"
              data-recent-path={recent.source_path}
            >
              Close Tab
            </Button>
          ) : null}
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
            onClick={() => void onDeleteRecent(recent.source_path, closeBrowserTabOnDelete)}
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
});

export function StarterRecentsPanel({
  busy,
  defaultCloseBrowserTabOnDelete,
  filteredRecents,
  hasFilteredRecents,
  hasRecents,
  loadingRecents,
  onCloseRecentBrowserTab,
  onDeleteRecent,
  onOpenPath,
  onRefreshRecents,
  recents,
  recentsScrollTop,
  recentsSort,
  recentsVirtualWindow,
  setRecentsScrollTop,
  setRecentsSearch,
  setRecentsSort,
  recentsSearch,
  recentsViewportHeight
}: {
  busy: boolean;
  defaultCloseBrowserTabOnDelete: boolean;
  filteredRecents: RecentBook[];
  hasFilteredRecents: boolean;
  hasRecents: boolean;
  loadingRecents: boolean;
  onCloseRecentBrowserTab: (path: string) => Promise<void>;
  onDeleteRecent: (path: string, closeBrowserTab?: boolean) => Promise<void>;
  onOpenPath: (path: string) => Promise<void>;
  onRefreshRecents: () => Promise<void>;
  recents: RecentBook[];
  recentsScrollTop: number;
  recentsSort: "recent_first" | "recent_last" | "title_asc" | "title_desc" | "path_asc" | "path_desc";
  recentsVirtualWindow: VirtualWindow<RecentBook>;
  setRecentsScrollTop: (value: number) => void;
  setRecentsSearch: (value: string) => void;
  setRecentsSort: (value: "recent_first" | "recent_last" | "title_asc" | "title_desc" | "path_asc" | "path_desc") => void;
  recentsSearch: string;
  recentsViewportHeight: number;
}) {
  void recentsScrollTop;
  return (
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
            {recentsVirtualWindow.items.map((recent) => (
              <StarterRecentRow
                key={recent.source_path}
                busy={busy}
                defaultCloseBrowserTabOnDelete={defaultCloseBrowserTabOnDelete}
                onCloseRecentBrowserTab={onCloseRecentBrowserTab}
                onDeleteRecent={onDeleteRecent}
                onOpenPath={onOpenPath}
                recent={recent}
              />
            ))}
            {recentsVirtualWindow.bottomSpacerPx > 0 ? (
              <div style={{ height: recentsVirtualWindow.bottomSpacerPx }} />
            ) : null}
          </div>
        </div>
      ) : null}
    </Stack>
  );
}

const StarterCalibreRow = memo(function StarterCalibreRow({
  book,
  busy,
  onOpenCalibreBook,
  thumbnailSrc
}: {
  book: CalibreBook;
  busy: boolean;
  onOpenCalibreBook: (bookId: number) => Promise<void>;
  thumbnailSrc: string | null;
}) {
  return (
    <div className="flex items-center justify-between gap-3 px-4 py-2.5">
      <div className="flex min-w-0 items-center gap-2.5">
        {thumbnailSrc ? (
          <img
            src={thumbnailSrc}
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
});

export function StarterCalibrePanel({
  busy,
  calibreSort,
  calibreThumbOverrides,
  calibreViewportHeight,
  filteredCalibre,
  loadingCalibre,
  onLoadCalibre,
  onOpenCalibreBook,
  setCalibreScrollTop,
  setCalibreSearch,
  setCalibreSort,
  setShowCalibre,
  showCalibre,
  virtualWindow,
  calibreSearch
}: {
  busy: boolean;
  calibreSort: CalibreSort;
  calibreThumbOverrides: Record<number, string>;
  calibreViewportHeight: number;
  filteredCalibre: CalibreBook[];
  loadingCalibre: boolean;
  onLoadCalibre: (forceRefresh?: boolean) => Promise<void>;
  onOpenCalibreBook: (bookId: number) => Promise<void>;
  setCalibreScrollTop: (value: number) => void;
  setCalibreSearch: (value: string) => void;
  setCalibreSort: (value: CalibreSort) => void;
  setShowCalibre: React.Dispatch<React.SetStateAction<boolean>>;
  showCalibre: boolean;
  virtualWindow: VirtualWindow<CalibreBook>;
  calibreSearch: string;
}) {
  return (
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
            {virtualWindow.topSpacerPx > 0 ? <div style={{ height: virtualWindow.topSpacerPx }} /> : null}
            {virtualWindow.items.map((book) => (
              <StarterCalibreRow
                key={book.id}
                book={book}
                busy={busy}
                onOpenCalibreBook={onOpenCalibreBook}
                thumbnailSrc={toThumbnailSrc(calibreThumbOverrides[book.id] ?? book.cover_thumbnail)}
              />
            ))}
            {virtualWindow.bottomSpacerPx > 0 ? <div style={{ height: virtualWindow.bottomSpacerPx }} /> : null}
          </div>
        </div>
      ) : null}
    </Stack>
  );
}
