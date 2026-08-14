# Benchmark corpus (generated, not committed)

The standard corpus for `squash-bench` (docs/05 §6: "fixed, versioned,
documented so numbers are reproducible"). **Nothing here is in git** — the
corpus is reproducible by construction: every byte derives from a seeded
SplitMix64 stream, so the same seed + scale generates byte-identical output
on every machine.

## Regenerate

```sh
cargo run -p squash-bench -- corpus generate            # seed 42, scale 1.0 (~245 MiB)
cargo run -p squash-bench -- corpus generate --scale 0.1  # ~25 MiB, quick runs
```

`manifest.json` (also gitignored) records the seed, scale, per-set file
counts/bytes and FNV-1a checksums of what was generated.

## Composition (scale 1.0)

| set | content | size | what it probes |
|---|---|---|---|
| `text/` | synthetic logs / source / CSV lines | 96 MiB | high-redundancy text (the docs/01 §4 ratio claim lives here) |
| `binary/` | structured 64-byte records, zero-run payloads | 64 MiB | medium compressibility (database-page-like) |
| `mixed/` | project tree: docs + code + assets | ~48 MiB | realistic mixed workload |
| `compressed/` | pure PRNG bytes | 32 MiB | already-compressed stand-in (ratio floor ≈ 100%) |
| `small-files/` | 2048 tiny text files in 16 dirs | ~5 MiB | per-file/container overhead |

Scale is a linear multiplier on sizes and file counts (floors keep tiny
scales non-empty for smoke tests). The benchmark baseline
(`benches/baseline.json`, committed) records the seed+scale it was captured
with; `squash-bench check` refuses to compare runs across mismatched
corpora.

## Why not fixtures?

`fixtures/` holds the *correctness* corpus (attack archives, Unicode names,
corrupt files). This corpus is for *performance* — large, synthetic, and
regenerable, so it never needs committing.
