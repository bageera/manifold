#!/usr/bin/env python3
"""MNF14-CS1 — Coastline Scaling probe (measurement-only; work-order memo).

Tests the coastline-paradox hypothesis across RESOLUTION SCALES: does a
multiscale / recursive representation's encoded size grow more slowly with
resolution than the shipped portfolio's, on content whose structure is
scale-recursive? The decisive metric is the GROWTH CURVE (bytes vs width),
not a single-scale ratio.

Arms (every arm = COMPLETE serialized bytes for that resolution, decode
asserted in-loop where a decoder exists):
  A  shipped portfolio       — release CLI pick (manifold encode; MNF5/6/7/8/9)
  B  multiscale residuals    — exact integer pyramid I_k = U(I_{k-1}) + E_k,
                               U = 2x2 mean floor; per-level E (mod 256) by
                               zlib-9; level 0 raw. All transport counted.
  C  recursive boundary      — binary masks: coarse = 2x2 mean floor; each
                               refinement codes ambiguous cells' quadrants
                               (1 B/cell, honest transport) + ambiguity bitmap,
                               zlib-9 per level.
  D  shared-rule refinement  — Koch generator: per-level subdivision sign
                               bits (zlib-9 per level) + flat fill + framing.
                               Home-turf arm: fires ONLY on the koch fixture;
                               decode-exact asserted (same subdivision code
                               as the rasterizer, one shared implementation).
  PNG                        — reference bytes (PIL, same vectors).

Classes (all seeded, deterministic; rasterized from scale-free definitions —
detail emerges with resolution by construction, never by downsampling):
  koch      synthetic Koch-style coastline (analytic D ~ 1.26 on 3 sides)
  realmap   Natural Earth 10m land/water mask (public domain), rasterized
            per resolution from the same polygons (Europe/Atlantic crop)
  alpha     binary sprite mask (blob + circle cutouts) — the MNFX alpha class
  frac      procedural fBm texture (regenerated per scale, exact integers)
  photo     pseudo-photography: repo photocorpus formula, scale-normalized
  noise     uniform random — negative control (no scale structure)

Reported per (class, scale): complete bytes per arm; per class: growth
exponent = OLS slope of log2(bytes) vs log2(width).

Usage: python3 scripts/mnf14_cs1.py [--levels 5 [--scales 64,128,...]]
Output: docs/mnf14-cs1-gate.log (JSON, per-arm rows) — the doc's tables
derive from this log mechanically.
"""
import argparse
import json
import math
import subprocess
import sys
import zlib
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

REPO = Path(__file__).resolve().parent.parent
BIN = REPO / "target" / "release" / "manifold"
OUT = REPO / "docs" / "mnf14-cs1-gate.log"
NE_SHP = Path("/tmp/mnf14/ne/ne_10m_land.shp")  # fetch: naciscdn.org NE 10m land

SCALES = [64, 128, 256, 512, 1024]
CROP = (-25.0, 34.0, 40.0, 72.0)  # lon0, lat0, lon1, lat1


# ---------------------------------------------------------------- fixtures

def koch_signs(order, seed=7):
    """The coastline's ONLY transmitted content: one sign bit per peak,
    drawn level-by-level (breadth-first) from the seeded stream."""
    rng = np.random.default_rng(seed)
    signs = []
    nseg = 4
    for _ in range(order):
        signs.append([1 if rng.random() < 0.5 else 0 for _ in range(nseg)])
        nseg *= 4
    return signs


def koch_polyline(signs):
    """Decoder-side subdivision from the sign stream (shared with the fixture
    rasterizer — this function IS the decoder rule). Axis-aligned base square;
    each segment sprouts a +/-60deg Koch triangle at its third-points."""
    pts = [(0.1, 0.1), (0.9, 0.1), (0.9, 0.9), (0.1, 0.9), (0.1, 0.1)]
    for signs_lvl in signs:
        out = []
        # interior segments: every consecutive pair EXCEPT the closing corner
        n = len(pts) - 1
        for i in range(n):
            p, q = pts[i], pts[i + 1]
            s = signs_lvl[i]
            d = (q[0] - p[0], q[1] - p[1])
            a = (p[0] + d[0] / 3, p[1] + d[1] / 3)
            b = (p[0] + 2 * d[0] / 3, p[1] + 2 * d[1] / 3)
            mx, my = (a[0] + b[0]) / 2, (a[1] + b[1]) / 2
            nx, ny = -d[1], d[0]
            norm = math.hypot(nx, ny) or 1.0
            h = (math.sqrt(3) / 6) * math.hypot(*d) * (1 if s else -1)
            peak = (mx + h * nx / norm, my + h * ny / norm)
            out += [p, a, peak, b]
        out.append(pts[-1])
        # closing segment p4->p0 carries NO peak (sign slot unused) — keep it
        pts = out
    return pts


