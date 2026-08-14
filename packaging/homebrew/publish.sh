#!/usr/bin/env bash
# Render the Homebrew formula + cask templates with the real version and
# SHA256 sums from a release, then push them to qtrcipher/homebrew-tap.
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

SHA256_MACOS_AARCH64="$(sum_for "squash-macos-aarch64.tar.gz")"
SHA256_MACOS_X86_64="$(sum_for "squash-macos-x86_64.tar.gz")"
SHA256_LINUX_X86_64="$(sum_for "squash-linux-x86_64.tar.gz")"
SHA256_DMG_AARCH64="$(sum_for "Squash_${VERSION}_aarch64.dmg")"
SHA256_DMG_X64="$(sum_for "Squash_${VERSION}_x64.dmg")"

render() {
  sed -e "s/@VERSION@/$VERSION/g" \
      -e "s/@SHA256_MACOS_AARCH64@/$SHA256_MACOS_AARCH64/g" \
      -e "s/@SHA256_MACOS_X86_64@/$SHA256_MACOS_X86_64/g" \
      -e "s/@SHA256_LINUX_X86_64@/$SHA256_LINUX_X86_64/g" \
      -e "s/@SHA256_DMG_AARCH64@/$SHA256_DMG_AARCH64/g" \
      -e "s/@SHA256_DMG_X64@/$SHA256_DMG_X64/g" \
      "$1"
}

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

git clone --depth 1 "https://x-access-token:${TAP_GITHUB_TOKEN}@github.com/${TAP_REPO}.git" "$WORK/tap"
mkdir -p "$WORK/tap/Formula" "$WORK/tap/Casks"

render "$HERE/squash.rb" > "$WORK/tap/Formula/squash.rb"
render "$HERE/squash-cask.rb" > "$WORK/tap/Casks/squash.rb"

cd "$WORK/tap"
git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"

if [ -z "$(git status --porcelain)" ]; then
  echo "Tap already up to date for $TAG — nothing to do."
  exit 0
fi

git add Formula/squash.rb Casks/squash.rb
git commit -m "squash ${TAG}"
git push origin HEAD
echo "Published formula + cask for $TAG to ${TAP_REPO}."
