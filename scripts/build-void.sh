#!/usr/bin/env bash
# Eden DAW — Void Linux build script
# Installs dependencies and builds a release binary.
#
# Usage:
#   chmod +x scripts/build-void.sh
#   ./scripts/build-void.sh

set -euo pipefail

echo "==> Eden DAW — Void Linux build"
echo ""

# ── Install dependencies ──────────────────────────────────────────────
echo "==> Installing dependencies via xbps..."
sudo xbps-install -Sy \
    rust \
    cargo \
    pkg-config \
    SDL2-devel \
    SDL2_gfx-devel \
    SDL2_ttf-devel \
    SDL2_image-devel \
    alsa-lib-devel

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
