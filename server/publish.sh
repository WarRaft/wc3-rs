#!/usr/bin/env bash
set -euo pipefail

# Release: cross-compiles `wc3` for all platforms (build.sh), publishes the
# extension to the VS Code Marketplace with vsce (which bumps the version in
# extension/package.json; that change is committed and pushed as `v<version>`),
# then makes a GitHub release `v<that version>` on that commit with
# the same binaries — wc3-macos, wc3-linux, wc3-windows.exe — for running
# `wc3 mcp` without the extension (MCP registries, other clients):
# https://github.com/WarRaft/wc3-rs/releases/latest/download/wc3-<os>.
# Extra arguments go to `vsce publish` (e.g. `./publish.sh --pre-release`).

need() { command -v "$1" >/dev/null 2>&1 || { echo "❌ Missing: $1"; exit 1; }; }
msg()  { echo -e "$*"; }

need vsce
need gh
need jq

SERVER_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXT_DIR="$SERVER_DIR/../extension"
BIN_DIR="$EXT_DIR/bin"

# Checked before anything is published: the version's commit goes on top of
# HEAD and is pushed, so GitHub must already have HEAD, and package.json must
# have no changes of its own (they'd go into that commit).
gh auth status >/dev/null 2>&1 || { echo "❌ gh isn't logged in: gh auth login"; exit 1; }
git -C "$SERVER_DIR" fetch --quiet
HEAD_SHA="$(git -C "$SERVER_DIR" rev-parse HEAD)"
git -C "$SERVER_DIR" branch -r --contains "$HEAD_SHA" | grep -q . \
  || { echo "❌ HEAD ($HEAD_SHA) isn't pushed: push it first"; exit 1; }
git -C "$EXT_DIR" diff --quiet HEAD -- package.json \
  || { echo "❌ extension/package.json has uncommitted changes"; exit 1; }

msg "🔨 Building server binaries…"
(cd "$SERVER_DIR" && ./build.sh)

msg "\n🚀 Publishing extension…"
(cd "$EXT_DIR" && vsce publish "$@" patch)

VERSION="$(jq -r .version "$EXT_DIR/package.json")"
TAG="v$VERSION"

# The bumped version is committed (that file alone) and pushed, so the tag
# names the commit whose package.json says $VERSION.
msg "\n📝 Committing ${TAG}…"
git -C "$EXT_DIR" commit --quiet -m "$TAG" -- package.json
git -C "$EXT_DIR" push --quiet
HEAD_SHA="$(git -C "$EXT_DIR" rev-parse HEAD)"
ASSETS=("$BIN_DIR/wc3-macos" "$BIN_DIR/wc3-linux" "$BIN_DIR/wc3-windows.exe")
for f in "${ASSETS[@]}"; do [[ -f "$f" ]] || { echo "❌ Missing: $f"; exit 1; }; done

msg "\n🏷  GitHub release ${TAG}…"
if gh release view "$TAG" >/dev/null 2>&1; then
  gh release upload "$TAG" "${ASSETS[@]}" --clobber
else
  gh release create "$TAG" "${ASSETS[@]}" \
    --target "$HEAD_SHA" \
    --title "$TAG" \
    --notes "\`wc3\` $VERSION, one binary per system: \`wc3 mcp\` is the MCP server (stdio), \`wc3 server\` the language backend. macOS (universal): \`wc3-macos\`, Linux (x86_64, static): \`wc3-linux\`, Windows (x86_64): \`wc3-windows.exe\`."
fi

msg "\n✅ Published $TAG"
