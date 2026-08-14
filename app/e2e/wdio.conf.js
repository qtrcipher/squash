/**
 * WebdriverIO config for the Squash E2E layer (docs/05 §6, Phase 5).
 *
 * Drives the REAL built app (target/debug/squash-app, cargo feature `e2e`)
 * through the embedded WebDriver server (tauri-plugin-wdio-webdriver) —
 * no external tauri-driver/WebKitWebDriver/Edge driver, which is also why
 * this works on macOS (WKWebView has no standalone WebDriver).
 *
 * One scenario = one wdio invocation = one fresh app launch (see run.mjs):
 *   SQUASH_E2E_APP_ARGS  JSON array of argv for the app (OS "open with"
 *                        routing, docs/03 F6 — the native-dialog bypass)
 *   SQUASH_E2E_WORK      scenario work dir (fixtures + expected outputs)
 *   SQUASH_E2E_ARTIFACTS screenshot output dir
 *   SQUASH_STORE_DIR     throwaway settings/queue store (squash-core hook)
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const appDir = path.resolve(here, "..");
const repoRoot = path.resolve(appDir, "..");

const binaryName = process.platform === "win32" ? "squash-app.exe" : "squash-app";
const appBinary = path.join(repoRoot, "target", "debug", binaryName);
const artifacts = process.env.SQUASH_E2E_ARTIFACTS ?? path.join(here, "artifacts");
const appArgs = JSON.parse(process.env.SQUASH_E2E_APP_ARGS ?? "[]");

export const config = {
  runner: "local",
  specs: [path.join(here, "specs", "*.spec.js")],
  maxInstances: 1,

  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": {
        application: appBinary,
        args: appArgs,
      },
    },
  ],

  services: [
    [
      "@wdio/tauri-service",
      {
        appBinaryPath: appBinary,
        appArgs,
        driverProvider: "embedded",
        // Debug builds + cold CI machines start slowly.
        startTimeout: 90000,
        statusPollTimeout: 10000,
      },
    ],
  ],

  framework: "mocha",
  reporters: ["spec"],
  logLevel: "warn",
  waitforTimeout: 30000,
  mochaOpts: {
    ui: "bdd",
    timeout: 120000,
  },

  // The service's direct-eval channel polls `window.__wdio_original_core__`
  // for 5s per call when absent (it's normally installed by the
  // @wdio/tauri-plugin guest-js, which we deliberately don't ship — happy
  // paths need no mocking). Point it at the real core once the app UI has
  // loaded, so the per-command window-focus check fails fast instead.
  before: async () => {
    await browser.waitUntil(
      () => browser.execute(() => typeof window.__TAURI__ !== "undefined"),
      { timeout: 30000, timeoutMsg: "app UI never loaded (window.__TAURI__ missing)" },
    );
    await browser.execute(() => {
      window.__wdio_original_core__ = window.__TAURI__.core;
    });
  },

  // Failure evidence (docs/05 §6 RTL visual check uploads the success shots
  // itself; these catch everything else).
  afterTest: async function (test, _context, { passed }) {
    if (passed) return;
    const safe = test.title.replace(/[^a-z0-9]+/gi, "-").toLowerCase();
    try {
      await browser.saveScreenshot(path.join(artifacts, `FAIL-${safe}.png`));
    } catch {
      // App may already be gone; the WDIO log is the evidence then.
    }
    // Host-side queue view (authoritative, docs/06 §7): distinguishes "row
    // stuck because the engine never ran it" (host: queued/running) from
    // "progress event lost on the way to the webview" (host: terminal).
    try {
      const queue = await browser.execute(() =>
        window.__TAURI__.core.invoke("list_queue"),
      );
      fs.writeFileSync(
        path.join(artifacts, `FAIL-${safe}-queue.json`),
        JSON.stringify(queue, null, 2),
      );
    } catch {
      // Same as above — the screenshot may still be enough.
    }
  },
};
