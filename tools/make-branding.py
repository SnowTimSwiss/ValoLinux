#!/usr/bin/env python3
"""Generate ValoLinux branding: wallpaper, greeter background, boot splash, logo.

Concept: "valo" means "light" (Finnish). Dawn over a mountain ridge: night sky, aurora,
a rising sun. Palette and fonts are documented in docs/design.md.

Usage: python3 -I tools/make-branding.py [output-dir]
Deterministic (fixed random seed), so re-running produces identical files.
"""
import pathlib
import sys

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

OUT = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "image/config/includes.chroot/usr/share/backgrounds")
FONT_DIR = pathlib.Path("/usr/share/fonts/opentype/inter")
F_WORD_BOLD = FONT_DIR / "InterDisplay-Bold.otf"
F_WORD_LIGHT = FONT_DIR / "Inter-Light.otf"

# Palette (see docs/design.md)
NIGHT = (8, 11, 32)
INDIGO = (30, 27, 84)
VIOLET = (88, 44, 128)
ROSE = (196, 84, 120)
AMBER = (247, 160, 84)
SUN = (255, 214, 140)
RIDGE = [(70, 46, 112), (40, 28, 80), (20, 15, 46), (10, 8, 26)]


def lerp(a, b, t):
    return a + (b - a) * t


def gradient_sky(w, h):
    """Vertical multi-stop gradient as float array (h, w, 3)."""
    stops = [(0.0, NIGHT), (0.35, INDIGO), (0.55, VIOLET), (0.68, ROSE), (0.76, AMBER)]
    y = np.linspace(0, 1, h)
    col = np.zeros((h, 3))
    for c in range(3):
        col[:, c] = np.interp(y, [s[0] for s in stops], [s[1][c] for s in stops])
    return np.repeat(col[:, None, :], w, axis=1)


def add_glow(img, cx, cy, radius, color, strength):
    h, w, _ = img.shape
    yy, xx = np.mgrid[0:h, 0:w]
    d = np.sqrt(((xx - cx) / radius) ** 2 + ((yy - cy) / (radius * 0.8)) ** 2)
    a = np.clip(np.exp(-d * d) * strength, 0, 1)[..., None]
    img[:] = img * (1 - a) + np.array(color) * a


def stars(img, rng, count, max_y):
    h, w, _ = img.shape
    xs = rng.integers(0, w, count)
    ys = (rng.random(count) ** 1.8 * max_y).astype(int)
    for x, y in zip(xs, ys):
        b = rng.uniform(0.3, 1.0) * (1 - y / max_y)
        img[y, x] = img[y, x] * (1 - b) + 255 * b
        if b > 0.8 and 0 < x < w - 1 and 0 < y < h - 1:  # a few brighter, slightly larger stars
            for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                img[y + dy, x + dx] = img[y + dy, x + dx] * 0.6 + 255 * 0.4


def aurora(w, h, rng):
    """Soft aurora ribbons as one RGBA layer. Each ribbon is blurred on its own layer whose
    transparent pixels already carry the ribbon colour, so blurring does not pull in black."""
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    for color, base, amp, thick in (
        ((90, 230, 200, 90), 0.20, 0.06, 0.05),
        ((150, 130, 255, 80), 0.28, 0.05, 0.07),
        ((255, 130, 210, 50), 0.36, 0.04, 0.04),
    ):
        layer = Image.new("RGBA", (w, h), color[:3] + (0,))
        d = ImageDraw.Draw(layer)
        phase = rng.uniform(0, np.pi * 2)
        freq = rng.uniform(1.2, 2.0)
        pts_top, pts_bot = [], []
        for x in np.linspace(-0.05, 1.05, 200):
            y = base + amp * np.sin(x * np.pi * freq + phase) + 0.02 * np.sin(x * 11 + phase)
            t = thick * (0.6 + 0.4 * np.sin(x * 3 + phase))
            pts_top.append((x * w, y * h))
            pts_bot.append((x * w, (y + t) * h))
        d.polygon(pts_top + pts_bot[::-1], fill=color)
        out = Image.alpha_composite(out, layer.filter(ImageFilter.GaussianBlur(h * 0.035)))
    return out


def ridge(w, h, rng, base, rough, octaves=7):
    """Fractal mountain ridge line (y in pixels for each x)."""
    x = np.linspace(0, 1, w)
    y = np.zeros(w)
    amp, freq = rough, 1.5
    for _ in range(octaves):
        phase = rng.uniform(0, 100)
        y += amp * np.abs(np.sin(x * freq * np.pi + phase))  # sharp peaks
        amp *= 0.5
        freq *= 2.1
    y = y - y.mean()
    return (base - y) * h


