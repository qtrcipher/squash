# packaging/

Package-manager distribution for Squash. **Nothing here is live until the first release exists** — every file is either a template with `@TOKEN@` placeholders (rendered from the release's `SHA256SUMS.txt` at release time) or a clearly-marked v0.1.0 template for a manual submission step.

| Directory | Channel | Status |
|---|---|---|
| `homebrew/` | [qtrcipher/homebrew-tap](https://github.com/qtrcipher/homebrew-tap): `squash.rb` CLI formula template + `squash-cask.rb` GUI cask template + `publish.sh` | Automated: `publish-homebrew` job in `release.yml` (needs `TAP_GITHUB_TOKEN` secret) |
| `scoop/` | [qtrcipher/scoop-bucket](https://github.com/qtrcipher/scoop-bucket): `squash.json` manifest template + `publish.sh` | Automated: `publish-scoop` job in `release.yml` (same secret) |
| `winget/` | microsoft/winget-pkgs (third-party repo, PR) | Templates for v0.1.0 with placeholder hashes; regenerate with `wingetcreate` at first release |
| `flatpak/` | Flathub | Starter manifest `dev.squash.app.yml`; needs generated offline sources + metainfo, then a `new-pr` PR |
| `aur/` | AUR | Template `PKGBUILD` (source build); submit after first release |
| — | Snap | Documented only (needs Snapcraft account + name registration) |

The publish scripts are idempotent (no commit when nothing changed) and fail the release job if an expected asset is missing from `SHA256SUMS.txt`. Full details and the owner checklist for the first release: [CONTRIBUTING.md → Releasing → Package-manager distribution](../CONTRIBUTING.md).
