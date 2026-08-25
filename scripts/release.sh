#!/usr/bin/env bash
# Cut a release: bump versions, commit, tag, push. CI does the rest.
#   scripts/release.sh 0.2.0
set -euo pipefail

VERSION="${1:?usage: scripts/release.sh <version, e.g. 0.2.0>}"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "version must be X.Y.Z"; exit 1; }

cd "$(dirname "$0")/.."

if [[ -n "$(git status --porcelain)" ]]; then
  echo "working tree not clean — commit or stash first"; exit 1
fi

# Bump the three version fields.
sed -i '' "s/^  \"version\": \".*\",/  \"version\": \"$VERSION\",/" package.json
sed -i '' "s/^  \"version\": \".*\",/  \"version\": \"$VERSION\",/" src-tauri/tauri.conf.json
sed -i '' "s/^version = \".*\"/version = \"$VERSION\"/" src-tauri/Cargo.toml
(cd src-tauri && cargo update -p vespry --offline >/dev/null 2>&1 || cargo check -q >/dev/null)

git add package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "Release v$VERSION"
git tag "v$VERSION"
git push origin main "v$VERSION"

echo "Pushed v$VERSION — the Release workflow will build and publish the DMG."
