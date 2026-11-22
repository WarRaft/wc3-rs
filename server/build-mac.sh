#!/usr/bin/env bash
set -euo pipefail

# Быстрая сборка wc3 (../cli: `wc3 server` и `wc3 mcp`) только под текущий
# Mac (нативный target, без universal-линковки и без Linux/Windows) — для
# локального тестирования. Полная кросс-сборка — в build.sh.

need() { command -v "$1" >/dev/null 2>&1 || { echo "❌ Missing: $1"; exit 1; }; }
msg()  { echo -e "$*"; }

need cargo
need jq

cd "$(dirname "${BASH_SOURCE[0]}")"

MANIFESTS=("../cli/Cargo.toml")

# Lands in extension/bin/ — vsce only packages files inside the folder that
# holds package.json (extension/), so the binaries must live there, not here.
DIST_DIR="../extension/bin"
mkdir -p "$DIST_DIR"

HOST_TARGET="$(rustc -vV | sed -n 's/^host: //p')"

for MANIFEST in "${MANIFESTS[@]}"; do
  [[ -f "$MANIFEST" ]] || { echo "❌ Missing: $MANIFEST"; exit 1; }
  META="$(cargo metadata --no-deps --format-version 1 --manifest-path "$MANIFEST")"
  NAME="$(jq -r '.packages[0].name' <<<"$META")"
  TARGET_DIR="$(jq -r '.target_directory' <<<"$META")"

  msg "📦 Building $NAME for $HOST_TARGET (debug)…"
  cargo build --manifest-path "$MANIFEST" --target "$HOST_TARGET"

  OUT_BIN="$DIST_DIR/$NAME-macos"
  cp "$TARGET_DIR/$HOST_TARGET/debug/$NAME" "$OUT_BIN"
  file "$OUT_BIN"
done

msg "\n✅ Build complete:"
ls -lh "$DIST_DIR"