def koch_mask(size, order=5, seed=7):
    pts = koch_polyline(koch_signs(order, seed))
    img = Image.new("L", (size, size), 0)
    ImageDraw.Draw(img).polygon([(p[0] * size, p[1] * size) for p in pts],
                                fill=255)
    return np.asarray(img, dtype=np.uint8)


def realmap_mask(size, shp_path=NE_SHP, crop=CROP):
    """Rasterize NE 10m land polygons (public domain) at each size from the
    same vectors; PIL clips out-of-frame coordinates per scanline."""
    import shapefile

    lon0, lat0, lon1, lat1 = crop
    sx = size / (lon1 - lon0)
    sy = size / (lat1 - lat0)
    img = Image.new("L", (size, size), 0)
    drw = ImageDraw.Draw(img)
    r = shapefile.Reader(str(shp_path))
    for shp in r.iterShapes():
        ppts = shp.points or []
        parts = list(shp.parts or [0]) + [len(ppts)]
        for a, b in zip(parts[:-1], parts[1:]):
            pr = [(int((x - lon0) * sx), size - 1 - int((y - lat0) * sy))
                  for x, y in ppts[a:b]]
            if len(pr) >= 3:
                drw.polygon(pr, fill=255, outline=255)
    return np.asarray(img, dtype=np.uint8)


def alpha_mask(size, seed=11):
    """Binary sprite mask: rounded blob + circle cutouts."""
    rng = np.random.default_rng(seed)
    yy, xx = np.mgrid[0:size, 0:size].astype(np.float64)
    m = ((xx - 0.45 * size) ** 2 + (yy - 0.50 * size) ** 2) <= (0.38 * size) ** 2
    for _ in range(3):
        kx, ky = rng.uniform(0.25, 0.65, 2) * size
        kr = rng.uniform(0.06, 0.14) * size
        m &= ((xx - kx) ** 2 + (yy - ky) ** 2) > kr**2
    return m.astype(np.uint8) * 255


def fbm_gray(size, seed=5, base=3, octaves=6):
    """fBm texture, regenerated per scale (grid coords normalized to size)."""
    rng = np.random.default_rng(seed)
    acc = np.zeros((size, size), np.float64)
    amp_total = 0.0
    for o in range(octaves):
        cells = base * (2**o)
        amp = 1.0 / (2**o)
        amp_total += amp
        grid = rng.random((cells + 2, cells + 2))
        ys = np.linspace(0, cells, size, endpoint=False)
        xs = np.linspace(0, cells, size, endpoint=False)
        y0, x0 = ys.astype(np.int64), xs.astype(np.int64)
        fy, fx = ys - y0, xs - x0
        v = (grid[np.ix_(y0, x0)] * (1 - fy) * (1 - fx)
             + grid[np.ix_(y0 + 1, x0)] * fy * (1 - fx)
             + grid[np.ix_(y0, x0 + 1)] * (1 - fy) * fx
             + grid[np.ix_(y0 + 1, x0 + 1)] * fy * fx)
        acc += v * amp
    return np.clip((acc / amp_total * 255.0).round(), 0, 255).astype(np.uint8)


def noise_rgb(size, seed=9):
    return np.random.default_rng(seed).integers(0, 256, (size, size, 3),
                                                dtype=np.uint8)


def photo_like(size, seed=3):
    """Repo photocorpus formula (value-noise base, slate palette), applied per
    scale with cells normalized to size — structure constant, pixels grow."""
    g = fbm_gray(size, seed=seed, base=3, octaves=5).astype(np.float64) / 255.0
    grain = np.random.default_rng(seed + 1).normal(0, 2.5, (size, size))
    luma = np.clip(g * 255.0 + grain, 0, 255).astype(np.uint8)
    ramp = [(0.0, (18, 22, 26)), (0.35, (42, 50, 56)), (0.6, (76, 88, 96)),
            (0.85, (126, 138, 146)), (1.0, (186, 196, 202))]
    lut = np.zeros((256, 3), np.uint8)
    for v in range(256):
        t = v / 255.0
        for (t0, c0), (t1, c1) in zip(ramp[:-1], ramp[1:]):
            if t <= t1:
                f = (t - t0) / (t1 - t0)
                lut[v] = [int(a0 + f * (a1 - a0)) for a0, a1 in zip(c0, c1)]
                break
        else:
            lut[v] = ramp[-1][1]
    return lut[luma]  # (size, size, 3) uint8


