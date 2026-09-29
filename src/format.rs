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

fn unix_secs(t: Option<SystemTime>) -> i64 {
    t.and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
