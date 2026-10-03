use std::collections::{BTreeMap, HashSet};
#[cfg(target_os = "macos")]
use std::ffi::CString;
use std::ffi::OsString;
use std::fs::{self, Metadata};
use std::io;
use std::os::unix::ffi::OsStrExt;
#[cfg(target_os = "macos")]
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;
#[cfg(target_os = "macos")]
use std::time::{Duration, UNIX_EPOCH};

use rayon::prelude::*;

use crate::ext::ext_key;
use crate::extents::ExtentLedger;

/// One file or folder in the expandable tree.
#[derive(Clone, Debug)]
pub struct Node {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    /// Allocated bytes. Directory size is the sum of children, not the inode.
    pub size: u64,
    /// `st_size` for a leaf. Directory logical size is the sum of children.
    pub logical: u64,
    /// File leaves under this node, including zero-allocated clones.
    pub files: u64,
    /// Canonical extension key with no dot. Empty means none. Directories store the dominant key.
    pub color_ext: String,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub percent_of_disk: f64,
    pub children: Vec<Node>,
}

/// APFS clone accounting shared by the scan workers.
#[derive(Default)]
struct CloneBook {
    /// Clone ids already counted. A pure clone shares every extent with the
    /// first file of its id, so it counts zero.
    ids: Mutex<HashSet<u64>>,
    /// Physical ranges counted for files that share some extents. A clone
    /// rewritten in part gets a new id but keeps most of its blocks shared.
    extents: Mutex<ExtentLedger>,
}

impl CloneBook {
    /// True when `id` is new to the scan.
    fn first_of_id(&self, id: u64) -> bool {
        self.ids
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id)
    }

    /// Allocated bytes of a file that shares extents, minus the bytes of its
    /// extents another file already counted. Unreadable extents keep `size`.
    fn unshared(&self, path: &Path, dev: u64, size: u64) -> u64 {
        match physical_extents(path) {
            Some(ranges) => {
                let counted = self
                    .extents
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .claim(dev, &ranges);
                size.saturating_sub(counted)
            }
            None => size,
        }
    }
}

struct ExtSum {
    physical: u64,
    files: u64,
}

/// Result of walking a path on this machine.
#[derive(Clone, Debug)]
pub struct ScanResult {
    pub root: Node,
    pub volume_total: u64,
    /// Failed directory reads or entry metadata reads. Zero means none were observed.
    pub error_count: u64,
}

/// Walk `path`. Follow a symlink-to-dir only at this user-chosen root (so `/tmp`
/// scans as a tree). Child symlink directories are not followed.
///
/// Size is allocated bytes (`st_blocks * 512`) for real files so `% of disk`
/// tracks used storage. Symlink leaves keep `lstat` length.
pub fn scan(path: &Path) -> io::Result<ScanResult> {
    let lmeta = fs::symlink_metadata(path)?;
    let volume_total = volume_total_bytes(path);
    let seen = Mutex::new(HashSet::new());
    let clones = CloneBook::default();
    let errors = AtomicU64::new(0);
    let root = if lmeta.file_type().is_dir() {
        walk_dir(path, &lmeta, volume_total, path, &seen, &clones, &errors)?.0
    } else if lmeta.file_type().is_symlink() {
        let followed = fs::metadata(path)?;
        if followed.is_dir() {
            walk_dir(path, &followed, volume_total, path, &seen, &clones, &errors)?.0
        } else {
            leaf(path, &lmeta, volume_total, &clones).0
        }
    } else {
        leaf(path, &lmeta, volume_total, &clones).0
    };
    Ok(ScanResult {
        root,
        volume_total,
        error_count: errors.into_inner(),
    })
}

pub(crate) fn percent_of_disk(size: u64, volume_total: u64) -> f64 {
    if volume_total == 0 {
        0.0
    } else {
        (size as f64) / (volume_total as f64) * 100.0
    }
}

