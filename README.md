# SpaceTree

SpaceTree is a macOS disk usage app in the spirit of WinDirStat: pick a folder or a whole disk, and see at a glance what is eating your space. A sortable table and a treemap show the same allocated bytes, so the biggest offenders stand out in seconds.

**Use it when** your disk is nearly full and you need to know which folder, file type, or forgotten download is responsible, before you decide what to remove. SpaceTree is read-only: it never deletes anything, and scans stay on your machine (no uploads, no telemetry).

![SpaceTree showing a synthetic folder: table, extension legend, and treemap](screenshots/treemap-table-folder-selection.png)

More captures are in [`screenshots/`](screenshots/). They use a synthetic tree, not real files.

## The window

- **Table.** Columns are Name, Size Proportion, Percentage, Physical Size, Logical Size, and Files; click a header to sort. Percentage is a row's allocated bytes divided by the scan root. Hover a size for the exact byte count.
- **Treemap.** One rectangle per file. Area is allocated size and color is the file extension. Folders are outlined, and large folders and files carry a name and size. Hover a tile for its size, kind, and share of the view and of the scan.
- **Legend.** The extension colors in the scan, each with its size and share. Hover a row to highlight that extension in the map.
- **Navigation.** A single click selects, and selecting a folder in the table outlines it in the map. Double-click a folder or its label to zoom in; double-click a file to reveal it in Finder. The bar above the map shows where you are: click any part of it to jump back, or press Esc or Zoom out to go up one level.
- **Context menu.** Right-click a table row, tile, or folder label to reveal it in Finder or copy its path. Finder receives the original path, including names with spaces or non-UTF-8 bytes.

## Scanning a whole disk

Scanning `/` or another local macOS volume root adds what a folder scan does not have:

- **Green figures.** The root row's Percentage cell is green and shows `~N% full`, and a green `Whole volume: ~N% free` line appears as soon as the scan starts. Folder scans show neither.
- **They match System Settings › Storage.** macOS counts purgeable space (local snapshots, caches, files iCloud also holds) as available although it is still on disk. SpaceTree asks macOS for that Available figure, so used is capacity minus available, and the green line says how much of it is purgeable. The cell's tooltip gives the on-disk figure `df` prints. A volume macOS reports no purgeable space for uses the `df` figures.
- **Hidden space.** One extra row holds used space the scan found in no file it could read: folders macOS did not let SpaceTree read, local snapshots, and system data outside the scanned folders. With it, the root row equals the used figure above. Purgeable files the scan did read still count at full size in their folders, so the row can understate what is hidden.
- **Blocked folders.** A scan that could not read everything keeps a warning in the status bar with the number of read errors and how many were folders the system refused to open; hover it for their paths. To read more, turn on Full Disk Access for SpaceTree in System Settings › Privacy & Security and scan again.
- **One volume only.** The scan does not enter other mounts (`/Volumes/*`, mounted disk images), also when the root is reached through a symlink.

## How sizes are measured

- **Physical Size** is allocated bytes (`st_blocks * 512`); **Logical Size** is `st_size`. A folder is the sum of its children. Units are decimal, as in Finder (1 GB = 1,000,000,000 bytes).
- **One system call per batch.** On macOS the scan lists each directory with `getattrlistbulk`, which returns names, sizes, dates, and APFS clone ids together, and walks subdirectories in parallel. On Linux, where the crate also builds and tests, it uses ordinary file metadata, without clone ids.
- **APFS clones count once.** A pure clone (`cp -c`, Finder Duplicate) counts as zero after the first file seen. A clone later edited in part counts only the blocks it no longer shares, read from its physical extents. `du` counts every clone in full.
- **No double counting.** On APFS a hard-linked file counts once. Child symlinks are not followed, and `/System/Volumes` is skipped unless it is the scan root, so the Data volume is not counted twice.
- **Errors do not stop a scan.** An unreadable entry is counted and the rest is reported. Only an unreadable root fails.

## Ollama models

Ollama stores each model layer once as `~/.ollama/models/blobs/sha256-<hash>`, so the folder is a list of hashes with no model names. SpaceTree reads the manifests beside it and names the blobs. It only reads: nothing is run or deleted.

- **In a scan** of the `blobs` folder or anything above it, each blob's row and tile shows the model and layer kind in front of the hash: `llama3:latest · model`, or `llama3:latest +1 · model` when two models share it. The tooltip lists every model that uses the blob. Blobs no model lists and unfinished downloads are labelled as such.
- **`--ollama`** prints a report without a scan: each model with its blobs and sizes, shared blobs, how much removing each model alone frees, unused blobs, unfinished downloads, models with a missing blob, and `ollama rm` commands sorted by space freed. Ollama itself removes unused blobs when its server starts, unless `OLLAMA_NOPRUNE` is set.

## Command line

```bash
cargo run -- --scan /path/to/folder   # text report of a scan
cargo run -- --volume /               # volume figures only, no scan
cargo run -- --ollama                 # $OLLAMA_MODELS, or ~/.ollama/models
cargo run -- --gui-scan /path         # open the app and scan
```

- `--scan` prints header lines, starting with `volume_total_bytes` and `root_size_bytes`, then one indented line per node. A whole-volume scan adds `volume_used_bytes`, `hidden_bytes`, `volume_on_disk_bytes`, and, when macOS reports purgeable space, `volume_available_bytes` and `volume_purgeable_bytes`. If entries could not be read it adds `scan_incomplete_errors`, lists the refused folders on stderr, and exits nonzero.
- `--volume` prints the same `volume_*` lines, then `macos_available_bytes[<mount>]`, the unfiltered answer from macOS. Use it to compare with System Settings. macOS refreshes these estimates on its own schedule, so expect close, not identical.

## Build and package

You need macOS 12 or newer, the Xcode Command Line Tools, and a current stable Rust. `Cargo.toml` records `rust-version` 1.85.0, but the pinned `eframe` 0.31.1 crates need a newer compiler.

```bash
./scripts/make-app.sh
open dist/SpaceTree.app
```

`scripts/make-app.sh` builds a release binary, builds `AppIcon.icns` from `assets/AppIcon.png`, and ad-hoc signs `dist/SpaceTree.app` (bundle id `com.cgfixit.spacetree`). The app is not signed with an Apple Developer ID: on first launch, right-click SpaceTree.app and choose Open, then confirm.

## Releases

Every merge to `main` whose checks all pass is published as a GitHub release by `.github/workflows/auto-release.yml`, tagged `v<version>-main.<date>.<sha>`. It starts when the bundle or test run for that commit finishes and waits up to 30 minutes for the remaining checks, so the release normally appears a few minutes after the merge. `release.yml` publishes a named `v<version>` release, and `release-bundle.yml` builds and publishes the app from any ref on demand.

## Comparing scan speed

Build two revisions with separate `CARGO_TARGET_DIR` values, then run:

```bash
python3 scripts/bench-scan.py /path/to/baseline/spacetree /path/to/candidate/spacetree
```

It times alternating CLI scans of a temporary 6,000-file tree and rejects differing totals or row counts.

## Where the code lives

- `src/scan.rs` walks the tree and reads sizes and volume figures.
- `src/layout.rs` places the treemap.
- `src/app.rs` draws the table, the map, and the legend.
- `src/format.rs` prints the `--scan` and `--volume` reports.
- `src/ollama.rs` maps Ollama blobs to models.

## License

MIT. See `LICENSE`.
