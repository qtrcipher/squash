import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// SNAPSHOT_MOCK=1 builds the dev-only snapshot harness (snapshots.html) with
// the Tauri bridge aliased to a deterministic mock (src/testing/mock-tauri.ts).
// The production build (Tauri, index.html) is untouched — no alias, no extra
// entry point.
const snapshotMock = process.env.SNAPSHOT_MOCK === "1";
const tauriMock = fileURLToPath(new URL("./src/testing/mock-tauri.ts", import.meta.url));

// Tauri expects a fixed port in dev and a relative asset base in prod.
export default defineConfig({
  plugins: [react()],
  base: "./",
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  test: {
    environment: "node",
    // app/e2e/specs/*.spec.js are WebdriverIO (mocha) specs, not vitest.
    exclude: ["**/node_modules/**", "**/dist/**", "e2e/**"],
  },
  ...(snapshotMock && {
    resolve: {
      alias: [
        { find: /^@tauri-apps\/api\/core$/, replacement: tauriMock },
        { find: /^@tauri-apps\/api\/event$/, replacement: tauriMock },
        { find: /^@tauri-apps\/api\/webview$/, replacement: tauriMock },
        { find: /^@tauri-apps\/plugin-dialog$/, replacement: tauriMock },
      ],
    },
    build: {
      outDir: "dist-snapshots",
      rollupOptions: {
        input: fileURLToPath(new URL("./snapshots.html", import.meta.url)),
      },
    },
  }),
});
