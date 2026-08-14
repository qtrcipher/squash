# Squash — Phase 3: Security Audit

> Date: 2026-08-14 · Scope: extraction safety on untrusted archives (the product's
> stated differentiator) — path traversal, decompression bombs, the unrar C ABI
> boundary, resource exhaustion, GUI/CLI surface, secrets hygiene.
> Method: claims in code comments were verified against the code, not trusted.

## 1. Findings

| # | Severity | Finding | Location | Status |
|---|----------|---------|----------|--------|
| 1 | **High** | **Symlink-chain zip-slip (check-vs-write bypass).** Link targets were validated *lexically*: an archive planting `c → .` then `a → c/..` passed validation (lexical pop of `c` stays inside), but the OS resolves `c` first and pops its *target* — a file entry `a/evil.txt` would be written outside the destination. | `squash-core/src/safety.rs` (old `sanitize_link_target`) | **Fixed**: physical resolution (canonicalize every existing prefix; `..` pops are re-anchored through the OS) + no writes through symlink ancestors + no clobbering symlinks. Integration tests: `zip_symlink_chain_escape_*`, `tar_symlink_chain_escape_*`, `write_through_planted_symlink_is_blocked`. |
| 2 | **High** | **No decompression-bomb protection.** Every decode loop was unbounded (the code said so itself). A ~100 KiB zip of zeros would expand to 100+ GiB until the disk filled. | all extraction handlers | **Fixed**: `ExtractGuard` (§2). |
| 3 | **Medium** | **Heap write past `std::wstring` size in `utf8_to_wide` (Windows).** `MultiByteToWideChar(…, -1)` returns a count *including* the NUL; the buffer was sized `n-1` and the second call wrote `n` wchar_t — one element past the string's size every call (UB; usually masked by capacity rounding). Triggered by any archive path, not attacker-controlled input. | `unrar-sys/shim/shim.cpp` | **Fixed**: buffer sized `n`, body resized to `n-1` after the call. |
| 4 | **Medium** | **Unbounded symlink-target read.** A zip entry marked symlink with a huge payload was read with `read_to_string` — attacker-sized allocation before any validation. | `squash-core/src/formats/zip.rs` | **Fixed**: 8192-byte cap (`MAX_SYMLINK_TARGET`, ≈ 2× PATH_MAX); longer targets are `CorruptArchive`. |
| 5 | **Medium** | **`UCM_PROCESSDATA` chunk size forwarded unchecked.** A negative/oversized `p2` from the C side would have become a giant `size_t` and an out-of-bounds `slice::from_raw_parts` in Rust. UnRAR passes slices of its own decode buffer, so this is defense-in-depth against C-side bugs, not a known-reachable exploit. | `unrar-sys/shim/shim.cpp` | **Fixed**: negative or > 64 MiB chunks abort the decode (`SQUASH_RAR_ABORTED`). |
| 6 | **Low** | **Unbounded scan of `FileNameW`.** `wide_to_utf8` scanned to NUL with no bound; a non-terminated name (hostile archive + an UnRAR header bug) would read past the struct. Not known-reachable with vendored UnRAR 7.2.7. | `unrar-sys/shim/shim.cpp` | **Fixed**: scan bounded to 8192 code units (8× UnRAR's NM). |
| 7 | **Low** | **Stats trusted declared sizes.** `JobStats::out_bytes` summed header-declared entry sizes (attacker-controlled). Cosmetic — nothing allocated on them — but wrong numbers are a trust hazard. | zip/7z/rar/tar handlers | **Fixed**: all handlers now count *actual bytes written* via the guard. |
| 8 | **Low** | Windows reserved device names (`CON`, `NUL`, …) and NTFS ADS (`file:stream`) in entry names. Device names fail `CreateFile` (job aborts, no escape); ADS writes stay inside the destination. | `safety.rs` | **Accepted** — no traversal; behavior is a clean abort. |
| 9 | **Low** | Entry-count floods (e.g. 1M-entry zip): metadata Vecs are proportional to *real* parsed entries (headers actually read), not declared counts; no `with_capacity` on untrusted lengths found anywhere. Files created 1:1 with real entries. | all handlers | **Mitigated**: `max_entries` cap in the guard (§2). |
| 10 | **Low** | `sevenz-rust2` internal allocations on header-declared sizes are outside our control. | dependency | **Follow-up**: Phase 5 fuzzing target. |
| 11 | **Info** | Tauri surface: no shell anywhere — `reveal_path`/`open_url` go through `tauri-plugin-opener` with a constant URL or a user-chosen path; CLI is pure clap; `settings.toml` hostile content is absorbed by the lenient-coerce loader (unknown → default + warn, corrupt → backup + defaults). Debug logs contain paths **by design** (accepted in docs/06 §3); no env dumps; atomic-write tmp files live in the user-private config dir, not world-writable locations. | `app/src-tauri`, `squash-cli`, `store/` | **Accepted** — no change. |

**Counts: 1 Critical / 2 High / 3 Medium / 4 Low / 1 Info.** (No Critical found.)

## 2. Decompression-bomb guard (the Phase 3 hardening deliverable)

`ExtractGuard` lives in `squash-core/src/safety.rs` next to the path sanitizer —
same non-bypassable posture: every extraction handler (zip, 7z, tar family,
single-file codecs, rar) counts **actual written bytes** per 64 KiB chunk and
entries per loop iteration. Declared header sizes are never used for the
decision (they're attacker-controlled).

**Default limits** (`ExtractLimits::default`):

| Limit | Value | Why |
|---|---|---|
| `max_ratio` | 200× compressed size | Zip bombs hit 1000:1+. Legit extremes (text logs, JSON) sit under ~50:1; 200 leaves headroom without passing bombs. |
| `ratio_floor` | 64 MiB expanded | Ratio only applies past this. Tiny archives legitimately compress >1000:1 (a 1 MiB zero file is fine); the floor stops false positives on small files. |
| `max_total_bytes` | 1 TiB expanded | Absolute backstop for large-but-valid-ratio streams (a 10 GiB archive at 150:1). Disk-full handling stays the OS's job below this. |
| `max_entries` | 1,000,000 | Entry-table floods (inode/progress churn). Real archives with >1M entries are vanishingly rare. |

Overrides for power users/tests: `SQUASH_EXTRACT_MAX_RATIO`,
`SQUASH_EXTRACT_MAX_BYTES`, `SQUASH_EXTRACT_MAX_ENTRIES` (unparseable values
are ignored — limits fail safe, never off).

**Contract on a trip:** the job aborts with
`SquashError::DecompressionBomb` (new stable code `decompression_bomb` —
additive to the docs/05 §3 taxonomy; CLI maps it to exit 6 "unsafe archive",
the same class as zip-slip) and **rolls back** every file/dir/symlink the job
created (tracked in the guard, removed deepest-first; pre-existing files are
never touched — only paths the job itself created are tracked).

**Nested bombs (42.zip-style)** are neutralized structurally: Squash extracts
exactly one level, so inner archives land as inert files; if the user then
extracts one explicitly, the guard trips there. Covered by
`nested_zip_bomb_extracts_as_plain_files`.

**Tests:** guard units (ratio/floor/absolute/entries/overflow, rollback
ordering) in `safety.rs`; end-to-end bombs for zip, tar.gz and single-file gz
(100 MiB zeros ≈ 1000:1) asserting abort + partial-output cleanup in
`tests/attacks.rs`.

## 3. The path-safety contract after this audit

Four layers, all in `safety.rs`, all non-bypassable by handlers:

1. `sanitize_entry_path` — lexical: no absolute paths, no Windows prefixes,
   no `..` (even inert ones), no empty names. Handlers normalize `\` → `/`
   first (Windows-produced zip/7z/rar names).
2. `sanitize_link_target` — **physical**: canonicalizes the destination and
   every existing prefix it walks, so `..` can never pop through a symlink;
   absolute targets rejected; result must stay inside the canonicalized
   destination.
3. `create_dir_all_guarded` / `create_file_guarded` / `reject_symlink_components`
   — no write (dir, file, hardlink) may descend through or overwrite an
   *existing* symlink below the destination. This closes the check-vs-write
   gap between entries. **Accepted strictness trade-off:** an archive that
   plants a symlink and then writes files through it (even legitimately
   inside the destination) is aborted — that pattern is exactly what makes
   chain attacks exploitable.
4. `ExtractGuard` — the bomb guard above, plus rollback.

## 4. Residual risks → Phase 5 fuzzing

- Vendored **UnRAR 7.2.7** C++ parses hostile bytes before any Rust guard
  runs (header parse, decompression VM). The shim constrains its *outputs*
  (bounded names, bounded chunks, no volume change, no large dict, RAR_TEST
  mode so it never touches the fs), but memory-safety inside UnRAR itself is
  the top fuzz target.
- **`sevenz-rust2`** allocations on declared header sizes (finding 10).
- **TOCTOU against a concurrent local process** mutating the destination
  mid-extraction is out of scope (single-user desktop threat model); within a
  job, extraction is single-threaded so the physical checks are exact.
- zip64/tar PAX parser robustness is delegated to the `zip`/`tar` crates —
  include them in fuzz seeds.
- The bomb guard does not bound **CPU** (e.g. pathological LZMA streams);
  cancellation is the mitigation today.

## 5. Verified-good (no change needed)

- Every entry path of every handler passes `sanitize_entry_path` — no
  exceptions found (audited zip, 7z, tar×5, gz/xz/zst, rar).
- tar device nodes/fifos/sparse entries are never materialized; 7z anti-items
  are skipped; rar runs in RAR_TEST (UnRAR never writes); multi-volume rar is
  refused by the shim.
- Encrypted entries abort before writes (zip checks upfront; rar per-entry).
- No `Vec::with_capacity` on untrusted declared lengths anywhere in core.
- GUI commands take no options that reach a shell; OS open-with paths are
  data, never command lines. Settings/history/queue stores fail closed
  (backup + defaults) on hostile content.
