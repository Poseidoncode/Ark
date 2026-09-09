#!/bin/sh
set -eu

REPO="Poseidoncode/Ark"
BRANCH="${ARK_BRANCH:-main}"
APP="Ark"

case "$(uname -s)" in
  Darwin)  OS="macos" ;;
  Linux)   OS="linux" ;;
  MINGW*|MSYS*|CYGWIN*) OS="windows" ;;
  *)       echo "Unsupported OS: $(uname -s)"; exit 1 ;;
esac

command -v node >/dev/null 2>&1 || { echo "Missing: Node.js (https://nodejs.org)";  missing=1; }
command -v rustc >/dev/null 2>&1 || { echo "Missing: Rust (https://rustup.rs)"; missing=1; }
command -v cargo >/dev/null 2>&1 || missing=1
command -v git >/dev/null 2>&1 || { echo "Missing: Git (https://git-scm.com)"; missing=1; }
[ -n "${missing:-}" ] && exit 1

CLEANUP_DIR=""
cleanup() {
  if [ -n "$CLEANUP_DIR" ] && [ -d "$CLEANUP_DIR" ]; then
    rm -rf "$CLEANUP_DIR"
  fi
}
trap cleanup EXIT INT TERM

if [ -f "Makefile" ] && grep -q "Ark" Makefile 2>/dev/null; then
  DIR=$(pwd)
else
  DIR=$(mktemp -d "${TMPDIR:-/tmp}/ark-install-XXXXXX")
  CLEANUP_DIR=""
  echo "Cloning $REPO@$BRANCH..."
  git clone --depth=1 --branch "$BRANCH" "https://github.com/$REPO.git" "$DIR"
  git -C "$DIR" rev-parse --verify HEAD
  cd "$DIR"
fi

echo "Installing dependencies..."
if [ -f "package-lock.json" ]; then
  npm ci --ignore-scripts --no-audit --no-fund
else
  npm install --ignore-scripts --no-audit --no-fund
fi
cd src-tauri && cargo fetch && cd ..

echo "Building $APP (this may take a while)..."
npx tauri build --ci

fail() { echo "Build finished but no installer artifact found in $1" >&2; exit 1; }

case "$OS" in
  macos)
    ARTIFACT=$(ls -t src-tauri/target/release/bundle/dmg/*.dmg 2>/dev/null | head -1 || true)
    if [ -z "$ARTIFACT" ]; then
      ARTIFACT=$(ls -t src-tauri/target/release/bundle/macos/*.app 2>/dev/null | head -1 || true)
    fi
    [ -n "$ARTIFACT" ] || fail "src-tauri/target/release/bundle/{dmg,macos}"
    echo ""
    echo "Build complete. Install with:"
    echo "   open \"$ARTIFACT\""
    ;;
  linux)
    ARTIFACT=$(ls -t src-tauri/target/release/bundle/appimage/*.AppImage 2>/dev/null | head -1 || true)
    [ -z "$ARTIFACT" ] && ARTIFACT=$(ls -t src-tauri/target/release/bundle/deb/*.deb 2>/dev/null | head -1 || true)
    [ -n "$ARTIFACT" ] || fail "src-tauri/target/release/bundle/{appimage,deb}"
    echo ""
    echo "Build complete. Run:"
    echo "   $ARTIFACT"
    ;;
  windows)
    ARTIFACT=$(ls -t src-tauri/target/release/bundle/nsis/*.exe 2>/dev/null | head -1 || true)
    [ -z "$ARTIFACT" ] && ARTIFACT=$(ls -t src-tauri/target/release/bundle/msi/*.msi 2>/dev/null | head -1 || true)
    [ -n "$ARTIFACT" ] || fail "src-tauri/target/release/bundle/{nsis,msi}"
    echo ""
    echo "Build complete. Run the installer:"
    echo "   start \"$ARTIFACT\""
    ;;
esac
