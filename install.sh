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
command -v npm >/dev/null 2>&1 || { echo "Missing: npm (https://nodejs.org)"; missing=1; }
command -v rustc >/dev/null 2>&1 || { echo "Missing: Rust (https://rustup.rs)"; missing=1; }
command -v cargo >/dev/null 2>&1 || missing=1
command -v git >/dev/null 2>&1 || { echo "Missing: Git (https://git-scm.com)"; missing=1; }
[ -n "${missing:-}" ] && exit 1

# Keep the checkout and target directory so an interrupted build can resume.
DIR=""
HEARTBEAT_PID=""
cleanup() {
  status=$?
  if [ -n "$HEARTBEAT_PID" ]; then
    kill "$HEARTBEAT_PID" 2>/dev/null || true
    wait "$HEARTBEAT_PID" 2>/dev/null || true
  fi
  if [ "$status" -ne 0 ] && [ -n "$DIR" ] && [ -f "$DIR/install.sh" ]; then
    printf '\nBuild files were kept in: %s\n' "$DIR"
    printf 'To resume using the existing compilation cache:\n  cd "%s" && sh install.sh\n' "$DIR"
  fi
}
trap cleanup 0
trap 'exit 130' INT
trap 'exit 143' TERM

run_step() {
  label=$1
  shift
  started=$(date +%s)
  printf '\n%s\n' "$label"
  (
    sleeper=""
    trap '[ -z "$sleeper" ] || kill "$sleeper" 2>/dev/null; exit 0' TERM INT
    while :; do
      sleep 30 &
      sleeper=$!
      wait "$sleeper"
      sleeper=""
      printf '[%s] Still running (%ss elapsed).\n' "$label" "$(($(date +%s) - started))"
    done
  ) &
  HEARTBEAT_PID=$!
  result=0
  "$@" || result=$?
  kill "$HEARTBEAT_PID" 2>/dev/null || true
  wait "$HEARTBEAT_PID" 2>/dev/null || true
  HEARTBEAT_PID=""
  [ "$result" -eq 0 ] || return "$result"
  printf '%s completed in %ss.\n' "$label" "$(($(date +%s) - started))"
}

printf 'Ark builds from source; the first Rust release build can take many minutes.\n'
echo 'Use Ctrl+C to cancel. Ctrl+Z suspends the build and may leave the Cargo lock held.'

if [ -f "Makefile" ] && grep -q "Ark" Makefile 2>/dev/null; then
  DIR=$(pwd)
  echo "[1/5] Using existing source checkout"
else
  DIR=$(mktemp -d "${TMPDIR:-/tmp}/ark-install-XXXXXX")
  run_step "[1/5] Downloading $REPO@$BRANCH" git clone --progress --depth=1 --branch "$BRANCH" "https://github.com/$REPO.git" "$DIR"
  git -C "$DIR" rev-parse --verify HEAD
  cd "$DIR"
fi

printf 'Build directory: %s\n' "$DIR"
if [ -f "package-lock.json" ]; then
  run_step "[2/5] Installing npm dependencies" npm ci --ignore-scripts --no-audit --no-fund --loglevel=http
else
  run_step "[2/5] Installing npm dependencies" npm install --ignore-scripts --no-audit --no-fund --loglevel=http
fi

run_step "[3/5] Building frontend (TypeScript / Vite)" npm run build

echo "Rust dependencies are downloaded during the build; compilation and linking can be slow."
echo 'If Cargo reports "Blocking waiting for file lock on artifact directory", another build holds the lock.'
echo 'In the terminal that started it, run jobs -l; use fg %N to resume that job (replace N with its job number).'
echo 'Cancel duplicate builds with Ctrl+C. Do not delete the Cargo lock file.'
# The frontend was built above; disable its hook only for this invocation.
run_step "[4/5] Compiling Rust (release)" ./node_modules/.bin/tauri build --ci --no-bundle --verbose --config '{"build":{"beforeBuildCommand":""}}'

# Remove stale bundle outputs: bundle_dmg.sh fails if a previous .dmg exists
rm -rf src-tauri/target/release/bundle
run_step "[5/5] Packaging $APP installer" ./node_modules/.bin/tauri bundle --ci --verbose

fail() { echo "Build finished but no installer artifact found in $1" >&2; exit 1; }

case "$OS" in
  macos)
    ARTIFACT=$(ls -t src-tauri/target/release/bundle/dmg/*.dmg 2>/dev/null | head -1 || true)
    if [ -z "$ARTIFACT" ]; then
      ARTIFACT=$(ls -dt src-tauri/target/release/bundle/macos/*.app 2>/dev/null | head -1 || true)
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
