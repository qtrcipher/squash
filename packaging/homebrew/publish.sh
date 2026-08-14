#!/usr/bin/env bash
# Render the Homebrew formula template (Linux-only CLI — macOS was dropped as
# a target on 2026-08-14) with the real version and SHA256 sums from a
# release, then push it to qtrcipher/homebrew-tap.
#
# Usage: publish.sh <tag> <sha256sums-file>
#   <tag>             release tag, e.g. v0.2.0
#   <sha256sums-file>  path to the release's SHA256SUMS.txt
#
# Requires: TAP_GITHUB_TOKEN (contents:write on qtrcipher/homebrew-tap),
# git, awk. Idempotent: no commit/push when nothing changed.
set -euo pipefail

TAG="${1:?usage: publish.sh <tag> <sha256sums-file>}"
SUMS="${2:?usage: publish.sh <tag> <sha256sums-file>}"
: "${TAP_GITHUB_TOKEN:?set TAP_GITHUB_TOKEN (contents:write on qtrcipher/homebrew-tap)}"

VERSION="${TAG#v}"
TAP_REPO="qtrcipher/homebrew-tap"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Extract the SHA256 of a release asset from SHA256SUMS.txt.
# sha256sum output is "<hash>  <name>" (name may carry a '*' binary marker).
sum_for() {
  local file="$1" sum
  sum="$(awk -v f="$file" '{n=$2; sub(/^\*/, "", n); if (n == f) print $1}' "$SUMS")"
  if [ -z "$sum" ]; then
    echo "::error::no SHA256 for $file in $SUMS" >&2
    exit 1
  fi
  echo "$sum"
}

SHA256_LINUX_X86_64="$(sum_for "squash-linux-x86_64.tar.gz")"

render() {
  sed -e "s/@VERSION@/$VERSION/g" \
      -e "s/@SHA256_LINUX_X86_64@/$SHA256_LINUX_X86_64/g" \
      "$1"
}

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

git clone --depth 1 "https://x-access-token:${TAP_GITHUB_TOKEN}@github.com/${TAP_REPO}.git" "$WORK/tap"
mkdir -p "$WORK/tap/Formula"

render "$HERE/squash.rb" > "$WORK/tap/Formula/squash.rb"

cd "$WORK/tap"
git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"

if [ -z "$(git status --porcelain)" ]; then
  echo "Tap already up to date for $TAG — nothing to do."
  exit 0
fi

git add Formula/squash.rb
git commit -m "squash ${TAG}"
git push origin HEAD
echo "Published formula for $TAG to ${TAP_REPO}."
