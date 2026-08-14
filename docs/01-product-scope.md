# Squash — Phase 0: Product Scope

> **Amendment 2026-08-14 — macOS target dropped.** Supported platforms are now
> **Windows + Linux only** (owner decision: Apple signing embeds a personal
> legal name in binaries — a privacy concern; macOS may return later via an
> organization account). This document is a historical planning record; any
> mention of macOS as a supported/shipped platform below is superseded. The
> codebase stays portable — only builds, packaging, and platform claims changed.

> Status: planning gate. No implementation code until Phase 0 docs are complete.
> Owner of this document: `product-strategist`. Scope changes require re-baselining here first.

## 1. Problem Statement

Compressing and extracting files is a solved *algorithm* problem but an unsolved *product* problem. The concrete pains:

- **Fragmented defaults, no great cross-platform option.** 7-Zip is excellent on Windows but effectively Windows-only (its GUI is dated and its macOS/Linux story is a bare CLI). Keka is Mac-only and paid on the App Store. The Unarchiver only extracts. WinRAR is nagware with a 1998 UI. PeaZip is free but cluttered. No single tool gives a modern GUI **and** a scriptable CLI on all three desktop OSes.
- **Format roulette.** Users receive `.zip`, `.7z`, `.rar`, `.tar.gz`, `.zst` and guess which tool opens what. Default OS handlers cover zip and little else; everything else is "install something."
- **Weak batching and automation UX.** Compressing 30 folders into 30 archives, or re-compressing nightly backups, means writing shell for-loops by hand or clicking through a GUI 30 times. Existing GUIs treat batch jobs as an afterthought; existing CLIs have inconsistent, memorization-heavy flags (`tar -xvf` vs `7z x` vs `unzip`).
- **Opaque defaults.** Users can't tell what "normal" vs "maximum" compression actually costs them in time, and tools never show predicted output size or ETA.
- **Trust gap.** WinRAR's license nag, WinZip's trialware, and Keka's App Store paywall push users to sketchy download mirrors. A trustworthy, free, open-source tool with signed binaries is genuinely differentiated.

Core problem in one sentence: **people who move files between machines lack one trustworthy, modern tool that compresses and extracts any common format the same way on every OS, from both GUI and terminal.**

## 2. Target Users

**P1 — Sam, the everyday desktop user.**
Receives weird archive formats at work/school; wants to double-click anything and extract it, and to zip up a folder to email. Low tech savviness. Pain: "which app opens `.7z`?" and nag/paywall surprises. JTBD: *open any archive, make a zip, done.* Aha moment: dropping a `.rar` file on Squash and it just extracts.

**P2 — Dana, the developer / power user.**
Lives on macOS or Linux, hops between machines and OSes. Wants sane, consistent CLI flags, pipes, presets (e.g. "web assets → brotli"), and visible ratio/speed feedback. Pain: memorizing five different tools' syntax; GUI tools that don't script. JTBD: *one command, one config, works identically everywhere.* Aha moment: `squash c src.tar.zst src/` with a progress bar and a summary of bytes saved.

**P3 — Morgan, the IT / automation scripter.**
Maintains backup and packaging scripts on mixed Windows/Linux fleets. Wants deterministic exit codes, machine-readable output, batch jobs from a file list, and no telemetry surprises. Pain: GUI tools unusable in scripts; per-OS tooling drift. JTBD: *script once, run everywhere, trust the exit code.*

**Explicitly NOT for:** mobile users (no iOS/Android v1), users needing forensic/archival-grade format archaeology (ARJ, LZH, CAB), or enterprise archive management suites.

## 3. MVP Scope (v1.0.0)

Prioritization: MoSCoW below; "Every yes has a cost" — anything beyond this list delays v1 by ~2+ weeks each and is deferred on that basis alone.

### Must Have (IN for v1.0.0)

