// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  BrowserTabInfo,
  BrowserWindowInfo,
  BrowsrHealth
} from "../src/api/tauri";
import { StarterShell } from "../src/components/StarterShell";
import type { BootstrapState } from "../src/types";

const browserTabsHealth = vi.fn<() => Promise<BrowsrHealth>>();
const browserTabsListWindows = vi.fn<() => Promise<BrowserWindowInfo[]>>();
const browserTabsListTabs = vi.fn<
  (windowId?: number, query?: string, refresh?: boolean) => Promise<BrowserTabInfo[]>
>();

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `asset:${path}`
}));

vi.mock("../src/api/tauri", () => ({
  backendApi: {
    browserTabsHealth: () => browserTabsHealth(),
    browserTabsListWindows: () => browserTabsListWindows(),
    browserTabsListTabs: (windowId?: number, query?: string, refresh?: boolean) =>
      browserTabsListTabs(windowId, query, refresh)
  }
}));

function makeBootstrap(): BootstrapState {
  return {
    app_name: "LanternLeaf",
    mode: "test",
    config: {
      theme: "day",
      font_family: "lexend",
      font_weight: "bold",
      day_highlight: { r: 0.2, g: 0.4, b: 0.7, a: 0.15 },
      night_highlight: { r: 0.8, g: 0.8, b: 0.5, a: 0.2 },
      log_level: "debug",
      default_font_size: 22,
      default_lines_per_page: 700,
      default_tts_speed: 2.5,
      default_pause_after_sentence: 0.06,
      key_toggle_play_pause: "space",
      key_next_sentence: "f",
      key_prev_sentence: "s",
      key_repeat_sentence: "r",
      key_toggle_search: "ctrl+f",
      key_safe_quit: "q",
      key_toggle_settings: "ctrl+t",
      key_toggle_stats: "ctrl+g",
      key_toggle_tts: "ctrl+y",
      browser_tabs_enabled: true,
      close_browser_tab_on_recent_delete: true
    }
  };
}

describe("StarterShell browser tabs", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    (
      globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
    ).IS_REACT_ACT_ENVIRONMENT = true;
    browserTabsHealth.mockReset();
    browserTabsListWindows.mockReset();
    browserTabsListTabs.mockReset();
    browserTabsHealth.mockResolvedValue({
      ok: true,
      extension_connected: true,
      now: "2026-03-06T20:00:00Z"
    });
    browserTabsListWindows.mockResolvedValue([
      { id: 11, focused: true, height: 900, incognito: false, left: 0, state: "normal", top: 0, type: "normal", width: 1400 }
    ]);
    browserTabsListTabs.mockResolvedValue([
      {
        id: 101,
        windowId: 11,
        index: 0,
        active: true,
        audible: false,
        pinned: false,
        status: "complete",
        title: "Alpha Tab",
        url: "https://example.com/alpha",
        favIconUrl: null,
        lastAccessed: 1
      },
      {
        id: 102,
        windowId: 99,
        index: 1,
        active: false,
        audible: true,
        pinned: true,
        status: "complete",
        title: "Beta Tab",
        url: "https://example.com/beta",
        favIconUrl: null,
        lastAccessed: 2
      }
    ]);
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
    delete (
      globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }
    ).IS_REACT_ACT_ENVIRONMENT;
  });

  it("loads browser tabs, filters by window and imports a selected tab", async () => {
    const onOpenBrowserTab = vi.fn<(tabId: number, windowId?: number) => Promise<void>>().mockResolvedValue();
    const onOpenBrowserTabBundle = vi.fn<(tabId: number, windowId?: number) => Promise<void>>().mockResolvedValue();

    await act(async () => {
      root.render(
        <StarterShell
          bootstrap={makeBootstrap()}
          recents={[]}
          calibreBooks={[]}
          busy={false}
          loadingRecents={false}
          loadingCalibre={false}
          onOpenPath={async () => {}}
          onOpenClipboardText={async () => {}}
          onOpenBrowserTab={onOpenBrowserTab}
          onOpenBrowserTabBundle={onOpenBrowserTabBundle}
          onDeleteRecent={async () => {}}
          onCloseRecentBrowserTab={async () => {}}
          onRefreshRecents={async () => {}}
          onLoadCalibre={async () => {}}
          onOpenCalibreBook={async () => {}}
          onSetRuntimeLogLevel={async () => {}}
          onToggleTheme={async () => {}}
          sourceOpenEvent={null}
          calibreLoadEvent={null}
          pdfTranscriptionEvent={null}
          runtimeLogLevel="debug"
        />
      );
    });

    await act(async () => {
      await Promise.resolve();
    });

    expect(browserTabsHealth).toHaveBeenCalledTimes(1);
    expect(browserTabsListWindows).toHaveBeenCalledTimes(1);
    expect(browserTabsListTabs).toHaveBeenCalledWith(undefined, "", false);
    expect(container.textContent).toContain("Browser Tabs");
    expect(container.textContent).toContain("Alpha Tab");
    expect(container.textContent).toContain("Beta Tab");

    const search = container.querySelector('[data-testid="starter-browser-tabs-search-input"]') as HTMLInputElement | null;
    expect(search).not.toBeNull();
    await act(async () => {
      search!.value = "Alpha";
      search!.dispatchEvent(new Event("input", { bubbles: true }));
    });
    expect(container.textContent).toContain("Alpha Tab");

    const importButton = container.querySelector('[data-testid="starter-browser-tab-open-101"]') as HTMLButtonElement | null;
    expect(importButton).not.toBeNull();
    await act(async () => {
      importButton!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect(onOpenBrowserTab).toHaveBeenCalledWith(101, 11);

    const bundleButton = container.querySelector('[data-testid="starter-browser-tab-open-bundle-101"]') as HTMLButtonElement | null;
    expect(bundleButton).not.toBeNull();
    await act(async () => {
      bundleButton!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect(onOpenBrowserTabBundle).toHaveBeenCalledWith(101, 11);
  });
});
