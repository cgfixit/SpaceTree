//! Reveal a path in Finder. Argv only — never interpolate into a shell.

use std::path::Path;
use std::process::Command;

/// `open -R -- <path>` so a path that starts with `-` is not an `open` flag.
pub fn finder_open_args(path: &Path) -> Vec<String> {
    vec![
        "-R".into(),
        "--".into(),
        path.as_os_str().to_string_lossy().into_owned(),
    ]
}

pub fn reveal_in_finder(path: &Path) -> Result<(), String> {
    let args = finder_open_args(path);
    let status = Command::new("open")
        .args(&args)
        .status()
        .map_err(|e| format!("open: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("open exited {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::finder_open_args;
    use std::path::Path;

    #[test]
    fn args_are_reveal_then_dashdash_then_path() {
        let a = finder_open_args(Path::new("/Users/cg"));
        assert_eq!(a, ["-R", "--", "/Users/cg"]);
    }

    #[test]
    fn leading_dash_path_stays_after_dashdash() {
        let a = finder_open_args(Path::new("-sneaky"));
        assert_eq!(a[0], "-R");
        assert_eq!(a[1], "--");
        assert_eq!(a[2], "-sneaky");
    }
}