/// Capacity of the volume that holds `path`, as Finder reports it.
///
/// macOS uses `statfs`: its block count is 64-bit. Darwin's `statvfs` stores
/// `f_blocks` in a 32-bit `fsblkcnt_t`, which cannot describe a volume of
/// 2^32 blocks or more (16 TiB at 4 KiB blocks).
fn volume_total_bytes(path: &Path) -> u64 {
    let c = match std::ffi::CString::new(path.as_os_str().as_bytes()) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    #[cfg(target_os = "macos")]
    unsafe {
        // SAFETY: `c` is a NUL-terminated path; `s` is a valid statfs out-param.
        let mut s: libc::statfs = std::mem::zeroed();
        if libc::statfs(c.as_ptr(), &mut s) != 0 {
            return 0;
        }
        s.f_blocks.saturating_mul(u64::from(s.f_bsize))
    }
    #[cfg(not(target_os = "macos"))]
    unsafe {
        // SAFETY: `c` is a NUL-terminated path; `s` is a valid statvfs out-param.
        let mut s: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut s) != 0 {
            return 0;
        }
        (s.f_blocks as u64).saturating_mul(s.f_frsize as u64)
    }
}

/// `st_dev` as the bulk reader reports it. Darwin's `dev_t` is a signed
/// 32-bit value; `MetadataExt::dev` sign-extends it while `ATTR_CMN_DEVID`
/// is read as unsigned, so normalize both to the same 32 bits.
fn dev_id(meta: &Metadata) -> u64 {
    if cfg!(target_os = "macos") {
        u64::from(meta.dev() as u32)
    } else {
        meta.dev()
    }
}

fn node_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn allocated_bytes(meta: &Metadata) -> u64 {
    if meta.file_type().is_symlink() {
        meta.len()
    } else {
        meta.blocks().saturating_mul(512)
    }
}

fn leaf(
    path: &Path,
    meta: &Metadata,
    volume_total: u64,
    clones: &CloneBook,
) -> (Node, BTreeMap<String, ExtSum>) {
    let mut size = allocated_bytes(meta);
    if !meta.file_type().is_symlink() {
        if let Some(id) = apfs_clone_id(path) {
            if !clones.first_of_id(id) {
                size = 0;
            }
        }
    }
    let name = node_name(path);
    let color_ext = ext_key(&name);
    let mut by_ext = BTreeMap::new();
    by_ext.insert(
        color_ext.clone(),
        ExtSum {
            physical: size,
            files: 1,
        },
    );
    (
        Node {
            name,
            path: path.to_path_buf(),
            is_dir: false,
            size,
            logical: meta.len(),
            files: 1,
            color_ext,
            modified: meta.modified().ok(),
            created: meta.created().ok(),
            percent_of_disk: percent_of_disk(size, volume_total),
            children: Vec::new(),
        },
        by_ext,
    )
}

fn is_hidden_data_mount(scan_root: &Path, path: &Path) -> bool {
    if path == scan_root {
        return false;
    }
    if scan_root.starts_with(Path::new("/System/Volumes")) {
        return false;
    }
    if path.starts_with(Path::new("/System/Volumes")) {
        return true;
    }
    match path.strip_prefix(scan_root) {
        Ok(rel) => {
            let mut c = rel.components();
            matches!(
                (c.next(), c.next()),
                (Some(std::path::Component::Normal(a)), Some(std::path::Component::Normal(b)))
                    if a == "System" && b == "Volumes"
            )
        }
        Err(_) => false,
    }
}

/// APFS clone group id (0 = unknown). Same id means shared extents. Count once.
fn apfs_clone_id(path: &Path) -> Option<u64> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        None
    }
    #[cfg(target_os = "macos")]
    {
        let c = CString::new(path.as_os_str().as_bytes()).ok()?;
        #[repr(C)]
        struct AttrList {
            bitmapcount: u16,
            reserved: u16,
            commonattr: u32,
            volattr: u32,
            dirattr: u32,
            fileattr: u32,
            forkattr: u32,
        }
        let mut alist = AttrList {
            bitmapcount: 5,
            reserved: 0,
            commonattr: 0,
            volattr: 0,
            dirattr: 0,
            fileattr: 0,
            forkattr: 0x0000_0100, // ATTR_CMNEXT_CLONEID
        };
        let mut buf = [0u8; 32];
        const FSOPT_NOFOLLOW: libc::c_ulong = 0x1;
        const FSOPT_ATTR_CMN_EXTENDED: libc::c_ulong = 0x20;
        extern "C" {
            fn getattrlist(
                path: *const libc::c_char,
                attr_list: *mut libc::c_void,
                attr_buf: *mut libc::c_void,
                attr_buf_size: libc::size_t,
                options: libc::c_ulong,
            ) -> libc::c_int;
        }
        let rc = unsafe {
            // SAFETY: `c` is NUL-terminated; `alist`/`buf` are valid out-params.
            getattrlist(
                c.as_ptr(),
                (&mut alist as *mut AttrList).cast(),
                buf.as_mut_ptr().cast(),
                buf.len(),
                FSOPT_NOFOLLOW | FSOPT_ATTR_CMN_EXTENDED,
            )
        };
        if rc != 0 {
            return None;
        }
        let len = u32::from_le_bytes(buf[0..4].try_into().ok()?) as usize;
        if len < 12 {
            return None;
        }
        let id = u64::from_le_bytes(buf[4..12].try_into().ok()?);
        if id == 0 {
            None
        } else {
            Some(id)
        }
    }
}

