# Fixtures — shared test corpus

Per `docs/05-architecture.md` §6: one shared corpus for core/CLI/GUI/bench —
per-format archives, zip-slip attacks, Unicode/Arabic filenames, corrupt files.

**Current state (Phase 2, first slice):** fixtures are generated
*programmatically* by the test suites, because everything needed so far can be
built with the workspace's own codec crates:

- canonical source tree (nested dirs, empty dir, Arabic names) —
  `crates/squash-core/tests/common/mod.rs` (`build_source_tree`), mirrored in
  `crates/squash-cli/tests/cli_e2e.rs`;
- foreign-produced extract-only archives (tar, tar.bz2, tar.xz) —
  `make_archive` in `crates/squash-core/tests/roundtrip.rs`;
- zip-slip / tar-slip / symlink-escape attack archives —
  `crates/squash-core/tests/attacks.rs` (crafted at the byte level where the
  writer crates rightly refuse `..` names);
- corrupt archives (garbage, truncated, bad frames) — same file.

Static files land here only when they can't be generated faithfully (e.g. a
real-world 7z/rar sample when those handlers arrive, golden corpus files for
`squash-bench`). Benchmark corpus stays separate in `benches/corpus/`.
