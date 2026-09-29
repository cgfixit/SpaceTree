---
name: space-tree-verify
description: Verify a SpaceTree change through the relevant Rust tests, CLI fixture, app bundle, and native UI. Use before claiming a fix works or opening a PR.
---

# SpaceTree verification

Select the checks that reach the changed behavior. Record the revision, platform, fixture, command, and result. Do not promote a passing source test to a native UI or hosted CI claim.

1. Run `cargo fmt --check` and `cargo test --locked --all-targets` for Rust changes. Run `./scripts/ci-runtime.sh target/debug/spacetree` after the test build to exercise `--help` and `--scan` on a temporary synthetic tree. Add a focused test when the defect needs a new falsifier.
2. For Rust changes, run the Clippy command from `.github/workflows/lint.yml`. Check edited workflows with `actionlint` when available. Keep the locked dependency graph and CI checks intact.
3. For macOS bundle or UI changes, run `./scripts/make-app.sh`. Confirm the executable and `Info.plist` identity, then run `codesign --verify --verbose=2 dist/SpaceTree.app`. Open the exact built app with a synthetic folder, using `--gui-scan <path>` when useful. Observe the changed table or treemap interaction in that process.
4. For scan changes, test allocated and logical size, clones, symlinks, and `/System/Volumes` as relevant. A synthetic local fixture does not prove USB or network-share resilience. Label those checks unrun until exercised on an appropriate mount.
5. For performance changes, record the fixture and toolchain. Compare the same workload and output before and after. Run ignored benchmarks explicitly only when they measure the changed cost.
6. Report pass, fail, skip, or not run for each relevant boundary. Include the reason for a failed or skipped check and do not silently replace it with a weaker check.