#[cfg(target_os = "macos")]
const ATTR_CMN_NAME: u32 = 0x0000_0001;
#[cfg(target_os = "macos")]
const ATTR_CMN_DEVID: u32 = 0x0000_0002;
#[cfg(target_os = "macos")]
const ATTR_CMN_OBJTYPE: u32 = 0x0000_0008;
#[cfg(target_os = "macos")]
const ATTR_CMN_CRTIME: u32 = 0x0000_0200;
#[cfg(target_os = "macos")]
const ATTR_CMN_MODTIME: u32 = 0x0000_0400;
#[cfg(target_os = "macos")]
const ATTR_CMN_FILEID: u32 = 0x0200_0000;
#[cfg(target_os = "macos")]
const ATTR_CMN_RETURNED_ATTRS: u32 = 0x8000_0000;
#[cfg(target_os = "macos")]
const ATTR_FILE_ALLOCSIZE: u32 = 0x0000_0004;
#[cfg(target_os = "macos")]
const ATTR_FILE_DATALENGTH: u32 = 0x0000_0200;
#[cfg(target_os = "macos")]
const ATTR_CMNEXT_PRIVATESIZE: u32 = 0x0000_0008;
#[cfg(target_os = "macos")]
const ATTR_CMNEXT_CLONEID: u32 = 0x0000_0100;
#[cfg(target_os = "macos")]
const FSOPT_NOFOLLOW: u64 = 0x1;
#[cfg(target_os = "macos")]
const FSOPT_ATTR_CMN_EXTENDED: u64 = 0x20;
const VDIR: u32 = 2;
const VLNK: u32 = 5;

#[cfg(target_os = "macos")]
#[repr(C)]
struct AttrList {
    bitmapcount: u16,
    reserved: u16,
    commonattr: u32,
    volattr: u32,
    dirattr: u32,
    fileattr: u32,
    forkattr: u32,
}

struct BulkEnt {
    name: OsString,
    objtype: u32,
    dev: u64,
    ino: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    data_length: u64,
    alloc_size: u64,
    has_sizes: bool,
    clone_id: Option<u64>,
    /// Bytes not shared with any other clone (`ATTR_CMNEXT_PRIVATESIZE`).
    private_size: Option<u64>,
}

#[cfg(target_os = "macos")]
struct DirFd(libc::c_int);

#[cfg(target_os = "macos")]
impl Drop for DirFd {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.0);
        }
    }
}

