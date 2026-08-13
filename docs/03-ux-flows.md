# Squash — Phase 0: UX Flows

> Status: planning gate. Inputs: `01-product-scope.md`, `02-market-check.md`. Owner: `ux-designer`.
> Decisions here are binding for `frontend-*` builders; CLI grammar belongs to `05-architecture.md`.
> Global house rules applied everywhere: EN+AR, full RTL, light/dark, and all four UI states on every data-bound screen. Layout is described in logical terms (start/end), never left/right.

## 1. Screen Inventory

One main window; everything else is a sheet, panel, or dialog attached to it. No tabs, no navigation stack, no dock-dropping in v1.

| # | Screen | Purpose |
|---|---|---|
| S1 | **Main window / drop zone** | Accept dropped files, folders, or archives; route to compress or extract |
| S2 | **Compress sheet** | Choose format + preset (fast/balanced/max), output name/location; shows predicted time |
| S3 | **Extract sheet** | Choose destination and single-folder-vs-loose-files handling |
| S4 | **Job queue** | Live list of queued/running/done/failed jobs with progress, ETA, sizes |
| S5 | **Archive contents list** | Read-only flat summary of an archive's top level (count, names, sizes) shown pre-extraction. **Not** a browsable folder tree — that is OUT per doc 01 |
| S6 | **Settings** | Theme, language, default preset/format, default destinations, opt-in update check, opt-in activation counter inspection |
| S7 | **First-launch sheet** | One-time: pick language + theme, set as default handler (optional) |
| D1–D4 | **Dialogs** | Overwrite-conflict (D1), password-protected archive (D2), update-available (D3), delete-after-extract confirmation is NOT offered in v1 — no destructive options |

S1 layout intent:

```
┌──────────────────────────────────────────┐
│ SQUASH                       [Settings]  │  ← toolbar, start-aligned app name
│ ┌──────────────────────────────────────┐ │
│ │        [squash-drop icon]            │ │
│ │   Drop files or an archive here      │ │
│ │   or  [Choose Files…]                │ │
│ └──────────────────────────────────────┘ │
│ ┌─ Job queue ──────────────────────────┐ │
│ │ ▸ backup.zip   62% ▓▓▓▓▓░░░  ~12s    │ │
│ │ ▸ photos.tar.zst   queued            │ │
│ └──────────────────────────────────────┘ │
└──────────────────────────────────────────┘
```

## 2. Per-screen UI states

Static sheets (S2, S3, S6, S7, D1–D4) have no loading/empty states; their error/success states are validation and confirmation as noted.

**S1 drop zone**
- Loading: none (nothing to load); job queue area shows a brief skeleton on launch only while restoring an interrupted queue.
- Empty: the drop zone illustration above — this IS the empty state; never a blank window.
- Error: none standalone; errors surface per-job in S4.
- Success: files dropped → routed to S2/S3 (see flows).

**S2 compress sheet**
- Success: inputs listed (names, total size), format segmented control (zip/7z/tar.gz/tar.zst), preset control (Fast = zstd-3 / Balanced = default / Max = LZMA2-max), output name + location, "Compress" primary action.
- Error (validation): red inline message under the field (e.g. "Name already exists at destination") with the offending field focused; primary action disabled until fixed. Predicted time line ("about N seconds on this machine") appears once inputs are known; if prediction is unavailable the line is omitted, never shown as "calculating…" forever.

**S3 extract sheet**
- Success: destination picker, and the routing decision from S5 (§3, flow 3) shown as a pre-selected radio the user can override.
- Error (validation): destination not writable or insufficient space → inline message + "Choose Another Location…" recovery action.

**S4 job queue** (data-bound — all four states)
- Loading: skeleton rows while restoring state on launch (<1s).
- Empty: queue section collapses entirely; only the drop zone shows. No "no jobs" placeholder text.
- Error: failed jobs stay in the list, tinted, with a one-line plain-language reason and a recovery action per job: **Retry**, **Show in Folder** (partial output), or **Dismiss**. Example: "Disk full — free 2.1 GB, then Retry."
- Success: running job shows animated bar, %, ETA, before/after byte counts; finished jobs collapse to a single summary line ("photos.tar.zst — 1.2 GB → 640 MB, saved 47%") with Reveal/Copy-path actions.

**S5 archive contents list**
- Loading: "Reading archive…" indeterminate bar (listing is fast but rar/7z headers can lag).
- Empty: "This archive contains no files" + Close.
- Error: corrupt/unreadable → plain message + **Try Repair** is NOT offered (out of scope); recovery is "Download or copy the file again" guidance + Close. Password-protected → D2.
- Success: flat top-level listing (max ~200 entries, "and N more" beyond), total uncompressed size, and the single-folder vs loose-files verdict banner that drives S3's default.

**S6 settings** — Success: controls reflect stored values. Error: a setting that fails to persist shows a non-blocking banner with Retry; the screen never blocks on save.

**S7 first-launch** — Success only; both actions ("Continue") always available. Skippable, never shown again.

## 3. Core user flows

**F1 — First launch.** Open app → S7 (language, theme, "make default handler" checkbox) → Continue → S1 empty state. Decision: no account, no telemetry prompt beyond the single opt-in activation counter mentioned in Settings, no tutorial — the drop zone explains itself.

