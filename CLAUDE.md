# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

`AGENTS.md` is the shared agent guide (product contracts, verification rules, repo skills). Read it first; this file adds commands and architecture and does not repeat it.

## Commands

```bash
cargo test --locked --all-targets              # what CI runs (macOS + Linux)
cargo test --test layout                       # one integration test file (tests/layout.rs)
cargo test --test scan some_test_name          # one test by name filter
cargo test --lib finder                        # unit tests inside src/ (e.g. src/finder.rs)
cargo fmt --check                              # CI only warns on drift, but keep it clean
cargo clippy --locked --all-targets -- -D warnings -A clippy::style -A clippy::complexity -A clippy::perf -A unused_imports   # exact CI lint
./scripts/ci-runtime.sh target/debug/spacetree # CLI smoke test on a temp tree (needs a prior debug build)
./scripts/verify-macos-sizes.sh target/debug/spacetree  # macOS only: sizes vs stat/du/df, clones (runs in CI on macOS)
cargo run -- --scan /path                      # text report; exits 1 if any read failed
cargo run -- --gui-scan /path                  # GUI that scans on launch (sets SPACETREE_AUTOSCAN)
./scripts/make-app.sh                          # macOS only: release build + AppIcon.icns -> ad-hoc signed dist/SpaceTree.app
NODE_PATH="$(npm root -g)" node scripts/render-icon.cjs  # re-render assets/AppIcon.png from assets/icon.svg (Playwright)
cargo test --release --test bench_scan -- --ignored --nocapture   # scan benchmark / digest, ignored by default
cargo test --test ui_cost -- --nocapture       # timing print for post-scan layout work, not a pass/fail gate
```

Toolchain: `rust-version` is 1.85.0, but the pinned `eframe`/`egui_extras` 0.31.1 and several exact-pinned transitive crates (`=` pins in `Cargo.toml`) need a newer rustc; use current stable. Don't loosen those pins casually — they hold the dependency graph to versions that build. Linux builds need the X11/Wayland dev libs listed in `.github/workflows/ci.yml`.

## Architecture

Single crate, library + thin binary. `src/main.rs` only parses `--scan`, `--gui-scan`, `--help`; everything else is in the `spacetree` lib (`src/lib.rs` re-exports the public API that the integration tests in `tests/` use). `scan`, `layout`, `sort`, `ext`, and `format` have no egui dependency; only `app.rs` touches the UI.

**Data flow:** `scan(path) -> ScanResult { root: Node, volume_total, error_count }` → `sort_tree` + `legend_of` → `app.rs` renders the table from the `Node` tree and the treemap from `layout_node(focus, bounds) -> Tiling`. `Node` is the one shared model: every directory already carries aggregated `size` (allocated), `logical`, `files`, and a dominant `color_ext`, so nothing downstream re-walks the disk.

**Scanner (`src/scan.rs`):**
- Two directory-listing backends behind `list_bulk`: on macOS it uses `getattrlistbulk` (raw `libc` with manually parsed attribute buffers, `ATTR_*` constants) to get name, type, alloc size, data length, times, dev/ino, APFS clone id, and private (unshared) size in one syscall per batch; elsewhere it falls back to `read_dir` + metadata with no clone ids. Changes to the bulk parser need macOS to exercise; Linux CI only covers the fallback.
- Subdirectories are walked in parallel with rayon (`into_par_iter`). Shared state across workers: `seen: Mutex<HashSet<(dev, ino)>>` (hard-link / loop guard), `clones: CloneBook`, and `errors: AtomicU64`. `CloneBook` counts APFS sharing once: a repeated clone id (pure clone) gets size 0; a file whose private size is below its allocation (a clone rewritten in part, which gets a new id) has its physical extents read with `fcntl(F_LOG2PHYS_EXT)` and claimed in `extents::ExtentLedger`, and bytes already claimed by another file are subtracted. Plain files never pay for the extent lookup. Which file of a sharing pair keeps the shared bytes depends on walk order; the total does not.
- Volume capacity comes from `statfs` on macOS (64-bit block count; Darwin's `statvfs` is 32-bit) and `statvfs` elsewhere.
- Errors are counted, not fatal: an unreadable child increments `error_count` and the scan returns partial results. Only an unreadable root returns `Err`. The CLI exits 1 on `error_count > 0`; the app shows a persistent warning.

**Treemap (`src/layout.rs`):** integer-pixel squarified layout producing `Tile`s (path, `PxRect`, byte `weight`) and `Frame`s (one per directory it recursed into, with `depth` 1 = child of the focus); `Tiling::hit`/`hit_tile`/`frames_at` map pointer positions back to paths. A directory's children fill exactly its frame, so frames are outlines and labels drawn on top; never inset them, or area stops matching allocated bytes. There is a per-bounds tile budget: when a level has more children than fit, the tail is merged into one `merged: true` "dust" tile that reuses the path of the first merged item (the app paints these differently and must not treat them as that real file). Ordering is size-descending with path as a tie-break, so layouts are deterministic — tests in `tests/layout.rs` rely on that.

**App (`src/app.rs`):** eframe/egui immediate-mode app. A scan runs on a `std::thread` and posts a `FinishedScan` (already sorted, legend computed) over an `mpsc` channel that `poll_scan` checks each frame; `Phase` tracks Idle/Scanning/Ready/Failed. UI state is keyed by `PathBuf` (selection, expanded rows, zoom focus), not by indices, so re-sorting doesn't invalidate it. The treemap layout is cached in `MapCache` and rebuilt only when `generation` (bumped per scan), focus path, or pixel bounds change — anything that changes what the map shows must invalidate this cache. Table selection and map selection share `self.selected`. The map draws tiles, then folder outlines, labels, and selection/hover outlines from the cached `Tiling` only (no tree searches per frame); `legend_hover` dims other extensions. egui uses its bundled fonts, not macOS's: glyphs such as ▾ ▸ exist only in the monospace font and show as boxes in proportional text, so check new symbols (⏵ ⏷ ⬆ are safe).

Sizes shown in the UI go through `format::format_bytes` (decimal SI, as Finder uses). The runtime Dock icon is `assets/AppIcon.png` passed via `ViewportBuilder::with_icon`; without it eframe substitutes the egui logo.

**Other modules:** `ext.rs` maps a filename to a canonical extension key, color, and description, and builds the legend; `sort.rs` sorts siblings recursively by `SortColumn`; `format.rs` produces the `--scan` text report (header lines `volume_total_bytes=` / `root_size_bytes=` are checked by `ci-runtime.sh`, so keep them stable); `finder.rs` reveals via `open -R -- <path>` as argv.

## CI

Workflows in `.github/workflows/`: `ci.yml` (tests + runtime script on macOS and Linux), `lint.yml` (rustfmt warning-only, Clippy with the flags above, actionlint), `bundle.yml` (app zip artifact), `release.yml` (manual `v<version>` release), `auto-release.yml` (gates on every check run on main's head, then publishes that commit's bundle as `v<version>-main.<date>.<sha7>`; its `REQUIRED` list names check runs by job name, so update it when renaming jobs), and `cargo-deny.yml` (uses `deny.toml`), `gitleaks.yml`, `MSDO.yml` for supply-chain/security scans. Actions are pinned by commit SHA; keep new ones pinned the same way.
