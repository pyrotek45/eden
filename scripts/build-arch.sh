#!/usr/bin/env bash
# Eden DAW — Arch Linux / Manjaro build script
# Installs dependencies and builds a release binary.
#
# Usage:
#   chmod +x scripts/build-arch.sh
#   ./scripts/build-arch.sh

set -euo pipefail

echo "==> Eden DAW — Arch Linux build"
echo ""

# ── Install dependencies ──────────────────────────────────────────────
echo "==> Installing dependencies via pacman..."
sudo pacman -S --needed --noconfirm \
    rust \
    sdl2 \
    sdl2_gfx \
    sdl2_ttf \
    sdl2_image \
    alsa-lib \
    pkg-config

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
