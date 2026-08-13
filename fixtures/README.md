# Fixtures — shared test corpus

Per `docs/05-architecture.md` §6: one shared corpus for core/CLI/GUI/bench —
per-format archives, zip-slip attacks, Unicode/Arabic filenames, corrupt files.

**Current state (Phase 2):** most fixtures are generated *programmatically* by
the test suites, because they can be built with the workspace's own codec
crates:

- canonical source tree (nested dirs, empty dir, Arabic names) —
  `crates/squash-core/tests/common/mod.rs` (`build_source_tree`), mirrored in
  `crates/squash-cli/tests/cli_e2e.rs`;
- foreign-produced extract-only archives (tar, tar.bz2, tar.xz) —
  `make_archive` in `crates/squash-core/tests/roundtrip.rs`;
- zip-slip / tar-slip / symlink-escape / rar-slip attack archives —
  `crates/squash-core/tests/attacks.rs` (crafted at the byte level where the
  writer crates rightly refuse `..` names) and `tests/rar.rs` (a rar4 file
  header's stored name is swapped for a same-length `../` name, header CRC
  recomputed);
- corrupt archives (garbage, truncated, bad frames) — same files.

## Static files

RAR is the exception: Squash can never *create* rar (RARLAB license) and no
`rar`/`unrar`/`7zz` binary exists on the dev machines, so real archives are
vendored here:

| File | Source | Notes |
|---|---|---|
| `rar4-sample.rar` | [sharpcompress](https://github.com/adamhathcock/sharpcompress) `tests/TestArchives/Archives/Rar4.rar` (BSD-licensed test corpus), fetched 2026-08-13 | RAR4; `тест.txt`, `exe/test.exe`, `jpg/test.jpg` (Cyrillic name exercises the non-ASCII path) |
| `rar5-sample.rar` | same repo, `Rar5.rar` | RAR5; same entries + `Empty/` dir |
| `rar5-encrypted-header.rar` | same repo, `Rar5.encrypted_filesAndHeader.rar` | RAR5 with encrypted headers → `PasswordRequired` |

SHA-256 (pinned; tests skip loudly if a file is absent):

```
9d9c261e50d3a84ab11a3701c7a736057fc970e59ca365885a41f60270c1875e  rar4-sample.rar
3319de3e8a91a58d08d8a83f48ebae7e6b022c542b4a58775bab64f4063988a9  rar5-sample.rar
63fc4b5576f7482311e950155607e171696588881bdd277c8c099efd2f98b6e2  rar5-encrypted-header.rar
```

**Known gap:** no fixture with an *Arabic* filename inside rar — none is
publicly available and creating one needs RARLAB's `rar` binary (creating RAR
is off-limits for Squash itself). The Cyrillic name covers the Unicode
decode path; Arabic entry-name handling is covered by the sanitizer/handler
unit tests with Arabic paths. If a `rar` binary ever becomes available,
capture an Arabic-named sample here and extend `tests/rar.rs`.

Benchmark corpus stays separate in `benches/corpus/`.
