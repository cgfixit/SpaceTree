//! Run the production Finder adapter against disposable files on a logged-in Mac.
//! rustc --edition=2021 scripts/verify-finder.rs -o /tmp/spacetree-finder-runtime
//! /tmp/spacetree-finder-runtime

#[path = "../src/finder.rs"]
mod finder;

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::ffi::OsStringExt;
    use std::process::Command;

    // Fixed AppleScript source; the selected path is data in argv, never code.
    const SELECTION: &str = r#"
on run argv
    set expectedPath to item 1 of argv
    with timeout of 15 seconds
        tell application "Finder"
            repeat 40 times
                set chosen to selection
                if (count of chosen) is 1 then
                    set chosenPath to POSIX path of ((item 1 of chosen) as alias)
                    if chosenPath is expectedPath or chosenPath is (expectedPath & "/") then return "selected"
                end if
                delay 0.1
            end repeat
        end tell
    end timeout
    error "Finder did not select the requested fixture"
end run
"#;

    struct Fixture(std::path::PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let root = std::env::temp_dir().join(format!(
        "spacetree-finder-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos(),
    ));
    fs::create_dir(&root)?;
    let fixture = Fixture(root.canonicalize()?);
    let folder = fixture.0.join("Folder with spaces");
    fs::create_dir(&folder)?;
    let file = folder.join("-report 'quoted' café.txt");
    fs::write(&file, b"synthetic Finder fixture\n")?;
    for path in [&file, &folder] {
        finder::reveal_in_finder(path)?;
        let output = Command::new("/usr/bin/osascript")
            .args(["-e", SELECTION])
            .arg(path)
            .output()?;
        if !output.status.success() {
            return Err(format!(
                "Finder selection verification failed: {}",
                String::from_utf8_lossy(&output.stderr).trim(),
            )
            .into());
        }
    }
    // AppleScript paths are text, so verify raw bytes at the adapter boundary
    // separately rather than claiming its selection query can round-trip them.
    let raw = fixture.0.join(OsString::from_vec(b"raw-\xff.txt".to_vec()));
    fs::write(&raw, b"raw filename fixture\n")?;
    assert_eq!(finder::finder_open_args(&raw)[2], raw.as_os_str());
    finder::reveal_in_finder(&raw)?;
    assert!(finder::reveal_in_finder(&fixture.0.join("missing-file")).is_err());
    println!("Finder runtime ok: file and folder selection; raw path argv; missing-path failure");
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("Finder runtime requires a logged-in macOS session; not verified on this OS");
    std::process::exit(1);
}
