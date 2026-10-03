use std::time::{SystemTime, UNIX_EPOCH};

use crate::{Node, ScanResult};

/// Stable text dump of a scan for the `--scan` CLI and tests.
pub fn format_scan(result: &ScanResult) -> String {
    let mut out = String::new();
    out.push_str(&format!("volume_total_bytes={}\n", result.volume_total));
    out.push_str(&format!("root_size_bytes={}\n", result.root.size));
    if result.error_count > 0 {
        out.push_str(&format!("scan_incomplete_errors={}\n", result.error_count));
    }
    push_node(&mut out, &result.root, 0);
    out
}

fn push_node(out: &mut String, node: &Node, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
    let kind = if node.is_dir { "dir" } else { "file" };
    out.push_str(&format!(
        "{}  {:.12}  {}  {}  {}  {}\n",
        node.size,
        node.percent_of_disk,
        unix_secs(node.modified),
        unix_secs(node.created),
        kind,
        node.name
    ));
    for child in &node.children {
        push_node(out, child, depth + 1);
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
