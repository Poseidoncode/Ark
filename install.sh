#!/bin/sh
set -eu

REPO="Poseidoncode/Ark"
BRANCH="main"
APP="Ark"

# ── OS detection ──────────────────────────────────────────────
case "$(uname -s)" in
  Darwin)  OS="macos" ;;
  Linux)   OS="linux" ;;
  MINGW*|MSYS*|CYGWIN*) OS="windows" ;;
  *)       echo "Unsupported OS: $(uname -s)"; exit 1 ;;
esac

# ── Prerequisites ─────────────────────────────────────────────
command -v node >/dev/null 2>&1 || { echo "Missing: Node.js (https://nodejs.org)";  missing=1; }
command -v rustc >/dev/null 2>&1 || { echo "Missing: Rust (https://rustup.rs)"; missing=1; }
command -v cargo >/dev/null 2>&1 || missing=1
[ -n "${missing:-}" ] && exit 1

# ── Clone or use current dir ──────────────────────────────────
if [ -f "Makefile" ] && grep -q "Ark" Makefile 2>/dev/null; then
  DIR=$(pwd)
else
  DIR="/tmp/ark-install-$$"
  echo "Cloning $REPO..."
  git clone --depth=1 --branch "$BRANCH" "https://github.com/$REPO.git" "$DIR"
  cd "$DIR"
fi

# ── Build ─────────────────────────────────────────────────────
echo "Installing dependencies..."
npm install --silent
cd src-tauri && cargo fetch && cd ..

echo "Building $APP (this may take a while)..."
npx tauri build --ci

# ── Locate artifact ───────────────────────────────────────────
case "$OS" in
  macos)
    ARTIFACT=$(ls -t src-tauri/target/release/bundle/dmg/*.dmg 2>/dev/null | head -1)
    if [ -z "$ARTIFACT" ]; then
      ARTIFACT=$(ls -t src-tauri/target/release/bundle/macos/*.app 2>/dev/null | head -1)
    fi
    echo ""
    echo "✅ Build complete. Install with:"
    echo "   open \"$ARTIFACT\""
    ;;
  linux)
    # ponytail: AppImage/DEB — pick whichever tauri generated
    ARTIFACT=$(ls -t src-tauri/target/release/bundle/appimage/*.AppImage 2>/dev/null | head -1)
    [ -z "$ARTIFACT" ] && ARTIFACT=$(ls -t src-tauri/target/release/bundle/deb/*.deb 2>/dev/null | head -1)
    echo ""
    echo "✅ Build complete. Run:"
    echo "   $ARTIFACT"
    ;;
  windows)
    ARTIFACT=$(ls -t src-tauri/target/release/bundle/nsis/*.exe 2>/dev/null | head -1)
    [ -z "$ARTIFACT" ] && ARTIFACT=$(ls -t src-tauri/target/release/bundle/msi/*.msi 2>/dev/null | head -1)
    echo ""
    echo "✅ Build complete. Run the installer:"
    echo "   start \"$ARTIFACT\""
    ;;
esac
