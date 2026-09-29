# SpaceTree agent guide

SpaceTree is a Rust and `eframe` macOS disk usage viewer. A user selects a folder or disk; the app scans it and shows a sortable table, an extension legend, and a treemap. `spacetree --scan <path>` prints a text report. The app does not offer file deletion.

## Read the current implementation

- `src/scan.rs` owns filesystem traversal and size accounting. `src/layout.rs` places treemap tiles. `src/app.rs` owns the native window and interactions. `src/ext.rs` defines extension colors. `src/finder.rs` opens a selected path in Finder.
- `tests/scan.rs`, `tests/layout.rs`, `tests/ext.rs`, and `tests/ui_cost.rs` cover the corresponding behavior. `scripts/ci-runtime.sh` exercises the built CLI. `scripts/make-app.sh` creates and ad-hoc signs `dist/SpaceTree.app`.
- `.github/workflows/ci.yml` tests macOS and Linux. `lint.yml` covers rustfmt, Clippy, and actionlint. `bundle.yml` packages the app; `cargo-deny.yml`, `gitleaks.yml`, and `MSDO.yml` scan dependencies or security findings. Inspect these jobs before adding a workflow.
- `README.md` describes the product and lists ideas. Check each idea against current code and the running app before treating it as missing. The committed screenshot is a reference, not proof of current behavior.

## Preserve product contracts

- Physical size is allocated bytes (`st_blocks * 512`); logical size is `st_size`. A directory sums its children. APFS clone accounting counts shared allocation once.
- The scan can follow a user-selected root symlink to a directory. It does not follow child directory symlinks. It skips `/System/Volumes` unless that path is the scan root.
- Treemap area represents allocated size. Single click selects; double-click enters a folder or reveals a file in Finder. Table sorting and map selection must stay consistent.
- Scanning and display are local. Use synthetic paths for tests and screenshots. Do not add uploads, telemetry, destructive file operations, or persistent records of scanned paths as incidental work.
- Keep Finder calls argument based. Do not interpolate a path into a shell command.

## Verify what changed

- For Rust changes, run `cargo fmt --check`, `cargo test --locked --all-targets`, and `./scripts/ci-runtime.sh target/debug/spacetree`. Run the relevant Clippy and workflow checks when those files change; `.github/workflows/` holds the CI definitions.
- For bundle or UI changes on macOS, build with `./scripts/make-app.sh`, verify the bundle with `codesign`, and exercise the exact app on a synthetic scan tree. A unit test or the committed screenshot alone does not verify a native interaction.
- For performance work, compare equivalent fixtures and outputs before and after the change. The ignored scan benchmarks in `tests/bench_scan.rs` do not run in the normal test suite.
- Report source, test, CLI, bundle, native UI, and hosted CI results separately. State a failed or unrun check precisely.

## Repo skills

The canonical skill files live in `.claude/skills/`. `.agents/skills/` links to those folders so Codex can discover the same instructions without copied content.

- `space-tree-optimize` scopes and implements a requested quality improvement.
- `space-tree-design` guides visual and interaction changes that make the app distinct and usable.
- `space-tree-verify` selects checks for code, CLI, bundle, and native UI changes.
