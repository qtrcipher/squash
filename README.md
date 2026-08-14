# Squash

**Open-source file compressor for macOS, Windows, and Linux — desktop GUI + CLI on one shared core.**

> Status: early development (pre-alpha). The planning docs are done; the code is being scaffolded.

## Install

**No releases yet** — the commands below go live with the first release (`v0.1.0`). Until then, build from source (see [CONTRIBUTING.md](CONTRIBUTING.md)).

- **macOS / Linux (CLI)** — `brew install qtrcipher/tap/squash`
- **macOS (GUI)** — `brew install --cask qtrcipher/tap/squash`
- **Windows (GUI)** — `winget install qtrcipher.Squash`
- **Windows (CLI)** — `scoop bucket add squash https://github.com/qtrcipher/scoop-bucket && scoop install squash`
- **Linux (GUI)** — download the `.deb` / `.rpm` / `.AppImage` from [Releases](https://github.com/qtrcipher/squash/releases) and install with your package manager (`sudo dpkg -i squash_*.deb`, `sudo dnf install squash-*.rpm`); Flatpak via Flathub is planned.
- **Arch Linux** — `yay -S squash` (AUR, planned after the first release)

Every release publishes `SHA256SUMS.txt` (GPG-signed once configured) so you can verify downloads.

Squash aims to beat the common compressors where they actually hurt: dated UIs, nagware, platform lock-in, and weak automation — not by chasing another fraction of compression ratio.

- **One app, every desktop OS** — same UI, same CLI, same behavior on macOS, Windows, and Linux.
- **Modern formats, first-class** — zstd/brotli alongside zip, 7z, tar.gz, and friends.
- **GUI for humans, CLI for scripts** — drag-and-drop and batch queues in the app; deterministic exit codes and `--json` in the terminal.
- **Trustworthy by construction** — open source, signed releases, zip-slip protection, no ads, no nagware, no silent telemetry (crash reporting is opt-in only — off unless you turn it on; see `docs/06-data-model.md` §6).

## Project layout

```
crates/squash-core   # compression engine: jobs, formats, presets (Rust)
crates/squash-cli    # command-line interface
crates/squash-bench  # benchmark harness (vs 7-Zip & co.)
app/                 # desktop GUI (Tauri v2 + React/TypeScript)
docs/                # planning: product, market, UX, design, architecture, data model
packaging/           # package-manager templates + publish scripts (Homebrew, Scoop, winget, Flatpak, AUR)
```

## Documentation

| Doc | Contents |
|---|---|
| [docs/01-product-scope.md](docs/01-product-scope.md) | Problem, users, MVP scope, success metrics |
| [docs/02-market-check.md](docs/02-market-check.md) | Competitor landscape, complaints, gaps |
| [docs/03-ux-flows.md](docs/03-ux-flows.md) | Screens, flows, UI states |
| [docs/04-design-direction.md](docs/04-design-direction.md) | Style, palette, typography, tokens |
| [docs/05-architecture.md](docs/05-architecture.md) | Stack, modules, format strategy |
| [docs/06-data-model.md](docs/06-data-model.md) | Local settings, presets, history |

Progress is tracked in [PROGRESS.md](PROGRESS.md); house rules for contributors (human or AI) in [AGENTS.md](AGENTS.md).

## Building

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.

RAR note: Squash **extracts** RAR archives (via RARLAB's unrar source, license-compatible) but will never **create** them — the RAR compression algorithm is proprietary. Use 7z or tar.zst instead; they're better anyway.
