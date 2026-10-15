#!/usr/bin/env python3
"""Wrap egui window captures in a macOS-style title bar (a mock-up, not a native capture).

usage: frame-screenshots.py <png>...
Already framed images (they have an alpha channel) are skipped, so this is safe to rerun.
"""
import sys

from PIL import Image, ImageDraw, ImageFilter, ImageFont

BAR_H, PAD, RADIUS = 30, 28, 10
BAR = (44, 44, 46, 255)
LIGHTS = [(255, 95, 87), (254, 188, 46), (40, 200, 64)]


def font(size):
    for path in ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                 "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf"):
        try:
            return ImageFont.truetype(path, size)
        except OSError:
            pass
    return ImageFont.load_default()


def frame(path):
    shot = Image.open(path)
    if shot.mode == "RGBA":
        print(f"skip {path}: already framed")
        return
    shot = shot.convert("RGBA")
    w, h = shot.size
    win_w, win_h = w, h + BAR_H
    window = Image.new("RGBA", (win_w, win_h), BAR)
    window.paste(shot, (0, BAR_H))
    d = ImageDraw.Draw(window)
    for i, rgb in enumerate(LIGHTS):
        cx, cy = 18 + i * 20, BAR_H // 2
        d.ellipse((cx - 6, cy - 6, cx + 6, cy + 6), fill=rgb + (255,))
    f = font(13)
    tw = d.textlength("SpaceTree", font=f)
    d.text(((win_w - tw) / 2, BAR_H / 2 - 8), "SpaceTree", fill=(200, 200, 204, 255), font=f)
    mask = Image.new("L", (win_w, win_h), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, win_w - 1, win_h - 1), RADIUS, fill=255)
    out = Image.new("RGBA", (win_w + 2 * PAD, win_h + 2 * PAD), (0, 0, 0, 0))
    shadow = Image.new("RGBA", out.size, (0, 0, 0, 0))
    shadow.paste((0, 0, 0, 120), (PAD, PAD + 8), mask)
    out = Image.alpha_composite(out, shadow.filter(ImageFilter.GaussianBlur(14)))
    out.paste(window, (PAD, PAD), mask)
    out.save(path)
    print(f"framed {path}")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    for p in sys.argv[1:]:
        frame(p)
