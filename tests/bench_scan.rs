//! Order-independent digest of a real scan. Ignored in `cargo test`.
//! `cargo test --release --test bench_scan -- --ignored --nocapture`

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::{Instant, UNIX_EPOCH};

use spacetree::{legend_of, scan, sort_tree, Node, SortColumn};

fn node_hash(node: &Node) -> u64 {
    let mut h = DefaultHasher::new();
    node.path.hash(&mut h);
    node.size.hash(&mut h);
    node.logical.hash(&mut h);
    node.files.hash(&mut h);
    node.is_dir.hash(&mut h);
    node.color_ext.hash(&mut h);
    secs(node.modified).hash(&mut h);
    secs(node.created).hash(&mut h);
    h.finish()
}

fn secs(t: Option<std::time::SystemTime>) -> i64 {
    t.and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn digest(node: &Node) -> (u64, u64) {
    let mut xor = 0u64;
    let mut count = 0u64;
    fn walk(n: &Node, xor: &mut u64, count: &mut u64) {
        *xor ^= node_hash(n);
        *count += 1;
        for c in &n.children {
            walk(c, xor, count);
        }
    }
    walk(node, &mut xor, &mut count);
    (xor, count)
}

#[test]
#[ignore]
fn digest_home() {
    let started = Instant::now();
    let result = scan(Path::new("/Users/cg")).unwrap();
    let ms = started.elapsed().as_millis();
    let (xor, count) = digest(&result.root);
    eprintln!(
        "home ms={ms} size={} logical={} files={} nodes={count} xor={xor:016x}",
        result.root.size, result.root.logical, result.root.files
    );
    let t = Instant::now();
    let legend = legend_of(&result.root);
    eprintln!(
        "legend ms={} rows={}",
        t.elapsed().as_millis(),
        legend.len()
    );
    let t = Instant::now();
    let mut owned = result;
    sort_tree(&mut owned.root, SortColumn::Size, true);
    eprintln!("sort ms={}", t.elapsed().as_millis());
}

#[test]
#[ignore]
fn digest_root() {
    let started = Instant::now();
    let result = scan(Path::new("/")).unwrap();
    let ms = started.elapsed().as_millis();
    let (xor, count) = digest(&result.root);
    eprintln!(
        "root ms={ms} size={} logical={} files={} nodes={count} xor={xor:016x}",
        result.root.size, result.root.logical, result.root.files
    );
}

#[test]
#[ignore]
fn digest_applications() {
    let started = Instant::now();
    let result = scan(Path::new("/Applications")).unwrap();
    let ms = started.elapsed().as_millis();
    let (xor, count) = digest(&result.root);
    eprintln!(
        "apps ms={ms} size={} logical={} files={} nodes={count} xor={xor:016x}",
        result.root.size, result.root.logical, result.root.files
    );
}
