# Changelog

All notable changes to Squash are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) (see "Versioning & changelog"
in [CONTRIBUTING.md](CONTRIBUTING.md)).

## [1.0.0] - 2026-08-15

First public release. Windows and Linux, desktop GUI + CLI.

### Added

- **`squash-core` compression engine** (Rust): single worker queue with cooperative cancel and
  progress reporting, shared by the GUI and CLI.
  - Compress: zip, 7z, tar.gz, tar.zst, plus single-file gz / xz / zst.
  - Extract: all of the above plus tar, tar.bz2, tar.xz, and rar.
  - Three presets (fast / balanced / max) mapped to clear levels per format.
  - RAR extraction via vendored RARLAB UnRAR 7.2.7 (`vendor/unrar/`) behind a small C-ABI shim
    (`crates/unrar-sys`); extract-only — Squash never creates RAR. `--no-default-features`
    produces a libre build without it.
- **CLI** (`crates/squash-cli`): `squash c` / `squash x` with full core parity — `--format`,
  stdout piping, machine-readable `--json` (JSONL) output, documented deterministic exit codes,
  verbose logging via `-v` / `SQUASH_LOG` (stderr only).
- **Desktop GUI** (`app/`, Tauri v2 + React/TypeScript): drag-and-drop onto the window, batch
  queue with per-job progress / ETA / cancel / retry, compress and extract flows, settings,
  first-launch welcome sheet, and loading / empty / error / success states on every
  data-bound screen.
- **i18n and theming**: English + Arabic with full RTL layout, light and dark themes.
- **OS integration**: file associations for 11 archive formats on Windows and Linux, open-with
  routing (cold and warm start), single-instance behavior, NSIS context-menu verbs (Windows),
  Linux desktop entry.
- **Opt-in crash reporting** (Sentry): default off, in-app consent, path scrubbing before send,
  DSN injected at build time via `SQUASH_SENTRY_DSN` — self-built binaries have it disabled.
- **Opt-in update checks** (default off): stable and beta channels via signed updater manifests
  (`tauri-plugin-updater`).
- **Benchmark harness** (`crates/squash-bench`): seeded synthetic corpus, honest level mapping
  vs system tools, regression gate against `benches/baseline.json`.
- **Distribution automation**: tag-driven release workflow (draft GitHub Releases, per-OS
  artifacts, `SHA256SUMS.txt`, updater manifests; signing steps gated on secrets), Homebrew /
  Scoop publish jobs, and winget / Flatpak / AUR packaging templates.
- **Test infrastructure**: round-trip corpus tests, crafted-archive attack tests, GUI snapshot
  matrix (screens × states × EN/AR × light/dark), and WebdriverIO E2E against the real binary.

### Security

- **Zip-slip / path-traversal protection, non-bypassable by format handlers**: lexical path
  sanitization plus *physical* symlink resolution — canonicalizes every existing prefix so a
  planted symlink chain can't escape the destination; no writes through symlink ancestors.
- **Decompression-bomb guard** (`ExtractGuard`): aborts on >200× expansion past a 64 MiB floor,
  1 TiB absolute, or 1M entries — counting *actual bytes written*, never header-declared sizes —
  and rolls back everything the job created.
- **UnRAR C-ABI shim hardening**: fixed a heap write past `std::wstring` size (Windows),
  capped symlink-target reads at 8 KiB, reject negative/oversized decode chunks from the C side,
  bounded wide-string scans.
- **Fuzzing**: six cargo-fuzz targets over the archive parsers plus a weekly CI fuzz job;
  found and fixed a null-pointer UB in the unrar trampoline (regression fixture pinned in
  `fixtures/`).
- **No silent telemetry**: all phone-home behavior (crash reports, update checks) is off by
  default and requires explicit user opt-in.

[1.0.0]: https://github.com/qtrcipher/squash/releases/tag/v1.0.0
