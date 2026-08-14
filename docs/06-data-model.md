# Squash — Phase 0: Data Model

> Status: planning gate. Inputs: `01-product-scope.md`, `03-ux-flows.md`, `05-architecture.md`. Owner: `data-modeler`.
> Squash is local-only: no accounts, no backend, no silent telemetry (crash reporting is opt-in only — §6). This doc applies Firestore-style modeling discipline (schema, validation, versioning, migration) to on-disk files. Everything here is a file the user can inspect, back up, or delete — that inspectability is part of the trust pitch (doc 02).

## 1. Decisions, up front

| Decision | Pick |
|---|---|
| Settings + presets format | **TOML** (one file each) in the config dir |
| Job history format | **JSONL** (append-only, bounded) in the data dir |
| Persisted queue format | **JSON** in the data dir |
| Directory resolution | `directories` crate `ProjectDirs` ("dev.squash", "Squash") |
| Write strategy | Write-temp-then-atomic-rename for whole files; locked append for history |
| Versioning | Top-level `version = 1` in every file; forward-only migration chain; newer-version files are never overwritten |

## 2. Entity catalog

All timestamps RFC 3339 UTC. All paths stored as plain strings, **never canonicalized** (no symlink resolution, no `~` expansion at rest — expansion happens at use time); paths may exceed OS limits only at use, stored strings are capped.

### Settings — `settings.toml`
| Field | Type | Default | Validation |
|---|---|---|---|
| `version` | int | 1 | == supported version, else migrate/refuse (§4) |
| `language` | enum `en\|ar` | OS locale if supported, else `en` | unknown → `en` + warn |
| `theme` | enum `system\|light\|dark` | `system` | unknown → `system` |
| `default_preset` | preset id | `builtin:balanced` | must resolve to a known preset, else `builtin:balanced` |
| `default_format` | enum `zip\|7z\|tar.gz\|tar.zst\|gz\|xz\|zst` | `zip` | create-capable formats only (doc 05 §4) |
| `extract.dest_policy` | enum `same_folder\|ask` | `same_folder` | matches S3 default (doc 03 F3) |
| `extract.loose_files_policy` | enum `new_folder\|here` | `new_folder` | anti–desktop-explosion default (doc 03 F3) |
| `update_check_opt_in` | bool | `false` | S6 auto-check-on-launch consent (docs/03 S6; manual check always available) |
| `activation_counter_opt_in` | bool | `false` | local-only counter (doc 01 §5) |
| `release_channel` | enum `stable\|beta` | `stable` | updater channel (doc 03 S6/D3); unknown → `stable` + warn |
| `first_launch_done` | bool | `false` | drives S7 (doc 03 F1) |
| `drop_zone_hint_dismissed` | bool | `false` | dismisses the one-time S1 drop-zone hint (doc 03 F1) |
| `debug_logging` | bool | `false` | S6 verbose toggle — writes the local debug log (§3 "Debug log") |
| `crash_reporting` | bool | `false` | S7/S6 consent toggle — opt-in Sentry crash reports (§6 "Crash reporting") |

Unknown keys are preserved on rewrite (forward-compat).

### Preset — `presets.toml`
Built-ins (`builtin:fast|balanced|max`) are **code-defined** in `squash-core`'s preset table (doc 05 §3) and never written to disk; this file holds user presets only.
| Field | Type | Default | Validation |
|---|---|---|---|
| `id` | string `user:<uuidv4>` | generated | unique; builtin ids reserved |
| `name` | string | required | trimmed, 1–40 chars, no control chars, case-insensitively unique |
| `format` | enum (create-capable set) | required | must exist in the format registry |
| `params.level` | int | format default | per-format bounds: zip `1–9`, 7z `0–9`, tar.gz `1–9`, tar.zst `1–22`, gz `1–9`, xz `0–9`, zst `1–22`; out-of-range → clamp + warn |
| `created_at` | timestamp | now | — |

⚠ **Conflict flagged:** doc 03 §7 says "three presets only" and doc 01 says "no per-codec flag jungle in the GUI." Task input requires user presets (Dana, doc 01 §2). Proposed resolution: user presets are creatable by **editing the file or via CLI in v1**; the GUI *lists* them but ships no preset editor. Needs owner sign-off; if rejected, this entity drops and CLI keeps exactly three presets.

