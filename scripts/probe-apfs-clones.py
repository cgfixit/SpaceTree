#!/usr/bin/env python3
"""Print APFS clone attributes for synthetic clone scenarios (macOS only).

For each file: allocated bytes (st_blocks * 512), ATTR_CMNEXT_PRIVATESIZE,
ATTR_CMNEXT_CLONEID, ATTR_CMNEXT_CLONE_REFCNT, read with getattrlist and with
getattrlistbulk, so the scanner's accounting can be chosen from evidence.
"""
import ctypes, os, platform, shutil, struct, subprocess, sys, tempfile

if platform.system() != "Darwin":
    print("probe-apfs-clones: skipped, needs macOS")
    sys.exit(0)

libc = ctypes.CDLL(None, use_errno=True)
RETURNED = 0x80000000
NAME = 0x1
PRIVATESIZE, CLONEID, CLONE_REFCNT = 0x8, 0x100, 0x1000
FORK = PRIVATESIZE | CLONEID | CLONE_REFCNT
OPTS = 0x1 | 0x20  # FSOPT_NOFOLLOW | FSOPT_ATTR_CMN_EXTENDED


class AttrList(ctypes.Structure):
    _fields_ = [("bitmapcount", ctypes.c_uint16), ("reserved", ctypes.c_uint16),
                ("common", ctypes.c_uint32), ("vol", ctypes.c_uint32), ("dir", ctypes.c_uint32),
                ("file", ctypes.c_uint32), ("fork", ctypes.c_uint32)]


def fork_attrs(rec, off, fork):
    out = {}
    if fork & PRIVATESIZE:
        out["private"] = struct.unpack_from("<q", rec, off)[0]; off += 8
    if fork & CLONEID:
        out["cloneid"] = struct.unpack_from("<Q", rec, off)[0]; off += 8
    if fork & CLONE_REFCNT:
        out["refcnt"] = struct.unpack_from("<I", rec, off)[0]; off += 4
    return out


def single(path):
    al = AttrList(5, 0, RETURNED, 0, 0, 0, FORK)
    buf = ctypes.create_string_buffer(256)
    rc = libc.getattrlist(path.encode(), ctypes.byref(al), buf, 256, ctypes.c_ulong(OPTS))
    if rc != 0:
        return {"err": os.strerror(ctypes.get_errno())}
    raw = buf.raw
    fork = struct.unpack_from("<5I", raw, 4)[4]
    return fork_attrs(raw, 24, fork)


def bulk(dirpath):
    fd = os.open(dirpath, os.O_RDONLY)
    al = AttrList(5, 0, RETURNED | NAME, 0, 0, 0, FORK)
    buf = ctypes.create_string_buffer(65536)
    libc.getattrlistbulk.argtypes = [ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_uint64]
    out = {}
    try:
        while True:
            n = libc.getattrlistbulk(fd, ctypes.byref(al), buf, 65536, OPTS)
            if n <= 0:
                if n < 0:
                    out["<err>"] = os.strerror(ctypes.get_errno())
                break
            raw, off = buf.raw, 0
            for _ in range(n):
                ln = struct.unpack_from("<I", raw, off)[0]
                rec = raw[off:off + ln]
                fork = struct.unpack_from("<5I", rec, 4)[4]
                rel, nlen = struct.unpack_from("<iI", rec, 24)
                name = rec[24 + rel:24 + rel + nlen].rstrip(b"\0").decode()
                out[name] = fork_attrs(rec, 32, fork)
                off += ln
    finally:
        os.close(fd)
    return out


F_LOG2PHYS_EXT = 65


class Log2Phys(ctypes.Structure):
    _pack_ = 4
    _fields_ = [("flags", ctypes.c_uint32), ("contig", ctypes.c_int64), ("devoffset", ctypes.c_int64)]


