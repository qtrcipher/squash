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

## Ground rules

- **Conventional commits** (`feat:`, `fix:`, `chore:`, `docs:`, `test:`, `refactor:`); branches `feature/*`, `fix/*`, `chore/*`.
- **i18n from day one**: all user-facing strings go through the i18n files (English + Arabic); UI must support RTL and both light and dark mode.
- **Four UI states** for every data-bound screen: loading, empty, error (with a recovery action), success.
- **Tests**: business logic must have tests; target pyramid 70% unit / 20% integration / 10% E2E. Tests land with the feature, not after.
- See `AGENTS.md` for project conventions and `PROGRESS.md` for the roadmap.

## License

By contributing, you agree your contributions are dual-licensed under MIT and Apache-2.0.
