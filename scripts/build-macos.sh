#!/usr/bin/env bash
# Eden DAW — macOS build script
# Installs dependencies via Homebrew and builds a release binary.
#
# Usage (run on a Mac):
#   chmod +x scripts/build-macos.sh
#   ./scripts/build-macos.sh

set -euo pipefail

echo "==> Eden DAW — macOS build"
echo ""

# ── Check for Homebrew ────────────────────────────────────────────────
if ! command -v brew &>/dev/null; then
    echo "ERROR: Homebrew is required."
    echo "Install it from: https://brew.sh"
    echo '  /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"'
    exit 1
fi

# ── Check for Rust ────────────────────────────────────────────────────
if ! command -v cargo &>/dev/null; then
    echo "==> Rust not found. Installing via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
fi

# ── Install dependencies ──────────────────────────────────────────────
echo "==> Installing dependencies via Homebrew..."
brew install sdl2 sdl2_gfx pkg-config

# ── Build ─────────────────────────────────────────────────────────────
echo "==> Building release binary..."
cargo build --release

BINARY="$(pwd)/target/release/eden"
if [[ -f "$BINARY" ]]; then
    SIZE=$(du -sh "$BINARY" | cut -f1)
    echo ""
    echo "==> Build successful!"
    echo "    Binary: $BINARY ($SIZE)"
    echo ""
    echo "    Run with: ./target/release/eden"
    echo "    Or package with: ./scripts/package-macos.sh"
else
    echo "ERROR: Build failed — binary not found." >&2
    exit 1
fi
