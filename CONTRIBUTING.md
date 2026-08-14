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
