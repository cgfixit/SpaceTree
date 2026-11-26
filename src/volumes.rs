//! Mounted volumes a user can pick to scan: the startup disk, other local
//! disks (APFS, Mac OS Extended, ExFAT, FAT, NTFS, ...), and network shares
//! (SMB, AFP, NFS, WebDAV). No egui dependency.

use std::path::{Path, PathBuf};

use crate::format_bytes;

/// Where a mounted volume lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum VolumeKind {
    /// The startup disk, mounted at `/`.
    Startup,
    /// Another disk attached to this machine: internal, USB, Thunderbolt, or
    /// a mounted disk image. Its root scans as a whole volume.
    Local,
    /// A share on another machine. Its root scans like a folder: the
    /// server's totals are not this share's.
    Network,
}

/// One volume from the system's mount table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MountedVolume {
    pub path: PathBuf,
    /// The file system type the kernel reports (`apfs`, `exfat`, `smbfs`, ...).
    pub fs_type: String,
    pub kind: VolumeKind,
    pub read_only: bool,
    /// Capacity and bytes available to this user, as the mount table last
    /// recorded them; a network share is not asked again.
    pub total: u64,
    pub free: u64,
}

impl MountedVolume {
    /// Name to show: the mount point's last component, or `/` for the
    /// startup disk.
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    /// File system as Disk Utility names it.
    pub fn fs_label(&self) -> &str {
        fs_label(&self.fs_type)
    }
}

/// Disk Utility's name for a file system type, or the type itself.
pub fn fs_label(fs_type: &str) -> &str {
    match fs_type {
        "apfs" => "APFS",
        "hfs" => "Mac OS Extended",
        "exfat" => "ExFAT",
        "msdos" | "vfat" | "fat" => "MS-DOS (FAT)",
        "ntfs" | "ntfs3" | "ufsd_NTFS" | "tuxera_ntfs" => "NTFS",
        "smbfs" | "cifs" | "smb3" => "SMB",
        "afpfs" => "AFP",
        "nfs" | "nfs4" => "NFS",
        "webdav" => "WebDAV",
        "cd9660" | "iso9660" => "ISO 9660",
        "udf" => "UDF",
        "ext4" => "ext4",
        "btrfs" => "Btrfs",
        "xfs" => "XFS",
        other => other,
    }
}

/// File system types that are another machine's storage.
fn is_network_type(fs_type: &str) -> bool {
    matches!(
        fs_type,
        "smbfs" | "cifs" | "smb3" | "afpfs" | "nfs" | "nfs4" | "webdav" | "fuse.sshfs" | "9p"
    )
}

/// Kernel bookkeeping mounts that hold no user files.
fn is_pseudo_type(fs_type: &str) -> bool {
    matches!(
        fs_type,
        "devfs"
            | "autofs"
            | "fdesc"
            | "nullfs"
            | "proc"
            | "sysfs"
            | "tmpfs"
            | "devtmpfs"
            | "devpts"
            | "cgroup"
            | "cgroup2"
            | "securityfs"
            | "debugfs"
            | "tracefs"
            | "pstore"
            | "bpf"
            | "mqueue"
            | "hugetlbfs"
            | "configfs"
            | "fusectl"
            | "binfmt_misc"
            | "overlay"
            | "squashfs"
            | "nsfs"
            | "ramfs"
            | "efivarfs"
            | "rpc_pipefs"
    )
}

/// One mount table entry before it is filtered.
struct RawMount {
    path: PathBuf,
    fs_type: String,
    /// The kernel marks this a local file system (`MNT_LOCAL`); `None` where
    /// the mount table has no such flag.
    local: Option<bool>,
    /// Hidden from Finder (`MNT_DONTBROWSE`): the boot volume group's
    /// System/Volumes mounts, autofs maps, Time Machine snapshots.
    hidden: bool,
    read_only: bool,
    total: u64,
    free: u64,
}

/// Keep the mounts a person would choose to scan, startup disk first, then
/// local disks, then network shares, each by path.
fn pick(raw: Vec<RawMount>) -> Vec<MountedVolume> {
    let mut out: Vec<MountedVolume> = raw
        .into_iter()
        .filter_map(|m| {
            let startup = m.path == Path::new("/");
            if !startup && (m.hidden || is_pseudo_type(&m.fs_type) || !browsable(&m.path)) {
                return None;
            }
            let network = match m.local {
                Some(local) => !local,
                None => is_network_type(&m.fs_type),
            };
            let kind = if startup {
                VolumeKind::Startup
            } else if network {
                VolumeKind::Network
            } else {
                VolumeKind::Local
            };
            Some(MountedVolume {
                path: m.path,
                fs_type: m.fs_type,
                kind,
                read_only: m.read_only,
                total: m.total,
                free: m.free,
            })
        })
        .collect();
    out.sort_by(|a, b| (a.kind, &a.path).cmp(&(b.kind, &b.path)));
    out.dedup_by(|a, b| a.path == b.path);
    out
}

