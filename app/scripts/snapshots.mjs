#!/usr/bin/env node
/**
 * GUI snapshot runner (Phase 5: "GUI snapshots: AR/EN × light/dark").
 *
 *   node scripts/snapshots.mjs capture   # build harness + write baselines
 *   node scripts/snapshots.mjs check     # build harness + compare to baselines
 *
 * What it does: builds the dev-only harness page (snapshots.html, vite with
 * SNAPSHOT_MOCK=1 → the Tauri bridge is a deterministic mock), serves it on
 * 127.0.0.1:1421, and screenshots every (screen × state) × en/ar × light/dark
 * combination with Chromium at 800×600 @2x, reduced motion forced.
 *
 * Baseline scheme (the honest one — font rasterization differs per OS):
 *   snapshots/    macOS baselines — the human design-review surface.
 *                 Reviewers read these PNGs like a designer: spacing,
 *                 truncation, RTL mirroring, Arabic typography.
 *   snapshots-ci/ Linux baselines — regression gate on ubuntu CI. They can
 *                 only be produced on Linux; when the CI check finds no
 *                 baseline it BOOTSTRAPS one (capture + warn + upload as
 *                 artifact) instead of failing, so the first run after adding
 *                 a scenario is green and the new baselines get committed by
 *                 a human from the artifact.
 * Arabic and mono glyphs are pinned to the bundled fonts (Noto Sans Arabic,
 * JetBrains Mono — docs/04 §3 fallbacks) in the harness, so only Latin UI
 * text rasterizes differently between platforms.
 *
 * Check tolerance: pixelmatch threshold 0.1, up to 0.5% differing pixels per
 * image — same-platform captures should be near-identical; the allowance
 * absorbs GPU rasterization jitter only, not layout or text changes.
 */
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import pixelmatch from "pixelmatch";
import { PNG } from "pngjs";

const APP_DIR = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const BUILD_DIR = path.join(APP_DIR, "dist-snapshots");
const TMP_DIR = path.join(APP_DIR, "snapshots-tmp");
const PORT = 1421;

const onCI = process.env.CI === "true";
const BASELINE_DIR = path.join(APP_DIR, onCI ? "snapshots-ci" : "snapshots");

const PIXEL_THRESHOLD = 0.1;
const MAX_DIFF_RATIO = 0.005;

/** The full matrix: 13 screen-states × 2 languages × 2 themes = 52 PNGs. */
const SCREEN_STATES = [
  ["s1", "empty"], // S1 drop zone = the empty state (docs/03 §2 S1)
  ["s1", "dragover"], // drag-over highlight (docs/04 §5)
  ["s2", "default"], // S2 compress sheet
  ["s2", "validation-error"], // S2 error state: name exists
  ["s3", "default"], // S3 extract sheet
  ["s4", "empty"], // queue collapses entirely
  ["s4", "restoring"], // launch skeleton (loading)
  ["s4", "running"], // progress bar + % + bytes
  ["s4", "success"], // finished summary lines (Arabic filename)
  ["s4", "failed"], // error tint + recovery actions
  ["s6", "default"], // settings
  ["s7", "default"], // first-launch welcome
  ["d3", "default"], // update-available sheet
];
const LANGS = ["en", "ar"];
const THEMES = ["light", "dark"];

/** What must be on screen before a capture is taken. */
function readySelector(screen, state) {
  switch (screen) {
    case "s1":
      return state === "dragover" ? ".drop-zone.drag-over" : ".drop-zone";
    case "s2":
      return state === "validation-error" ? ".sheet .field-error" : ".sheet";
    case "s4":
      switch (state) {
        case "restoring":
          return ".queue-row.skeleton";
        case "running":
          return ".queue-row .progress-fill";
        case "success":
          return ".queue-row .badge-done";
        case "failed":
          return ".queue-row .badge-failed";
        default:
          return ".drop-zone";
      }
    default: // s3, s6, s7, d3 — sheets
      return ".sheet";
  }
}

function buildHarness() {
  console.log("Building snapshot harness (SNAPSHOT_MOCK=1)…");
  const result = spawnSync("npx", ["vite", "build"], {
    cwd: APP_DIR,
    env: { ...process.env, SNAPSHOT_MOCK: "1" },
    stdio: "inherit",
  });
  if (result.status !== 0) {
    console.error("vite build failed");
    process.exit(1);
  }
}

const MIME = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".css": "text/css",
  ".woff2": "font/woff2",
  ".svg": "image/svg+xml",
  ".png": "image/png",
};

