#!/bin/sh
# Scan the boot volume and check what only a whole-volume scan exercises:
# the totals add up to the volume's used space, and the Data volume's own
# root adds what `/` does not show without walking any firmlinked folder a
# second time. Read errors are expected: the runner is not root.
set -eu
bin=${1:-./target/debug/spacetree}
if [ "$(uname -s)" != "Darwin" ]; then
  echo "verify-macos-root: skipped, needs macOS" >&2
  exit 0
fi
out=$(mktemp)
err=$(mktemp)
trap 'rm -f "$out" "$err"' EXIT
fail=0
start=$(date +%s)
"$bin" --scan / --depth 2 > "$out" 2> "$err" || true
echo "info  scan of / took $(( $(date +%s) - start ))s"
grep -E '^[a-z_]+=' "$out" | sed 's/^/info  /'
tail -n 6 "$err" | sed 's/^/info  stderr: /'

field() { awk -F= -v k="$1" '$1 == k { print $2; exit }' "$out"; }
root=$(field root_size_bytes)
used=$(field volume_used_bytes)
disk=$(field volume_on_disk_bytes)
hidden=$(field hidden_bytes)
if [ -z "$root" ] || [ -z "$used" ] || [ -z "$disk" ] || [ -z "$hidden" ]; then
  echo "FAIL  the report has no whole-volume header" >&2
  exit 1
fi

# Rows are indented two spaces per level; the name is everything after the kind.
echo "info  top-level rows:"
awk '/^  [^ ]/ { n = $6; for (i = 7; i <= NF; i++) n = n " " $i; printf "info    %15s  %s\n", $1, n }' "$out"
children=$(awk '/^  [^ ]/ { s += $1 } END { printf "%.0f\n", s }' "$out")
if [ "$children" = "$root" ]; then
  printf 'ok    %-36s %s\n' "top-level rows sum to the root" "$root"
else
  printf 'FAIL  %-36s rows %s root %s\n' "top-level rows sum to the root" "$children" "$root"
  fail=1
fi
if [ "$hidden" -gt 0 ]; then
  if [ "$root" = "$used" ]; then
    printf 'ok    %-36s %s\n' "root = volume used" "$root"
  else
    printf 'FAIL  %-36s root %s used %s\n' "root = volume used" "$root" "$used"
    fail=1
  fi
else
  echo "note  the walk found at least the volume's used space; no Hidden space row"
fi

# A firmlinked folder walked twice would put far more in files than the
# volume holds on disk.
walked=$((root - hidden))
if [ "$walked" -le $((disk + disk / 50)) ]; then
  printf 'ok    %-36s %s <= %s + 2%%\n' "files fit in the volume's on-disk use" "$walked" "$disk"
else
  printf 'FAIL  %-36s walked %s on disk %s\n' "files fit in the volume's on-disk use" "$walked" "$disk"
  fail=1
fi

# What the Data volume's own root added, and that none of it is a folder
# `/` already shows through a firmlink.
data=$(awk '
  /^  [^ ]/ { inside = ($6 == "System/Volumes/Data") }
  /^    [^ ]/ && inside { n = $6; for (i = 7; i <= NF; i++) n = n " " $i; print $1 "\t" n }' "$out")
if [ -z "$data" ]; then
  echo "note  the Data volume's root added nothing this process could read"
else
  echo "info  Data volume, outside the firmlinks:"
  printf '%s\n' "$data" | awk -F'\t' '{ printf "info    %15s  %s\n", $1, $2 }'
  for name in Users Applications Library Volumes private opt cores; do
    if printf '%s\n' "$data" | awk -F'\t' -v n="$name" '$2 == n { found = 1 } END { exit !found }'; then
      printf 'FAIL  %-36s %s\n' "firmlinked folder walked twice" "$name"
      fail=1
    fi
  done
  [ "$fail" -ne 0 ] || printf 'ok    %-36s\n' "no firmlinked folder walked twice"
fi

if [ "$fail" -ne 0 ]; then
  echo "verify-macos-root: FAILED" >&2
  exit 1
fi
echo "verify-macos-root: ok"
