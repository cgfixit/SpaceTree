//! Drives the shipped scan/sort/% path against a real on-disk tree.

use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static FIXTURE_SEQ: AtomicU64 = AtomicU64::new(0);

use spacetree::{format_scan, scan, sort_tree, Node, SortColumn};

struct Fixture {
    root: PathBuf,
    alpha: u64,
    beta: u64,
    gamma: u64,
}

impl Fixture {
    fn total(&self) -> u64 {
        self.alpha + self.beta + self.gamma
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_bytes(path: &Path, n: usize) {
    let mut f = File::create(path).unwrap();
    f.write_all(&vec![b'x'; n]).unwrap();
    f.sync_all().unwrap();
}

fn allocated(path: &Path) -> u64 {
    fs::symlink_metadata(path).unwrap().blocks() * 512
}

fn set_mtime(path: &Path, unix_secs: i64) {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    let tv = libc::timeval {
        tv_sec: unix_secs,
        tv_usec: 0,
    };
    let times = [tv, tv];
    let rc = unsafe { libc::utimes(c.as_ptr(), times.as_ptr()) };
    assert_eq!(rc, 0, "utimes {}", path.display());
}

fn created(path: &Path) -> SystemTime {
    fs::metadata(path)
        .ok()
        .and_then(|meta| meta.created().ok())
        .unwrap_or_else(SystemTime::now)
}

fn pause_until_clock_moves(prev: SystemTime) {
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(20));
        if SystemTime::now() > prev {
            return;
        }
    }
}

/// Nested tree: beta (50B, oldest created, oldest mtime), alpha (100B), nest/gamma (200B).
fn build_fixture() -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "spacetree-fix-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        FIXTURE_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();

    let beta = root.join("beta.txt");
    write_bytes(&beta, 4096);
    let t_beta = created(&beta);

    pause_until_clock_moves(t_beta);
    let alpha = root.join("alpha.txt");
    write_bytes(&alpha, 8192);
    let t_alpha = created(&alpha);
    assert!(
        t_alpha > t_beta,
        "alpha created {t_alpha:?} <= beta {t_beta:?}"
    );

    pause_until_clock_moves(t_alpha);
    let nest = root.join("nest");
    fs::create_dir(&nest).unwrap();
    let t_nest = created(&nest);
    assert!(
        t_nest > t_alpha,
        "nest created {t_nest:?} <= alpha {t_alpha:?}"
    );

    pause_until_clock_moves(t_nest);
    let gamma = nest.join("gamma.txt");
    write_bytes(&gamma, 16384);
    let t_gamma = created(&gamma);
    assert!(t_gamma > t_nest);

    // Distinct mtimes after birthtime so APFS does not clamp created().
    // beta oldest mtime, nest middle, alpha newest — opposite of created order.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    set_mtime(&beta, now + 10);
    set_mtime(&nest, now + 110);
    set_mtime(&alpha, now + 210);

    Fixture {
        root,
        alpha: allocated(&alpha),
        beta: allocated(&beta),
        gamma: allocated(&gamma),
    }
}

fn child<'a>(node: &'a Node, name: &str) -> &'a Node {
    node.children
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("missing child {name} in {:?}", names(node)))
}

fn names(node: &Node) -> Vec<&str> {
    node.children.iter().map(|c| c.name.as_str()).collect()
}

fn sibling_names(node: &Node) -> Vec<String> {
    node.children.iter().map(|c| c.name.clone()).collect()
}

#[test]
fn scan_file_sizes_match_bytes_written() {
    let fix = build_fixture();
    let result = scan(&fix.root).unwrap();
    assert_eq!(child(&result.root, "alpha.txt").size, fix.alpha);
    assert_eq!(child(&result.root, "beta.txt").size, fix.beta);
    let nest = child(&result.root, "nest");
    assert!(nest.is_dir);
    assert_eq!(child(nest, "gamma.txt").size, fix.gamma);
}

