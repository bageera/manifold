#!/usr/bin/env python3
"""Photographic-class corpora for the MNF gates (repo contract: dev data
separate from held-out evaluation data; held-out is never tuned on).

dev:      testdata/photos/      (this corpus IS allowed in tuning studies)
held-out: testdata/heldout-photos/ (disjoint generators; gate-only)

Content per image (pseudo-photography, deterministic):
  - 3..6 octaves of bilinear value noise  -> tonal variation / structure
  - natural palette ramp (terrain, dusk-sky, forest, sand+shadow, slate,
    autumn) selected per image                            -> color realism
  - per-pixel sensor grain (gaussian, sigma ~2-6)         -> sensor noise
  - low-pass chroma noise (stronger than luma noise)      -> color pipeline
  - radial vignette (-20..0 %)                            -> lens falloff
  - optional 1-radius box defocus on every 3rd image      -> optical softness
All parameters drawn from a per-image seed; dirs are seed-disjoint, so a
finding on one cannot leak into the other except through shared code (which
is the point of independent generators).

Sizes: 160x120, 192x144, 224x160 — small enough for the zlib-proxy
encoder to stay seconds-fast across 20 files.
"""
import itertools
import os
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold/testdata")
SIZES = [(160, 120), (192, 144), (224, 160)]

# palette ramps: (name, [(t, (r, g, b)), ...]) — t in 0..1
PALETTES = [
    ("terrain", [(0.0, (24, 32, 44)), (0.3, (56, 72, 58)), (0.55, (104, 96, 70)), (0.8, (158, 148, 118)), (1.0, (218, 210, 190))]),
    ("dusk", [(0.0, (18, 20, 42)), (0.35, (52, 34, 62)), (0.6, (124, 60, 52)), (0.85, (212, 128, 72)), (1.0, (244, 196, 130))]),
    ("forest", [(0.0, (12, 28, 16)), (0.3, (26, 52, 24)), (0.55, (54, 84, 36)), (0.8, (96, 116, 60)), (1.0, (150, 156, 96))]),
    ("sand", [(0.0, (36, 26, 18)), (0.35, (74, 54, 34)), (0.6, (128, 100, 66)), (0.85, (186, 158, 118)), (1.0, (222, 202, 164))]),
    ("slate", [(0.0, (18, 22, 26)), (0.35, (42, 50, 56)), (0.6, (76, 88, 96)), (0.85, (126, 138, 146)), (1.0, (186, 196, 202))]),
    ("autumn", [(0.0, (30, 18, 8)), (0.3, (84, 42, 14)), (0.55, (160, 88, 22)), (0.8, (206, 140, 46)), (1.0, (236, 196, 110))]),
    ("ocean", [(0.0, (8, 18, 34)), (0.35, (16, 44, 74)), (0.6, (32, 84, 116)), (0.85, (92, 148, 168)), (1.0, (186, 216, 220))]),
    ("stone", [(0.0, (26, 24, 22)), (0.35, (56, 54, 50)), (0.6, (98, 96, 92)), (0.85, (150, 148, 144)), (1.0, (206, 204, 200))]),
]


def value_noise(h, w, rng, octaves=5, base=3):
    """Sum of bilinear-upsampled random grids, octaves 2^(base+i); returns f64 in ~0..1."""
    acc = np.zeros((h, w), dtype=np.float64)
    amp_total = 0.0
    for o in range(octaves):
        cells = base * (2**o)
        amp = 1.0 / (2**o)
        amp_total += amp
        grid = rng.random((cells + 2, cells + 2))
        ys = np.linspace(0, cells, h, endpoint=False)
        xs = np.linspace(0, cells, w, endpoint=False)
        y0 = ys.astype(np.int64)
        x0 = xs.astype(np.int64)
        fy = ys - y0
        fx = xs - x0
        y0c, x0c = y0[:, None], x0[None, :]
        fy, fx = fy[:, None], fx[None, :]
        g0 = grid[y0c, x0c]
        g1 = grid[y0c, x0c + 1]
        g2 = grid[y0c + 1, x0c]
        g3 = grid[y0c + 1, x0c + 1]
        top = g0 * (1 - fx) + g1 * fx
        bot = g2 * (1 - fx) + g3 * fx
        acc += (top * (1 - fy) + bot * fy) * amp
    return acc / amp_total


def ramp_color(field, palette):
    ts = np.clip(field, 0, 1)
    img = np.zeros(field.shape + (3,), dtype=np.float64)
    for (t0, c0), (t1, c1) in zip(palette, palette[1:]):
        m = (ts >= t0) & (ts <= t1) if t1 < 1.0 else (ts >= t0)
        if not m.any():
            continue
        local = np.where(m, (ts - t0) / max(t1 - t0, 1e-9), 0.0)
        for c in range(3):
            img[..., c] = np.where(m, c0[c] + (c1[c] - c0[c]) * local, img[..., c])
    return img


def vignette(h, w, strength):
    y, x = np.mgrid[0:h, 0:w]
    cy, cx = (h - 1) / 2, (w - 1) / 2
    r = np.sqrt(((y - cy) / cy) ** 2 + ((x - cx) / cx) ** 2)
    v = 1 - strength * np.clip(r, 0, 1) ** 2
    return v[..., None]


def one_image(seed, w, h):
    rng = np.random.default_rng(seed)
    palette = PALETTES[int(rng.integers(0, len(PALETTES)))]
    octaves = int(rng.integers(3, 7))
    blur = bool(rng.integers(0, 3) == 0)
    grain_sigma = float(rng.uniform(2.0, 6.0))
    chroma_extra = float(rng.uniform(1.2, 3.0))
    vig = float(rng.uniform(0.0, 0.35))

    field = value_noise(h, w, rng, octaves=octaves)
    img = ramp_color(field, palette[1])

    # sensor grain (luma) + low-pass chroma noise
    grain = rng.normal(0, grain_sigma, (h, w, 1))
    cn = rng.normal(0, 1, (h, w, 3))
    # simple box blur x2 for chroma (cheap low-pass)
    cmap = Image.fromarray(np.uint8(np.clip(128 + cn * 20, 0, 255)), "RGB")
    cmap = cmap.filter(ImageFilter.GaussianBlur(radius=2))
    chroma_noise = np.asarray(cmap, dtype=np.float64) - 128.0
    chroma_noise *= chroma_extra
    img = img + grain + chroma_noise * 0.35
    img *= vignette(h, w, vig)
    img = np.clip(img, 0, 255).astype(np.uint8)

    if blur:
        pim = Image.fromarray(img, "RGB").filter(ImageFilter.GaussianBlur(radius=1))
        img = np.asarray(pim)
    return img


def main():
    import sys

    force = "--force" in sys.argv
    for subdir, base_seed, count in [
        ("photos", 900_000, 10),
        ("heldout-photos", 3_100_000, 10),
    ]:
        d = ROOT / subdir
        d.mkdir(exist_ok=True)
        seeds = range(base_seed, base_seed + count)
        for i, seed in enumerate(seeds):
            w, h = SIZES[i % len(SIZES)]
            img = one_image(seed, w, h)
            name = f"{palette_of_one(seed)}_{w}x{h}_s{seed}.png"
            Image.fromarray(img, "RGB").save(d / name)
            print(f"{subdir}/{name}: {img.shape[1]}x{img.shape[0]}")


def palette_of_one(seed):
    rng = np.random.default_rng(seed)
    return PALETTES[int(rng.integers(0, len(PALETTES)))][0]


if __name__ == "__main__":
    main()