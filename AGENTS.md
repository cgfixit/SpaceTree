# SpaceTree agent guide

SpaceTree is a Rust and `eframe` macOS disk usage viewer. A user selects a folder or disk; the app scans it and shows a sortable table, an extension legend, and a treemap. `spacetree --scan <path>` prints a text report. The app does not offer file deletion.

## Read the current implementation

- `src/scan.rs` owns filesystem traversal and size accounting. `src/layout.rs` places treemap tiles. `src/app.rs` owns the native window and interactions. `src/ext.rs` defines extension colors. `src/finder.rs` opens a selected path in Finder. `src/ollama.rs` maps Ollama blobs to models.
- `tests/scan.rs`, `tests/layout.rs`, `tests/ext.rs`, `tests/format.rs`, and `tests/ui_cost.rs` cover the corresponding behavior. `scripts/ci-runtime.sh` exercises the built CLI. `scripts/verify-macos-sizes.sh` compares scan sizes with `stat`, `du`, and `df` on macOS, and `scripts/verify-macos-root.sh` scans `/` there and checks the whole-volume totals. `scripts/make-app.sh` creates and ad-hoc signs `dist/SpaceTree.app`.
- `.github/workflows/ci.yml` tests macOS and Linux. `lint.yml` covers rustfmt, Clippy, and actionlint. `bundle.yml` packages the app; `release.yml` publishes it on demand and `auto-release.yml` publishes each fully green commit on main; `cargo-deny.yml`, `gitleaks.yml`, and `MSDO.yml` scan dependencies or security findings. Inspect these jobs before adding a workflow.
- `README.md` describes the product. The committed screenshots are a reference, not proof of current behavior. On Linux the app renders under `Xvfb` (see `screenshots/README.md`), which is enough to check labels and layout but not native macOS behavior.

## Preserve product contracts

- Physical size is allocated bytes (`st_blocks * 512`); logical size is `st_size`. A directory sums its children. APFS clone accounting counts shared allocation once.
- The scan can follow a user-selected root symlink to a directory. It does not follow child directory symlinks. It skips `/System/Volumes` unless that path is the scan root.
- A whole-volume scan (the root is a local macOS mount point) does not enter other mounts (`/Volumes/*`, `/dev`, mounted disk images) and adds one synthetic `Hidden space` child to the root: `volume_used` minus the walked total (unreadable folders, local snapshots, system data outside the walk). Its `color_ext` is `HIDDEN_KEY`; it is not a file, has no legend row, and cannot be revealed. Folder scans have neither.
- Always keep the green whole-volume figures unless the operator says otherwise: when the scan root is `/` or another local macOS volume root, the root row's Percentage cell is green and shows `~N% full`, and the status area shows a green `Whole volume: ~N% free (...)` line from the moment the scan starts. Only whole-volume scans show them; child rows and folder scans keep scan-share percentages. Code: `volume_used` and `volume_available` in `src/scan.rs`, drawn in `src/app.rs`. Do not remove, restyle away, or repurpose either one.
- Used, available, and fullness match System Settings › Storage, which the operator treats as the truth: `volume_available` is macOS's Available (free plus purgeable space), `volume_used` is capacity minus it, and the root row totals `volume_used`. `volume_on_disk` (what `df` prints) and `volume_free` (`f_bavail`) are shown only as detail in tooltips and `--volume`. A volume with no Available figure from macOS falls back to the on-disk figures.
- Ollama support is read-only. `src/ollama.rs` reads `models/manifests` to name the blobs in `models/blobs` in the table, the treemap, and `--ollama`. It prints `ollama rm` commands and never runs them or deletes a blob.
- Treemap area represents allocated size. Single click selects; double-click enters a folder or reveals a file in Finder. Table sorting and map selection must stay consistent.
- Scanning and display are local. Use synthetic paths for tests and screenshots. Do not add uploads, telemetry, destructive file operations, or persistent records of scanned paths as incidental work.
- Keep Finder calls argument based. Do not interpolate a path into a shell command.

## Verify what changed

- Always prefer linting and running the code over running the whole test suite, whatever the change. For Rust changes, run `cargo fmt --check`, the Clippy command from `.github/workflows/lint.yml`, and the built binary on the changed behavior: `./scripts/ci-runtime.sh target/debug/spacetree`, plus `--scan`, `--volume`, or `--ollama` on a synthetic fixture. Run a test only by name when it covers the change (`cargo test --lib <name>`); leave `cargo test --locked --all-targets` to hosted CI. macOS-only code can be linted from Linux with `cargo clippy --target aarch64-apple-darwin`.
- For bundle or UI changes on macOS, build with `./scripts/make-app.sh`, verify the bundle with `codesign`, and exercise the exact app on a synthetic scan tree. A unit test or the committed screenshot alone does not verify a native interaction.
- For performance work, compare equivalent fixtures and outputs before and after the change. The ignored scan benchmarks in `tests/bench_scan.rs` do not run in the normal test suite.
- Report source, test, CLI, bundle, native UI, and hosted CI results separately. State a failed or unrun check precisely.

## Repo skills

The canonical skill files live in `.claude/skills/`. `.agents/skills/` links to those folders so Codex can discover the same instructions without copied content.

- `space-tree-optimize` scopes and implements a requested quality improvement.
- `space-tree-design` guides visual and interaction changes that make the app distinct and usable.
- `space-tree-verify` selects checks for code, CLI, bundle, and native UI changes.