function serve() {
  const server = http.createServer((req, res) => {
    const url = new URL(req.url, `http://127.0.0.1:${PORT}`);
    let file = path.join(BUILD_DIR, decodeURIComponent(url.pathname));
    if (url.pathname === "/" || !path.extname(file)) {
      file = path.join(BUILD_DIR, "snapshots.html");
    }
    if (!file.startsWith(BUILD_DIR) || !fs.existsSync(file)) {
      res.writeHead(404).end("not found");
      return;
    }
    res.writeHead(200, { "content-type": MIME[path.extname(file)] ?? "application/octet-stream" });
    fs.createReadStream(file).pipe(res);
  });
  return new Promise((resolve) => {
    server.listen(PORT, "127.0.0.1", () => resolve(server));
  });
}

async function captureAll(outDir) {
  fs.rmSync(outDir, { recursive: true, force: true });
  fs.mkdirSync(outDir, { recursive: true });
  const browser = await chromium.launch();
  const page = await browser.newPage({
    viewport: { width: 800, height: 600 },
    deviceScaleFactor: 2,
    reducedMotion: "reduce",
  });
  const files = [];
  for (const [screen, state] of SCREEN_STATES) {
    for (const lang of LANGS) {
      for (const theme of THEMES) {
        const name = `${screen}-${state}-${lang}-${theme}.png`;
        const url =
          `http://127.0.0.1:${PORT}/snapshots.html` +
          `?screen=${screen}&state=${state}&lang=${lang}&theme=${theme}`;
        await page.goto(url, { waitUntil: "load" });
        await page.waitForSelector(readySelector(screen, state), { timeout: 10_000 });
        await page.evaluate(() => document.fonts.ready);
        await page.screenshot({ path: path.join(outDir, name) });
        files.push(name);
        process.stdout.write(`  ${name}\n`);
      }
    }
  }
  await browser.close();
  return files;
}

function compareOne(name) {
  const baselinePath = path.join(BASELINE_DIR, name);
  const currentPath = path.join(TMP_DIR, name);
  if (!fs.existsSync(baselinePath)) {
    if (onCI) {
      // Bootstrap: first CI run after a scenario is added seeds the baseline.
      fs.mkdirSync(BASELINE_DIR, { recursive: true });
      fs.copyFileSync(currentPath, baselinePath);
      return { name, status: "bootstrapped" };
    }
    return { name, status: "missing" };
  }
  const baseline = PNG.sync.read(fs.readFileSync(baselinePath));
  const current = PNG.sync.read(fs.readFileSync(currentPath));
  if (baseline.width !== current.width || baseline.height !== current.height) {
    return { name, status: "size-mismatch" };
  }
  const diff = new PNG({ width: baseline.width, height: baseline.height });
  const diffPixels = pixelmatch(baseline.data, current.data, diff.data, baseline.width, baseline.height, {
    threshold: PIXEL_THRESHOLD,
  });
  const ratio = diffPixels / (baseline.width * baseline.height);
  if (ratio > MAX_DIFF_RATIO) {
    fs.writeFileSync(path.join(TMP_DIR, `diff-${name}`), PNG.sync.write(diff));
    return { name, status: "diff", ratio };
  }
  return { name, status: "match" };
}

async function main() {
  const mode = process.argv[2] ?? "check";
  if (!["capture", "check"].includes(mode)) {
    console.error("usage: node scripts/snapshots.mjs [capture|check]");
    process.exit(2);
  }
  buildHarness();
  const server = await serve();
  try {
    if (mode === "capture") {
      console.log(`Capturing ${SCREEN_STATES.length * 4} snapshots → ${path.relative(APP_DIR, BASELINE_DIR)}/`);
      await captureAll(BASELINE_DIR);
      console.log("Done. Review the PNGs like a designer, then commit them.");
      return;
    }
    console.log(`Checking against ${path.relative(APP_DIR, BASELINE_DIR)}/…`);
    const files = await captureAll(TMP_DIR);
    const results = files.map(compareOne);
    let failed = 0;
    for (const r of results) {
      if (r.status === "match") continue;
      failed += r.status === "diff" || r.status === "size-mismatch" || r.status === "missing" ? 1 : 0;
      const detail = r.status === "diff" ? ` (${(r.ratio * 100).toFixed(2)}% pixels)` : "";
      console.log(`  ${r.status.toUpperCase()}: ${r.name}${detail}`);
    }
    const matched = results.filter((r) => r.status === "match").length;
    console.log(`${matched}/${results.length} match.`);
    if (results.some((r) => r.status === "bootstrapped")) {
      console.log("Baselines were bootstrapped — commit snapshots-ci/ from this run's output.");
    }
    if (results.some((r) => r.status === "missing")) {
      console.log("Missing baselines — run `npm run test:snapshots` to create them.");
    }
    if (failed > 0) {
      console.log(`Diff images written to ${path.relative(APP_DIR, "snapshots-tmp")}/diff-*.png`);
      process.exit(1);
    }
  } finally {
    server.close();
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
