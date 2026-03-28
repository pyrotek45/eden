#!/usr/bin/env bash
# Eden DAW — Fedora / RHEL / CentOS build script
# Installs dependencies and builds a release binary.
#
# Usage:
#   chmod +x scripts/build-fedora.sh
#   ./scripts/build-fedora.sh

set -euo pipefail

echo "==> Eden DAW — Fedora build"
echo ""

# ── Install dependencies ──────────────────────────────────────────────
echo "==> Installing dependencies via dnf..."
sudo dnf install -y \
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
