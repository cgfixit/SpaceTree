use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::scan::{raw_available, volume_space};
use crate::{full_disk_access, DiskAccess, Node, ScanResult, HIDDEN_KEY};

/// Stable text dump of a scan for the `--scan` CLI and tests.
pub fn format_scan(result: &ScanResult) -> String {
    format_scan_to_depth(result, usize::MAX)
}

/// [`format_scan`] with rows no deeper than `max_depth` below the root, for
/// `--scan <path> --depth <n>`. The header and every total are unchanged.
pub fn format_scan_to_depth(result: &ScanResult, max_depth: usize) -> String {
    let mut out = String::new();
    out.push_str(&format!("volume_total_bytes={}\n", result.volume_total));
    out.push_str(&format!("root_size_bytes={}\n", result.root.size));
    out.push_str(&format!("volume_free_bytes={}\n", result.volume_free));
    if let Some(used) = result.volume_used {
        out.push_str(&format!("volume_used_bytes={used}\n"));
        out.push_str(&format!("hidden_bytes={}\n", result.hidden));
        out.push_str(&format!("hidden_outside_bytes={}\n", result.hidden_outside));
    }
    push_available(
        &mut out,
        result.volume_free,
        result.volume_available,
        result.volume_on_disk,
    );
    if result.error_count > 0 {
        out.push_str(&format!("scan_incomplete_errors={}\n", result.error_count));
    }
    push_node(&mut out, &result.root, 0, max_depth);
    out
}

/// The volume's on-disk bytes, then macOS's Available figure and the
/// purgeable part of it.
fn push_available(out: &mut String, free: u64, available: Option<u64>, on_disk: Option<u64>) {
    if let Some(on_disk) = on_disk {
        out.push_str(&format!("volume_on_disk_bytes={on_disk}\n"));
    }
    let Some(available) = available else {
        return;
    };
    out.push_str(&format!("volume_available_bytes={available}\n"));
    out.push_str(&format!(
        "volume_purgeable_bytes={}\n",
        available.saturating_sub(free)
    ));
}

/// Volume figures for `path` without walking it, for the `--volume` CLI: the
/// header lines `--scan` prints, then macOS's unfiltered Available answer for
/// each mount it was asked about.
pub fn format_volume(path: &Path) -> String {
    let volume = volume_space(path);
    let mut out = String::new();
    out.push_str(&format!("volume_total_bytes={}\n", volume.total));
    out.push_str(&format!("volume_free_bytes={}\n", volume.free));
    if let Some(used) = volume.shown_used() {
        out.push_str(&format!("volume_used_bytes={used}\n"));
    }
    push_available(&mut out, volume.free, volume.available, volume.root_used);
    match full_disk_access() {
        DiskAccess::Granted => out.push_str("full_disk_access=granted\n"),
        DiskAccess::Denied => out.push_str("full_disk_access=denied\n"),
        DiskAccess::Unknown => {}
    }
    for (mount, raw) in raw_available(path) {
        let raw = raw.map_or_else(|| "none".to_string(), |bytes| bytes.to_string());
        out.push_str(&format!(
            "macos_available_bytes[{}]={raw}\n",
            mount.display()
        ));
    }
    out
}

/// Lines for stderr after an incomplete scan: why folders were refused and
/// what, if anything, would open them. Shared by the CLI and its tests.
pub fn format_refusals(result: &ScanResult) -> String {
    let mut out = String::new();
    if result.blocked_count == 0 {
        return out;
    }
    out.push_str(&format!(
        "folders the system refused to open: {}\n",
        result.blocked_count
    ));
    for blocked in &result.blocked {
        out.push_str(&format!(
            "  {}  ({})\n",
            blocked.path.display(),
            blocked.kind.label()
        ));
    }
    if result.blocked_count > result.blocked.len() as u64 {
        out.push_str("  ...\n");
    }
    for (count, what) in [
        (
            result.system_count,
            "owned by system accounts: only root reads them, as on every Mac",
        ),
        (
            result.protected_count,
            "protected by macOS although Full Disk Access is on: no setting opens them",
        ),
        (
            result.privacy_count,
            "refused by macOS privacy protection: Full Disk Access opens them",
        ),
        (
            result.permission_count(),
            "closed by file permissions or an ACL",
        ),
    ] {
        if count > 0 {
            out.push_str(&format!("{count} {what}\n"));
        }
    }
    if result.privacy_count > 0 && result.disk_access == DiskAccess::Denied {
        out.push_str(
            "Full Disk Access is not in effect for this process. For the app, turn it on in System Settings > Privacy & Security; if it is on already, remove SpaceTree from that list and add it again, because macOS treats each ad-hoc signed build as a new app. From Terminal, the grant that counts is Terminal's.\n",
        );
    }
    out
}

fn push_node(out: &mut String, node: &Node, depth: usize, max_depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
    let kind = if node.is_dir {
        "dir"
    } else if node.color_ext == HIDDEN_KEY {
        "hidden"
    } else {
        "file"
    };
    out.push_str(&format!(
        "{}  {:.12}  {}  {}  {}  {}\n",
        node.size,
        node.percent_of_disk,
        unix_secs(node.modified),
        unix_secs(node.created),
        kind,
        node.name
    ));
    if depth >= max_depth {
        return;
    }
    for child in &node.children {
        push_node(out, child, depth + 1, max_depth);
    }
}

/// Human size in decimal SI units (1 KB = 1000 bytes), the convention Finder,
/// Disk Utility, and About This Mac have used since Mac OS X 10.6. Values
/// under 10 keep two decimals; larger values keep one.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut unit = 0;
    let mut scale = 1u64;
    while unit < UNITS.len() - 1 && bytes / scale >= 1000 {
        scale *= 1000;
        unit += 1;
    }
    let mut value = bytes as f64 / scale as f64;
    // Rounding can carry 999.96 up to 1000.0; show the next unit instead.
    if (value * 10.0).round() >= 10_000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if (value * 100.0).round() < 1000.0 {
        format!("{value:.2} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn unix_secs(t: Option<SystemTime>) -> i64 {
    t.and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
