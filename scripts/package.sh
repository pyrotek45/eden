#!/usr/bin/env bash
# Eden DAW — packaging script
# Builds a self-contained tar.gz that runs on any x86_64 Linux distro.
#
# Usage (from the project root, inside nix-shell):
#   ./scripts/package.sh
#
# Output: dist/eden-linux-x86_64.tar.gz  (also copied to ~/Desktop)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DIST_DIR="$PROJECT_DIR/dist"
BUNDLE_DIR="/tmp/eden_bundle"
OUT_TAR="$DIST_DIR/eden-linux-x86_64.tar.gz"

echo "==> Building release binary..."
cd "$PROJECT_DIR"
cargo build --release

BINARY="$PROJECT_DIR/target/release/eden"

# ── Locate bundled libs from nix store ───────────────────────────────────────
echo "==> Locating SDL2 and libgcc_s from nix store..."

find_lib() {
    local name="$1"
    local result
    result=$(find /nix/store -name "$name" -not -path "*/include/*" 2>/dev/null | head -1)
    if [[ -z "$result" ]]; then
        echo "ERROR: could not find $name in nix store" >&2
        exit 1
    fi
    echo "$result"
}

LIB_SDL2=$(find_lib "libSDL2-2.0.so.0")
LIB_SDL2_GFX=$(find_lib "libSDL2_gfx-1.0.so.0")
LIB_GCC=$(find_lib "libgcc_s.so.1")

echo "  libSDL2:     $LIB_SDL2"
echo "  libSDL2_gfx: $LIB_SDL2_GFX"
echo "  libgcc_s:    $LIB_GCC"

# ── Assemble bundle ───────────────────────────────────────────────────────────
echo "==> Assembling bundle at $BUNDLE_DIR..."
rm -rf "$BUNDLE_DIR"
mkdir -p "$BUNDLE_DIR/lib"

# Copy and patch the binary
cp "$BINARY" "$BUNDLE_DIR/eden"
chmod u+w "$BUNDLE_DIR/eden"
echo "==> Patching ELF interpreter and rpath..."
patchelf --set-interpreter /lib64/ld-linux-x86-64.so.2 "$BUNDLE_DIR/eden"
patchelf --remove-rpath                                  "$BUNDLE_DIR/eden"
# Remove spurious ld-linux NEEDED entry if present
if patchelf --print-needed "$BUNDLE_DIR/eden" | grep -q "ld-linux-x86-64.so.2"; then
    patchelf --remove-needed ld-linux-x86-64.so.2 "$BUNDLE_DIR/eden"
fi

# Copy libs and strip their nix store rpaths (--set-rpath '' clears both DT_RPATH and DT_RUNPATH)
cp "$LIB_SDL2"     "$BUNDLE_DIR/lib/libSDL2-2.0.so.0"
cp "$LIB_SDL2_GFX" "$BUNDLE_DIR/lib/libSDL2_gfx-1.0.so.0"
cp "$LIB_GCC"      "$BUNDLE_DIR/lib/libgcc_s.so.1"
chmod u+w "$BUNDLE_DIR/lib/"*.so*
for lib in "$BUNDLE_DIR/lib/"*.so*; do
    # --force-rpath converts DT_RUNPATH to DT_RPATH, then --remove-rpath drops it
    patchelf --force-rpath "$lib" 2>/dev/null || true
    patchelf --remove-rpath "$lib" 2>/dev/null || true
done

# ── Launcher script ───────────────────────────────────────────────────────────
cat > "$BUNDLE_DIR/eden.sh" << 'EOF'
#!/usr/bin/env bash
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
export LD_LIBRARY_PATH="$SCRIPT_DIR/lib:${LD_LIBRARY_PATH:-}"
exec "$SCRIPT_DIR/eden" "$@"
EOF
chmod +x "$BUNDLE_DIR/eden.sh"

# ── README ────────────────────────────────────────────────────────────────────
cat > "$BUNDLE_DIR/README.txt" << 'EOF'
Eden DAW — Linux x86_64
=======================

Requirements: glibc 2.17+ and libasound (ALSA).
Everything else is bundled.

Install ALSA if needed:
  Arch/Manjaro:  sudo pacman -S alsa-lib
  Ubuntu/Debian: sudo apt install libasound2
  Fedora:        sudo dnf install alsa-lib

Run:
  chmod +x eden.sh
  ./eden.sh

Supported audio formats: WAV, OGG/Vorbis
EOF

# ── Pack ──────────────────────────────────────────────────────────────────────
echo "==> Packing $OUT_TAR..."
mkdir -p "$DIST_DIR"
cd /tmp
tar -czf "$OUT_TAR" eden_bundle/

SIZE=$(du -sh "$OUT_TAR" | cut -f1)
echo "==> Done: $OUT_TAR ($SIZE)"

# Copy to Desktop if it exists
if [[ -d "$HOME/Desktop" ]]; then
    cp "$OUT_TAR" "$HOME/Desktop/eden-linux-x86_64.tar.gz"
    echo "==> Copied to ~/Desktop/eden-linux-x86_64.tar.gz"
fi

# ── Final audit ───────────────────────────────────────────────────────────────
echo ""
echo "==> ELF audit:"
echo -n "  interpreter: "
patchelf --print-interpreter "$BUNDLE_DIR/eden"
echo -n "  rpath:       "
patchelf --print-rpath "$BUNDLE_DIR/eden" || echo "(none)"
echo "  NEEDED:"
patchelf --print-needed "$BUNDLE_DIR/eden" | sed 's/^/    /'
echo ""
echo "==> Nix store path check (should be empty for each lib):"
for lib in "$BUNDLE_DIR/lib/"*.so*; do
    hits=$(readelf -d "$lib" 2>/dev/null | grep -E 'RPATH|RUNPATH' | grep nix || true)
    echo "  $(basename "$lib"): ${hits:-(clean)}"
done