#[test]
fn logical_size_and_file_count_follow_the_leaves() {
    let fix = build_fixture();
    let result = scan(&fix.root).unwrap();
    let alpha = child(&result.root, "alpha.txt");
    assert_eq!(alpha.logical, 8192);
    assert_eq!(alpha.files, 1);
    assert_eq!(alpha.color_ext, "txt");
    let nest = child(&result.root, "nest");
    assert_eq!(nest.logical, 16384);
    assert_eq!(nest.files, 1);
    assert_eq!(nest.size, fix.gamma);
    assert_eq!(result.root.logical, 4096 + 8192 + 16384);
    assert_eq!(result.root.files, 3);
    assert_eq!(result.root.size, fix.total());
    let text = format_scan(&result);
    assert!(text.contains("root_size_bytes="));
    assert!(!text.contains("color_ext"));
    assert!(!text.contains("logical="));
    let alpha_line = text
        .lines()
        .find(|line| line.ends_with("alpha.txt"))
        .expect("alpha line");
    assert_eq!(alpha_line.split_whitespace().count(), 6);
}

#[test]
fn folder_size_is_sum_of_descendant_files() {
    let fix = build_fixture();
    let result = scan(&fix.root).unwrap();
    assert_eq!(child(&result.root, "nest").size, fix.gamma);
    assert_eq!(result.root.size, fix.total());
}

#[test]
fn expanding_folder_yields_child_files_and_folders() {
    let fix = build_fixture();
    let result = scan(&fix.root).unwrap();
    let mut top = sibling_names(&result.root);
    top.sort();
    assert_eq!(top, ["alpha.txt", "beta.txt", "nest"]);
    assert_eq!(sibling_names(child(&result.root, "nest")), ["gamma.txt"]);
}

#[test]
fn dates_are_populated_and_distinct() {
    let fix = build_fixture();
    let result = scan(&fix.root).unwrap();
    let alpha = child(&result.root, "alpha.txt");
    let beta = child(&result.root, "beta.txt");
    let nest = child(&result.root, "nest");
    if let (Some(alpha_c), Some(beta_c), Some(nest_c)) = (alpha.created, beta.created, nest.created)
    {
        assert!(alpha_c > beta_c);
        assert!(nest_c > alpha_c);
    }
    assert!(alpha.modified.unwrap() > nest.modified.unwrap());
    assert!(nest.modified.unwrap() > beta.modified.unwrap());
}

#[test]
fn percent_of_overall_disk_uses_scan_volume_total() {
    let fix = build_fixture();
    let result = scan(&fix.root).unwrap();
    assert_eq!(result.volume_used, None, "a folder is not a whole volume");
    assert!(
        result.volume_total > 0,
        "volume_total must be the live volume capacity"
    );
    let alpha = child(&result.root, "alpha.txt");
    let beta = child(&result.root, "beta.txt");
    let nest = child(&result.root, "nest");
    let expected = |size: u64| size as f64 / result.volume_total as f64 * 100.0;
    assert!((alpha.percent_of_disk - expected(fix.alpha)).abs() < 1e-12);
    assert!((beta.percent_of_disk - expected(fix.beta)).abs() < 1e-12);
    assert!((nest.percent_of_disk - expected(fix.gamma)).abs() < 1e-12);
    assert!((result.root.percent_of_disk - expected(fix.total())).abs() < 1e-12);
    assert!(alpha.percent_of_disk > 0.0);
    assert!(nest.percent_of_disk > alpha.percent_of_disk);
    assert!(alpha.percent_of_disk > beta.percent_of_disk);
}

#[test]
fn sort_by_each_column_changes_sibling_order() {
    let fix = build_fixture();
    let mut result = scan(&fix.root).unwrap();

    sort_tree(&mut result.root, SortColumn::Name, false);
    assert_eq!(
        sibling_names(&result.root),
        ["alpha.txt", "beta.txt", "nest"]
    );

    sort_tree(&mut result.root, SortColumn::Size, false);
    assert_eq!(
        sibling_names(&result.root),
        ["beta.txt", "alpha.txt", "nest"]
    );

    sort_tree(&mut result.root, SortColumn::Size, true);
    assert_eq!(
        sibling_names(&result.root),
        ["nest", "alpha.txt", "beta.txt"]
    );

    sort_tree(&mut result.root, SortColumn::PercentOfDisk, true);
    assert_eq!(
        sibling_names(&result.root),
        ["nest", "alpha.txt", "beta.txt"]
    );

    sort_tree(&mut result.root, SortColumn::Modified, false);
    assert_eq!(
        sibling_names(&result.root),
        ["beta.txt", "nest", "alpha.txt"]
    );

    sort_tree(&mut result.root, SortColumn::Created, false);
    assert_eq!(
        sibling_names(&result.root),
        ["beta.txt", "alpha.txt", "nest"]
    );
}

