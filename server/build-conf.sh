#!/usr/bin/env bash
set -euo pipefail

# One-time environment setup for build.sh / build-mac.sh: installs the
# Homebrew packages and rustup targets/components needed to cross-compile
# wc3-server for macOS (universal), Linux (musl), and Windows (gnu).

msg() { echo -e "$*"; }

if ! command -v brew >/dev/null 2>&1; then
  echo "❌ Homebrew не найден. Установи: https://brew.sh"
  exit 1
fi

need() { command -v "$1" >/dev/null 2>&1 || { echo "❌ Missing: $1"; exit 1; }; }
need cargo
need rustup

# ---------------- Homebrew packages ----------------
brew_install() {
  # $1 = formula, $2 = command to check for (defaults to formula name)
  local formula="$1" cmd="${2:-$1}"
  if command -v "$cmd" >/dev/null 2>&1; then
    msg "✅ $formula уже установлен"
  else
    msg "🍺 Устанавливаю $formula…"
    brew install "$formula"
  fi
}

brew_install jq
brew_install llvm
brew_install mingw-w64 x86_64-w64-mingw32-gcc
brew_install FiloSottile/musl-cross/musl-cross x86_64-linux-musl-gcc

# ---------------- rustup targets ----------------
msg "🎯 Добавляю rustup targets…"
rustup target add \
  aarch64-apple-darwin \
  x86_64-apple-darwin \
  x86_64-unknown-linux-musl \
  x86_64-pc-windows-gnu

# ---------------- rustup components ----------------
msg "🔧 Добавляю llvm-tools (для llvm-strip)…"
rustup component add llvm-tools-preview || rustup component add llvm-tools_preview || true

msg "\n✅ Окружение готово. Теперь можно запускать ./build.sh или ./build-mac.sh"