/// Mount points a person picks: macOS shows every browsable mount, Linux
/// keeps the usual places for removable disks and shares.
fn browsable(path: &Path) -> bool {
    if cfg!(target_os = "macos") {
        return true;
    }
    ["/media", "/mnt", "/run/media", "/Volumes", "/home"]
        .iter()
        .any(|base| path.starts_with(base))
}

/// The volumes currently mounted that can be scanned.
pub fn mounted_volumes() -> Vec<MountedVolume> {
    pick(raw_mounts())
}

#[cfg(target_os = "macos")]
fn raw_mounts() -> Vec<RawMount> {
    let mut list: *mut libc::statfs = std::ptr::null_mut();
    // SAFETY: getmntinfo fills `list` with a buffer it owns; read `n`
    // entries. MNT_NOWAIT returns cached figures without asking servers.
    let n = unsafe { libc::getmntinfo(&mut list, libc::MNT_NOWAIT) };
    if n <= 0 || list.is_null() {
        return Vec::new();
    }
    // SAFETY: `list` points at `n` initialized statfs records.
    let mounts = unsafe { std::slice::from_raw_parts(list, n as usize) };
    mounts
        .iter()
        .map(|s| {
            let flag = |bit: libc::c_int| s.f_flags & bit as u32 != 0;
            let fs_type: Vec<u8> = s
                .f_fstypename
                .iter()
                .take_while(|&&b| b != 0)
                .map(|&b| b as u8)
                .collect();
            let bsize = u64::from(s.f_bsize);
            RawMount {
                path: crate::scan::mount_path(s),
                fs_type: String::from_utf8_lossy(&fs_type).into_owned(),
                local: Some(flag(libc::MNT_LOCAL)),
                hidden: flag(libc::MNT_DONTBROWSE),
                read_only: flag(libc::MNT_RDONLY),
                total: s.f_blocks.saturating_mul(bsize),
                free: s.f_bavail.saturating_mul(bsize),
            }
        })
        .collect()
}

#[cfg(not(target_os = "macos"))]
fn raw_mounts() -> Vec<RawMount> {
    let Ok(text) = std::fs::read_to_string("/proc/self/mounts") else {
        return Vec::new();
    };
    parse_proc_mounts(&text)
        .into_iter()
        .map(|(path, fs_type, read_only)| {
            // Only the kept mounts are asked for their size.
            let (total, free) =
                if path == Path::new("/") || (!is_pseudo_type(&fs_type) && browsable(&path)) {
                    statvfs_space(&path)
                } else {
                    (0, 0)
                };
            RawMount {
                path,
                fs_type,
                local: None,
                hidden: false,
                read_only,
                total,
                free,
            }
        })
        .collect()
}

#[cfg(not(target_os = "macos"))]
fn statvfs_space(path: &Path) -> (u64, u64) {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return (0, 0);
    };
    // SAFETY: `s` is a valid out-param; `c` is NUL-terminated.
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return (0, 0);
    }
    let frsize = s.f_frsize as u64;
    (
        (s.f_blocks as u64).saturating_mul(frsize),
        (s.f_bavail as u64).saturating_mul(frsize),
    )
}