def draw_logo(size, bg=None):
    """Sunrise mark: sun disk above a horizon, with reflection stripes below. RGBA."""
    s = size * 4  # supersample
    img = Image.new("RGBA", (s, s), bg or (0, 0, 0, 0))
    # vertical gradient for the mark
    grad = np.zeros((s, s, 4), dtype=np.uint8)
    t = np.linspace(0, 1, s)[:, None]
    for c, (a, b) in enumerate(zip(SUN, ROSE)):
        grad[..., c] = lerp(a, b, t).astype(np.uint8)
    grad[..., 3] = 255
    grad_img = Image.fromarray(grad, "RGBA")
    mask = Image.new("L", (s, s), 0)
    m = ImageDraw.Draw(mask)
    cx, r = s / 2, s * 0.38
    horizon = s * 0.56
    m.pieslice([cx - r, horizon - r, cx + r, horizon + r], 180, 360, fill=255)  # upper half sun
    gap = s * 0.055
    for i, width in enumerate((0.76, 0.56, 0.34)):  # reflection stripes
        y0 = horizon + gap + i * (gap * 1.9)
        half = r * width
        m.rounded_rectangle([cx - half, y0, cx + half, y0 + gap], radius=gap / 2, fill=255)
    img.paste(grad_img, (0, 0), mask)
    return img.resize((size, size), Image.LANCZOS)


def wordmark(draw, x, y, height, align="left", light=(235, 232, 255)):
    fb = ImageFont.truetype(str(F_WORD_BOLD), height)
    fl = ImageFont.truetype(str(F_WORD_LIGHT), height)
    w1 = draw.textlength("valo", font=fb)
    w2 = draw.textlength("linux", font=fl)
    total = w1 + w2
    if align == "center":
        x -= total / 2
    draw.text((x, y), "valo", font=fb, fill=(255, 255, 255))
    draw.text((x + w1, y), "linux", font=fl, fill=light)
    return total


def scene(w, h, seed=7, logo_corner=True, sun_y=0.74):
    rng = np.random.default_rng(seed)
    img = gradient_sky(w, h)
    add_glow(img, w * 0.5, h * sun_y, w * 0.30, AMBER, 0.45)
    add_glow(img, w * 0.5, h * sun_y, w * 0.12, SUN, 0.80)
    stars(img, rng, int(w * h / 2600), int(h * 0.5))
    base = Image.fromarray(np.clip(img, 0, 255).astype(np.uint8), "RGB").convert("RGBA")
    base = Image.alpha_composite(base, aurora(w, h, rng))

    # sun disk
    sun = Image.new("RGBA", (w, h), SUN + (0,))
    sd = ImageDraw.Draw(sun)
    r = h * 0.085
    sd.ellipse([w / 2 - r, h * sun_y - r, w / 2 + r, h * sun_y + r], fill=SUN + (255,))
    base = Image.alpha_composite(base, sun.filter(ImageFilter.GaussianBlur(h * 0.004)))

    # mountain layers, far to near, each with a little haze
    for i, (col, b, rough) in enumerate(zip(RIDGE, (0.70, 0.76, 0.84, 0.92), (0.10, 0.12, 0.10, 0.08))):
        ys = ridge(w, h, rng, b, rough)
        layer = Image.new("RGBA", (w, h), col + (0,))
        poly = [(x, ys[x]) for x in range(0, w, 2)] + [(w, h), (0, h)]
        ImageDraw.Draw(layer).polygon(poly, fill=col + (255,))
        if i < 2:
            layer = layer.filter(ImageFilter.GaussianBlur(h * 0.0006 * (2 - i)))
        base = Image.alpha_composite(base, layer)

    # film grain against banding
    arr = np.asarray(base).astype(np.int16)
    noise = rng.normal(0, 2.2, arr.shape[:2])[..., None]
    arr[..., :3] = np.clip(arr[..., :3] + noise, 0, 255)
    out = Image.fromarray(arr.astype(np.uint8), "RGBA")

    if logo_corner:
        d = ImageDraw.Draw(out)
        ls = int(h * 0.045)
        m = int(h * 0.04)
        out.alpha_composite(draw_logo(ls), (m, h - m - ls))
        wordmark(d, m + ls * 1.25, h - m - ls * 0.92, int(ls * 0.62))
    return out.convert("RGB")


def splash(w, h):
    img = scene(w, h, seed=3, logo_corner=False, sun_y=0.80)
    img = img.convert("RGBA")
    # darken top for readable boot menu text
    shade = Image.new("RGBA", (w, h), (8, 11, 32, 110))
    img = Image.alpha_composite(img, shade)
    d = ImageDraw.Draw(img)
    ls = int(h * 0.16)
    img.alpha_composite(draw_logo(ls), (int(w / 2 - ls / 2), int(h * 0.10)))
    wordmark(d, w / 2, int(h * 0.10 + ls * 1.05), int(h * 0.075), align="center")
    return img.convert("RGB")


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    scene(1920, 1080).save(OUT / "valolinux.png", optimize=True)
    scene(1920, 1080, logo_corner=False).save(OUT / "valolinux-greeter.png", optimize=True)
    splash(640, 480).save(OUT / "valolinux-splash.png", optimize=True)
    logo_dir = OUT.parent / "pixmaps"
    logo_dir.mkdir(parents=True, exist_ok=True)
    draw_logo(256).save(logo_dir / "valolinux-logo.png")
    print("written to", OUT, "and", logo_dir)


if __name__ == "__main__":
    main()
