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
> session start/end routine) load automatically from `AGENTS.md` — deliberately not duplicated here.
> **NOT an iOS app**: ignore all App Store / ASC / Xcode / Firebase instructions wherever
> they appear (see `AGENTS.md`).
> Work top to bottom; phases are in order. AI: read this first — if a task is checked,
> confirm before redoing it; update this file and commit at session end.

## Phase 0 — Plan (GATE: no implementation code until every item below is checked)
- [x] Problem, users, MVP scope, success metrics → `docs/01-product-scope.md` — `product-strategist` agent · `product-frameworks` · `brainstorming` (done 2026-08-13; 4 open questions await owner)
- [x] Market check: competitors, demand, keywords → `docs/02-market-check.md` — `app-market-research` (done 2026-08-13; verdict: Go)
- [x] Screens, flows, all four UI states per screen → `docs/03-ux-flows.md` — `ux-designer` agent (done 2026-08-13)
- [x] Design direction: style, palette, typography → `docs/04-design-direction.md` — `ui-ux-pro-max` (done 2026-08-13; platform-native flat, tokens locked)
- [x] Architecture + module plan → `docs/05-architecture.md` — `technical-architect` agent (done 2026-08-13; Rust core + Tauri v2 GUI)
- [x] Data model: config, presets, job history (local-only, no backend) → `docs/06-data-model.md` — `database-design-patterns` (done 2026-08-13; TOML/JSONL local stores)

> **Phase 0 gate: PASSED 2026-08-13.** Open owner decisions (do not block the gate, resolve before the phases they touch):
> 1. RAR messaging sign-off: "extracts RAR, never creates it" (RARLAB license) — touches Phase 2/6 copy.
> 2. Opt-in crash reporting posture — touches Phase 3 (currently unmodeled in data model).
> 3. Donations from day one vs wait — touches Phase 6.
> 4. User-created presets: docs 01/03 say "exactly 3 presets" but doc 06 models user presets — doc 06 proposal: GUI lists them, no preset editor in v1. Sign off or adjust.

## Phase 1 — Foundation
- [x] Public GitHub repo; LICENSE (OSS), README, CONTRIBUTING; `.gitignore` covers secrets BEFORE first commit (done 2026-08-13: github.com/qtrcipher/squash, MIT+Apache-2.0 dual)
- [x] Project scaffold: shared compression core lib + CLI shell + desktop GUI shell (done 2026-08-13: cargo workspace, `squash-core` API surface + store types, clap CLI shell, Tauri v2 + React shell; 24 Rust + 2 frontend tests green)
- [x] CI: GitHub Actions build/test matrix (macOS / Linux / Windows) — `release-automation` (done 2026-08-13: `.github/workflows/ci.yml`, fmt+clippy+test × 3 OS, frontend job, `[ci skip]` honored on pushes)
- [x] i18n scaffold EN/AR (RTL) from day one — `i18n-patterns` · `arabic-localization` (done 2026-08-13: react-i18next, en/ar locales w/ MSA Arabic, `dir="rtl"` switch, key-parity test)
- [x] App icon — `icon-design-guide` · `art-asset-designer` agent (done 2026-08-13: pressed-package mark, `assets/brand/` + full Tauri icon set)
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
- 2026-08-13 — Phase 2 first slice: engine executes jobs (single worker thread, FIFO, cooperative cancel, progress channel); formats zip + tar.gz + tar.zst (create/extract) and tar + tar.bz2 + tar.xz (extract-only) via zip/tar/flate2/zstd/bzip2/xz2; non-bypassable zip-slip sanitizer (`safety.rs`) + docs/03 F3 extract layout rule; CLI `squash c/x` wired to engine with human summary + `--json` JSONL schema + documented exit codes; 84 Rust tests green (unit/integration/CLI e2e incl. crafted zip-slip + corrupt-archive cases) — next: Phase 2 7z + rar handlers, then single-file codecs (gz/xz/zst)
- 2026-08-13 — Phase 1 complete: public repo qtrcipher/squash, MIT+Apache-2.0, README/CONTRIBUTING; cargo workspace scaffold (core API surface, CLI/bench shells, Tauri+React GUI shell w/ EN/AR RTL + design tokens); CI matrix; brand icon — next: Phase 2 core compress/extract (zip first)
- 2026-08-13 — added project `AGENTS.md`: house rule — not an iOS app, ignore ASC/Xcode/Firebase instructions — next: Phase 1 repo setup + scaffold
- 2026-08-13 — Phase 0 complete: wrote docs/01–06 (product scope, market check = Go, UX flows, design direction, Rust+Tauri architecture, local data model); gate passed; 4 owner decisions listed under Phase 0 — next: Phase 1 repo setup (public GitHub repo, LICENSE, README) + project scaffold
- 2026-08-13 — scaffolded this roadmap from ios-ship-gate template (adapted: cross-platform OSS, app named "Squash", desktop GUI + CLI) — next: Phase 0 product scope + market check
