#!/usr/bin/env bash
# Eden DAW — macOS packaging script
# Builds a self-contained .app bundle and a distributable .tar.gz.
#
# Usage (run on a Mac from the project root):
#   chmod +x scripts/package-macos.sh
#   ./scripts/package-macos.sh
#
# Prerequisites:
#   - Homebrew with: brew install sdl2 sdl2_gfx pkg-config
#   - Rust (via rustup)
#
# Output: dist/Eden.app  +  dist/eden-macos-universal.tar.gz

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DIST_DIR="$PROJECT_DIR/dist"
APP_NAME="Eden"
APP_DIR="$DIST_DIR/${APP_NAME}.app"

echo "==> Eden DAW — macOS packaging"
echo ""

# ── Preflight checks ─────────────────────────────────────────────────
if [[ "$(uname)" != "Darwin" ]]; then
    echo "ERROR: This script must be run on macOS." >&2
    exit 1
fi

if ! command -v cargo &>/dev/null; then
    echo "ERROR: Rust is not installed. Install via: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh" >&2
    exit 1
fi

if ! brew ls --versions sdl2 &>/dev/null; then
    echo "==> Installing SDL2 via Homebrew..."
    brew install sdl2 sdl2_gfx
fi

# ── Detect architecture & build ──────────────────────────────────────
ARCH="$(uname -m)"   # arm64 or x86_64
echo "==> Building release binary for $ARCH..."
cd "$PROJECT_DIR"
cargo build --release

BINARY="$PROJECT_DIR/target/release/eden"
if [[ ! -f "$BINARY" ]]; then
    echo "ERROR: Build failed — binary not found." >&2
    exit 1
fi
SIZE=$(du -sh "$BINARY" | cut -f1)
echo "    Binary: $BINARY ($SIZE)"

# ── Locate dylibs ────────────────────────────────────────────────────
echo "==> Locating SDL2 dylibs..."

find_dylib() {
    local name="$1"
    local result
    # Try Homebrew Cellar first, then common lib paths
    result=$(find "$(brew --prefix)/lib" -name "$name" -type f 2>/dev/null | head -1)
    if [[ -z "$result" ]]; then
        result=$(find "$(brew --prefix)" -name "$name" -type f 2>/dev/null | head -1)
    fi
    if [[ -z "$result" ]]; then
        echo "ERROR: could not find $name" >&2
        exit 1
    fi
    echo "$result"
}

LIB_SDL2=$(find_dylib "libSDL2-2.0.0.dylib")
LIB_SDL2_GFX=$(find_dylib "libSDL2_gfx-1.0.0.dylib")

echo "  libSDL2:     $LIB_SDL2"
echo "  libSDL2_gfx: $LIB_SDL2_GFX"

# ── Create .app bundle ───────────────────────────────────────────────
echo "==> Assembling ${APP_NAME}.app bundle..."
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Frameworks"
mkdir -p "$APP_DIR/Contents/Resources"

# Copy binary
cp "$BINARY" "$APP_DIR/Contents/MacOS/eden"

# Copy dylibs into Frameworks
cp "$LIB_SDL2"     "$APP_DIR/Contents/Frameworks/libSDL2-2.0.0.dylib"
cp "$LIB_SDL2_GFX" "$APP_DIR/Contents/Frameworks/libSDL2_gfx-1.0.0.dylib"

# Create symlinks matching the SONAME the binary expects
(
    cd "$APP_DIR/Contents/Frameworks"
    ln -sf libSDL2-2.0.0.dylib     libSDL2.dylib
    ln -sf libSDL2_gfx-1.0.0.dylib libSDL2_gfx.dylib
)

# ── Fix dylib load paths ─────────────────────────────────────────────
echo "==> Patching dylib load paths..."

# Rewrite the binary's references to point to @executable_path/../Frameworks/
install_name_tool -change "$(otool -L "$APP_DIR/Contents/MacOS/eden" | grep libSDL2-2 | awk '{print $1}')" \
    "@executable_path/../Frameworks/libSDL2-2.0.0.dylib" \
    "$APP_DIR/Contents/MacOS/eden" 2>/dev/null || true

