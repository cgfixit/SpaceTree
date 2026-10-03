# Screenshots

`SpaceTree-9.28.26.jpeg` is the historical macOS screenshot from the README.

The PNGs below show the treemap redesign on a synthetic tree (`/Users/demo/Synthetic`, about 110 MB in 520 files). They are egui renders under Xvfb on Linux, captured with XTest pointer input, not native macOS captures. Fonts and layout match egui on macOS; window chrome does not.

| File | Shows |
| --- | --- |
| `before-treemap-main.png` | `main` before the change: unlabeled tiles, binary sizes labeled MB (104.6 "MB"). |
| `treemap-hover-tooltip.png` | Folder outlines and labels, file names and sizes, hover tooltip. Decimal units (109.7 MB, as Finder shows). |
| `treemap-legend-highlight.png` | Hovering `.wav` in the legend dims every other extension. |
| `treemap-table-folder-selection.png` | Selecting `Archives` in the table outlines it in the map. |
| `treemap-zoom-breadcrumb.png` | Double-clicking the `Photos` label zooms in; the breadcrumb shows `Synthetic › Photos`. |
| `app-icon-256.png` | The app icon (`assets/icon.svg`) at 256 px. |
