#!/usr/bin/env bash
# Eden DAW — NixOS build script
# Enters the nix-shell and builds a release binary.
#
# Usage:
#   chmod +x scripts/build-nixos.sh
#   ./scripts/build-nixos.sh
#
# Note: A shell.nix is provided at the project root with all dependencies.

set -euo pipefail

echo "==> Eden DAW — NixOS build"
echo ""

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

if ! command -v nix-shell &>/dev/null; then
    echo "ERROR: nix-shell not found. Install Nix first:" >&2
    echo "  curl -L https://nixos.org/nix/install | sh" >&2
    exit 1
fi

echo "==> Entering nix-shell and building release binary..."
nix-shell --run "cargo build --release"

BINARY="$PROJECT_DIR/target/release/eden"
if [[ -f "$BINARY" ]]; then
    SIZE=$(du -sh "$BINARY" | cut -f1)
    echo ""
    echo "==> Build successful!"
    echo "    Binary: $BINARY ($SIZE)"
    echo ""
    echo "    To run (inside nix-shell):"
    echo "      nix-shell --run ./target/release/eden"
    echo ""
    echo "    To create a portable package:"
    echo "      ./scripts/package.sh"
else
    echo "ERROR: Build failed — binary not found." >&2
    exit 1
fi
