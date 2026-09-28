#!/bin/sh
# Boot the built binary. --help must exit 0. --scan must print the two
# header lines and the names in a tiny temporary tree.
set -eu
bin=${1:-./target/debug/spacetree}
if [ ! -x "$bin" ]; then
  echo "missing binary: $bin" >&2
  exit 1
fi
"$bin" --help >/dev/null 2>&1
root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT
printf 'hello\n' > "$root/note.txt"
mkdir "$root/nest"
printf 'world\n' > "$root/nest/inner.txt"
out=$("$bin" --scan "$root")
printf '%s\n' "$out" | grep -q '^volume_total_bytes='
printf '%s\n' "$out" | grep -q '^root_size_bytes='
printf '%s\n' "$out" | grep -q 'note.txt'
printf '%s\n' "$out" | grep -q 'inner.txt'
echo "runtime ok: $bin"
