# Screenshots

`SpaceTree-9.28.26.jpeg` is the historical macOS screenshot. `before-treemap-main.png` shows an earlier treemap (unlabeled tiles, binary sizes labeled MB).

The other PNGs are renders of the current UI on a synthetic tree (`/Users/demo/Synthetic`, about 110 MB in 520 generated files). They are egui renders under Xvfb on Linux with XTest pointer input, not native macOS captures: fonts and layout match egui on macOS, window chrome does not. Regenerate them with:

```bash
python3 scripts/make-synthetic-tree.py /Users/demo/Synthetic
python3 scripts/capture-screenshots.py target/debug/spacetree /Users/demo/Synthetic screenshots
```

(needs Xvfb, ImageMagick `import`, and `pip install python-xlib`)

| File | Shows |
| --- | --- |
| `empty-state.png` | First launch: what the app does and how to start. |
| `treemap-table-folder-selection.png` | Selecting `Archives` in the table outlines it in the map. |
| `treemap-hover-tooltip.png` | Folder outlines and labels, file names and sizes, hover tooltip. |
| `treemap-legend-highlight.png` | Hovering `.mp4` in the legend dims every other extension. |
| `treemap-zoom-breadcrumb.png` | Double-clicking the `Photos` label zooms in; the breadcrumb shows `Synthetic › Photos`. |
| `app-icon-256.png` | The app icon (`assets/icon.svg`) at 256 px. |
