# Squash — Phase 0: Architecture

> Status: planning gate. No implementation code until this document is approved.
> Owner of this document: `technical-architect`. Stack changes after this point require an ADR.
> Inputs (do not re-derive): `docs/01-product-scope.md`, `docs/02-market-check.md`.

## 0. Decisions, up front

| Decision | Pick |
|---|---|
| Core language / toolchain | **Rust** (stable, cargo workspace) |
| GUI toolkit | **Tauri v2** (Rust host + web frontend), core consumed **in-process as a crate** — no FFI, no IPC |
| Extraction engine | Per-format Rust crates + RarLAB **unrar** C source (extraction-only) — **not** libarchive |
| 7z create/extract | `sevenz-rust2` (pure Rust) with a bundled `7zz` fallback path if corpus tests fail |
| Distribution | Single static binaries + OS installers via GitHub Releases, GitHub Actions matrix |

Everything below is rationale and structure for these five calls.

## 1. Core language: **Rust**

Evaluated against the five criteria from the task:

| Candidate | Compression lib access | Single binary | 3-OS cross-compile | OSS contributor pool | GUI interop | Verdict |
|---|---|---|---|---|---|---|
| **Rust** | Excellent: `zstd`, `flate2`, `xz2`, `brotli`, `tar`, `zip`, `sevenz-rust2` (pure-Rust 7z write+read, [actively maintained](https://github.com/hasenbanck/sevenz-rust2)), `unrar` crate wrapping RarLAB source | Yes, static | Yes (rustup targets; musl for Linux) | Largest and growing for OSS CLI tooling | Native — Tauri core is Rust; zero FFI | **Pick** |
| C++ | Best (libarchive, LZMA SDK direct) | Yes | Painful (per-OS toolchain, vcpkg) | Shrinking; memory-safety CVEs in 7-Zip are the exact trust gap we're exploiting (doc 02 §3) | Qt native, everything else awkward | Rejected: safety + contributor pool |
| Go | Weak: no mature pure-Go 7z writer; cgo breaks static single binaries | Yes, until cgo | Good pure-Go, bad with cgo | Strong for CLIs | No good GUI story at all | Rejected: 7z write + GUI |
| .NET/C# | Weak: SharpCompress reads 7z but write support is limited | Single-file works but large | NativeAOT still rough for GUI apps | Medium | Avalonia is decent | Rejected: core lib maturity |

**What would change the decision:** if `sevenz-rust2` proves unreliable on the corpus round-trip suite in CI and the bundled-`7zz` fallback also becomes untenable (e.g. licensing of redistribution or unacceptable binary bloat), we re-evaluate C++ with libarchive/LZMA SDK and accept the GUI bridge cost. Nothing else in the dependency graph is contested.

## 2. GUI toolkit: **Tauri v2**

Requirement (doc 01 §6, fixed): native feel, small binaries, full RTL Arabic, three desktop OSes, one shared core.

| Candidate | RTL / Arabic | Binary size | Native feel | Core consumption | Verdict |
|---|---|---|---|---|---|
| **Tauri v2** ([production-ready since 2.0, 2024](https://v2.tauri.app/blog/tauri-2-0-0-release-candidate/)) | Full: HTML/CSS `dir="rtl"` + mature JS i18n (`i18next`, `fluent`); bidi text is the browser's solved problem | ~10 MB (OS webview, no bundled Chromium) | Good; system webview per OS | **In-process**: core is a Rust crate linked into the Tauri host; GUI calls typed commands, receives progress events over Tauri's channel API | **Pick** |
| Qt 6 | Full RTL, best-in-class | 30–60 MB | Native | Requires C++ core or bridge; LGPL obligations for a non-Qt-authored app | Rejected: license friction + forces C++ decision |
| Flutter Desktop | Full RTL | ~30 MB + runner | Material feel, not native | `dart:ffi` to Rust — workable but adds a whole toolchain; desktop still the least-mature Flutter target | Rejected: interop + maturity |
| Avalonia | Full RTL | Medium | Good on Windows | Pulls the core toward .NET, already rejected | Rejected |
| egui / Slint | **RTL text shaping immature** — egui's bidi support is partial; this is a hard product requirement, not a nice-to-have | Tiny | Non-native widgets | Native Rust | Rejected: RTL risk on a hard requirement |
| Native-per-OS (SwiftUI/WinUI/GTK) | Best per OS | Small | Best | 3× GUI code, 3× state-management bugs, defeats the cross-platform-parity thesis (doc 02 §4) | Rejected: 3× the work |

**Trade-offs accepted:** (a) Linux depends on system WebKitGTK — mitigated by CI testing on Ubuntu LTS and the planned Flatpak bundle; (b) webview version skew — mitigated by testing on the oldest supported webview per OS; (c) GUI logic lives in TypeScript, so frontend discipline (strict TS, typed command contracts) is required. Frontend stack within Tauri: **React 18 + TypeScript + Vite** (decided 2026-08-13 during scaffold — the fleet's house web stack; supersedes the earlier Svelte/Solid deferral).

**Core consumption:** no FFI, no subprocess, no IPC serialization layer to design. The GUI is a Tauri app whose Rust host side depends on the `squash-core` crate directly; commands are thin async wrappers mapping GUI events to the core's job API. The CLI depends on the same crate. One core, two thin shells — the product's reason to exist (doc 01 §3.1).

## 3. Module plan (cargo workspace monorepo)

```
squash/
├── crates/
│   ├── squash-core/        # the library: formats, jobs, presets, progress, errors
│   ├── squash-cli/         # `squash` binary: clap CLI, --json, exit codes
│   └── squash-bench/       # benchmark harness vs 7-Zip (§6)
├── app/                    # Tauri v2 app (Rust host in src-tauri/ + React/TS frontend)
├── fixtures/               # shared test corpus: per-format archives, zip-slip
│   │                       # attacks, Unicode/Arabic filenames, corrupt files
├── benches/corpus/         # standard benchmark corpus (separate from fixtures)
├── fuzz/                   # cargo-fuzz targets (§6)
└── docs/
```

### `squash-core` public API surface (interface level, no code)

- **Job model.** `Job` = { operation: Compress | Extract, inputs, destination, format, preset, options }. `JobId` handles. Jobs are the *only* unit of work — GUI batch queue and CLI both submit `Job`s to one `Engine::submit(job) -> JobHandle`. Cancellation via the handle.
- **Progress events.** Every job emits a `ProgressEvent` stream: `Started { total_bytes_estimate }`, `Advanced { bytes_done, entries_done, current_path }`, `Finished { stats: in_bytes, out_bytes, duration }`, `Failed { error }`. Consumed as a Rust `Stream`/callback — Tauri channels and CLI indicatif bars both adapt this one type.
- **Error taxonomy.** One `SquashError` enum, `thiserror`-based, with stable machine-readable codes (serialized into CLI `--json` and GUI error states): `UnsupportedFormat`, `CorruptArchive`, `PathTraversalBlocked`, `PermissionDenied`, `DiskFull`, `PasswordRequired` (stubbed, out of scope v1), `Cancelled`, `Internal`. Codes are a stability contract — majors only.
- **Format registry.** `FormatRegistry`: maps extension/magic-bytes → `FormatHandler` trait (capabilities: `can_extract`, `can_create`, `list`). Format *detection* is magic-bytes-first, extension as hint (P3 scripters feed it pipes).
- **Preset system.** `Preset::{Fast, Balanced, Max}` → per-format parameter table defined in one module (e.g. fast tar.zst = zstd level 3; max 7z = LZMA2 high). Presets are data, not code paths — exactly three, no flag jungle (doc 01 §3.3).
- **Safety layer.** Extraction path sanitizer (zip-slip): every entry path canonicalized and verified inside the destination before a single byte is written. Lives in core, format-agnostic, cannot be bypassed by a handler.

## 4. Format strategy

| Format | Extract | Create | Provider | Notes |
|---|---|---|---|---|
| zip | ✅ | ✅ | `zip` crate | Streaming, no system zip needed |
| 7z | ✅ | ✅ | `sevenz-rust2` ([fork of unmaintained `sevenz-rust`, active through 2025](https://github.com/hasenbanck/sevenz-rust2/blob/main/CHANGELOG.md)) | Young fork → hedged by `7zz` fallback (§7) |
| rar | ✅ | **never** (license) | `unrar` crate wrapping [RarLAB unrar C source](https://www.rarlab.com/rar_add.htm) | License permits use "to handle RAR archives" but forbids building RAR-*creating* tools; `license.txt` shipped verbatim; isolated behind a `rar` cargo feature so libre-only builds are possible |
| tar, tar.gz/tgz | ✅ | ✅ | `tar` + `flate2` | |
| tar.bz2 | ✅ | — | `tar` + `bzip2` | Extract-only (matches MVP compress list) |
| tar.xz | ✅ | — | `tar` + `xz2` | Extract-only; xz supply-chain history (CVE-2024-3094) noted — pin vendored liblzma from upstream release tarballs only |
| tar.zst | ✅ | ✅ | `tar` + `zstd` | The speed differentiator (doc 02 §4) |
| gz / xz / zst (single-file) | ✅ | zst only | codec crates | |
| brotli | — | — | — | Not in MVP (doc 01 §3.2); registry leaves room |

**Plugin-in path:** adding a format = one new module implementing `FormatHandler` + registry entry + fixtures + fuzz target. No changes to job model, presets (add a row to the preset table), CLI, or GUI. Brotli and encrypted-archive support slot in this way post-v1.

## 5. Build / release pipeline sketch

Architectural implications only; mechanics belong to release-automation.

- **CI matrix (GitHub Actions):** `macos-14` (arm64) + `macos-13` (x86_64, lipo universal2), `windows-latest` (MSVC), `ubuntu-latest` (gnu) + musl static build. Native runners per OS — **no cross-compilation for release artifacts** (avoids C-dependency cross toolchain pain from unrar/zstd/xz). Linux dev/CI sanity builds can run in Docker on the dev Mac, but releases come from CI runners.
- **Signing:** macOS Developer ID + notarization (`notarytool`) and stapled dmg; Windows `signtool` — OSS cert via SignPath.io or purchase; until then, unsigned + checksums and we say so honestly (trust is the brand — doc 02 §3). Linux: tarball + SHA256 sums, GPG-signed.
- **Reproducibility:** `Cargo.lock` committed; `cargo vendor` + `cargo audit` + `cargo deny` (license gate — blocks anything GPL-incompatible or unrar-license violations) run in CI. Dependency updates via Dependabot, auto-merged only if the full corpus suite passes.
- **Homebrew/Scoop/winget:** formula/manifests generated from release artifacts in CI (doc 01 §3.7).

## 6. Testing architecture (house rule 70/20/10)

| Layer | Share | Lives in | What it covers |
|---|---|---|---|
| Unit | 70% | `squash-core` (`#[cfg(test)]` per module) | path sanitizer (the security-critical one), preset table, format detection, error taxonomy mapping, per-codec round-trips on in-memory buffers |
| Integration | 20% | `squash-core/tests/` + `squash-cli/tests/` against `fixtures/` | end-to-end extract/create per format on fixture archives; CLI contract tests: exit codes, `--json` schema stability, piping; **golden round-trip suite**: create with Squash → verify readable by upstream `7zz`/`bsdtar` and vice versa |
| E2E | 10% | `app/` smoke tests (`tauri-driver`/WebDriver) + benchmark harness | GUI happy paths (drop → compress → progress → done) per OS in CI; RTL visual screenshot check |

- **Benchmark harness** (`squash-bench`): runs the standard corpus through Squash and `7zz` (installed in CI), records ratio + wall time, fails CI on regression > 2%, and publishes results as release-page tables — this *is* product requirement §3.8, not a nicety. Corpus is fixed, versioned, and documented so numbers are reproducible.
- **Fuzzing:** `cargo-fuzz` targets on every extraction entry point + the path sanitizer; seeded with `fixtures/`; run continuously (weekly CI cron, OSS-Fuzz eligibility post-launch). Given that incumbent RCEs drive user switching (doc 02 §2), the fuzz story is marketing as much as engineering.
- **Test fixtures are shared** across core/CLI/GUI/bench — one `fixtures/` dir, one manifest.

## 7. Top technical risks

1. **unrar licensing.** The RarLAB license permits extraction-only use and redistribution of the source with attribution, but it is *not* OSI-approved and forbids RAR creation. Mitigations: extraction-only enforced by the capability flags in the registry (no code path to create); `license.txt` shipped verbatim; `rar` as an opt-out cargo feature for downstream libre builds; explicit owner sign-off on "extracts rar, never creates it" messaging (doc 01 §6.2).
2. **`sevenz-rust2` maturity.** Single-maintainer young fork; 7z *creation* correctness is the weak link on all three OSes. Mitigations: golden round-trip suite against upstream `7zz` in CI gates every release; architecture keeps a documented fallback — bundle the `7zz` binary and route 7z jobs through a subprocess adapter behind the same `FormatHandler` trait (swap is one registry entry, no API change).
3. **Tauri RTL/WebKitGTK variance.** Arabic RTL in a webview is solved, but Linux WebKitGTK versions differ across distros (rendering + security patch lag). Mitigations: CI smoke tests on oldest supported webview; RTL screenshot regression in the E2E 10%; Flatpak bundles a known runtime post-v1.
4. **Security correctness of extraction (zip-slip, bombs).** One CVE would destroy the trust positioning. Mitigations: sanitizer is format-agnostic and non-bypassable by design; dedicated attack fixtures; fuzzing; decompression-bomb ratio caps in core defaults.
5. **Solo-maintainer bus factor on signing/release.** Certs, notarization credentials, and release secrets are single-person knowledge. Mitigation: release-automation phase documents the full pipeline as code in-repo; secrets in GitHub Environments, documented recovery.

## Handoffs

- Repo scaffolding, workspace setup, CI matrix → build/release phase (`release-automation`).
- Frontend stack final pick + RTL design tokens → UX/frontend phase.
- Any change to a §0 decision → ADR in `docs/`, amending this file.