### Job record — `history.jsonl` (one JSON object per line)
| Field | Type | Validation |
|---|---|---|
| `id` | uuid v4 | required |
| `op` | enum `compress\|extract` | required |
| `inputs` | string[] | 1–100 entries, each ≤ 1024 chars |
| `output` | string, nullable | ≤ 1024 chars |
| `format` / `preset` | string / preset id | from registry at read; unknown → record renders as-is (formats can be uninstalled later) |
| `status` | enum `finished\|failed\|cancelled` | required |
| `error_code` | `SquashError` code string, nullable | stable codes only (doc 05 §3); messages are re-localized at render, never stored |
| `in_bytes` / `out_bytes` / `duration_ms` | u64, nullable | null for failed-before-start |
| `started_at` | timestamp | required |
| `source` | enum `gui\|cli` | required |

### Queued job — `queue.json`
`{ version, jobs: [ { id, position, job: <core Job shape: op, inputs, destination, format, preset, options> } ] }` — only *unfinished* jobs (queued, or running-at-quit, which restores as queued). At restore, entries whose input paths no longer exist are dropped into history as `cancelled`, silently (doc 03 S4 loading state).

## 3. Storage format & location

- **TOML for settings/presets:** human-editable, diffable, comment-friendly — Morgan's "one config, works everywhere" (doc 01 §2). `toml_edit` preserves user comments on rewrite. JSON rejected: no comments; SQLite rejected: uneditable binary for two tiny documents.
- **JSONL for history:** append-only writes are cheap and crash-safe per line; human-greppable; no index needed for one chronological list (S4). SQLite rejected: extra dependency and binary opacity for a ≤200-row list.
- **JSON for queue:** exact serde mirror of the core `Job` type — single source of truth, zero mapping layer. Machine-only file; TOML's human-friendliness buys nothing.
- **Locations via `directories` `ProjectDirs`:** config files → config dir (macOS `~/Library/Application Support/dev.squash.Squash`, Windows `%APPDATA%\dev.squash\Squash\config`, Linux `~/.config/squash`); history + queue → data dir (Windows `%LOCALAPPDATA%\dev.squash\Squash\data`, Linux `~/.local/share/squash`). Four small files, one per entity — no single store, so a corrupt history can never take settings down with it.
- **Debug log — `logs/squash.log` in the data dir:** written only while verbose logging is on (GUI S6 `debug_logging` toggle; CLI `-v`/`--verbose` or `SQUASH_LOG=debug`, which logs to **stderr** instead — stdout stays pipe-clean for `--json`). Facility is the `log` facade over the whole Rust codebase: `squash-core` instruments decision points (job start/end with stats, per-format create/extract choices, the extract-layout decision, sanitizer blocks, store writes) at debug level; the shells install the sink (CLI: `env_logger`; GUI: a rolling file writer — 1 MiB cap, one rotated generation `squash.1.log`). Each verbose session opens with a header line carrying app version, OS/arch, and enabled features (e.g. `rar`) — the support gold for reproducing bug reports. `ProjectDirs` has no dedicated log dir, so logs live under the data dir on every platform; S6's "Reveal log folder" lands the user on the file.

## 4. Schema versioning & migration

- Every file carries `version = 1` (JSONL: a version field on each record, since old lines persist).
- **Upgrade:** forward-only step chain (v1→v2→…), applied in memory at load; the migrated file is written back on the next normal save. Migrations are pure data transforms with unit tests per step; a failed migration keeps the original file untouched (backed up as `<file>.v<N>.bak`) and falls back to defaults.
- **Downgrade (old build reads newer files):** never overwrite. The app loads what it recognizes into read-only in-memory state, warns once (GUI banner / CLI stderr), and does not write until the user resets via a "reset settings" action. History/queue with newer record versions: unknown lines are skipped, not deleted. Old builds' unknown-key preservation (§2) makes mixed-version use survivable.

## 5. Concurrency & atomicity

- **Ownership:** GUI is the sole writer of `settings.toml`, `presets.toml`, and `queue.json`. CLI reads settings/presets (parity requirement, doc 03 §4) and stays **fully stateless by default — zero writes**; history is appended only when asked (`--save-history` flag / `history.enabled` setting, default off for CLI). `--no-config` gives Morgan hermetic runs.
- **Crash mid-write:** whole files are written to `<name>.tmp` in the same directory, `fsync`ed, then atomically renamed over the target. A leftover `.tmp` at load is ignored. History lines are appended with an advisory file lock (`fs2` crate) so GUI and CLI appends never interleave mid-line; a truncated last line after a crash is skipped on read, not fatal.
- **Growth bound:** history retention is **200 records or 30 days, whichever is hit first**, enforced by compaction (rewrite via atomic rename) at launch and after every 50 appends. Queue is unbounded in memory but restores are dropped per §2; settings/presets are naturally bounded (user presets capped at 100).

