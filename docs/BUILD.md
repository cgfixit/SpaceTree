# Building SpaceTree on macOS

## Prerequisites

- macOS 12.0+
- Rust (`rustc` and `cargo`). `Cargo.toml` records `rust-version` 1.85.0. The pinned eframe and rfd crates need a newer compiler. Homebrew rustc 1.98 builds this tree. A rustup toolchain stuck at 1.85.0 does not.
- Xcode Command Line Tools (`xcode-select --install`) for linking and `codesign`

## CLI / tests

From the repo root:

```bash
cargo test
cargo run -- --scan /path/to/folder
```

## Package `SpaceTree.app`

```bash
chmod +x scripts/make-app.sh
./scripts/make-app.sh
open dist/SpaceTree.app
```

The script builds a release binary for the host architecture (and optionally the other Apple arch if the target/SDK is available), copies `Info.plist`, and ad-hoc codesigns the bundle. Output: `dist/SpaceTree.app`.

## Gatekeeper

The app is **not** signed with an Apple Developer ID. On first open, macOS may block it:

1. Finder → right-click `SpaceTree.app` → **Open**
2. Confirm in the dialog

Or: System Settings → Privacy & Security → allow the blocked app, then open again.
