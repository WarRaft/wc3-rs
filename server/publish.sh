#!/usr/bin/env bash
set -euo pipefail

# Release: cross-compiles wc3-server and wc3-mcp for all platforms (build.sh), then
# publishes the extension to the VS Code Marketplace with vsce.
# Extra arguments go to `vsce publish` (e.g. `./publish.sh patch`).

need() { command -v "$1" >/dev/null 2>&1 || { echo "❌ Missing: $1"; exit 1; }; }
msg()  { echo -e "$*"; }

need vsce

SERVER_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXT_DIR="$SERVER_DIR/../extension"

msg "🔨 Building server binaries…"
(cd "$SERVER_DIR" && ./build.sh)

msg "\n🚀 Publishing extension…"
(cd "$EXT_DIR" && vsce publish "$@" patch)

msg "\n✅ Published"
