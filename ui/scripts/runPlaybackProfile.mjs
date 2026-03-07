import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = fileURLToPath(new URL(".", import.meta.url));
const uiRoot = path.resolve(__dirname, "..");
const repoRoot = path.resolve(uiRoot, "..");

const scenarios = [
  { key: "large_epub", label: "large EPUB" },
  { key: "imported_browser_tab", label: "imported browser tab" },
  { key: "image_heavy_html", label: "image-heavy HTML" }
];

function parseMetrics(output) {
  const match = output.match(
    /\[playback-profile\] scenario=(\S+) open_ms=(\d+) page_advance_ms=(\d+) tts_start_ms=(\d+) sentence_advance_ms=(\d+) resize_ms=(\d+)/
  );
  if (!match) {
    return null;
  }
  return {
    scenario: match[1],
    open_ms: Number(match[2]),
    page_advance_ms: Number(match[3]),
    tts_start_ms: Number(match[4]),
    sentence_advance_ms: Number(match[5]),
    resize_ms: Number(match[6])
  };
}

function runScenario(key) {
  const env = {
    ...process.env,
    LL_PERF_SCENARIO: key
  };
  return spawnSync(
    "pnpm",
    ["playwright", "test", "e2e/playbackProfile.spec.ts", "--project=chromium"],
    {
      cwd: uiRoot,
      env,
      encoding: "utf8",
      maxBuffer: 1024 * 1024 * 50
    }
  );
}

const runs = [];
for (const scenario of scenarios) {
  const result = runScenario(scenario.key);
  const combined = `${result.stdout ?? ""}\n${result.stderr ?? ""}`;
  const metrics = parseMetrics(combined);
  process.stdout.write(`\n=== Playback profile: ${scenario.label} ===\n`);
  if (result.stdout) {
    process.stdout.write(result.stdout);
  }
  if (result.stderr) {
    process.stderr.write(result.stderr);
  }
  runs.push({
    scenario: scenario.key,
    label: scenario.label,
    status: result.status === 0 ? "passed" : "failed",
    exit_code: result.status ?? 1,
    metrics
  });
}

const reportPath = path.resolve(repoRoot, "tmp", "playback-profile-report.json");
mkdirSync(path.dirname(reportPath), { recursive: true });
writeFileSync(reportPath, `${JSON.stringify({ generated_at: new Date().toISOString(), runs }, null, 2)}\n`);

process.stdout.write(`\nPlayback profile report written to ${reportPath}\n`);

if (runs.some((run) => run.status !== "passed")) {
  process.exitCode = 1;
}
