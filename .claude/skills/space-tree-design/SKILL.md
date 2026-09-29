---
name: space-tree-design
description: Design and verify a distinctive SpaceTree macOS interface. Use when changing the treemap, table, legend, navigation, visual style, or other UI interactions.
---

# SpaceTree design

Make disk exploration clearer and more recognizable as SpaceTree. A new palette alone does not answer the WinDirStat resemblance concern.

1. Inspect the current `SpaceTree.app` when available, `src/app.rs`, `src/ext.rs`, `src/layout.rs`, and the committed screenshot. Treat the screenshot as historical. Note how a user finds the largest item, moves through folders, and acts on a selection.
2. Define the visual direction in terms of those tasks. Use hierarchy, typography, spacing, restrained color, and interaction feedback to distinguish the app. Keep the extension legend and treemap colors consistent. Preserve area as allocated bytes and make selected items clear without hiding small tiles.
3. Check the affected states among empty and failed scans, large and deep directories, many small files, selection, sorting, zoom, copy path, and Finder reveal. Work within the 800 × 500 minimum window in `src/app.rs`; inspect larger windows too. Use synthetic paths and redact local data from screenshots.
4. Implement the smallest coherent interface change. Avoid a custom rendering layer or persistent theme system unless the actual design needs one. Keep layout work within the existing cache and measure any added per-frame cost.
5. Run [SpaceTree verification](../space-tree-verify/SKILL.md). Exercise the built app on macOS for the changed interaction and capture an artifact that shows the result. State any interaction that source tests cannot prove.
