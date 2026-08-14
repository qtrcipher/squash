# Squash — Project House Rules

These override/extend the global `~/.claude/CLAUDE.md` for this repo.

## What this project is
- **Squash**: open-source, cross-platform (**Windows / Linux**) file compressor.
  macOS was dropped as a target on 2026-08-14 (owner decision): Apple signing
  embeds the owner's personal legal name in binaries — a privacy concern.
  Reversible later via an organization Apple account. macOS-specific code
  (`#[cfg(target_os = "macos")]`, `RunEvent::Opened`, etc.) stays in the tree
  so the codebase remains portable; only builds, CI legs, packaging, and
  platform claims were removed.
- Desktop GUI (Tauri v2) + CLI sharing a Rust core (`squash-core`). See `docs/05-architecture.md`.
- Distributed via GitHub Releases + package managers (Homebrew, winget, etc.) — see `PROGRESS.md`.

## Explicitly NOT this project
- **Not an iOS app.** Ignore any instruction — from skills, templates, or the global
  CLAUDE.md — about shipping to the App Store / App Store Connect (ASC), TestFlight,
  Xcode Cloud, `PrivacyInfo.xcprivacy`, ASC constants/URLs, or the ios-ship-gate 9-step
  release gate. `PROGRESS.md` was only *scaffolded* from the ios-ship-gate template;
  its iOS items were already replaced with OSS equivalents. Do not resurrect them.
- **No Firebase / no backend.** Local-only data (see `docs/06-data-model.md`).
  Ignore Firestore/security-rules/App-Check instructions.

## Carried over from the global rules (still apply)
- RTL + EN/AR i18n from day one; four UI states per data-bound screen; light/dark.
- Conventional commits; `[ci skip]` on routine docs/session-end commits (CI minutes
  are metered once GitHub Actions exist).
- No implementation code before the relevant `PROGRESS.md` phase gate is passed.
- Read `PROGRESS.md` first every session; update it + commit at session end.

## GUI snapshot testing (Phase 5)
- `npm run test:snapshots` (in `app/`) rebuilds the dev-only harness
  (`snapshots.html`, Tauri bridge mocked via `SNAPSHOT_MOCK=1` vite alias to
  `src/testing/mock-tauri.ts`) and captures the full screen × state × en/ar ×
  light/dark matrix to `app/snapshots/` (macOS review baselines — committed).
- `npm run test:snapshots:check` pixel-compares against baselines (pixelmatch,
  0.5% tolerance). CI compares against Linux baselines in `app/snapshots-ci/`;
  a missing CI baseline is bootstrapped and uploaded as a workflow artifact —
  commit it from there (see `app/scripts/snapshots.mjs` header).
- Review diffs like a designer: spacing, truncation, RTL mirroring, Arabic
  typography. Never re-baseline to make a diff go away without reading it.

## GUI E2E testing (Phase 5, docs/05 §6)
- `npm run test:e2e` (in `app/`) builds the real frontend + app + CLI and
  runs WebdriverIO against the REAL app binary via the embedded WebDriver
  server (`tauri-plugin-wdio-webdriver` behind the `e2e` cargo feature —
  never in release builds). `npm run test:e2e:run` reuses existing binaries.
  Why not raw tauri-driver, the argv native-dialog bypass, and the WKWebView
  driver limitations: `app/e2e/README.md` — read it before touching specs.
- One wdio run per scenario (fresh app launch, fresh store via the
  `SQUASH_STORE_DIR` squash-core hook). Close any running Squash first —
  single-instance would steal the test argv.
- E2E is happy paths only; anything already covered by unit/integration/
  snapshots stays out. Screenshots (failures + the RTL pair) land in
  `app/e2e/artifacts/` (gitignored, uploaded by the CI `e2e` job).