/// `(mount point, type, read-only)` from `/proc/self/mounts`, whose fields
/// escape space, tab, newline, and backslash as three octal digits.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn parse_proc_mounts(text: &str) -> Vec<(PathBuf, String, bool)> {
    use std::os::unix::ffi::OsStringExt;
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split(' ');
            let _source = fields.next()?;
            let path = fields.next()?;
            let fs_type = fields.next()?;
            let opts = fields.next()?;
            let path = PathBuf::from(std::ffi::OsString::from_vec(unescape_octal(path)));
            let read_only = opts.split(',').any(|o| o == "ro");
            Some((path, fs_type.to_string(), read_only))
        })
        .collect()
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
fn unescape_octal(field: &str) -> Vec<u8> {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let octal = bytes
            .get(i + 1..i + 4)
            .filter(|d| bytes[i] == b'\\' && d.iter().all(|b| (b'0'..=b'7').contains(b)))
            .and_then(|d| u8::from_str_radix(std::str::from_utf8(d).ok()?, 8).ok());
        match octal {
            Some(b) => {
                out.push(b);
                i += 4;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    out
}

/// What scanning a volume's root gives, in one line.
pub fn scan_note(kind: VolumeKind) -> &'static str {
    match kind {
        VolumeKind::Startup | VolumeKind::Local => {
            "whole volume: fullness and Hidden space (macOS)"
        }
        VolumeKind::Network => "network share: scans like a folder, no Hidden space",
    }
}

/// The `--volumes` report: one line per volume.
pub fn format_volumes(volumes: &[MountedVolume]) -> String {
    let mut out = String::new();
    for v in volumes {
        let kind = match v.kind {
            VolumeKind::Startup => "startup",
            VolumeKind::Local => "local",
            VolumeKind::Network => "network",
        };
        out.push_str(&format!(
            "{kind:<8} {:<16} {:>10} free of {:>10}{}  {}\n",
            v.fs_label(),
            format_bytes(v.free),
            format_bytes(v.total),
            if v.read_only { "  read-only" } else { "" },
            v.path.display()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(path: &str, fs_type: &str, local: Option<bool>, hidden: bool) -> RawMount {
        RawMount {
            path: PathBuf::from(path),
            fs_type: fs_type.into(),
            local,
            hidden,
            read_only: false,
            total: 100,
            free: 40,
        }
    }

    #[test]
    fn macos_mount_table_keeps_disks_and_shares_and_drops_system_mounts() {
        let picked = pick(vec![
            raw("/Volumes/Share", "smbfs", Some(false), false),
            raw("/", "apfs", Some(true), false),
            raw("/dev", "devfs", Some(true), true),
            raw("/System/Volumes/Data", "apfs", Some(true), true),
            raw("/System/Volumes/Data/home", "autofs", Some(false), true),
            raw("/Volumes/USB", "exfat", Some(true), false),
            raw("/Volumes/Old Mac", "hfs", Some(true), false),
            raw("/Volumes/NAS", "afpfs", Some(false), false),
        ]);
        let got: Vec<(&str, VolumeKind)> = picked
            .iter()
            .map(|v| (v.path.to_str().unwrap(), v.kind))
            .collect();
        // /Volumes is a browsable place on Linux too, so both agree.
        let want = vec![
            ("/", VolumeKind::Startup),
            ("/Volumes/Old Mac", VolumeKind::Local),
            ("/Volumes/USB", VolumeKind::Local),
            ("/Volumes/NAS", VolumeKind::Network),
            ("/Volumes/Share", VolumeKind::Network),
        ];
        assert_eq!(got, want);
        assert_eq!(picked[2].fs_label(), "ExFAT");
        assert_eq!(picked[4].fs_label(), "SMB");
        assert_eq!(picked[1].name(), "Old Mac");
        assert_eq!(picked[0].name(), "/");
    }

    #[test]
    fn linux_mount_table_is_parsed_and_unescaped() {
        let text = "\
/dev/sda1 / ext4 rw,relatime 0 0
proc /proc proc rw,nosuid 0 0
/dev/sdb1 /media/me/MY\\040USB exfat ro,nosuid 0 0
//nas/share /mnt/share cifs rw 0 0
tmpfs /run tmpfs rw 0 0
/dev/sdc1 /srv/data xfs rw 0 0
";
        let parsed = parse_proc_mounts(text);
        assert_eq!(parsed[2].0, PathBuf::from("/media/me/MY USB"));
        assert!(parsed[2].2, "ro");
        let picked = pick(
            parsed
                .into_iter()
                .map(|(path, fs_type, read_only)| RawMount {
                    path,
                    fs_type,
                    local: None,
                    hidden: false,
                    read_only,
                    total: 1,
                    free: 1,
                })
                .collect(),
        );
        let got: Vec<(&str, VolumeKind, &str)> = picked
            .iter()
            .map(|v| (v.path.to_str().unwrap(), v.kind, v.fs_label()))
            .collect();
        if !cfg!(target_os = "macos") {
            assert_eq!(
                got,
                vec![
                    ("/", VolumeKind::Startup, "ext4"),
                    ("/media/me/MY USB", VolumeKind::Local, "ExFAT"),
                    ("/mnt/share", VolumeKind::Network, "SMB"),
                ]
            );
        }
        assert!(picked[1].read_only);
    }

    #[test]
    fn unescape_leaves_lone_backslashes_and_short_escapes() {
        assert_eq!(unescape_octal("a\\040b"), b"a b");
        assert_eq!(unescape_octal("a\\134b"), b"a\\b");
        assert_eq!(unescape_octal("a\\9b\\04"), b"a\\9b\\04");
    }
}