**F2 — Compress.** Drop files/folders on S1 (or Choose Files / OS context menu) → S2 pre-filled (output name = first item's name, location = source folder, format/preset = Settings defaults) → Compress → S4 running job → success line with Reveal. Cancel anytime; partial output is deleted automatically.

**F3 — Extract.** Drop an archive → S5 lists contents and classifies:
- **Single root folder** (e.g. everything under `photos/`) → extract as-is; S3 default destination = same folder as archive.
- **Loose files** → S3 pre-selects "Create new folder named after the archive" (prevents desktop explosion — the classic complaint). User can switch to "extract here".
Then S3 → Extract → S4 → success. Overwrite conflicts → D1 per batch, not per file: "Replace all / Keep both / Skip".

**F4 — Batch queue.** Drop 30 folders at once → one S2 appears with a summary ("30 folders, 4.2 GB total") and output pattern choice: **one archive per item** (default, `name.format` next to each source) or **one combined archive**. Each item becomes its own S4 row; failures don't block the rest.

**F5 — Drag-and-drop rules.** Any mix is accepted: archives in a drop are extracted, non-archives are compressed; if both, two queues form after one confirmation sheet. Dropping onto the app icon/dock icon behaves identically to dropping on the window.

**F6 — OS integration entry points.** Open-with / double-click an associated archive → straight to S5 (skips S1). macOS Services / Windows 11 context-menu ("Compress with Squash", "Extract with Squash") / Linux file-manager action → straight to S2/S3 pre-filled. If integration is unavailable on a platform, the app never pretends otherwise — no greyed-out menu teasing.

**F7 — Error recovery.**
- *Corrupt archive*: S5 error state; copy names the file and suggests re-copying/re-downloading. Exit: Close only.
- *Password-protected*: D2 — "This archive is encrypted. Squash v1 can't open encrypted archives." + Close + link-style button to the GitHub issue tracking encryption (turns a dead end into a contribution funnel).
- *Disk full*: detected before starting (pre-flight space check in S3) AND mid-job → S4 error state with exact bytes needed and Retry that resumes/restarts cleanly.
- *Zip-slip detected*: job aborts, S4 error: "This archive tried to write outside the destination folder and was blocked." No override switch in v1.

## 4. CLI ↔ GUI parity notes

Grammar is owned by the architecture doc; these are experiential parity requirements:

- `squash c` and `squash x` show the same information the GUI does: live progress bar with % and ETA, then the same summary line (before → after, bytes saved, %).
- CLI defaults = GUI Settings defaults: same preset names (fast/balanced/max), same format mapping, same loose-files-into-folder behavior, same zip-slip blocking.
- Errors are the same plain-language strings as the GUI (same i18n keys), e.g. disk-full names the bytes needed; corrupt archive names the file.
- Exit codes are deterministic and documented (0 success; distinct non-zero per failure class: usage, corrupt, encrypted, I/O/disk, unsafe path) — Morgan's "trust the exit code" requirement.
- `--json` output mirrors S4's job model one-to-one (status, progress, sizes, error) so scripts and GUI render the same truth.
- First-run CLI use never prompts interactively; non-interactive contexts fail with a clear message instead of hanging.

## 5. Accessibility notes (design intent)

- **Keyboard:** full operability without a pointer. Tab order: drop zone → Choose Files → job rows (each row's actions in visual order) → Settings. Sheets trap focus, Esc cancels, Return activates the primary action. Job rows expose Cancel/Retry/Reveal as focusable buttons, not hover-only affordances.
- **Screen readers:** drop zone has role/label "Drop files or archives to compress or extract" plus an explicit button equivalent for SR users. Progress bars use native progress semantics with announced percent and ETA at throttled intervals (no per-percent chatter). State changes (job finished, job failed) are announced via live-region/notification announcements: "photos.tar.zst finished — 640 megabytes saved". S5's folder-vs-loose verdict is a labelled status, not icon-only.
- **Per-platform:** macOS VoiceOver via standard AppKit/Catalyst accessibility elements; Windows Narrator via UIA (Name, ControlType, live regions); Linux Orca via AT-SPI — whatever toolkit the architect picks must expose these; flag to `05-architecture.md` as a hard constraint.
- **General:** text scales with OS font-size settings without truncation of job rows; minimum hit targets 24×24pt; all color-coded states (error tint, success tint) paired with icon + text, never color alone; respects Reduce Motion (progress bar still updates, drop-zone idle animation stops); contrast AA in both themes.

## 6. RTL / i18n UX notes

- **Mirrors:** window chrome, toolbar alignment, sheet layout, form field alignment, queue row layout, settings sidebar. All layout specified start/end; Arabic build must be visually verified, not just flipped.
- **Does NOT mirror:** progress bars fill in the locale's reading direction — so in AR they fill right-to-left, **but** playback-style controls would not (none exist in v1). Keep this rule: *progress direction follows reading direction; media direction never does.*
- **Never mirrored / always LTR:** archive paths, file names, extensions, sizes' numeric values, and the contents of S5's listing are laid out LTR inside an RTL window (paths are data, not prose). Mixed-direction strings use bidi isolation so an Arabic filename inside an English sentence (or vice versa) doesn't scramble punctuation.
- **Icons with directionality:** arrows implying forward/back navigation mirror; the drop icon, format glyphs, and Reveal/Show-in-Folder icons do not.
- **Copy:** all user-facing strings including CLI errors come from one i18n table (EN + AR day one); no string concatenation for sentences — use full-message templates with placeholders, because Arabic word order differs ("وفر 47%" constructions differ from "saved 47%"). Numbers in UI use Western Arabic numerals (0-9) in both locales for sizes/percents (house default; matches technical-user expectation in Gulf markets).
- **Fonts:** UI font must have full Arabic coverage with matching metrics; fallback stacks declared per platform so EN/AR mixed lines don't jump baseline.

## 7. Explicitly decided against (so it isn't re-litigated)

- No in-window archive folder-tree browser (OUT per doc 01) — S5 is a flat preview list only.
- No encryption UI in v1 — D2 is an honest dead end with a tracker link, not a disabled password field.
- No per-codec level sliders — three presets only.
- No destructive defaults (no auto-delete of source or archive anywhere).
