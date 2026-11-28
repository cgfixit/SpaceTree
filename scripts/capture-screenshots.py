#!/usr/bin/env python3
"""Capture README screenshots of the egui app under Xvfb with XTest input.

Needs: Xvfb, ImageMagick `import`, python-xlib, a debug/release spacetree binary,
and a synthetic tree from make-synthetic-tree.py. Linux render, not native macOS.

usage: capture-screenshots.py <spacetree-binary> <synthetic-dir> <out-dir>
"""
import os
import subprocess
import sys
import time

from Xlib import X, display
from Xlib.ext import xtest

W, H = 1200, 780



def shot(out, name):
    subprocess.run(["import", "-display", DISP, "-window", "root", os.path.join(out, name)], check=True)
    subprocess.run([sys.executable, os.path.join(os.path.dirname(__file__), "frame-screenshots.py"), os.path.join(out, name)], check=True)


class Pointer:
    def __init__(self):
        self.d = display.Display(DISP)

    def move(self, x, y):
        xtest.fake_input(self.d, X.MotionNotify, x=x, y=y)
        self.d.sync()
        time.sleep(0.15)

    def click(self, x, y, n=1):
        self.move(x, y)
        for _ in range(n):
            xtest.fake_input(self.d, X.ButtonPress, 1)
            xtest.fake_input(self.d, X.ButtonRelease, 1)
            self.d.sync()
            time.sleep(0.08)
        time.sleep(0.5)


def launch(binary, tree, env_extra):
    env = dict(os.environ, DISPLAY=DISP, LIBGL_ALWAYS_SOFTWARE="1", **env_extra)
    if "SPACETREE_AUTOSCAN" not in env_extra:
        env.pop("SPACETREE_AUTOSCAN", None)
    return subprocess.Popen([binary], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def main(binary, tree, out):
    os.makedirs(out, exist_ok=True)
    global DISP
    read_fd, write_fd = os.pipe()
    try:
        xvfb = subprocess.Popen(["Xvfb", "-displayfd", str(write_fd), "-nolisten", "tcp", "-screen", "0", f"{W}x{H}x24", "+extension", "GLX"], pass_fds=(write_fd,))
    finally:
        os.close(write_fd)
    try:
        with os.fdopen(read_fd) as ready:
            number = ready.readline().strip()
        if not number.isdecimal() or xvfb.poll() is not None:
            raise RuntimeError("Xvfb failed to allocate a display")
        DISP = ":" + number
        p = Pointer()
        app = launch(binary, tree, {"HOME": os.path.dirname(tree)})
        time.sleep(3)
        shot(out, "empty-state.png")
        app.terminate(); app.wait()

        app = launch(binary, tree, {"SPACETREE_AUTOSCAN": tree, "HOME": os.path.dirname(tree)})
        time.sleep(4)
        p.click(460, 148)  # Archives row (percentage cell: selects without expanding)
        shot(out, "treemap-table-folder-selection.png")
        p.move(1000, 640)
        p.move(1002, 641)  # a second motion event wakes egui so the tooltip appears
        time.sleep(1.5)
        shot(out, "treemap-hover-tooltip.png")
        p.move(1050, 125)
        time.sleep(0.6)
        shot(out, "treemap-legend-highlight.png")
        p.click(688, 407, n=2)  # double-click the Photos label to zoom
        p.move(600, 20)
        time.sleep(0.6)
        shot(out, "treemap-zoom-breadcrumb.png")
        app.terminate(); app.wait()
    finally:
        xvfb.terminate()
        xvfb.wait()


if __name__ == "__main__":
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    main(*sys.argv[1:])
