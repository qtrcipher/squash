# Contributing to Squash

Thanks for your interest! Squash is in early development — the best way to help right now is to open an issue with ideas, bug reports, or format requests.

## Prerequisites

- **Rust** (stable, via [rustup](https://rustup.rs)) — core, CLI, and GUI host
- **Node.js** 20+ and npm — GUI frontend
- Platform extras for Tauri: see <https://v2.tauri.app/start/prerequisites/>

## Build & test

```sh
cargo build            # core + CLI + bench
cargo test             # all Rust tests
cd app && npm install && npm run build   # GUI frontend
```

## Crash reporting (Sentry)

Crash reporting is **opt-in** (docs/06 §6) and needs a Sentry DSN at build time. Self-built and CI builds without a DSN have the feature disabled — the consent toggles render as "not available in this build".

To produce a build with crash reporting available:

1. Create a Sentry project (sentry.io → New Project → platform "Rust"; one project serves the GUI host, frontend, and CLI).
2. Copy its DSN (Project Settings → Client Keys) — a DSN is public by design (it ships inside the released binary), but **never commit it**.
3. Build with the env var set (release CI should inject it from secrets):

   ```sh
   SQUASH_SENTRY_DSN="https://<key>@o<org>.ingest.sentry.io/<project>" cargo build --release
   ```

`option_env!` tracks the variable, so changing or unsetting it re-triggers compilation of `squash-core` — no manual `cargo clean` needed.

## Releasing

Releases are built by `.github/workflows/release.yml`. Everything is driven by the version tag.

### Cutting a release

1. Bump the version in all three places (the workflow fails the release if they don't match the tag):
   - `Cargo.toml` → `[workspace.package] version` (the Rust crates inherit it)
   - `app/src-tauri/tauri.conf.json` → `version`
   - `app/package.json` → `version`
2. Commit (`chore(release): vX.Y.Z`), tag, push:
   ```sh
   git tag vX.Y.Z && git push origin main vX.Y.Z
   ```
   Alternatively: Actions → **Release** → Run workflow with the version (no `v`); the tag is created at the selected ref.
3. The workflow builds a **draft** release. Review the artifacts and `SHA256SUMS.txt` on the release page, then publish manually.
4. Prerelease: any tag with a hyphen (`v0.2.0-beta.1`) is marked prerelease automatically — it feeds the **beta** update channel (see below).

### Package-manager distribution

Templates and automation live under [`packaging/`](packaging/) (see its README). Nothing here publishes before a release exists — templates render from real release assets only.

**Automated on every stable tag** (after the release workflow's finalize job, draft still unpublished — publish the draft promptly so the channels never point at non-public assets):

- **Homebrew tap** ([qtrcipher/homebrew-tap](https://github.com/qtrcipher/homebrew-tap)): `publish-homebrew` renders `packaging/homebrew/squash.rb` (CLI formula: macOS arm64/Intel + Linux tarballs) and `packaging/homebrew/squash-cask.rb` (GUI cask: per-arch dmg) with the version and SHA256s from `SHA256SUMS.txt`, then pushes to `Formula/squash.rb` + `Casks/squash.rb` in the tap repo. Users: `brew install qtrcipher/tap/squash` (CLI) / `brew install --cask qtrcipher/tap/squash` (GUI).
- **Scoop bucket** ([qtrcipher/scoop-bucket](https://github.com/qtrcipher/scoop-bucket)): `publish-scoop` renders `packaging/scoop/squash.json` (Windows CLI zip) to `bucket/squash.json`. The manifest carries `checkver`/`autoupdate` as a fallback, but the release workflow is the source of truth. Users: `scoop bucket add squash https://github.com/qtrcipher/scoop-bucket && scoop install squash`.

Both jobs are **gated on the `TAP_GITHUB_TOKEN` secret** and no-op without it. To enable them, create a fine-grained PAT (github.com → Settings → Developer settings → Fine-grained tokens) scoped to **only** `qtrcipher/homebrew-tap` and `qtrcipher/scoop-bucket` with **Contents: Read and write**, and add it as the `TAP_GITHUB_TOKEN` secret (repo-level or in the `release` Environment).

**Manual steps at/after the first release** (need external accounts or third-party repos — not automated):

- **winget**: manifests live in Microsoft's [winget-pkgs](https://github.com/microsoft/winget-pkgs). Templates for v0.1.0 are in `packaging/winget/` (version / installer / default-locale, schema 1.10.0, placeholder hashes). Regenerate against the real release and submit:
  ```sh
  wingetcreate new --urls <release-msi-url> <release-nsis-exe-url> --version 0.1.0 --token <github-token>
  ```
  wingetcreate computes the hashes, builds the three manifests, and opens the PR to winget-pkgs for you (requires a GitHub token with `public_repo` scope and a fork of winget-pkgs, which it creates on first use). Compare its output with `packaging/winget/` if anything drifts.
- **Flatpak / Flathub**: starter manifest at `packaging/flatpak/dev.squash.app.yml` (see its header comment). Needs generated `node-sources.json` / `cargo-sources.json`, appstream metainfo, then a PR against `flathub/flathub` branch `new-pr`. Note the app ID follows the Tauri identifier (`dev.squash.app`) — Flathub requires proving control of `squash.dev`, otherwise the ID must become `io.github.qtrcipher.Squash`.
- **Snap**: needs a Snapcraft account + `squash` name registration, then a `snapcraft.yaml` and store upload. Documented here only; no manifest yet.
- **AUR**: template PKGBUILD at `packaging/aur/PKGBUILD` (source build, GUI + CLI). After the first release: update `pkgver`/`sha256sums` (`updpkgsums`), test with `makepkg -si`, push to `ssh://aur@aur.archlinux.org/squash.git` (requires an AUR account).
- **apt / rpm repos**: the `.deb`/`.rpm` from the release install directly (`dpkg -i`, `dnf install`); a hosted apt repo is out of scope for now (would need signing-key hosting; revisit if requested).

### Update channels (stable / beta)

The GUI self-updates via `tauri-plugin-updater` (docs/03 S6/D3). Update checks are **opt-in** — a manual "Check for updates" button in Settings, plus an automatic check on launch only when the user turned it on (default off; the check is a single GET of a manifest, no user data sent).

- **stable** (default): `https://github.com/qtrcipher/squash/releases/latest/download/latest.json` — GitHub's `latest` alias never resolves to a prerelease. The finalize job attaches `latest.json` to every stable release.
- **beta**: `https://github.com/qtrcipher/squash/releases/download/updates/beta.json` — prerelease tags refresh `beta.json` on the long-lived `updates` release (the workflow creates it on first use; it is not a Squash release).

The finalize job generates the manifest from the assets *as published* and re-signs the updater bundles (`*.app.tar.gz`, `*.AppImage`, `*-setup.exe`) with the updater private key — the re-sign matters because the Windows `signtool` pass modifies the installer after the bundler signed it. The manifest embeds each platform's minisign signature; the app verifies it against the public key baked into `tauri.conf.json` (`plugins.updater.pubkey`) before installing.

Rotating the keypair: `npm tauri signer generate -- -w <path>`, put the public key into `app/src-tauri/tauri.conf.json`, update the `TAURI_SIGNING_PRIVATE_KEY` secret. **Never commit the private key** — installed builds can only ever accept updates signed by their baked-in public key's counterpart, so losing it means no more updates for existing installs.

### Artifacts per release

- **macOS**: `.dmg` + `.app.tar.gz` for Apple Silicon (`aarch64`) and Intel (`x86_64`) — per-arch builds, not universal2.
- **Windows**: `.msi` + NSIS `-setup.exe` (x86_64).
- **Linux**: `.deb`, `.rpm`, `.AppImage` (x86_64).
- **CLI** (all OSes): `squash-<os>-<arch>.tar.gz` / `.zip` containing the `squash` binary, both LICENSE files, and the README.
- **Checksums**: `SHA256SUMS.txt` covering every asset (hashed after upload, so sums match what's published), plus `SHA256SUMS.txt.asc` when GPG signing is configured.

### Secrets — which enable what

All signing is **gated on secret presence**: absent secrets no-op cleanly and the release ships unsigned (unsigned-first, docs/05 §5). Add secrets at repo level or in the `release` GitHub Environment.

| Secret | Unlocks |
|---|---|
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD` | macOS codesigning (base64 `.p12` + its export password) |
| `APPLE_SIGNING_IDENTITY` | Optional signing-identity override (default: first valid identity) |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | macOS notarization (Apple ID + app-specific password) |
| `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD` | Windows `signtool` signing of the `.msi`/`.exe` (base64 `.pfx`). An OSS SignPath.io cert can replace this later |
| `GPG_PRIVATE_KEY` (+ optional `GPG_PASSPHRASE`) | Detached ASCII-armored signature of `SHA256SUMS.txt` |
| `TAURI_SIGNING_PRIVATE_KEY` (+ optional `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) | Updater bundle signatures (`.sig`) and the `latest.json`/`beta.json` manifests — without it, releases ship without updater manifests |
| `TAP_GITHUB_TOKEN` | Fine-grained PAT (contents:write on `qtrcipher/homebrew-tap` + `qtrcipher/scoop-bucket`) — enables the `publish-homebrew`/`publish-scoop` jobs; see "Package-manager distribution" below |
| `SQUASH_SENTRY_DSN` | Crash reporting *available* in the build (still opt-in at runtime; see above) |

### Notes

- One release at a time (workflow concurrency guard); reruns are idempotent — an existing draft for the tag is reused and assets are overwritten (`--clobber`).
- The release cache (`shared-key: release`) is separate from CI because release builds may embed the Sentry DSN.
- macOS codesigning/notarization run inside `tauri-action` when the Apple secrets are present; the workflow itself contains no Apple-specific steps.

## Ground rules

- **Conventional commits** (`feat:`, `fix:`, `chore:`, `docs:`, `test:`, `refactor:`); branches `feature/*`, `fix/*`, `chore/*`.
- **i18n from day one**: all user-facing strings go through the i18n files (English + Arabic); UI must support RTL and both light and dark mode.
- **Four UI states** for every data-bound screen: loading, empty, error (with a recovery action), success.
- **Tests**: business logic must have tests; target pyramid 70% unit / 20% integration / 10% E2E. Tests land with the feature, not after.
- See `AGENTS.md` for project conventions and `PROGRESS.md` for the roadmap.

## License

By contributing, you agree your contributions are dual-licensed under MIT and Apache-2.0.
