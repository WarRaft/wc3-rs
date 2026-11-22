#!/usr/bin/env bash
set -euo pipefail

# Cross-compiles the one binary the extension ships, `wc3` (../cli), for
# macOS (universal), Linux, and Windows, into extension/bin/ — the layout
# extension.js expects for a packaged (non-dev) install: wc3-macos,
# wc3-linux, wc3-windows.exe. It's the language backend (`wc3 server`,
# this crate) and the MCP server (`wc3 mcp`, ../../wc3-mcp), which the
# extension registers with VS Code. Every cargo call runs from here, so
# .cargo/config.toml (the cross linkers) applies.

# ---------------- helpers ----------------
need() { command -v "$1" >/dev/null 2>&1 || { echo "❌ Missing: $1"; exit 1; }; }
msg()  { echo -e "$*"; }

need cargo
need jq
need rustup
need lipo
need file

cd "$(dirname "${BASH_SOURCE[0]}")"

# Manifests of the crates to build; each one's binary has the package's name.
MANIFESTS=("../cli/Cargo.toml")

# Lands in extension/bin/ — vsce only packages files inside the folder that
# holds package.json (extension/), so the binaries must live there, not here.
DIST_DIR="../extension/bin"
mkdir -p "$DIST_DIR"

# -------- find llvm-strip (prefer rustup, then brew, then xcode) --------
find_llvm_strip() {
  # shellcheck disable=SC2034
  local host triple sysroot p
  host="$(rustc -vV | sed -n 's/^host: //p')"
  sysroot="$(rustc --print sysroot)"
  for p in \
    "$sysroot/lib/rustlib/$host/bin/llvm-strip" \
    "$sysroot/lib/rustlib/bin/llvm-strip" \
    "/opt/homebrew/opt/llvm/bin/llvm-strip" \
    "/usr/local/opt/llvm/bin/llvm-strip" \
    "$(xcrun --find llvm-strip 2>/dev/null || true)"; do
    if [[ -n "${p:-}" && -x "$p" ]]; then
      echo "$p"
      return 0
    fi
  done
  return 1
}

LLVM_STRIP="$(find_llvm_strip || true)"
if [[ -z "$LLVM_STRIP" ]]; then
  msg "🔧 Installing rustup component llvm-tools-preview..."
  rustup component add llvm-tools_preview || rustup component add llvm-tools-preview
  LLVM_STRIP="$(find_llvm_strip || true)"
fi

if [[ -n "$LLVM_STRIP" ]]; then
  msg "🔎 llvm-strip: $LLVM_STRIP"
else
  msg "⚠️  llvm-strip не найден. Буду использовать системный 'strip' где возможно."
fi

strip_safe() {
  # $1 = file path, $2 = kind: macos|linux|windows
  local f="$1" kind="${2:-}"
  if [[ ! -f "$f" ]]; then return 0; fi
  case "$kind" in
    macos)
      # macOS системный strip отлично чистит universal бинарь
      if command -v strip >/dev/null 2>&1; then /usr/bin/strip -x "$f" || true; fi
      ;;
    linux|windows)
      if [[ -n "$LLVM_STRIP" ]]; then
        "$LLVM_STRIP" -s "$f" || true
      else
        # как запасной вариант: системный strip может сломать чужую ABI — лучше просто предупредить
        msg "⚠️  Пропускаю strip для $f (нет llvm-strip)"
      fi
      ;;
    *)
      # неизвестно — попробуем по расширению
      if [[ -n "$LLVM_STRIP" ]]; then "$LLVM_STRIP" -s "$f" || true; fi
      ;;
  esac
}

maybe_upx() {
  # упаковываем только если UPX=1 и есть upx
  if [[ "${UPX:-0}" = "1" ]] && command -v upx >/dev/null 2>&1; then
    upx --best --lzma "$1" || true
  fi
}

# ----------- optional: size-oriented RUSTFLAGS -----------
# Можно отключить, если используешь профиль в Cargo.toml.
export RUSTFLAGS="${RUSTFLAGS:-} -C strip=symbols"
export AR="/usr/local/opt/llvm/bin/llvm-ar"

rustup target add aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-musl x86_64-pc-windows-gnu >/dev/null 2>&1 || true

for MANIFEST in "${MANIFESTS[@]}"; do
  [[ -f "$MANIFEST" ]] || { echo "❌ Missing: $MANIFEST"; exit 1; }
  META="$(cargo metadata --no-deps --format-version 1 --manifest-path "$MANIFEST")"
  NAME="$(jq -r '.packages[0].name' <<<"$META")"
  TARGET_DIR="$(jq -r '.target_directory' <<<"$META")"
  build() { cargo build --release --manifest-path "$MANIFEST" --target "$1"; }

  # ---------------- macOS (universal) ----------------
  msg "📦 $NAME: universal macOS binary…"
  build aarch64-apple-darwin
  build x86_64-apple-darwin
  MAC_UNI="$DIST_DIR/$NAME-macos"
  lipo -create \
    -output "$MAC_UNI" \
    "$TARGET_DIR/aarch64-apple-darwin/release/$NAME" \
    "$TARGET_DIR/x86_64-apple-darwin/release/$NAME"
  strip_safe "$MAC_UNI" macos
  file "$MAC_UNI"

  # ---------------- Linux (x86_64-unknown-linux-musl) ----------------
  msg "🐧 $NAME: Linux (x86_64-unknown-linux-musl)…"
  build x86_64-unknown-linux-musl
  LIN_BIN="$DIST_DIR/$NAME-linux"
  cp "$TARGET_DIR/x86_64-unknown-linux-musl/release/$NAME" "$LIN_BIN"
  strip_safe "$LIN_BIN" linux
  file "$LIN_BIN"

  # ---------------- Windows (x86_64-pc-windows-gnu) ----------------
  msg "🪟 $NAME: Windows (x86_64-pc-windows-gnu)…"
  build x86_64-pc-windows-gnu
  WIN_EXE="$DIST_DIR/$NAME-windows.exe"
  cp "$TARGET_DIR/x86_64-pc-windows-gnu/release/$NAME.exe" "$WIN_EXE"
  strip_safe "$WIN_EXE" windows
  maybe_upx "$WIN_EXE"
  file "$WIN_EXE"
done

# ---------------- summary ----------------
msg "\n✅ Build complete:"
ls -lh "$DIST_DIR"
