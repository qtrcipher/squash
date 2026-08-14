#!/usr/bin/env bash
# Render the Scoop manifest template with the real version and SHA256 from a
# release, then push it to bucket/squash.json in qtrcipher/scoop-bucket.
#
# Usage: publish.sh <tag> <sha256sums-file>
#   <tag>              release tag, e.g. v0.2.0
#   <sha256sums-file>  path to the release's SHA256SUMS.txt
#
# Requires: TAP_GITHUB_TOKEN (contents:write on qtrcipher/scoop-bucket),
# git, awk. Idempotent: no commit/push when nothing changed.
set -euo pipefail

TAG="${1:?usage: publish.sh <tag> <sha256sums-file>}"
SUMS="${2:?usage: publish.sh <tag> <sha256sums-file>}"
: "${TAP_GITHUB_TOKEN:?set TAP_GITHUB_TOKEN (contents:write on qtrcipher/scoop-bucket)}"

VERSION="${TAG#v}"
BUCKET_REPO="qtrcipher/scoop-bucket"
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

SHA256_WINDOWS_X86_64="$(sum_for "squash-windows-x86_64.zip")"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

git clone --depth 1 "https://x-access-token:${TAP_GITHUB_TOKEN}@github.com/${BUCKET_REPO}.git" "$WORK/bucket"
mkdir -p "$WORK/bucket/bucket"

# Render the version + hash placeholders.
sed -e "s/@VERSION@/$VERSION/g" \
    -e "s/@SHA256_WINDOWS_X86_64@/$SHA256_WINDOWS_X86_64/g" \
    "$HERE/squash.json" > "$WORK/bucket/bucket/squash.json"

cd "$WORK/bucket"
git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"

if [ -z "$(git status --porcelain)" ]; then
  echo "Bucket already up to date for $TAG — nothing to do."
  exit 0
fi

git add bucket/squash.json
git commit -m "squash: Update to version ${VERSION}"
git push origin HEAD
echo "Published manifest for $TAG to ${BUCKET_REPO}."
