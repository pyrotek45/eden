#!/usr/bin/env bash
# Eden DAW — Debian / Ubuntu / Mint build script
# Installs dependencies and builds a release binary.
#
# Usage:
#   chmod +x scripts/build-debian.sh
#   ./scripts/build-debian.sh

set -euo pipefail

echo "==> Eden DAW — Debian/Ubuntu build"
echo ""

# ── Install dependencies ──────────────────────────────────────────────
echo "==> Installing dependencies via apt..."
sudo apt-get update
sudo apt-get install -y \
    rustc \
    cargo \
    pkg-config \
    libsdl2-dev \
    libsdl2-gfx-dev \
    libsdl2-ttf-dev \
    libsdl2-image-dev \
    libasound2-dev

# ── Build ─────────────────────────────────────────────────────────────
echo "==> Building release binary..."
cargo build --release

BINARY="$(pwd)/target/release/eden"
if [[ -f "$BINARY" ]]; then
    SIZE=$(du -sh "$BINARY" | cut -f1)
    echo ""
    echo "==> Build successful!"
    echo "    Binary: $BINARY ($SIZE)"
    echo "    Run with: ./target/release/eden"
else
    echo "ERROR: Build failed — binary not found." >&2
    exit 1
fi
