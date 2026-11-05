#!/usr/bin/env python3
"""Generate ValoLinux placeholder branding (wallpaper, boot splash). Replace with real artwork later."""
from PIL import Image, ImageDraw, ImageFont
import sys, pathlib

out = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "image/config/includes.chroot/usr/share/backgrounds")
out.mkdir(parents=True, exist_ok=True)
BOLD = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"
REG = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"

def make(w, h, name, tagline=True):
    img = Image.new("RGB", (w, h))
    px = img.load()
    top, bot = (14, 22, 48), (52, 24, 92)
    for y in range(h):
        t = y / (h - 1)
        c = tuple(int(top[i] + (bot[i] - top[i]) * t) for i in range(3))
        for x in range(w):
            px[x, y] = c
    d = ImageDraw.Draw(img)
    # simple flame-ish mark: two stacked rounded shapes
    s = h // 6
    cx, cy = w // 2, int(h * 0.36)
    d.ellipse([cx - s, cy - s, cx + s, cy + s], fill=(255, 140, 50))
    d.ellipse([cx - s // 2, cy - s // 3, cx + s // 2, cy + s], fill=(255, 214, 102))
    f = ImageFont.truetype(BOLD, h // 9)
    text = "ValoLinux"
    tw = d.textlength(text, font=f)
    d.text(((w - tw) / 2, int(h * 0.55)), text, font=f, fill=(255, 255, 255))
    if tagline:
        f2 = ImageFont.truetype(REG, h // 28)
        t2 = "Schul-Linux  |  Test-Image"
        d.text(((w - d.textlength(t2, font=f2)) / 2, int(h * 0.55) + h // 7), t2, font=f2, fill=(190, 190, 225))
    img.save(out / name)

make(1920, 1080, "valolinux.png")
make(640, 480, "valolinux-splash.png", tagline=False)
print("written to", out)
