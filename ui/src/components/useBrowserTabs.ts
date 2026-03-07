import { useCallback, useEffect, useMemo, useRef, useState, type MutableRefObject } from "react";

import {
  backendApi,
  type BrowserTabInfo,
  type BrowserWindowInfo,
  type BrowsrHealth
} from "../api/tauri";
import { computeVirtualWindow, type VirtualWindow } from "./calibreList";
import { toUiErrorMessage } from "./starterShared";

export interface BrowserTabsState {
  browserHealth: BrowsrHealth | null;
  browserHealthError: string | null;
  browserTabSearch: string;
  browserTabsError: string | null;
  browserTabsLoading: boolean;
  browserTabsRowHeight: number;
  browserTabsViewportHeight: number;
  browserTabsVirtualWindow: VirtualWindow<BrowserTabInfo>;
  browserWindows: BrowserWindowInfo[];
  loadBrowserTabs: (refresh?: boolean) => Promise<void>;
  selectedBrowserWindowId: number | "all";
  setBrowserTabSearch: (value: string) => void;
  setBrowserTabsScrollTop: (value: number) => void;
  setSelectedBrowserWindowId: (value: number | "all") => void;
  visibleBrowserTabs: BrowserTabInfo[];
  windowRef: MutableRefObject<number>;
}

export function useBrowserTabs(): BrowserTabsState {
  const [browserHealth, setBrowserHealth] = useState<BrowsrHealth | null>(null);
  const [browserHealthError, setBrowserHealthError] = useState<string | null>(null);
  const [browserWindows, setBrowserWindows] = useState<BrowserWindowInfo[]>([]);
  const [browserTabs, setBrowserTabs] = useState<BrowserTabInfo[]>([]);
  const [browserTabsLoading, setBrowserTabsLoading] = useState(false);
  const [browserTabsError, setBrowserTabsError] = useState<string | null>(null);
  const [selectedBrowserWindowId, setSelectedBrowserWindowId] = useState<number | "all">("all");
  const [browserTabSearch, setBrowserTabSearch] = useState("");
  const [browserTabsScrollTop, setBrowserTabsScrollTop] = useState(0);
  const browserTabsWindowRef = useRef<number>(0);
  const browserTabsRowHeight = 92;
  const browserTabsOverscan = 6;
  const browserTabsViewportHeight = 320;

  const loadBrowserTabs = useCallback(async (refresh = false): Promise<void> => {
    setBrowserTabsLoading(true);
    setBrowserTabsError(null);
    setBrowserHealthError(null);
    try {
      const [healthResult, windowsResult, tabsResult] = await Promise.allSettled([
        backendApi.browserTabsHealth(),
        backendApi.browserTabsListWindows(),
        backendApi.browserTabsListTabs(
          selectedBrowserWindowId === "all" ? undefined : selectedBrowserWindowId,
          browserTabSearch,
          refresh
        )
      ]);

      if (healthResult.status === "fulfilled") {
        setBrowserHealth(healthResult.value);
      } else {
        setBrowserHealth(null);
        setBrowserHealthError(
          toUiErrorMessage(healthResult.reason, "[starter-browser-tabs] Browsr health failed")
        );
      }

      if (windowsResult.status === "fulfilled") {
        setBrowserWindows(windowsResult.value);
      } else {
        setBrowserWindows([]);
      }

      if (tabsResult.status === "fulfilled") {
        setBrowserTabs(tabsResult.value);
      } else {
        setBrowserTabs([]);
        setBrowserTabsError(
          toUiErrorMessage(tabsResult.reason, "[starter-browser-tabs] Tab listing failed")
        );
      }
    } catch (error) {
      setBrowserTabsError(
        toUiErrorMessage(error, "[starter-browser-tabs] Browser tabs load failed")
      );
    } finally {
      setBrowserTabsLoading(false);
    }
  }, [browserTabSearch, selectedBrowserWindowId]);

  useEffect(() => {
    void loadBrowserTabs(false);
  }, [loadBrowserTabs]);

  useEffect(() => {
    setBrowserTabsScrollTop(0);
    browserTabsWindowRef.current = 0;
  }, [browserTabSearch, selectedBrowserWindowId]);

  const visibleBrowserTabs = useMemo(() => {
    const needle = browserTabSearch.trim().toLowerCase();
    return browserTabs.filter((tab) => {
      if (selectedBrowserWindowId !== "all" && tab.windowId !== selectedBrowserWindowId) {
        return false;
      }
      if (!needle) {
        return true;
      }
      return tab.title.toLowerCase().includes(needle) || tab.url.toLowerCase().includes(needle);
    });
  }, [browserTabSearch, browserTabs, selectedBrowserWindowId]);

  const browserTabsVirtualWindow = useMemo(
    () =>
      computeVirtualWindow(
        visibleBrowserTabs,
        browserTabsScrollTop,
        browserTabsRowHeight,
        browserTabsViewportHeight,
        browserTabsOverscan
      ),
    [
      browserTabsOverscan,
      browserTabsRowHeight,
      browserTabsScrollTop,
      browserTabsViewportHeight,
      visibleBrowserTabs
    ]
  );

  return {
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
  };
}