def extents(p):
    """[(file_off, dev_off, len)] from F_LOG2PHYS_EXT, or an error string."""
    size = os.lstat(p).st_size
    fd = os.open(p, os.O_RDONLY)
    out, off, calls = [], 0, 0
    libc.fcntl.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
    try:
        while off < size:
            l2p = Log2Phys(0, size - off, off)
            calls += 1
            if libc.fcntl(fd, F_LOG2PHYS_EXT, ctypes.byref(l2p)) == -1:
                out.append(("err", off, os.strerror(ctypes.get_errno())))
                off += 4096
                if calls > 64:
                    break
                continue
            out.append((off, l2p.devoffset, l2p.contig))
            if l2p.contig <= 0:
                break
            off += l2p.contig
    finally:
        os.close(fd)
    return out


def alloc(p):
    return os.lstat(p).st_blocks * 512


def run(*cmd):
    subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def write(p, n):
    with open(p, "wb") as f:
        f.write(os.urandom(n))


def overwrite(p, offset, n):
    with open(p, "r+b") as f:
        f.seek(offset); f.write(os.urandom(n)); f.flush(); os.fsync(f.fileno())


root = tempfile.mkdtemp()
MiB = 1 << 20
try:
    s = {}
    d = os.path.join(root, "pure"); os.mkdir(d); s["pure clone pair"] = d
    write(f"{d}/a", 8 * MiB); run("cp", "-c", f"{d}/a", f"{d}/b")
    d = os.path.join(root, "edit1"); os.mkdir(d); s["clone, b rewrote 4 KiB"] = d
    write(f"{d}/a", 8 * MiB); run("cp", "-c", f"{d}/a", f"{d}/b"); overwrite(f"{d}/b", 0, 4096)
    d = os.path.join(root, "edithalf"); os.mkdir(d); s["clone, b rewrote 4 MiB"] = d
    write(f"{d}/a", 8 * MiB); run("cp", "-c", f"{d}/a", f"{d}/b"); overwrite(f"{d}/b", 0, 4 * MiB)
    d = os.path.join(root, "three"); os.mkdir(d); s["3-way clone, c rewrote 4 KiB"] = d
    write(f"{d}/a", 8 * MiB); run("cp", "-c", f"{d}/a", f"{d}/b"); run("cp", "-c", f"{d}/a", f"{d}/c")
    overwrite(f"{d}/c", 0, 4096)
    d = os.path.join(root, "origgone"); os.mkdir(d); s["clone, original deleted"] = d
    write(f"{d}/a", 8 * MiB); run("cp", "-c", f"{d}/a", f"{d}/b"); os.remove(f"{d}/a")
    d = os.path.join(root, "sparse"); os.mkdir(d); s["sparse 64 MiB with 2 data runs, cloned, b rewrote 4 KiB"] = d
    with open(f"{d}/a", "wb") as f:
        f.write(os.urandom(1 * MiB)); f.seek(48 * MiB); f.write(os.urandom(1 * MiB)); f.truncate(64 * MiB)
    run("cp", "-c", f"{d}/a", f"{d}/b"); overwrite(f"{d}/b", 48 * MiB, 4096)
    d = os.path.join(root, "plain"); os.mkdir(d); s["plain file"] = d
    write(f"{d}/a", 2 * MiB)
    d = os.path.join(root, "hardlink"); os.mkdir(d); s["hard link pair"] = d
    write(f"{d}/a", 2 * MiB); os.link(f"{d}/a", f"{d}/b")
    os.sync()
    for label, d in s.items():
        b = bulk(d)
        print(f"== {label}  (du -sk: {subprocess.run(['du', '-sk', d], capture_output=True, text=True).stdout.split()[0]} KiB)")
        for n in sorted(os.listdir(d)):
            p = f"{d}/{n}"
            print(f"   {n}: alloc={alloc(p)} single={single(p)} bulk={b.get(n)}")
            ex = extents(p)
            print(f"      extents({len(ex)}): {ex[:6]}{' ...' if len(ex) > 6 else ''}")
finally:
    shutil.rmtree(root, ignore_errors=True)
sdk = subprocess.run(["xcrun", "--show-sdk-path"], capture_output=True, text=True).stdout.strip()
hdr = os.path.join(sdk, "usr/include/sys/attr.h")
if os.path.exists(hdr):
    for line in open(hdr):
        if "ATTR_CMNEXT_" in line and "#define" in line:
            print("hdr:", " ".join(line.split()))