#[test]
fn format_scan_lists_names_and_root_total() {
    let fix = build_fixture();
    let result = scan(&fix.root).unwrap();
    let text = format_scan(&result);
    assert!(text.contains("volume_total_bytes="));
    assert!(text.contains(&format!("root_size_bytes={}", fix.total())));
    assert!(text.contains("alpha.txt"));
    assert!(text.contains("beta.txt"));
    assert!(text.contains("gamma.txt"));
    assert!(text.contains("nest"));
}

#[test]
fn does_not_follow_symlink_directories() {
    let fix = build_fixture();
    let loop_link = fix.root.join("loop");
    std::os::unix::fs::symlink(&fix.root, &loop_link).unwrap();
    let result = scan(&fix.root).unwrap();
    let loop_node = child(&result.root, "loop");
    assert!(
        !loop_node.is_dir,
        "symlink must not be walked as a directory"
    );
    assert!(loop_node.children.is_empty());
    let files_only = child(&result.root, "alpha.txt").size
        + child(&result.root, "beta.txt").size
        + child(&result.root, "nest").size;
    assert_eq!(files_only, fix.total());
    // Cycle would inflate size far above total + link length if followed.
    assert_eq!(result.root.size, fix.total() + loop_node.size);
}

#[test]
fn scan_root_follows_symlink_to_directory() {
    let fix = build_fixture();
    let link = std::env::temp_dir().join(format!(
        "spacetree-rootlink-{}-{}",
        std::process::id(),
        FIXTURE_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::os::unix::fs::symlink(&fix.root, &link).unwrap();
    struct LinkGuard(PathBuf);
    impl Drop for LinkGuard {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    let _g = LinkGuard(link.clone());

    let result = scan(&link).unwrap();
    assert!(
        result.root.is_dir,
        "symlink-to-dir scan root must be walked as a directory, not a {}-byte file",
        result.root.size
    );
    assert_eq!(result.root.size, fix.total());
    assert_eq!(child(&result.root, "alpha.txt").size, fix.alpha);
    assert_eq!(child(&result.root, "beta.txt").size, fix.beta);
    let nest = child(&result.root, "nest");
    assert!(nest.is_dir);
    assert_eq!(nest.size, fix.gamma);
    assert_eq!(child(nest, "gamma.txt").size, fix.gamma);

    let exe = env!("CARGO_BIN_EXE_spacetree");
    let out = std::process::Command::new(exe)
        .args(["--scan", link.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains(&format!("root_size_bytes={}", fix.total())),
        "{s}"
    );
    let root_line = s
        .lines()
        .find(|l| !l.starts_with("volume_") && !l.starts_with("root_size"))
        .unwrap_or("");
    assert!(
        root_line.contains("  dir  "),
        "scan root row must be a directory, got: {root_line:?}\n{s}"
    );
    assert!(
        !root_line.contains("  file  "),
        "scan root row must not be a file, got: {root_line:?}\n{s}"
    );
}

#[test]
fn binary_scan_prints_fixture_names_and_folder_total() {
    let fix = build_fixture();
    let exe = env!("CARGO_BIN_EXE_spacetree");
    let out = std::process::Command::new(exe)
        .args(["--scan", fix.root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains(&format!("root_size_bytes={}", fix.total())),
        "{s}"
    );
    assert!(s.contains("alpha.txt"), "{s}");
    assert!(s.contains("gamma.txt"), "{s}");
}

fn unique_dir() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "spacetree-fix-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        FIXTURE_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&p).unwrap();
    p
}

#[test]
fn scan_skips_system_volumes_duplicate_data_tree() {
    let root = unique_dir();
    let _g = Fixture {
        root: root.clone(),
        alpha: 0,
        beta: 0,
        gamma: 0,
    };
    let users = root.join("Users");
    fs::create_dir(&users).unwrap();
    let keep = users.join("keep.bin");
    write_bytes(&keep, 4096);
    let keep_sz = allocated(&keep);

    let data_users = root.join("System/Volumes/Data/Users");
    fs::create_dir_all(&data_users).unwrap();
    let dup = data_users.join("dup.bin");
    write_bytes(&dup, 8192);
    let dup_sz = allocated(&dup);
    assert!(dup_sz > 0);

    let result = scan(&root).unwrap();
    assert_eq!(
        result.root.size, keep_sz,
        "System/Volumes/Data duplicate must not be added; got {} want {keep_sz} (dup was {dup_sz})",
        result.root.size
    );
    assert_eq!(child(&result.root, "Users").size, keep_sz);
    if let Some(sys) = result.root.children.iter().find(|c| c.name == "System") {
        let names = sibling_names(sys);
        assert!(
            !names.iter().any(|n| n == "Volumes"),
            "System must not expose Volumes/Data: {names:?}"
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn apfs_clones_count_allocated_once() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let root = unique_dir();
    let _g = Fixture {
        root: root.clone(),
        alpha: 0,
        beta: 0,
        gamma: 0,
    };
    let a = root.join("a.bin");
    write_bytes(&a, 1024 * 1024);
    let b = root.join("b.bin");
    let src = CString::new(a.as_os_str().as_bytes()).unwrap();
    let dst = CString::new(b.as_os_str().as_bytes()).unwrap();
    let rc = unsafe { libc::clonefile(src.as_ptr(), dst.as_ptr(), 0) };
    assert_eq!(
        rc,
        0,
        "clonefile failed: {}",
        std::io::Error::last_os_error()
    );
    let one = allocated(&a);
    assert_eq!(allocated(&b), one);
    let result = scan(&root).unwrap();
    assert_eq!(
        result.root.size, one,
        "APFS clones must count unique allocated bytes once, got {} want {one}",
        result.root.size
    );
}

/// Clone `a` to `b` with clonefile(2), then rewrite `rewrite` bytes at the start of `b`.
#[cfg(target_os = "macos")]
fn clone_then_rewrite(a: &Path, b: &Path, rewrite: usize) {
    use std::ffi::CString;
    use std::io::{Seek, SeekFrom};
    use std::os::unix::ffi::OsStrExt;
    let src = CString::new(a.as_os_str().as_bytes()).unwrap();
    let dst = CString::new(b.as_os_str().as_bytes()).unwrap();
    let rc = unsafe { libc::clonefile(src.as_ptr(), dst.as_ptr(), 0) };
    assert_eq!(
        rc,
        0,
        "clonefile failed: {}",
        std::io::Error::last_os_error()
    );
    let mut f = fs::OpenOptions::new().write(true).open(b).unwrap();
    f.seek(SeekFrom::Start(0)).unwrap();
    f.write_all(&vec![b'y'; rewrite]).unwrap();
    f.sync_all().unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn partly_rewritten_clones_count_shared_blocks_once() {
    // A rewritten clone gets a new clone id but still shares the untouched
    // blocks. Counting both files in full would double those blocks.
    const SLACK: u64 = 64 * 1024; // APFS may rewrite more than the bytes written
    for rewrite in [4096usize, 4 * 1024 * 1024] {
        let root = unique_dir();
        let _g = Fixture {
            root: root.clone(),
            alpha: 0,
            beta: 0,
            gamma: 0,
        };
        fs::create_dir(root.join("orig")).unwrap();
        fs::create_dir(root.join("copy")).unwrap();
        let a = root.join("orig/a.bin");
        let b = root.join("copy/b.bin");
        let mut f = File::create(&a).unwrap();
        for i in 0..8u8 {
            f.write_all(&vec![i.wrapping_mul(37).wrapping_add(1); 1024 * 1024])
                .unwrap();
        }
        f.sync_all().unwrap();
        drop(f);
        clone_then_rewrite(&a, &b, rewrite);
        let one = allocated(&a);
        assert_eq!(allocated(&b), one, "a clone allocates like its source");
        let result = scan(&root).unwrap();
        let total = result.root.size;
        let rewritten = rewrite as u64;
        assert!(
            total >= one + rewritten && total <= one + rewritten + SLACK,
            "rewrite {rewrite}: total {total}, want one copy {one} + about {rewritten}"
        );
        let orig = child(&result.root, "orig").size;
        let copy = child(&result.root, "copy").size;
        assert_eq!(orig + copy, total);
        assert!(
            orig.min(copy) <= rewritten + SLACK,
            "rewrite {rewrite}: one folder keeps the shared blocks, the other only its own (orig {orig}, copy {copy})"
        );
        assert_eq!(result.error_count, 0);
    }
}

#[test]
fn scan_root_under_system_volumes_is_still_walked() {
    let root = unique_dir();
    let _g = Fixture {
        root: root.clone(),
        alpha: 0,
        beta: 0,
        gamma: 0,
    };
    let data = root.join("System/Volumes/Data");
    fs::create_dir_all(&data).unwrap();
    let only = data.join("only.bin");
    write_bytes(&only, 4096);
    let only_sz = allocated(&only);

    let result = scan(&data).unwrap();
    assert_eq!(result.root.size, only_sz);
    assert_eq!(child(&result.root, "only.bin").size, only_sz);
}

struct DeniedDirectory(PathBuf);

impl DeniedDirectory {
    fn new(path: &Path) -> Self {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o000)).unwrap();
        Self(path.to_path_buf())
    }
}

impl Drop for DeniedDirectory {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700)).unwrap();
    }
}

#[test]
fn denied_root_fails_instead_of_reporting_empty() {
    if unsafe { libc::geteuid() } == 0 {
        eprintln!("permission fixture requires an unprivileged user");
        return;
    }
    let fix = build_fixture();
    let _denied = DeniedDirectory::new(&fix.root);
    assert_eq!(
        scan(&fix.root).unwrap_err().kind(),
        std::io::ErrorKind::PermissionDenied
    );
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_spacetree"))
        .arg("--scan")
        .arg(&fix.root)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("scan failed"));
}

#[test]
fn denied_child_keeps_readable_siblings_and_reports_incomplete() {
    if unsafe { libc::geteuid() } == 0 {
        eprintln!("permission fixture requires an unprivileged user");
        return;
    }
    let fix = build_fixture();
    let _denied = DeniedDirectory::new(&fix.root.join("nest"));
    let result = scan(&fix.root).unwrap();
    assert_eq!(result.error_count, 1);
    assert_eq!(result.root.size, fix.alpha + fix.beta);
    assert_eq!(result.root.logical, 8192 + 4096);
    assert_eq!(result.root.files, 2);
    assert_eq!(child(&result.root, "alpha.txt").size, fix.alpha);
    assert!(child(&result.root, "nest").children.is_empty());
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_spacetree"))
        .arg("--scan")
        .arg(&fix.root)
        .output()
        .unwrap();
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("scan_incomplete_errors=1"));
    assert!(text.contains("alpha.txt"));
    assert!(String::from_utf8_lossy(&out.stderr).contains("scan incomplete: 1 read error"));
}

#[test]
fn empty_directory_is_complete() {
    let root = unique_dir();
    let _guard = Fixture {
        root: root.clone(),
        alpha: 0,
        beta: 0,
        gamma: 0,
    };
    let result = scan(&root).unwrap();
    assert_eq!(result.error_count, 0);
    assert_eq!(result.root.size, 0);
    assert!(result.root.children.is_empty());
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_spacetree"))
        .arg("--scan")
        .arg(&root)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(out.stderr.is_empty());
    assert!(!String::from_utf8_lossy(&out.stdout).contains("incomplete"));
}