#[cfg(target_os = "macos")]
fn open_dir(path: &Path) -> io::Result<DirFd> {
    let mut bytes = Vec::with_capacity(path.as_os_str().len() + 1);
    bytes.extend_from_slice(path.as_os_str().as_bytes());
    if bytes.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path contains an interior NUL",
        ));
    }
    bytes.push(0);
    let fd = unsafe {
        libc::open(
            bytes.as_ptr().cast(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(DirFd(fd))
    }
}

#[cfg(target_os = "macos")]
fn list_bulk(path: &Path, errors: &AtomicU64) -> io::Result<Vec<BulkEnt>> {
    let dir = open_dir(path)?;
    list_bulk_fd(dir.0, errors)
}

#[cfg(target_os = "macos")]
fn list_bulk_fd(fd: libc::c_int, errors: &AtomicU64) -> io::Result<Vec<BulkEnt>> {
    let mut alist = AttrList {
        bitmapcount: 5,
        reserved: 0,
        commonattr: ATTR_CMN_RETURNED_ATTRS
            | ATTR_CMN_NAME
            | ATTR_CMN_DEVID
            | ATTR_CMN_OBJTYPE
            | ATTR_CMN_CRTIME
            | ATTR_CMN_MODTIME
            | ATTR_CMN_FILEID,
        volattr: 0,
        dirattr: 0,
        fileattr: ATTR_FILE_DATALENGTH | ATTR_FILE_ALLOCSIZE,
        forkattr: ATTR_CMNEXT_PRIVATESIZE | ATTR_CMNEXT_CLONEID,
    };
    let mut buf = vec![0u8; 256 * 1024];
    let mut out = Vec::new();
    let mut got_batch = false;
    unsafe extern "C" {
        fn getattrlistbulk(
            dirfd: libc::c_int,
            attr_list: *mut libc::c_void,
            attr_buf: *mut libc::c_void,
            attr_buf_size: libc::size_t,
            options: u64,
        ) -> libc::c_int;
    }
    loop {
        let n = unsafe {
            getattrlistbulk(
                fd,
                (&mut alist as *mut AttrList).cast(),
                buf.as_mut_ptr().cast(),
                buf.len(),
                FSOPT_NOFOLLOW | FSOPT_ATTR_CMN_EXTENDED,
            )
        };
        if n < 0 {
            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::ERANGE) && buf.len() < 8 * 1024 * 1024 {
                buf.resize(buf.len() * 2, 0);
                continue;
            }
            if got_batch {
                errors.fetch_add(1, Ordering::Relaxed);
                break;
            }
            return Err(err);
        }
        if n == 0 {
            break;
        }
        got_batch = true;
        let mut off = 0usize;
        for _ in 0..n {
            let Some((next, ent)) = parse_bulk_entry(&buf, off) else {
                errors.fetch_add(1, Ordering::Relaxed);
                break;
            };
            off = next;
            if ent.name == "." || ent.name == ".." {
                continue;
            }
            out.push(ent);
        }
    }
    Ok(out)
}

#[cfg(target_os = "macos")]
fn align4(off: usize) -> usize {
    off.div_ceil(4) * 4
}

