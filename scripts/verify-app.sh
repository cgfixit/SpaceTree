#!/bin/sh
# Verify a built SpaceTree.app or its zip. Print the bundle version for callers.
set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: $0 <SpaceTree.app or zip>" >&2
    exit 2
fi

app=$1
case "$app" in
    *.zip)
        test -f "$app"
        unpack=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/spacetree-verify.XXXXXX")
        trap 'rm -rf "$unpack"' EXIT
        ditto -x -k "$app" "$unpack"
        app="$unpack/SpaceTree.app"
        ;;
esac

plist="$app/Contents/Info.plist"
test -x "$app/Contents/MacOS/spacetree"
test "$(plutil -extract CFBundleIdentifier raw "$plist")" = "com.cgfixit.spacetree"
codesign --verify --verbose=2 "$app"
plutil -extract CFBundleShortVersionString raw "$plist"
