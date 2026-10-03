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
cargo run -- --scan /path                      # text report; exits 1 if any read failed
cargo run -- --gui-scan /path                  # GUI that scans on launch (sets SPACETREE_AUTOSCAN)
./scripts/make-app.sh                          # macOS only: release build -> ad-hoc signed dist/SpaceTree.app
cargo test --release --test bench_scan -- --ignored --nocapture   # scan benchmark / digest, ignored by default
cargo test --test ui_cost -- --nocapture       # timing print for post-scan layout work, not a pass/fail gate
```

Toolchain: `rust-version` is 1.85.0, but the pinned `eframe`/`egui_extras` 0.31.1 and several exact-pinned transitive crates (`=` pins in `Cargo.toml`) need a newer rustc; use current stable. Don't loosen those pins casually — they hold the dependency graph to versions that build. Linux builds need the X11/Wayland dev libs listed in `.github/workflows/ci.yml`.

## Architecture

Single crate, library + thin binary. `src/main.rs` only parses `--scan`, `--gui-scan`, `--help`; everything else is in the `spacetree` lib (`src/lib.rs` re-exports the public API that the integration tests in `tests/` use). `scan`, `layout`, `sort`, `ext`, and `format` have no egui dependency; only `app.rs` touches the UI.

**Data flow:** `scan(path) -> ScanResult { root: Node, volume_total, error_count }` → `sort_tree` + `legend_of` → `app.rs` renders the table from the `Node` tree and the treemap from `layout_node(focus, bounds) -> Tiling`. `Node` is the one shared model: every directory already carries aggregated `size` (allocated), `logical`, `files`, and a dominant `color_ext`, so nothing downstream re-walks the disk.

**Scanner (`src/scan.rs`):**
- Two directory-listing backends behind `list_bulk`: on macOS it uses `getattrlistbulk` (raw `libc` with manually parsed attribute buffers, `ATTR_*` constants) to get name, type, alloc size, data length, times, dev/ino, and APFS clone id in one syscall per batch; elsewhere it falls back to `read_dir` + metadata with no clone ids. Changes to the bulk parser need macOS to exercise; Linux CI only covers the fallback.
- Subdirectories are walked in parallel with rayon (`into_par_iter`). Shared state across workers: `seen: Mutex<HashSet<(dev, ino)>>` (hard-link / loop guard), `clones: Mutex<HashSet<u64>>` (first file in an APFS clone group keeps the bytes, later ones get size 0), `errors: AtomicU64`.
- Errors are counted, not fatal: an unreadable child increments `error_count` and the scan returns partial results. Only an unreadable root returns `Err`. The CLI exits 1 on `error_count > 0`; the app shows a persistent warning.

**Treemap (`src/layout.rs`):** integer-pixel squarified layout producing `Tile`s with a `PxRect`; `Tiling::hit(x, y)` maps clicks back to paths. There is a per-bounds tile budget: when a level has more children than fit, the tail is merged into one `merged: true` "dust" tile that reuses the path of the first merged item (the app paints these differently and must not treat them as that real file). Ordering is size-descending with path as a tie-break, so layouts are deterministic — tests in `tests/layout.rs` rely on that.

**App (`src/app.rs`):** eframe/egui immediate-mode app. A scan runs on a `std::thread` and posts a `FinishedScan` (already sorted, legend computed) over an `mpsc` channel that `poll_scan` checks each frame; `Phase` tracks Idle/Scanning/Ready/Failed. UI state is keyed by `PathBuf` (selection, expanded rows, zoom focus), not by indices, so re-sorting doesn't invalidate it. The treemap layout is cached in `MapCache` and rebuilt only when `generation` (bumped per scan), focus path, or pixel bounds change — anything that changes what the map shows must invalidate this cache. Table selection and map selection share `self.selected`.

**Other modules:** `ext.rs` maps a filename to a canonical extension key, color, and description, and builds the legend; `sort.rs` sorts siblings recursively by `SortColumn`; `format.rs` produces the `--scan` text report (header lines `volume_total_bytes=` / `root_size_bytes=` are checked by `ci-runtime.sh`, so keep them stable); `finder.rs` reveals via `open -R -- <path>` as argv.

## CI

Workflows in `.github/workflows/`: `ci.yml` (tests + runtime script on macOS and Linux), `lint.yml` (rustfmt warning-only, Clippy with the flags above, actionlint), `bundle.yml`/`release.yml` (app packaging), and `cargo-deny.yml` (uses `deny.toml`), `gitleaks.yml`, `MSDO.yml` for supply-chain/security scans. Actions are pinned by commit SHA; keep new ones pinned the same way.
