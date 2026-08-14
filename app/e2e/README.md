# Squash GUI E2E (WebDriver)

Phase 5, docs/05 §6: the 10% E2E layer. Real E2E against the **real built
app** — GUI happy paths only; everything else is covered by unit /
integration / snapshot / fuzz suites.

## Stack choice (and why not raw tauri-driver)

- **WebdriverIO + `@wdio/tauri-service`, `driverProvider: "embedded"`**, with
  the app's own embedded W3C WebDriver server (`tauri-plugin-wdio-webdriver`,
  cargo feature `e2e` in `app/src-tauri`).
- Raw `tauri-driver` was rejected: it only supports Linux (WebKitWebDriver)
  and Windows (Edge WebDriver) — **macOS has no WKWebView WebDriver**
  (safaridriver drives Safari, not embedded webviews). The embedded provider
  needs no external driver at all and works on all three OSes, which is also
  what makes the suite runnable on a dev Mac.
- Node/WebdriverIO over Rust/thirtyfour: the app toolchain is already Node,
  the service handles app lifecycle/binary discovery, and the specs need
  Node anyway for filesystem + CLI assertions.

The `e2e` cargo feature is **never** enabled for release builds — the
embedded WebDriver server is an automation backdoor by design.

## The native-dialog problem

Native file pickers are not automatable via WebDriver. Instead of fake
pickers in production code paths, tests enter through the **OS "open with"
argv route** (docs/03 F6, `open::paths_from_argv`): each scenario is a fresh
app launch whose argv is the fixture folder/archive, which routes to S2/S3
exactly like a real double-click. Consequence: one wdio invocation per
scenario (`run.mjs` orchestrates).

Each scenario also gets a throwaway settings/queue store via the
`SQUASH_STORE_DIR` env hook in `squash-core` (`StoreDirs::resolve`), so E2E
never touches real user settings, and the `compress`/`extract` scenarios
seed a complete `settings.toml` (schema v1) to skip the first-launch sheet
deterministically.

## Scenarios (specs/)

| Spec | Flow | Asserts |
|---|---|---|
| `launch` | F1: fresh store → S7 → Continue → S1 | S7 shown/dismissed, S1 empty state, collapsed queue, Tab reaches drop zone, sheet focus trap, Esc closes sheets, S6 language switch → `dir="rtl"` + RTL screenshots |
| `compress` | F2: argv folder → S2 → Compress → S4 | S2 pre-fill (name/location), row `state-finished`, archive exists on disk, **round-trips through the CLI** (`squash x`), Arabic filename intact |
| `extract` | F3: argv zip (CLI-built, loose files) → S3 → Extract → S4 | S3 pre-filled destination, row done, files land in `<dest>/<archive-stem>/` (anti-desktop-explosion), byte-identical |

Deliberately **not** tested here: pressing Enter on the drop zone / "Choose
Files…" (opens the OS-native picker — undrivable), drag-and-drop itself
(Tauri native drop events can't be injected via WebDriver), and anything
already covered by cheaper layers.

Driver limitations discovered on WKWebView (embedded provider), reflected in
the specs:

- Key events are synthetic (`isTrusted: false`), so Tab's focus-traversal
  default action never runs — Tab-reachability can't be asserted via
  WebDriver here. Esc/Enter handlers work (they're DOM keydown listeners);
  the keyboard smoke asserts focus-into-sheet, Esc-to-close, and focus
  restore instead.
- Native `<select>` popups aren't drivable; specs set `.value` and dispatch
  a bubbling `change` event, exercising the app's real onChange path.
- The legacy `/session/:id/keys` endpoint is unimplemented — use WDIO's
  `browser.keys()` (W3C actions), which the plugin does implement.

## Running

```sh
npm run test:e2e          # build frontend + app (feature e2e) + CLI, run all
npm run test:e2e:run      # skip the build, reuse existing binaries
node e2e/run.mjs launch   # single scenario (add --skip-build to reuse)
```

Close any running Squash instance first — the single-instance plugin would
forward the test argv to it instead of launching a test app.

Screenshots (failure shots + the RTL pair) land in `e2e/artifacts/`
(gitignored; uploaded by CI). Failures also dump the host-side queue view
(`FAIL-*-queue.json` — the authoritative `list_queue` snapshot), so a stuck
row shows whether the engine never ran it (host: queued) or its progress
event was lost in transit (host: terminal). Failed scenarios keep their
fixture dirs under `$TMPDIR` for post-mortem (the path is printed).

## CI

The `e2e` job in `.github/workflows/ci.yml` runs this on macOS, Ubuntu
(headless via Xvfb), and Windows, honoring the `[ci skip]` convention.
