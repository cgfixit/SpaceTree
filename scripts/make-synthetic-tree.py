#!/usr/bin/env python3
"""Create the synthetic demo tree used for README screenshots (about 110 MB).

usage: make-synthetic-tree.py <dir>   (e.g. /Users/demo/Synthetic)
File names and sizes are invented; nothing is read from the real filesystem.
"""
import os
import sys

MB = 1_000_000
TREE = {
    "Movies": [("trip-2026.mp4", 22 * MB), ("launch.mov", 9 * MB), ("intro.mp4", 4 * MB), ("outro.mp4", 2.5 * MB)],
    "Archives": [("backup-q3.zip", 11 * MB), ("installer.dmg", 8 * MB), ("old.7z", 3 * MB)],
    "Photos": [("DSC_9001.dng", 6.1 * MB), ("beach.png", 4.1 * MB), ("IMG_0001.jpg", 3.2 * MB), ("IMG_0002.jpg", 2.9 * MB), ("scan.tiff", 2.1 * MB)],
    "Music": [("track01.wav", 7 * MB), ("track02.wav", 6.4 * MB), ("podcast.mp3", 3.1 * MB)],
    "Documents": [("slides.key", 2.6 * MB), ("report.pdf", 1.8 * MB), ("budget.xlsx", 0.55 * MB), ("notes.docx", 0.14 * MB)],
}


def write(path, size):
    with open(path, "wb") as f:
        f.write(b"\0" * int(size))


def main(root):
    for folder, files in TREE.items():
        d = os.path.join(root, folder)
        os.makedirs(d, exist_ok=True)
        for name, size in files:
            write(os.path.join(d, name), size)
    code = os.path.join(root, "Code", "app")
    os.makedirs(code, exist_ok=True)
    write(os.path.join(root, "Code", "app", "bundle.js"), 5.2 * MB)
    for i in range(200):
        write(os.path.join(root, "Code", f"mod_{i:03}.rs"), 4_000 + (i * 37) % 9_000)
    cache = os.path.join(root, "Cache")
    os.makedirs(cache, exist_ok=True)
    for i in range(300):
        write(os.path.join(cache, f"blob_{i:03}.tmp"), 4_096 + (i * 91) % 6_000)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(sys.argv[1])
