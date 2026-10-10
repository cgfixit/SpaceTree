# SpaceTree

SpaceTree is a macOS disk usage app. You pick a folder or a disk. The window lists each child and draws a treemap of the same allocated bytes.

![CG Agent Harness running on macOS](/screenshots/treemap-table-folder-selection.png)

- The table columns are Name, Size Proportion, Percentage, Physical Size, Logical Size, and Files. Percentage is that row's allocated bytes divided by the scan root. The scan root is 100%, except when scanning `/` or another local macOS volume root: only its Percentage cell is green and shows `~N% full`, based on volume capacity minus available space. Child rows and folder scans keep their scan-share percentages.
- The treemap draws one rectangle per file. Area is allocated size. Color is the file extension. Each folder is outlined around its files; large folders carry a name and size label, and large files show their name and size. Hover a tile for its size, kind, and share of the view and of the scan. Double-click a folder (or its label) to zoom in. Double-click a file to reveal it in Finder. Right-click a table row, treemap tile, or folder label to reveal it in Finder or copy its path. The bar above the map shows the path you are in; click any part of it to jump back, or press Esc or Zoom out to go up one level. A single click selects a row and does not zoom; selecting a folder in the table outlines it in the map.
- The legend lists the extension colors in the current scan with each one's size and share. Hover a legend row to highlight that extension in the map.

Physical Size is allocated bytes (`st_blocks * 512`). Logical Size is `st_size`. Sizes use decimal units, as Finder does (1 GB = 1,000,000,000 bytes); hover a size for the exact byte count. The volume size is the capacity `statfs` reports, the same figure `df` shows. A directory's size is the sum of its children. APFS clones share their blocks, and the scan counts each shared block once: a pure clone (`cp -c`, Finder Duplicate) counts as zero after the first file seen, and a clone later edited in part counts only the blocks it no longer shares. `du` counts every clone in full. The walk does not follow a child symlink. It skips `/System/Volumes` unless that path is the scan root, so the Data volume is not counted twice.

## Requirements

- macOS 12 or newer, for the app bundle.
- Rust. `Cargo.toml` records `rust-version` 1.85.0. The pinned `eframe` 0.31.1 crates need a newer compiler. Homebrew rustc 1.98 builds this tree.
- Xcode Command Line Tools, for linking and `codesign`.

`cargo test` also builds on Linux. That build reads directories with ordinary file metadata. It does not read APFS clone ids or shared extents.

## Build and run

From the repo root:

```bash
cargo test
cargo run -- --scan /path/to/folder
```

`--scan` prints two header lines, `volume_total_bytes` and `root_size_bytes`, then one indented line per node. If some entries could not be read, the report adds `scan_incomplete_errors`, retains readable results, and exits nonzero with a warning on stderr. A root directory that cannot be read fails without a report. The app keeps an incomplete-scan warning visible alongside partial results.

Package the app:

```bash
./scripts/make-app.sh
open dist/SpaceTree.app
```

`scripts/make-app.sh` builds a release binary, copies `Info.plist`, builds `AppIcon.icns` from `assets/AppIcon.png`, and ad-hoc signs `dist/SpaceTree.app`. The icon source is `assets/icon.svg`; `node scripts/render-icon.cjs` re-renders the PNG (it needs Playwright). The bundle id is `com.cgfixit.spacetree`, version 0.1.0. The app is not signed with an Apple Developer ID. On first launch, right-click SpaceTree.app and choose Open, then confirm.


Every merge to `main` whose checks all pass is published as a GitHub release by `.github/workflows/auto-release.yml`, tagged `v<version>-main.<date>.<sha>`. `release.yml` still publishes a named `v<version>` release on demand.

## Repeatable scan comparison

Build two revisions with separate `CARGO_TARGET_DIR` values, then run:

```bash
python3 scripts/bench-scan.py /path/to/baseline/spacetree /path/to/candidate/spacetree
```

The script creates and removes a temporary tree of 6,000 files, warms each binary once, alternates seven timed CLI scans, and rejects differing root byte totals or row counts. The median includes process startup and report formatting. This local fixture does not exercise USB or network-volume behavior.

## Continuous Development (kinda; im sure df can be formatted and used with fancy flags to look all pretty but I mean unless im literally using linux without x why would I act like thats better. Research that later to make sure im not missing out on things that are better than the GUI habit I keep vibe coding from windows concepts (shoutout to WinDirStat):

- make default zoom and column widths closer to what I'd set it to myself
- See if theres a way to increase speed of scan without shortcuts
- Resiliency testing for non internal drive (usb/network share/etc ...)

## Where the code lives

- `src/scan.rs` walks the tree and records allocated size, logical size, file count, and extension.
- `src/layout.rs` places the treemap.
- `src/app.rs` draws the table, the map, and the legend.
- `src/format.rs` prints the `--scan` report.

## License

MIT. See `LICENSE`.