#[cfg(target_os = "macos")]
fn parse_bulk_entry(buf: &[u8], start: usize) -> Option<(usize, BulkEnt)> {
    if start + 4 > buf.len() {
        return None;
    }
    let rec_len = u32::from_ne_bytes(buf[start..start + 4].try_into().ok()?) as usize;
    if rec_len < 4 || start + rec_len > buf.len() {
        return None;
    }
    let rec = &buf[start..start + rec_len];
    let mut o = 4usize;
    if o + 20 > rec.len() {
        return None;
    }
    let common = u32::from_ne_bytes(rec[o..o + 4].try_into().ok()?);
    let file = u32::from_ne_bytes(rec[o + 12..o + 16].try_into().ok()?);
    let fork = u32::from_ne_bytes(rec[o + 16..o + 20].try_into().ok()?);
    o += 20;

    let mut name = OsString::new();
    if common & ATTR_CMN_NAME != 0 {
        o = align4(o);
        if o + 8 > rec.len() {
            return None;
        }
        let rel = i32::from_ne_bytes(rec[o..o + 4].try_into().ok()?) as isize;
        let len = u32::from_ne_bytes(rec[o + 4..o + 8].try_into().ok()?) as usize;
        let at = o.checked_add_signed(rel)?;
        if len == 0 || at.checked_add(len)? > rec.len() {
            return None;
        }
        let raw = &rec[at..at + len];
        let raw = raw.strip_suffix(&[0]).unwrap_or(raw);
        name = OsString::from_vec(raw.to_vec());
        o += 8;
    }
    let mut dev = 0u64;
    if common & ATTR_CMN_DEVID != 0 {
        o = align4(o);
        if o + 4 > rec.len() {
            return None;
        }
        dev = u64::from(u32::from_ne_bytes(rec[o..o + 4].try_into().ok()?));
        o += 4;
    }
    let mut objtype = 0u32;
    if common & ATTR_CMN_OBJTYPE != 0 {
        o = align4(o);
        if o + 4 > rec.len() {
            return None;
        }
        objtype = u32::from_ne_bytes(rec[o..o + 4].try_into().ok()?);
        o += 4;
    }
    let created = read_time(rec, &mut o, common & ATTR_CMN_CRTIME != 0)?;
    let modified = read_time(rec, &mut o, common & ATTR_CMN_MODTIME != 0)?;
    let mut ino = 0u64;
    if common & ATTR_CMN_FILEID != 0 {
        o = align4(o);
        if o + 8 > rec.len() {
            return None;
        }
        ino = u64::from_ne_bytes(rec[o..o + 8].try_into().ok()?);
        o += 8;
    }
    let mut data_length = 0u64;
    let mut alloc_size = 0u64;
    let mut has_sizes = false;
    // File attributes arrive in ascending bit order. ALLOCSIZE is the lower bit.
    if file & ATTR_FILE_ALLOCSIZE != 0 {
        o = align4(o);
        if o + 8 > rec.len() {
            return None;
        }
        alloc_size = u64::from_ne_bytes(rec[o..o + 8].try_into().ok()?);
        o += 8;
        has_sizes = true;
    }
    if file & ATTR_FILE_DATALENGTH != 0 {
        o = align4(o);
        if o + 8 > rec.len() {
            return None;
        }
        data_length = u64::from_ne_bytes(rec[o..o + 8].try_into().ok()?);
        o += 8;
        has_sizes = true;
    }
    // Fork (extended common) attributes also arrive in ascending bit order.
    let mut private_size = None;
    if fork & ATTR_CMNEXT_PRIVATESIZE != 0 {
        o = align4(o);
        if o + 8 > rec.len() {
            return None;
        }
        let bytes = i64::from_ne_bytes(rec[o..o + 8].try_into().ok()?);
        private_size = u64::try_from(bytes).ok();
        o += 8;
    }
    let mut clone_id = None;
    if fork & ATTR_CMNEXT_CLONEID != 0 {
        o = align4(o);
        if o + 8 > rec.len() {
            return None;
        }
        let id = u64::from_ne_bytes(rec[o..o + 8].try_into().ok()?);
        if id != 0 {
            clone_id = Some(id);
        }
    }
    Some((
        start + rec_len,
        BulkEnt {
            name,
            objtype,
            dev,
            ino,
            modified,
            created,
            data_length,
            alloc_size,
            has_sizes,
            clone_id,
            private_size,
        },
    ))
}

#[cfg(target_os = "macos")]
fn read_time(rec: &[u8], o: &mut usize, present: bool) -> Option<Option<SystemTime>> {
    if !present {
        return Some(None);
    }
    *o = align4(*o);
    if *o + 16 > rec.len() {
        return None;
    }
    let sec = i64::from_ne_bytes(rec[*o..*o + 8].try_into().ok()?);
    let nsec = i64::from_ne_bytes(rec[*o + 8..*o + 16].try_into().ok()?);
    *o += 16;
    Some(system_time(sec, nsec))
}

#[cfg(target_os = "macos")]
fn system_time(sec: i64, nsec: i64) -> Option<SystemTime> {
    let nsec = u32::try_from(nsec).ok()?;
    if sec >= 0 {
        UNIX_EPOCH.checked_add(Duration::new(sec as u64, nsec))
    } else {
        let mag = sec.unsigned_abs();
        UNIX_EPOCH
            .checked_sub(Duration::new(mag, 0))?
            .checked_add(Duration::from_nanos(u64::from(nsec)))
    }
}

#[cfg(not(target_os = "macos"))]
fn list_bulk(path: &Path, errors: &AtomicU64) -> io::Result<Vec<BulkEnt>> {
    let mut out = Vec::new();
    for ent in fs::read_dir(path)? {
        let ent = match ent {
            Ok(ent) => ent,
            Err(_) => {
                errors.fetch_add(1, Ordering::Relaxed);
                continue;
            }
        };
        let name = ent.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(ent.path()) else {
            errors.fetch_add(1, Ordering::Relaxed);
            continue;
        };
        let objtype = if meta.file_type().is_symlink() {
            VLNK
        } else if meta.is_dir() {
            VDIR
        } else {
            1
        };
        let alloc_size = if meta.file_type().is_symlink() {
            meta.len()
        } else {
            meta.blocks().saturating_mul(512)
        };
        out.push(BulkEnt {
            name,
            objtype,
            dev: meta.dev(),
            ino: meta.ino(),
            modified: meta.modified().ok(),
            created: meta.created().ok(),
            data_length: meta.len(),
            alloc_size,
            has_sizes: true,
            clone_id: None,
            private_size: None,
        });
    }
    Ok(out)
}

