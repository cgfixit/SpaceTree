# Screenshots

`SpaceTree-9.28.26.jpeg` is the historical macOS screenshot. `before-treemap-main.png` shows an earlier treemap (unlabeled tiles, binary sizes labeled MB).

The other PNGs are renders of the current UI on a synthetic tree (`/Users/demo/Synthetic`, about 110 MB in 520 generated files). They are egui renders under Xvfb on Linux with XTest pointer input, not native macOS captures. The title bar, traffic lights and drop shadow are a mock-up added by `scripts/frame-screenshots.py`; the content inside the window is the real egui render. Fonts and layout match egui on macOS. Regenerate them with:

```bash
python3 scripts/make-synthetic-tree.py /Users/demo/Synthetic
python3 scripts/capture-screenshots.py target/debug/spacetree /Users/demo/Synthetic screenshots
```

(needs Xvfb, ImageMagick `import`, `pip install python-xlib pillow`; the capture script frames each PNG at the end)

The green `~N% full` cell is not visible in these captures by design: it appears only when the scan root is `/` or another local macOS volume root, and these captures scan a synthetic folder on Linux.

| File | Shows |
| --- | --- |
| `empty-state.png` | First launch: what the app does and how to start. |
| `treemap-table-folder-selection.png` | Selecting `Archives` in the table outlines it in the map. |
| `treemap-hover-tooltip.png` | Folder outlines and labels, file names and sizes, hover tooltip. |
| `treemap-legend-highlight.png` | Hovering `.mp4` in the legend dims every other extension. |
| `treemap-zoom-breadcrumb.png` | Double-clicking the `Photos` label zooms in; the breadcrumb shows `Synthetic › Photos`. |
| `app-icon-256.png` | The app icon (`assets/icon.svg`) at 256 px. |
