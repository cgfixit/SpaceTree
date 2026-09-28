//! Times the post-scan UI work. Not a pass/fail contract. Run with --nocapture.

use std::path::PathBuf;
use std::time::{Instant, SystemTime};

use spacetree::{layout_node, Node, PxRect};

fn file(path: &str, size: u64) -> Node {
    Node {
        name: path.rsplit('/').next().unwrap_or(path).to_string(),
        path: PathBuf::from(path),
        is_dir: false,
        size,
        logical: size,
        files: 1,
        color_ext: "txt".to_string(),
        modified: None::<SystemTime>,
        created: None,
        percent_of_disk: 0.0,
        children: Vec::new(),
    }
}

fn dir(path: &str, children: Vec<Node>) -> Node {
    let size = children.iter().map(|c| c.size).sum();
    let logical = children.iter().map(|c| c.logical).sum();
    let files = children.iter().map(|c| c.files).sum();
    Node {
        name: path.rsplit('/').next().unwrap_or(path).to_string(),
        path: PathBuf::from(path),
        is_dir: true,
        size,
        logical,
        files,
        color_ext: "txt".to_string(),
        modified: None,
        created: None,
        percent_of_disk: 0.0,
        children,
    }
}

#[test]
fn time_layout_of_a_wide_directory() {
    let n = 20_000u64;
    let children: Vec<Node> = (0..n)
        .map(|i| file(&format!("/scan/f{i}.txt"), 1 + (i % 50)))
        .collect();
    let root = dir("/scan", children);
    let bounds = PxRect {
        x: 0,
        y: 0,
        w: 900,
        h: 400,
    };
    let started = Instant::now();
    let tiling = layout_node(&root, bounds);
    let elapsed = started.elapsed();
    let tiles = tiling.tiles().len();
    let area: u64 = tiling
        .tiles()
        .iter()
        .map(|t| u64::from(t.rect.w) * u64::from(t.rect.h))
        .sum();
    eprintln!(
        "wide n={n} tiles={tiles} area={area} bounds={} layout_ms={}",
        u64::from(bounds.w) * u64::from(bounds.h),
        elapsed.as_millis()
    );
    assert!(tiles <= 1024, "layout emitted {tiles} tiles for {n} files");
    assert_eq!(area, u64::from(bounds.w) * u64::from(bounds.h));
    assert!(
        elapsed.as_millis() < 500,
        "layout took {} ms",
        elapsed.as_millis()
    );
}
