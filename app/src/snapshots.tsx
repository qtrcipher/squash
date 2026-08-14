/**
 * Snapshot harness entry — dev-only, NOT part of the shipped bundle.
 * vite.config.ts builds this page (snapshots.html) only when SNAPSHOT_MOCK=1,
 * with the Tauri bridge aliased to src/testing/mock-tauri.ts, so the REAL App
 * renders every screen × state with deterministic data and no backend.
 *
 * Scenario from URL params (consumed by the mock):
 *   ?screen=s1|s2|s3|s4|s6|s7|d3&state=…&lang=en|ar&theme=light|dark
 *
 * Fonts: the design stack's bundled fallbacks (JetBrains Mono, Noto Sans
 * Arabic — docs/04 §3) are pinned first-class here so macOS and CI (Linux)
 * captures render the same Arabic/mono glyphs; Latin UI text stays on system
 * fonts, which is exactly why baselines are per-platform (see
 * scripts/snapshots.mjs header).
 */
import React, { useEffect } from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./i18n";
import "./styles.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/noto-sans-arabic/400.css";
import "@fontsource/noto-sans-arabic/600.css";
import "./testing/snapshot-harness.css";

/** S6 opens through the real toolbar button, like a user would. */
function HarnessDriver() {
  useEffect(() => {
    const screen = new URLSearchParams(window.location.search).get("screen");
    if (screen !== "s6") return;
    const timer = window.setInterval(() => {
      const button = document.querySelector<HTMLButtonElement>(".toolbar .button");
      if (button) {
        button.click();
        window.clearInterval(timer);
      }
    }, 25);
    return () => window.clearInterval(timer);
  }, []);
  return null;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
    <HarnessDriver />
  </React.StrictMode>,
);
