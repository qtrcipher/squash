# Squash — Open Source Roadmap

> Cross-platform file compressor (macOS / Windows / Linux) — desktop GUI + CLI on a
> shared core, open source on GitHub. Goal: better than the common compressors
> (7-Zip, WinRAR, Keka, PeaZip, …).
>
> THE todo list. Adapted from the `ios-ship-gate` skill's `references/progress-template.md`
> for a cross-platform OSS app (iOS/App Store/Firestore items replaced with OSS equivalents).
> Checkboxes = where I am. Tag convention: bare `` `name` `` = skill; `` `name` `` agent = subagent;
> a tag on a phase HEADER applies to every item in that phase.
> House rules (RTL, light/dark, EN/AR i18n, four UI states, conventional commits,
> session start/end routine) load automatically from CLAUDE.md — deliberately not duplicated here.
> Work top to bottom; phases are in order. AI: read this first — if a task is checked,
> confirm before redoing it; update this file and commit at session end.

## Phase 0 — Plan (GATE: no implementation code until every item below is checked)
- [ ] Problem, users, MVP scope, success metrics → `docs/01-product-scope.md` — `product-strategist` agent · `product-frameworks` · `brainstorming`
- [ ] Market check: competitors, demand, keywords → `docs/02-market-check.md` — `app-market-research`
- [ ] Screens, flows, all four UI states per screen → `docs/03-ux-flows.md` — `ux-designer` agent
- [ ] Design direction: style, palette, typography → `docs/04-design-direction.md` — `ui-ux-pro-max`
- [ ] Architecture + module plan → `docs/05-architecture.md` — `technical-architect` agent
- [ ] Data model: config, presets, job history (local-only, no backend) → `docs/06-data-model.md` — `database-design-patterns`

## Phase 1 — Foundation
- [ ] Public GitHub repo; LICENSE (OSS), README, CONTRIBUTING; `.gitignore` covers secrets BEFORE first commit
- [ ] Project scaffold: shared compression core lib + CLI shell + desktop GUI shell
- [ ] CI: GitHub Actions build/test matrix (macOS / Linux / Windows) — `release-automation`
- [ ] i18n scaffold EN/AR (RTL) from day one — `i18n-patterns` · `arabic-localization`
- [ ] App icon — `icon-design-guide` · `art-asset-designer` agent
- [x] Scaffold this file into the repo as `PROGRESS.md` — `ios-ship-gate` (template in its references/)

## Phase 2 — Features
- [ ] Core: compress/extract (zip, 7z, tar, gz, zstd, …), presets, drag-and-drop — `state-management` · `persistence-patterns`
- [ ] CLI: full parity with the core (scriptable, pipe-friendly)
- [ ] OS integration: file associations, context menus / Finder integration
- [ ] Onboarding + in-app guides
- [ ] Accessibility (full keyboard nav, screen reader, per-platform conventions) — `accessibility-specialist` agent
- [ ] Verbose/debug logging mode for support issues

## Phase 3 — Quality & Security
- [ ] Security audit: zip-slip / path traversal, malicious/crafted archives, decompression bombs — `security-checklist`
- [ ] Crash reporting: opt-in, OSS-friendly (no silent telemetry) — `error-monitoring`
- [ ] Performance: benchmark suite vs 7-Zip/WinRAR/Keka (ratio + speed), tracked over time — `performance-optimizer` agent

## Phase 4 — Distribution
- [ ] GitHub Releases: signed binaries per OS (notarized macOS, signed Windows) — `release-automation`
- [ ] Package managers: Homebrew, winget/Scoop, apt/Snap/Flatpak, AUR
- [ ] Update checks (opt-in) + release channels (stable/beta)

## Phase 5 — Testing (house bar: 70% unit / 20% integration / 10% E2E)
- [ ] Tests written WITH each feature (not after)
- [ ] Every control × every state: disabled, loading, empty, error
- [ ] GUI snapshots: AR/EN × light/dark
- [ ] Round-trip corpus tests + fuzzing of archive parsers
- [ ] Suite green on all three OSes (CI matrix)
- [ ] Bugs found → root-cause first — `systematic-debugging`

## Phase 6 — Ship v1.0.0
- [ ] README polish: screenshots, GIF demo, feature matrix vs competitors
- [ ] Changelog + semver discipline
- [ ] v1.0.0 GitHub Release, announcement posts (OSS communities)

## Session log
Format, newest first, one line per session: `YYYY-MM-DD — what changed — next: <task>`
- 2026-08-13 — scaffolded this roadmap from ios-ship-gate template (adapted: cross-platform OSS, app named "Squash", desktop GUI + CLI) — next: Phase 0 product scope + market check