install_name_tool -change "$(otool -L "$APP_DIR/Contents/MacOS/eden" | grep libSDL2_gfx | awk '{print $1}')" \
    "@executable_path/../Frameworks/libSDL2_gfx-1.0.0.dylib" \
    "$APP_DIR/Contents/MacOS/eden" 2>/dev/null || true

# Fix the dylib IDs so they reference themselves via @rpath
install_name_tool -id "@rpath/libSDL2-2.0.0.dylib" \
    "$APP_DIR/Contents/Frameworks/libSDL2-2.0.0.dylib" 2>/dev/null || true
install_name_tool -id "@rpath/libSDL2_gfx-1.0.0.dylib" \
    "$APP_DIR/Contents/Frameworks/libSDL2_gfx-1.0.0.dylib" 2>/dev/null || true

# Fix SDL2_gfx's reference to SDL2 within Frameworks
install_name_tool -change "$(otool -L "$APP_DIR/Contents/Frameworks/libSDL2_gfx-1.0.0.dylib" | grep libSDL2-2 | awk '{print $1}')" \
    "@loader_path/libSDL2-2.0.0.dylib" \
    "$APP_DIR/Contents/Frameworks/libSDL2_gfx-1.0.0.dylib" 2>/dev/null || true

# Add rpath to the binary
install_name_tool -add_rpath "@executable_path/../Frameworks" \
    "$APP_DIR/Contents/MacOS/eden" 2>/dev/null || true

# ── Info.plist ────────────────────────────────────────────────────────
cat > "$APP_DIR/Contents/Info.plist" << 'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Eden</string>
    <key>CFBundleDisplayName</key>
    <string>Eden DAW</string>
    <key>CFBundleIdentifier</key>
    <string>com.pyrotek45.eden</string>
    <key>CFBundleVersion</key>
    <string>0.1.0</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleExecutable</key>
    <string>eden</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleSignature</key>
    <string>????</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSMicrophoneUsageDescription</key>
    <string>Eden DAW needs microphone access for audio recording.</string>
</dict>
</plist>
PLIST

# ── Launcher wrapper (sets DYLD paths as safety net) ─────────────────
mv "$APP_DIR/Contents/MacOS/eden" "$APP_DIR/Contents/MacOS/eden-bin"
cat > "$APP_DIR/Contents/MacOS/eden" << 'LAUNCHER'
#!/usr/bin/env bash
DIR="$(cd "$(dirname "$0")" && pwd)"
export DYLD_LIBRARY_PATH="$DIR/../Frameworks:${DYLD_LIBRARY_PATH:-}"
exec "$DIR/eden-bin" "$@"
LAUNCHER
chmod +x "$APP_DIR/Contents/MacOS/eden"

# ── Pack tar.gz ───────────────────────────────────────────────────────
echo "==> Packing distributable archive..."
OUT_TAR="$DIST_DIR/eden-macos-${ARCH}.tar.gz"
cd "$DIST_DIR"
tar -czf "$OUT_TAR" "${APP_NAME}.app"

SIZE=$(du -sh "$OUT_TAR" | cut -f1)
echo ""
echo "==> Done!"
echo "    App:     $APP_DIR"
echo "    Archive: $OUT_TAR ($SIZE)"

# Copy to Desktop if it exists
if [[ -d "$HOME/Desktop" ]]; then
    cp "$OUT_TAR" "$HOME/Desktop/"
    echo "    Copied to ~/Desktop/$(basename "$OUT_TAR")"
fi

echo ""
echo "==> Dependency audit:"
echo "  Binary dylib references:"
otool -L "$APP_DIR/Contents/MacOS/eden-bin" | grep -v "eden-bin" | sed 's/^/    /'
echo ""
echo "  To run: open $APP_DIR"
echo "  Or:     $APP_DIR/Contents/MacOS/eden"
