import { expect, test } from "@playwright/test";

type ProfileScenario = {
  key: "large_epub" | "imported_browser_tab" | "image_heavy_html";
  openPath: string;
  expectedSourceName: string;
};

const scenarioMap: Record<ProfileScenario["key"], ProfileScenario> = {
  large_epub: {
    key: "large_epub",
    openPath: "/tmp/mock-large.epub",
    expectedSourceName: "mock-large.epub"
  },
  imported_browser_tab: {
    key: "imported_browser_tab",
    openPath: ".cache/browser-tabs/mock-profile/browser-tab.lltab",
    expectedSourceName: "browser-tab.lltab"
  },
  image_heavy_html: {
    key: "image_heavy_html",
    openPath: "/tmp/mock-image-heavy.html",
    expectedSourceName: "mock-image-heavy.html"
  }
};

function getScenario(): ProfileScenario {
  const raw = (process.env.LL_PERF_SCENARIO ?? "large_epub").trim() as ProfileScenario["key"];
  return scenarioMap[raw] ?? scenarioMap.large_epub;
}

test.describe("playback profile", () => {
  test("captures long-running playback profile metrics for the selected scenario", async ({ page }) => {
    const scenario = getScenario();
    await page.goto("/");
    const closeSessionButton = page.getByTestId("reader-close-session-button");
    if (await closeSessionButton.isVisible().catch(() => false)) {
      await closeSessionButton.click();
    }
    await expect(page.getByTestId("starter-open-path-input")).toBeVisible();

    const openStart = Date.now();
    await page.getByTestId("starter-open-path-input").fill(scenario.openPath);
    await page.getByTestId("starter-open-path-button").click();
    await expect(page.getByTestId("reader-close-session-button")).toBeVisible();
    const openMs = Date.now() - openStart;

    const pageAdvanceStart = Date.now();
    for (let step = 0; step < 5; step += 1) {
      await page.getByTestId("reader-next-page-button").click();
    }
    const pageAdvanceMs = Date.now() - pageAdvanceStart;

    const ttsToggle = page.getByTestId("reader-tts-player-play-pause");
    const ttsStart = Date.now();
    await ttsToggle.click();
    await expect(ttsToggle).toHaveAttribute("aria-label", "Pause");
    const ttsStartMs = Date.now() - ttsStart;

    const sentenceAdvanceStart = Date.now();
    for (let step = 0; step < 24; step += 1) {
      await page.getByTestId("reader-next-sentence-button").click();
    }
    const sentenceAdvanceMs = Date.now() - sentenceAdvanceStart;

    const resizeStart = Date.now();
    await page.setViewportSize({ width: 1320, height: 900 });
    await expect(page.getByTestId("reader-close-session-button")).toBeVisible();
    const resizeMs = Date.now() - resizeStart;

    console.info(
      `[playback-profile] scenario=${scenario.key} open_ms=${openMs} page_advance_ms=${pageAdvanceMs} tts_start_ms=${ttsStartMs} sentence_advance_ms=${sentenceAdvanceMs} resize_ms=${resizeMs}`
    );

    expect(openMs).toBeLessThan(5000);
    expect(pageAdvanceMs).toBeLessThan(5000);
    expect(ttsStartMs).toBeLessThan(2500);
    expect(sentenceAdvanceMs).toBeLessThan(10000);
    expect(resizeMs).toBeLessThan(3000);
  });
});
