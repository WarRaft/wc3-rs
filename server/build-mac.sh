#!/usr/bin/env bash
set -euo pipefail

# Быстрая сборка wc3-server только под текущий Mac (нативный target,
# без universal-линковки и без Linux/Windows) — для локального тестирования.
# Полная кросс-сборка — в build.sh.

need() { command -v "$1" >/dev/null 2>&1 || { echo "❌ Missing: $1"; exit 1; }; }
msg()  { echo -e "$*"; }

need cargo
need jq

PROJECT_NAME="$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].name')"
# Lands in extension/bin/ — vsce only packages files inside the folder that
# holds package.json (extension/), so the binary must live there, not here.
DIST_DIR="../extension/bin"
mkdir -p "$DIST_DIR"

HOST_TARGET="$(rustc -vV | sed -n 's/^host: //p')"

msg "📦 Building $PROJECT_NAME for $HOST_TARGET (debug)…"
cargo build --target "$HOST_TARGET"

OUT_BIN="$DIST_DIR/$PROJECT_NAME-macos"
cp "target/$HOST_TARGET/debug/$PROJECT_NAME" "$OUT_BIN"

file "$OUT_BIN"

msg "\n✅ Build complete: $OUT_BIN"