1. **Shared compression core + CLI + GUI on one core.** Single library; CLI and GUI are thin shells. (This is the product's reason to exist.)
2. **Extract:** zip, 7z, rar, tar, tar.gz/tgz, tar.bz2, tar.xz, gz, xz, zstd. Zip-slip / path-traversal protection on by default.
3. **Compress:** zip, 7z, tar.gz, tar.zst — with three presets (fast / balanced / max) mapped to clear levels. No per-codec flag jungle in the GUI.
4. **GUI essentials:** drag-and-drop files/folders onto the window; compress and extract with progress, ETA, and before/after sizes; batch queue (compress many items in one run). All data-bound screens ship loading / empty / error / success states.
5. **CLI:** `squash c` / `squash x` with the same presets, `--format`, stdout piping, machine-readable `--json` output, deterministic exit codes.
6. **i18n from day one:** EN + AR with full RTL layout, light/dark themes (house rule).
7. **Distribution:** GitHub Releases with signed macOS (notarized) and Windows binaries + plain archives; Homebrew and Scoop/winget submission.
8. **Benchmark harness:** repeatable ratio/speed suite vs 7-Zip on a standard corpus, run in CI. (Needed to prove Section 4 claims — this is a product requirement, not a nicety.)

### Explicitly OUT for v1.0.0

- Archive *editing* (add/remove files inside an existing archive) — high complexity, low reach.
- Encryption / password-protected archives — security scope deserves its own phase, not a rushed v1 feature.
- Self-extracting archives, archive repair, volume splitting — niche, power-user-later.
- File-manager replacement features (browse inside archives as folders, preview pane) — scope magnet.
- Cloud storage integration (S3/Drive/Dropbox) — different product.
- Mobile (iOS/Android) and Linux distro packages beyond a tarball + Flatpak — post-v1.
- Auto-updater — opt-in update *check* only in v1; in-app updating is a fast-follow.

### Backlog (post-v1, rough priority)

P1 fast-follows: archive editing, encrypted zip/7z create+extract, context-menu / Finder integration polish, in-app updater, Flatpak/AUR packages.
P2 later: volume splitting, SFX, recovery records, plugin API for new codecs, portable mode.

## 4. Positioning

**Squash is the free, open-source compressor that works the same everywhere: one modern GUI and one scriptable CLI on macOS, Windows, and Linux, opening every common archive format with sane defaults, honest progress, and no nags, trials, or telemetry.** Where 7-Zip wins on ratio but loses on UX and platform reach, and Keka wins on Mac polish but loses everywhere else, Squash's bet is that *consistency + trust + automation* beat another point of compression ratio.

Claims engineering must prove (these become benchmark/test requirements):

- **Better than 7-Zip because** we ship a native-feeling modern GUI on all three OSes, not just Windows — and match or beat its default ratio/speed on the standard corpus (benchmark suite, CI-tracked).
- **Better than WinRAR/WinZip because** fully free and open source — no nagware, no trial wall — and rar *extraction* works out of the box.
- **Better than Keka because** cross-platform and scriptable: same presets and behavior from GUI and CLI.
- **Better than gzip/xz/zstd CLIs because** one consistent command surface (`squash c`/`squash x`) with progress, `--json`, and sane exit codes — no flag memorization.
- **Better than The Unarchiver because** we compress too, not just extract.

## 5. Success Metrics

Realistic for a solo-maintainer OSS utility; measured from public launch (v1.0.0 release date).

| Metric | 6 months | 12 months |
|---|---|---|
| GitHub stars | 1,500 | 5,000 |
| Release downloads (all OSes) | 20k | 100k |
| Homebrew installs (analytics) | 3k | 15k |
| External contributors (merged PRs) | 10 | 30 |
| Benchmark | publish ratio/speed vs 7-Zip; within 5% of its ratio at faster default speed | at least one published corpus where Squash wins on ratio or speed at matched settings |
| Issue health | median first response < 72h; < 20 open bugs | median first response < 48h; < 15 open bugs |
| Activation proxy | 60% of GUI first-launches complete one compress/extract (opt-in, local-only counter users can inspect) | 70% |

North-star: weekly active machines (opt-in update-check ping count) — target 5k at 6 months, 25k at 12.

## 6. Open Questions (owner decisions needed)

1. **GUI toolkit** — Tauri, Electron, Qt, or native-per-OS? This is the biggest effort/risk fork in the roadmap; needs architecture input (feeds `docs/05-architecture.md`), but the *product* constraint is fixed: must feel native, ship small binaries, and support RTL well. Owner call after architect's recommendation.
2. **RAR compression** — RARLAB's license forbids creating rar archives in third-party tools. Confirmed out of scope forever; needs owner sign-off on messaging ("extracts rar, never creates it").
3. **Crash reporting** — opt-in Sentry-style reporting vs none at all? OSS community cares; lean opt-in, but it's the owner's trust posture to set. **RESOLVED (owner): opt-in Sentry.** Default off; consent via the S7 checkbox (unchecked) and the S6 toggle; DSN via build-time `SQUASH_SENTRY_DSN`, never committed; builds without it show the toggle disabled. What's sent and the scrub rules are documented in docs/06 §6 "Crash reporting".
4. **Donations/sustainability** — GitHub Sponsors/Open Collective from day one, or stay clean until traction exists? (No paywall either way; house rule stands.)
