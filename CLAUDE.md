# Squash — Project House Rules

These override/extend the global `~/.claude/CLAUDE.md` for this repo.

## What this project is
- **Squash**: open-source, cross-platform (macOS / Windows / Linux) file compressor.
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
