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
TARGET_DIR="$(cargo metadata --format-version=1 --no-deps | plutil -extract target_directory raw -o - -)"
BIN="$TARGET_DIR/release/spacetree"
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

# AppIcon.icns from the 1024 px master, at every size Finder and the Dock ask for.
ICONSET="$DIST/AppIcon.iconset"
rm -rf "$ICONSET"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$ROOT/assets/AppIcon.png" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" "$ROOT/assets/AppIcon.png" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"
rm -rf "$ICONSET"
codesign --force --sign - "$APP"
echo "built $APP"
lipo -info "$OUT" || true
