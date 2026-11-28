#!/bin/sh
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
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
mkdir -p "$DIST"
STAGE=$(mktemp -d "$DIST/.bundle.XXXXXX")
trap 'rm -rf "$STAGE"' EXIT
APP="$STAGE/SpaceTree.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
OUT="$APP/Contents/MacOS/spacetree"
cp "$BIN" "$OUT"
chmod +x "$OUT"
cp "$ROOT/Info.plist" "$APP/Contents/Info.plist"

# AppIcon.icns from the 1024 px master, at every size Finder and the Dock ask for.
ICONSET="$STAGE/AppIcon.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$ROOT/assets/AppIcon.png" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" "$ROOT/assets/AppIcon.png" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"
rm -rf "$ICONSET"
codesign --force --sign - "$APP"
codesign --verify --strict "$APP"
lipo -info "$OUT"
# renamex_np swaps two directory entries atomically on macOS.
python3 - "$APP" "$DIST/SpaceTree.app" <<'PY'
import ctypes
import os
import sys

source, target = map(os.fsencode, sys.argv[1:])
if os.path.lexists(target):
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.renamex_np(source, target, 2) != 0:  # RENAME_SWAP
        raise OSError(ctypes.get_errno(), "could not replace SpaceTree.app")
else:
    os.rename(source, target)
PY
echo "built $DIST/SpaceTree.app"
