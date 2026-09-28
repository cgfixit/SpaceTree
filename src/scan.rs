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
use std::sync::Mutex;
use std::time::SystemTime;
#[cfg(target_os = "macos")]
use std::time::{Duration, UNIX_EPOCH};

use rayon::prelude::*;

use crate::ext::ext_key;

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

struct ExtSum {
    physical: u64,
    files: u64,
}

/// Result of walking a path on this machine.
#[derive(Clone, Debug)]
pub struct ScanResult {
    pub root: Node,
    pub volume_total: u64,
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
    let clones = Mutex::new(HashSet::new());
    let root = if lmeta.file_type().is_dir() {
        walk_dir(path, &lmeta, volume_total, path, &seen, &clones).0
    } else if lmeta.file_type().is_symlink() {
        let followed = fs::metadata(path)?;
        if followed.is_dir() {
            walk_dir(path, &followed, volume_total, path, &seen, &clones).0
        } else {
            leaf(path, &lmeta, volume_total, &clones).0
        }
    } else {
        leaf(path, &lmeta, volume_total, &clones).0
    };
    Ok(ScanResult { root, volume_total })
}

pub(crate) fn percent_of_disk(size: u64, volume_total: u64) -> f64 {
    if volume_total == 0 {
        0.0
    } else {
        (size as f64) / (volume_total as f64) * 100.0
    }
}

fn volume_total_bytes(path: &Path) -> u64 {
    let c = match std::ffi::CString::new(path.as_os_str().as_bytes()) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    unsafe {
        // SAFETY: `c` is a NUL-terminated path; `s` is a valid statvfs out-param.
        let mut s: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut s) != 0 {
            return 0;
        }
        (s.f_blocks as u64).saturating_mul(s.f_frsize as u64)
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
    clones: &Mutex<HashSet<u64>>,
) -> (Node, BTreeMap<String, ExtSum>) {
    let mut size = allocated_bytes(meta);
    if !meta.file_type().is_symlink() {
        if let Some(id) = apfs_clone_id(path) {
            if !clones.lock().unwrap_or_else(|e| e.into_inner()).insert(id) {
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
