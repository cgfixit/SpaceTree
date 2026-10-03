#!/bin/sh
# Check SpaceTree's size accounting against macOS's own tools on a synthetic
# APFS tree. Asserts what has a ground truth (stat, du, df) and prints what
# does not (hard links, partially rewritten clones) so the CI log records it.
set -eu
bin=${1:-./target/debug/spacetree}
if [ "$(uname -s)" != "Darwin" ]; then
  echo "verify-macos-sizes: skipped, needs macOS" >&2
  exit 0
fi
if [ ! -x "$bin" ]; then
  echo "missing binary: $bin" >&2
  exit 1
fi

root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT
fail=0
check() {
  # check <label> <want> <got>
  if [ "$2" = "$3" ]; then
    printf 'ok    %-34s %s\n' "$1" "$3"
  else
    printf 'FAIL  %-34s want %s got %s\n' "$1" "$2" "$3"
    fail=1
  fi
}
alloc() { echo $(( $(stat -f %b "$1") * 512 )); }
du_bytes() { echo $(( $(BLOCKSIZE=512 du -s "$1" | awk '{print $1}') * 512 )); }
# Size column of the first --scan row named $2 (names here have no spaces).
row() { printf '%s\n' "$1" | awk -v n="$2" '$NF == n { print $1; exit }'; }

printf 'macOS %s (%s) %s; fixture volume: %s\n' \
  "$(sw_vers -productVersion)" "$(sw_vers -buildVersion)" "$(uname -m)" \
  "$(df -h "$root" | awk 'NR==2{print $1" "$2" on "$NF}')"

# Plain files, a sparse file, a decmpfs-compressed copy, a resource fork.
plain="$root/plain"
mkdir -p "$plain"
printf 'x' > "$plain/one.txt"
head -c 1048576 /dev/urandom > "$plain/rand.bin"
dd if=/dev/zero of="$plain/sparse.img" bs=1 count=1 seek=104857599 2>/dev/null
yes spacetree | head -c 4194304 > "$root/compressible.txt"
ditto --hfsCompression "$root/compressible.txt" "$plain/compressed.txt"
rm "$root/compressible.txt"
printf 'data' > "$plain/forked.txt"
head -c 65536 /dev/urandom > "$plain/forked.txt/..namedfork/rsrc"
mkdir "$plain/sub"
head -c 300000 /dev/urandom > "$plain/sub/nested.bin"

out=$("$bin" --scan "$plain")
for f in one.txt rand.bin sparse.img compressed.txt forked.txt; do
  check "alloc $f" "$(alloc "$plain/$f")" "$(row "$out" "$f")"
done
check "alloc sub/nested.bin" "$(alloc "$plain/sub/nested.bin")" "$(row "$out" nested.bin)"
check "folder total = du -s" "$(du_bytes "$plain")" "$(printf '%s\n' "$out" | awk -F= '/^root_size_bytes=/{print $2}')"
if [ "$(stat -f %z "$plain/sparse.img")" -le "$(alloc "$plain/sparse.img")" ]; then
  echo "note  sparse.img is not sparse on this volume"
fi

# Volume capacity: df -k reports f_blocks * f_bsize / 1024 from statfs.
vol=$(printf '%s\n' "$out" | awk -F= '/^volume_total_bytes=/{print $2}')
df_bytes=$(( $(df -k "$plain" | awk 'NR==2{print $2}') * 1024 ))
check "volume_total = df -k" "$df_bytes" "$vol"

# Symlinks count their lstat length (scan.rs documents this), not st_blocks.
mkdir "$root/symlink"
ln -s ../plain/rand.bin "$root/symlink/link.bin"
sout=$("$bin" --scan "$root/symlink")
printf 'info  symlink: spacetree=%s st_size=%s st_blocks*512=%s\n' \
  "$(row "$sout" link.bin)" "$(stat -f %z "$root/symlink/link.bin")" "$(alloc "$root/symlink/link.bin")"

# APFS clones: unique allocation counts once; du counts each name.
clones="$root/clones"
mkdir -p "$clones/a" "$clones/b"
head -c 8388608 /dev/urandom > "$clones/a/orig.bin"
cp -c "$clones/a/orig.bin" "$clones/b/clone.bin"
cout=$("$bin" --scan "$clones")
csize=$(printf '%s\n' "$cout" | awk -F= '/^root_size_bytes=/{print $2}')
check "pure clone counted once" "$(alloc "$clones/a/orig.bin")" "$csize"
printf 'info  du -s on the clone pair             %s (du counts clones twice)\n' "$(du_bytes "$clones")"

# Observations without a single right answer: printed, not asserted.
links="$root/links"
mkdir -p "$links"
head -c 2097152 /dev/urandom > "$links/first.bin"
ln "$links/first.bin" "$links/second.bin"
lout=$("$bin" --scan "$links")
printf 'info  hard link pair: first=%s second=%s total=%s du=%s\n' \
  "$(row "$lout" first.bin)" "$(row "$lout" second.bin)" \
  "$(printf '%s\n' "$lout" | awk -F= '/^root_size_bytes=/{print $2}')" "$(du_bytes "$links")"

partial="$root/partial"
mkdir -p "$partial"
head -c 8388608 /dev/urandom > "$partial/base.bin"
cp -c "$partial/base.bin" "$partial/edited.bin"
dd if=/dev/urandom of="$partial/edited.bin" bs=4096 count=1 conv=notrunc 2>/dev/null
sync
pout=$("$bin" --scan "$partial")
printf 'info  clone with one 4 KiB block rewritten: base=%s edited=%s total=%s (one copy is %s)\n' \
  "$(row "$pout" base.bin)" "$(row "$pout" edited.bin)" \
  "$(printf '%s\n' "$pout" | awk -F= '/^root_size_bytes=/{print $2}')" "$(alloc "$partial/base.bin")"

if [ "$fail" -ne 0 ]; then
  echo "verify-macos-sizes: FAILED" >&2
  exit 1
fi
echo "verify-macos-sizes: ok"