## 6. Privacy posture

- **Stored:** user preferences, user preset definitions, job metadata (paths in/out, sizes, timestamps, error codes), unfinished queue specs. All of it lives in two user-visible directories.
- **Never stored:** archive *contents* (S5's listing is computed live and discarded), file contents or hashes, passwords (no encryption in v1 — doc 01 §3), hostnames/usernames beyond what paths already contain, any analytics. The opt-in activation counter is a local integer the user can inspect in S6 (doc 01 §5).
- **Caveat to state honestly:** history and queue contain absolute paths, which may reveal user names and folder structures. That is the entire privacy surface; docs must say so plainly. The debug log (`logs/squash.log`) is the same story with more detail — it records absolute paths and timings on purpose (they are what make a bug report actionable). It is written only when the user turns verbose logging on, is never redacted, and **never leaves the device**: the user reveals the folder from S6 and chooses what to attach to a GitHub issue. The S6 toggle label says exactly that.
- **Crash reporting (opt-in Sentry; owner decision, doc 01 §6.3):** off by default, enabled only by the S7 welcome checkbox (unchecked) or the S6 toggle. When consent is off, the Sentry SDK is never initialized — no crash-reporting code runs and no network call is possible, verifiable in source (`squash-core/src/crash.rs`; the frontend SDK is a separate lazy-loaded chunk). When consent is on **and** the build has a DSN (see below), a report is sent **only when the app crashes or an unhandled error occurs**. A report contains exactly:
  - the stack trace (crash location),
  - the app version (`squash@<version>` release tag) and environment (`production`/`development`),
  - OS and CPU architecture,
  - enabled features (e.g. `rar=on/off`),
  - the UI locale (`en`/`ar`) and which shell reported (`gui`/`cli`/`gui-frontend`).
  Scrub rules, applied to every event before send: hostname and user are dropped, breadcrumbs are dropped entirely (nothing derived from file contents or paths), and the user's home directory in any path or message is rewritten to `~`. Reports never contain file contents, archive names beyond what a stack frame carries, environment variables, or argv beyond the command name. Turning the toggle off takes effect immediately: the consent gate drops every later event and the client is unbound. The CLI honors the same `crash_reporting` key unless `--no-config` is passed; `SQUASH_CRASH_REPORTING=1` is an explicit per-run opt-in that wins even over `--no-config` (hermetic CI runs).
  **DSN:** the Sentry DSN is supplied at build time via `SQUASH_SENTRY_DSN` and is never committed. A build without it has the feature disabled: the consent toggles render disabled with a "not available in this build" note. Setup instructions live in `CONTRIBUTING.md` ("Crash reporting").
- **Wipe:** Settings → "Clear history" (deletes `history.jsonl`); uninstall docs list the two directories; deleting the app-data directory returns Squash to a first-launch state. No hidden stores, no caches with user data — verifiable in source, which is the OSS trust story.

## 7. Interface-level sketch (who owns what)

- **`squash-core::store`** (new module per doc 05 §3's plan): owns all serialization types, validation, version/migration chain, atomic-write + locked-append helpers, and `ProjectDirs` resolution. Pure functions: `load_settings(dir) -> Settings`, `append_history(dir, record)`, etc. No global state, no singletons — shells pass the dirs in (also enables test tempdirs).
- **`squash-core::crash`:** opt-in crash reporting (§6 "Crash reporting"): the build-time DSN constant, the runtime consent gate, the path/message scrubbers, and the shared Sentry client options (behind the `crash-reporting` cargo feature, enabled by both shells, not by `squash-bench`).
- **`squash-cli`:** calls `store` readers at startup (unless `--no-config`); calls `append_history` only with `--save-history`. Never writes settings, presets, or queue.
- **`apps/gui` Tauri host (Rust side):** owns the live `Settings`/`Queue` state, calls `store` writers through its typed commands, and on launch restores `queue.json` into core `Job`s re-submitted to `Engine::submit` (doc 05 §3). The TS frontend never touches the filesystem; it renders whatever the host returns.
- **Frontend contract:** S4's row model and CLI `--json` are both projections of the job record above — one shape, two renderers (doc 03 §4).
