#!/bin/sh
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# Homebrew rustc 1.98 builds this tree; rustup 1.85 in ~/.cargo/bin cannot
# compile current eframe/rfd transitive crates.
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH"
export MACOSX_DEPLOYMENT_TARGET=12.0
cd "$ROOT"

echo "shipping host arch only ($(rustc --version | awk '{print $2}'))" >&2
cargo build --release --locked
BIN="$ROOT/target/release/spacetree"
if [ ! -x "$BIN" ]; then
  echo "no spacetree binary at $BIN" >&2
  exit 1
fi

DIST="$ROOT/dist"
APP="$DIST/SpaceTree.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
OUT="$APP/Contents/MacOS/spacetree"
cp "$BIN" "$OUT"
chmod +x "$OUT"
cp "$ROOT/Info.plist" "$APP/Contents/Info.plist"
codesign --force --sign - "$APP"
echo "built $APP"
lipo -info "$OUT" || true
