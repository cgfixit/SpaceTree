# SpaceTree

SpaceTree is a macOS disk usage app. You pick a folder. The window lists each child and draws a treemap of the same allocated bytes.

The window has three parts.

- The table columns are Name, Size Proportion, Percentage, Physical Size, Logical Size, and Files. Percentage is that row's allocated bytes divided by the scan root. The scan root is 100%.
- The treemap draws one rectangle per file. Area is allocated size. Color is the file extension. A folder is a frame around its children. Double-click a folder to zoom in. Double-click a file to reveal it in Finder. Zoom out walks back to the parent, then to the scan root. A single click selects a row and does not zoom.
- The legend lists the extension colors in the current scan.

Physical Size is allocated bytes (`st_blocks * 512`). Logical Size is `st_size`. A directory's size is the sum of its children. An APFS clone keeps its allocated bytes on the first file seen and counts as zero after that. The walk does not follow a child symlink. It skips `/System/Volumes` unless that path is the scan root, so the Data volume is not counted twice.

## Requirements

- macOS 12 or newer, for the app bundle.
- Rust. `Cargo.toml` records `rust-version` 1.85.0. The pinned `eframe` 0.31.1 crates need a newer compiler. Homebrew rustc 1.98 builds this tree.
- Xcode Command Line Tools, for linking and `codesign`.

`cargo test` also builds on Linux. That build reads directories with ordinary file metadata. It does not read APFS clone ids.

## Build and run

From the repo root:

```bash
cargo test
cargo run -- --scan /path/to/folder
```

`--scan` prints two header lines, `volume_total_bytes` and `root_size_bytes`, then one indented line per node.

Package the app:

```bash
./scripts/make-app.sh
open dist/SpaceTree.app
```

`scripts/make-app.sh` builds a release binary, copies `Info.plist`, and ad-hoc signs `dist/SpaceTree.app`. The bundle id is `com.cgfixit.spacetree`, version 0.1.0. The app is not signed with an Apple Developer ID. On first launch, right-click SpaceTree.app and choose Open, then confirm.

Packaging notes are in `docs/BUILD.md`.

## Continuous integration

GitHub Actions runs on each push and pull request.

- `.github/workflows/ci.yml` runs `cargo test --locked --all-targets` on macOS and Ubuntu, then boots `spacetree --help` and `spacetree --scan` on a temporary folder.
- `.github/workflows/gitleaks.yml` scans git history with the gitleaks binary.
- `.github/workflows/cargo-deny.yml` checks dependency licenses, advisories, and crate sources.
- `.github/workflows/lint.yml` checks formatting and the workflow files.

## Where the code lives

- `src/scan.rs` walks the tree and records allocated size, logical size, file count, and extension.
- `src/layout.rs` places the treemap.
- `src/app.rs` draws the table, the map, and the legend.
- `src/format.rs` prints the `--scan` report.

## License

MIT. See `LICENSE`.
