# Vendored RARLAB UnRAR source

- **Version:** 7.2.7 (`unrarsrc-7.2.7.tar.gz`)
- **Download URL:** https://www.rarlab.com/rar/unrarsrc-7.2.7.tar.gz (from https://www.rarlab.com/rar_add.htm)
- **SHA-256 of tarball:** `01d903a7dcf413cb2925696d7796e48e38d471f79bfe7ef3ad2aebf6c12dbefd`
- **Downloaded/vendored:** 2026-08-13
- **Contents:** unmodified RARLAB UnRAR 7.2.7 source tree (`unrar/` from the tarball), including `license.txt` verbatim.

## Why vendored

- The UnRAR license (`license.txt`, clause 2) requires that its full text
  accompany the source; vendoring guarantees the exact source we ship is
  available with the project and keeps builds reproducible (no network fetch
  at build time, no upstream drift).
- `docs/05-architecture.md` §4 picked RARLAB's unrar C source — compiled in
  via `crates/unrar-sys` — over libarchive for RAR extraction.

## Extraction-only constraint (license)

The UnRAR license permits using this source "to handle RAR archives" but
**forbids using it to develop a RAR-compatible archiver or to re-create the
RAR compression algorithm**. Squash therefore uses this code for
**extraction only**:

- `Format::Rar` is never in `Format::CREATE_CAPABLE`; the `RarHandler` has no
  create code path (docs/05 §7 risk 1).
- The source is compiled only into `crates/unrar-sys`, isolated behind the
  `rar` cargo feature of `squash-core` (default ON; build with
  `--no-default-features` for a libre-only build without RAR support).
- Do not modify these files to add encoding capability; do not lift the RAR
  compression logic into other code.

## Layout notes for `crates/unrar-sys/build.rs`

The compiled file list mirrors the upstream makefile's `lib` target
(`OBJECTS` + `filestr scantree dll qopen`), built with `RARDLL` defined
(which implies `SILENT` per `os.hpp`). Files such as `unpack15.cpp`,
`crypt1.cpp`, `blake2s_sse.cpp`, `hardlinks.cpp` etc. are `#include`d from
other translation units — compiling them separately would duplicate symbols.
`rar.cpp`'s `main()` is compiled out under `RARDLL`.