/// Physical `(device offset, length)` ranges of a file's data, from
/// `F_LOG2PHYS_EXT`. Holes are skipped. `None` when the file cannot be read.
#[cfg(target_os = "macos")]
fn physical_extents(path: &Path) -> Option<Vec<(u64, u64)>> {
    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    let fd = unsafe {
        // SAFETY: `c` is a NUL-terminated path.
        libc::open(
            c.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return None;
    }
    let fd = DirFd(fd);
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: `fd` is open; `st` is a valid out-param.
    if unsafe { libc::fstat(fd.0, &mut st) } != 0 {
        return None;
    }
    let size = u64::try_from(st.st_size).ok()?;
    let mut out = Vec::new();
    let mut off = 0u64;
    // A file with more extents than this is counted at its full allocation.
    for _ in 0..65_536 {
        if off >= size {
            return Some(out);
        }
        let mut l2p = libc::log2phys {
            l2p_flags: 0,
            l2p_contigbytes: i64::try_from(size - off).ok()?,
            l2p_devoffset: i64::try_from(off).ok()?,
        };
        // SAFETY: `fd` is open; `l2p` is a valid in/out struct for F_LOG2PHYS_EXT.
        if unsafe { libc::fcntl(fd.0, libc::F_LOG2PHYS_EXT, &mut l2p) } == -1 {
            return None;
        }
        let len = u64::try_from(l2p.l2p_contigbytes).ok().filter(|&n| n > 0)?;
        // A hole reports a negative device offset.
        if let Ok(dev_off) = u64::try_from(l2p.l2p_devoffset) {
            out.push((dev_off, len));
        }
        off = off.saturating_add(len);
    }
    None
}

#[cfg(not(target_os = "macos"))]
fn physical_extents(_path: &Path) -> Option<Vec<(u64, u64)>> {
    None
}

struct Subdir {
    path: PathBuf,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    dev: u64,
    ino: u64,
}

fn note_file(map: &mut BTreeMap<String, ExtSum>, name: &str, physical: u64) -> String {
    let key = ext_key(name);
    let entry = map.entry(key.clone()).or_insert(ExtSum {
        physical: 0,
        files: 0,
    });
    entry.physical = entry.physical.saturating_add(physical);
    entry.files = entry.files.saturating_add(1);
    key
}

fn leaf_from_bulk(
    path: PathBuf,
    ent: &BulkEnt,
    volume_total: u64,
    clones: &CloneBook,
    by_ext: &mut BTreeMap<String, ExtSum>,
    errors: &AtomicU64,
) -> Node {
    let name = ent.name.to_string_lossy().into_owned();
    let symlink = ent.objtype == VLNK;
    let (logical, mut size, clone_id) = if ent.has_sizes && !symlink {
        (ent.data_length, ent.alloc_size, ent.clone_id)
    } else if let Ok(meta) = fs::symlink_metadata(&path) {
        let clone = if meta.file_type().is_symlink() {
            None
        } else {
            apfs_clone_id(&path)
        };
        (meta.len(), allocated_bytes(&meta), clone)
    } else {
        errors.fetch_add(1, Ordering::Relaxed);
        (0, 0, None)
    };
    if !symlink {
        if clone_id.is_some_and(|id| !clones.first_of_id(id)) {
            size = 0;
        } else if ent.private_size.is_some_and(|private| private < size) {
            size = clones.unshared(&path, ent.dev, size);
        }
    }
    let color_ext = note_file(by_ext, &name, size);
    Node {
        name,
        path,
        is_dir: false,
        size,
        logical,
        files: 1,
        color_ext,
        modified: ent.modified,
        created: ent.created,
        percent_of_disk: percent_of_disk(size, volume_total),
        children: Vec::new(),
    }
}

fn insert_seen(seen: &Mutex<HashSet<(u64, u64)>>, dev: u64, ino: u64) -> bool {
    seen.lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert((dev, ino))
}

fn walk_dir(
    path: &Path,
    meta: &Metadata,
    volume_total: u64,
    scan_root: &Path,
    seen: &Mutex<HashSet<(u64, u64)>>,
    clones: &CloneBook,
    errors: &AtomicU64,
) -> io::Result<(Node, BTreeMap<String, ExtSum>)> {
    walk_dir_known(
        path,
        meta.modified().ok(),
        meta.created().ok(),
        dev_id(meta),
        meta.ino(),
        volume_total,
        scan_root,
        seen,
        clones,
        errors,
    )
}

#[allow(clippy::too_many_arguments)]
fn walk_dir_known(
    path: &Path,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    dev: u64,
    ino: u64,
    volume_total: u64,
    scan_root: &Path,
    seen: &Mutex<HashSet<(u64, u64)>>,
    clones: &CloneBook,
    errors: &AtomicU64,
) -> io::Result<(Node, BTreeMap<String, ExtSum>)> {
    let _ = insert_seen(seen, dev, ino);
    let mut children = Vec::new();
    let mut by_ext: BTreeMap<String, ExtSum> = BTreeMap::new();
    let mut subdirs = Vec::new();
    let entries = match list_bulk(path, errors) {
        Ok(entries) => entries,
        Err(err) if path == scan_root => return Err(err),
        Err(_) => {
            errors.fetch_add(1, Ordering::Relaxed);
            Vec::new()
        }
    };
    for ent in entries {
        let child = path.join(&ent.name);
        if is_hidden_data_mount(scan_root, &child) {
            continue;
        }
        if ent.objtype == VDIR {
            if !insert_seen(seen, ent.dev, ent.ino) {
                continue;
            }
            subdirs.push(Subdir {
                path: child,
                modified: ent.modified,
                created: ent.created,
                dev: ent.dev,
                ino: ent.ino,
            });
        } else {
            children.push(leaf_from_bulk(
                child,
                &ent,
                volume_total,
                clones,
                &mut by_ext,
                errors,
            ));
        }
    }
    let nested: Vec<(Node, BTreeMap<String, ExtSum>)> = subdirs
        .into_par_iter()
        .map(|sub| {
            walk_dir_known(
                &sub.path,
                sub.modified,
                sub.created,
                sub.dev,
                sub.ino,
                volume_total,
                scan_root,
                seen,
                clones,
                errors,
            )
        })
        .collect::<io::Result<_>>()?;
    for (node, totals) in nested {
        merge_ext(&mut by_ext, totals);
        children.push(node);
    }
    let size: u64 = children.iter().map(|c| c.size).sum();
    let logical = children
        .iter()
        .fold(0u64, |acc, c| acc.saturating_add(c.logical));
    let files = children
        .iter()
        .fold(0u64, |acc, c| acc.saturating_add(c.files));
    Ok((
        Node {
            name: node_name(path),
            path: path.to_path_buf(),
            is_dir: true,
            size,
            logical,
            files,
            color_ext: dominant_ext(&by_ext),
            modified,
            created,
            percent_of_disk: percent_of_disk(size, volume_total),
            children,
        },
        by_ext,
    ))
}

fn merge_ext(into: &mut BTreeMap<String, ExtSum>, from: BTreeMap<String, ExtSum>) {
    for (key, sum) in from {
        let entry = into.entry(key).or_insert(ExtSum {
            physical: 0,
            files: 0,
        });
        entry.physical = entry.physical.saturating_add(sum.physical);
        entry.files = entry.files.saturating_add(sum.files);
    }
}

fn dominant_ext(totals: &BTreeMap<String, ExtSum>) -> String {
    let mut best: Option<(&str, u64, u64)> = None;
    for (key, sum) in totals {
        let take = match best {
            None => true,
            Some((best_key, physical, files)) => {
                (sum.physical, sum.files, std::cmp::Reverse(key.as_str()))
                    > (physical, files, std::cmp::Reverse(best_key))
            }
        };
        if take {
            best = Some((key.as_str(), sum.physical, sum.files));
        }
    }
    best.map(|(key, _, _)| key.to_string()).unwrap_or_default()
}
