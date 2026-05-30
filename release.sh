#!/usr/bin/env bash
set -euo pipefail

# Release a new version in one step.
#
#   ./release.sh <new-version>      e.g. ./release.sh 0.2.0
#
# Bumps the workspace version (and the path-dependency version constraints that
# crates.io requires), commits, pushes, then pushes the matching
# `vt-prebuilt-v<version>` tag. That tag triggers .github/workflows/release.yml,
# which builds the prebuilt libghostty-vt artifacts for every target and
# publishes them with a SHA256SUMS manifest. The vendored crate derives its tag
# from CARGO_PKG_VERSION and verifies downloads against that manifest, so there
# is nothing to commit back afterwards.

new="${1:-}"
if [ -z "$new" ]; then
  echo "usage: $0 <new-version>  (e.g. 0.2.0)" >&2
  exit 1
fi

cd "$(dirname "$0")"

if [ -n "$(git status --porcelain)" ]; then
  echo "working tree not clean; commit or stash changes first" >&2
  exit 1
fi

old=$(grep -m1 '^version = ' Cargo.toml | sed -E 's/version = "(.*)"/\1/')
if [ "$old" = "$new" ]; then
  echo "version is already $new" >&2
  exit 1
fi
echo "bumping $old -> $new"

# Only these two manifests carry literal version strings; libghostty-vt-sys and
# libghostty-vt-sys-vendored inherit via version.workspace.
esc=${old//./\\.}
for f in Cargo.toml crates/libghostty-vt/Cargo.toml; do
  sed -i.bak -E "s/version = \"${esc}\"/version = \"${new}\"/g" "$f"
  rm -f "$f.bak"
done

# Refresh Cargo.lock and sanity-check the manifests.
cargo metadata --no-deps --format-version 1 >/dev/null

git add Cargo.toml crates/libghostty-vt/Cargo.toml Cargo.lock
git commit -m "chore: release v${new}"
git push origin HEAD

tag="vt-prebuilt-v${new}"
git tag -a "$tag" -m "Prebuilt libghostty-vt artifacts v${new}"
git push origin "$tag"

echo
echo "Pushed $tag. Watch the build: gh run list --workflow=release.yml"
