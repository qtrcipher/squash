#!/usr/bin/env bash
# Seed the cargo-fuzz corpora (fuzz/corpus/<target>/) with tiny valid
# archives — plus truncated variants, so the fuzzer starts on both sides of
# the parse boundary.
#
# Seeds are generated with Squash itself (the real create path), mirroring
# the integration-test fixtures (crates/squash-core/tests/common/mod.rs):
#   nested dirs, an empty dir, an Arabic name. RAR is the exception — Squash
#   can never create rar (RARLAB license), so the vendored fixtures are
#   copied (fixtures/README.md pins their provenance + SHA-256).
#
# Usage: fuzz/seed-corpus.sh   (idempotent; safe to re-run)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Canonical source tree (mirrors tests/common/mod.rs::build_source_tree).
TREE="$WORK/tree"
mkdir -p "$TREE/data/nested" "$TREE/data/empty" "$TREE/data/مجلد عربي"
printf 'hello world\n' > "$TREE/data/hello.txt"
for i in $(seq 0 255); do printf "\\$(printf '%03o' "$i")"; done > "$TREE/data/nested/deep.bin"
printf 'محتوى عربي\n' > "$TREE/data/مجلد عربي/ملف.txt"

SQUASH=(cargo run -q -p squash-cli -- --no-config)

"${SQUASH[@]}" c "$TREE/data" -o "$WORK/seed.zip" -f zip
"${SQUASH[@]}" c "$TREE/data" -o "$WORK/seed.tar.gz" -f tar.gz
"${SQUASH[@]}" c "$TREE/data" -o "$WORK/seed.7z" -f 7z
"${SQUASH[@]}" c "$TREE/data/hello.txt" -o "$WORK/seed.zst" -f zst
# Raw tar seed from the tar.gz (the CLI rightly refuses bare-tar creation;
# extract-only format per docs/05 §4).
gunzip -c "$WORK/seed.tar.gz" > "$WORK/seed.tar"

seed() { # <target> <file>
    mkdir -p "fuzz/corpus/$1"
    cp "$2" "fuzz/corpus/$1/$(basename "$2")"
}
truncated() { # <file> — first 64 bytes, a classic half-header seed
    head -c 64 "$1" > "$1.trunc"
}

for f in "$WORK"/seed.zip "$WORK"/seed.tar "$WORK"/seed.tar.gz "$WORK"/seed.7z "$WORK"/seed.zst; do
    truncated "$f"
done

seed fuzz_zip "$WORK/seed.zip"
seed fuzz_zip "$WORK/seed.zip.trunc"
seed fuzz_tar "$WORK/seed.tar"
seed fuzz_tar "$WORK/seed.tar.trunc"
seed fuzz_tar_gz "$WORK/seed.tar.gz"
seed fuzz_tar_gz "$WORK/seed.tar.gz.trunc"
seed fuzz_sevenz "$WORK/seed.7z"
seed fuzz_sevenz "$WORK/seed.7z.trunc"
seed fuzz_zst "$WORK/seed.zst"
seed fuzz_zst "$WORK/seed.zst.trunc"
seed fuzz_rar fixtures/rar4-sample.rar
seed fuzz_rar fixtures/rar5-sample.rar
seed fuzz_rar fixtures/rar5-encrypted-header.rar

echo "seeded: $(find fuzz/corpus -type f | wc -l | tr -d ' ') files under fuzz/corpus/"