# ------------------------------------------------------------- arm coders

def zlib9(b):
    return len(zlib.compress(bytes(b), 9))


def arm_a_portfolio(rgb, tag):
    """RELEASE CLI pick — complete file bytes; decode-exact asserted in-loop."""
    src = Path(f"/tmp/mnf14_{tag}.png")
    enc = src.with_suffix(".mnf5")
    dec = src.with_name(src.stem + "_dec.png")
    Image.fromarray(rgb).save(src)
    subprocess.run([BIN, "encode", str(src), "-o", enc], check=True,
                   capture_output=True)
    subprocess.run([BIN, "decode", enc, "-o", dec], check=True,
                   capture_output=True)
    a = np.asarray(Image.open(src).convert("RGB"))
    b = np.asarray(Image.open(dec).convert("RGB"))
    assert np.array_equal(a, b), f"arm A not lossless: {tag}"
    return enc.stat().st_size


def arm_b_pyramid(rgb):
    """Exact multiscale residual pyramid; all transport counted (level 0 raw).
    Decode-exact asserted in-loop: root + E_k reconstructs the input."""
    assert rgb.shape[0] % 4 == 0 and rgb.shape[1] % 4 == 0
    level = rgb.astype(np.int64)
    levels = []
    total = 0
    while min(level.shape[:2]) > 4:
        nxt = level.reshape(level.shape[0] // 2, 2, level.shape[1] // 2, 2,
                            level.shape[2])
        mean = (nxt.sum(axis=(1, 3)) // 4).astype(np.int64)
        up = np.repeat(np.repeat(mean, 2, axis=0), 2, axis=1)
        E = (level - up) % 256
        total += zlib9(E.astype(np.uint8))
        levels.append((level.shape, level[:0], E))  # (shape, _, E stored)
        level = mean
    total += zlib9(level.astype(np.uint8))
    # decoder: root then per-level mean + E
    rec = level
    for (shape, _p, E) in reversed(levels):
        up = np.repeat(np.repeat(rec, 2, axis=0), 2, axis=1)
        rec = (up + E) % 256
        assert rec.shape == shape
    assert np.array_equal(rec.astype(np.uint8), rgb.astype(np.uint8)), \
        "arm B reconstruction inexact"
    return total


def arm_c_boundary(mask):
    """Recursive boundary refinement, binary mask; all transport counted.

    Cell semantics (exact by construction): a parent cell REFINES iff its 4
    quadrants are not all equal; unrefined cells reconstruct as 4 copies of
    the parent value (the encoder's mean rule inverts exactly); refined cells
    send their 4 child bytes raw (zlib over the level's codes takes the
    boundary repetition). Decoder rebuilds through the same tree, asserted."""
    level = mask.astype(np.int64)
    refs = []  # (pshape, amb2d, qvals)
    total = 0
    while level.shape[0] > 4:
        nxt = level.reshape(level.shape[0] // 2, 2, level.shape[1] // 2, 2)
        a, b = nxt[:, 0, :, 0], nxt[:, 0, :, 1]
        c, d = nxt[:, 1, :, 0], nxt[:, 1, :, 1]
        amb2 = (a != b) | (a != c) | (a != d)  # (cells_h, cells_w)
        amb = amb2.reshape(-1)
        # qvals enumerated cell-major (cells_r, v, cells_c, h flattened as
        # (v, cells_c) pairs per cell — reshape must match on BOTH sides)
        qvals = nxt.transpose(0, 2, 1, 3).reshape(-1, 4)[amb].astype(np.uint8)
        total += zlib9(qvals) + zlib9(amb.astype(np.uint8) * 255)
        refs.append(((level.shape[0], level.shape[1]), amb2, qvals))
        level = nxt.sum(axis=(1, 3)) // 4
    total += zlib9(level.astype(np.uint8))
    # decoder: rebuild through the same tree, exact
    rec = level
    for (pshape, amb2, qvals) in reversed(refs):
        up = np.repeat(np.repeat(rec, 2, axis=0), 2, axis=1)
        flat = up.reshape(amb2.shape[0], 2, amb2.shape[1], 2).transpose(
            0, 2, 1, 3).reshape(-1, 4)
        flat[amb2.reshape(-1)] = qvals.astype(np.int64)
        rec = flat.reshape(amb2.shape[0], amb2.shape[1], 2, 2).transpose(
            0, 2, 1, 3).reshape(pshape)
    assert np.array_equal(rec.astype(np.uint8), mask.astype(np.uint8)), \
        "arm C reconstruction inexact"
    return total


def arm_d_koch(size, order=5, seed=7):
    """Shared-rule complete file: per-level sign bits + flat fill + framing.
    Decode-exact asserted: signs -> decoder polyline -> raster -> == fixture."""
    signs = koch_signs(order, seed)
    pts = koch_polyline(signs)               # the DECODER's reconstruction
    img = Image.new("L", (size, size), 0)
    ImageDraw.Draw(img).polygon([(p[0] * size, p[1] * size) for p in pts],
                                fill=255)
    assert np.array_equal(np.asarray(img), koch_mask(size, order, seed)), \
        "arm D rule stream does not reproduce the fixture exactly"
    total = 0
    for lvl in signs:
        total += zlib9(np.packbits(np.array(lvl, np.uint8))) + 2
    return total + 16  # framing: magic + size + fill byte + lengths (4 B)


# ---------------------------------------------------------------- harness

def growth_exp(rows, cls, arm):
    xs = [math.log2(r["width"]) for r in rows
          if r["class"] == cls and r.get(arm)]
    ys = [math.log2(r[arm]) for r in rows
          if r["class"] == cls and r.get(arm)]
    if len(xs) < 2:
        return None
    n = len(xs)
    sx = sum(xs)
    sy = sum(ys)
    n_sxx = sum(x * x for x in xs) * n
    n_sxy = sum(x * y for x, y in zip(xs, ys)) * n
    denom = n_sxx - sx * sx
    return (n_sxy - sx * sy) / denom if denom else None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--levels", type=int, default=5)
    ap.add_argument("--scales", default=",".join(map(str, SCALES)))
    args = ap.parse_args()
    if not BIN.exists():
        print(f"missing {BIN}; run cargo build --release first", file=sys.stderr)
        return 2
    if not NE_SHP.exists():
        print(f"missing {NE_SHP}; fetch ne10m_land.zip from"
              " https://naciscdn.org/naturalearth/10m/physical/ne_10m_land.zip"
              " (public domain) and unzip into /tmp/mnf14/ne/", file=sys.stderr)
        return 2
    scales = [int(s) for s in args.scales.split(",")][: args.levels]

    rows = []
    for S in scales:
        fixtures = {
            "koch": ("mask", koch_mask(S)),
            "realmap": ("mask", realmap_mask(size=S)),
            "alpha": ("mask", alpha_mask(S)),
            "frac": ("gray", fbm_gray(S)),
            "photo": ("rgb", photo_like(S)),
            "noise": ("rgb", noise_rgb(S)),
        }
        for tag, (kind, arr) in fixtures.items():
            rgb = arr if kind == "rgb" else np.repeat(arr[..., None], 3, axis=2)
            row = {"class": tag, "width": S, "height": S}
            # PNG reference bytes (same vectors)
            import io
            buf = io.BytesIO()
            Image.fromarray(rgb).save(buf, format="PNG")
            row["png"] = len(buf.getvalue())
            row["A_portfolio"] = arm_a_portfolio(rgb, f"{tag}_{S}")
            row["B_pyramid"] = arm_b_pyramid(rgb)
            if kind == "mask":
                row["C_boundary"] = arm_c_boundary(arr)
                if tag == "koch":
                    row["D_koch"] = arm_d_koch(S)
            rows.append(row)
            print(f"scale {S:4d} {tag:7s}: A={row['A_portfolio']:7d} "
                  f"B={row['B_pyramid']:7d} C={row.get('C_boundary')} "
                  f"D={row.get('D_koch')} png={row['png']}", flush=True)

    print("\n== growth exponent: slope of log2(bytes) vs log2(width) ==")
    exps = {}
    for cls in ["koch", "realmap", "alpha", "frac", "photo", "noise"]:
        cells = []
        for arm in ["png", "A_portfolio", "B_pyramid", "C_boundary", "D_koch"]:
            e = growth_exp(rows, cls, arm)
            exps[f"{cls}.{arm}"] = e
            if e is not None:
                cells.append(f"{arm}={e:.3f}")
        print(cls, " ".join(cells) if cells else "(no arms)", flush=True)

    OUT.write_text(
        "# MNF14-CS1 gate log (complete serialized bytes per arm; "
        "growth exponents below)\n" + json.dumps(rows, indent=1)
        + "\n\n# growth exponents\n" + json.dumps(exps, indent=1) + "\n")
    print("wrote", OUT)


if __name__ == "__main__":
    main()