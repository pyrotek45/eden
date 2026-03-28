#!/usr/bin/env bash
# Eden DAW — Windows cross-compilation guide
# Cross-compiles from Linux to Windows x86_64 using the MinGW toolchain.
#
# Usage:
#   chmod +x scripts/build-windows.sh
#   ./scripts/build-windows.sh
#
# Prerequisites:
#   - Rust with the x86_64-pc-windows-gnu target installed
#   - MinGW-w64 cross-compiler
#   - SDL2 development libraries for Windows (mingw)
#
# This script will guide you through the setup or attempt to build
# if prerequisites are already installed.

set -euo pipefail

echo "==> Eden DAW — Windows cross-compilation"
echo ""

# ── Check for Rust cross-compilation target ───────────────────────────
if ! rustup target list --installed 2>/dev/null | grep -q "x86_64-pc-windows-gnu"; then
    echo "==> Adding Windows cross-compilation target..."
    rustup target add x86_64-pc-windows-gnu
fi

# ── Check for MinGW ───────────────────────────────────────────────────
if ! command -v x86_64-w64-mingw32-gcc &>/dev/null; then
    echo ""
    echo "ERROR: MinGW-w64 cross-compiler not found."
    echo ""
    echo "Install it for your distro:"
    echo "  Arch:        sudo pacman -S mingw-w64-gcc"
    echo "  Debian:      sudo apt install gcc-mingw-w64-x86-64"
    echo "  Fedora:      sudo dnf install mingw64-gcc"
    echo "  Void:        sudo xbps-install -S cross-x86_64-w64-mingw32"
    echo "  NixOS:       Add pkgsCross.mingwW64.buildPackages.gcc to shell.nix"
    echo ""
    echo "You also need SDL2 development libraries for Windows (mingw)."
    echo "Download from: https://github.com/libsdl-org/SDL/releases"
    echo "  - SDL2-devel-<ver>-mingw.tar.gz"
    echo "  - SDL2_gfx, SDL2_ttf, SDL2_image mingw packages"
    echo ""
    echo "Then set these environment variables before building:"
    echo "  export LIBRARY_PATH=/path/to/sdl2-mingw/lib"
    echo "  export PKG_CONFIG_PATH=/path/to/sdl2-mingw/lib/pkgconfig"
    echo ""
    exit 1
fi

# ── Attempt build ─────────────────────────────────────────────────────
echo "==> Building for Windows (x86_64-pc-windows-gnu)..."
echo "    Note: SDL2 mingw libraries must be in LIBRARY_PATH."
echo ""

if cargo build --release --target x86_64-pc-windows-gnu; then
    BINARY="$(pwd)/target/x86_64-pc-windows-gnu/release/eden.exe"
    if [[ -f "$BINARY" ]]; then
        SIZE=$(du -sh "$BINARY" | cut -f1)
        echo ""
        echo "==> Build successful!"
        echo "    Binary: $BINARY ($SIZE)"
        echo ""
        echo "    To distribute, bundle eden.exe with these DLLs:"
        echo "      SDL2.dll, SDL2_gfx.dll, SDL2_ttf.dll, SDL2_image.dll"
        echo "    Download them from: https://github.com/libsdl-org/SDL/releases"
    fi
else
    echo ""
    echo "ERROR: Cross-compilation failed."
    echo "Make sure SDL2 Windows development libraries are available."
    echo "See instructions above."
    exit 1
fi
