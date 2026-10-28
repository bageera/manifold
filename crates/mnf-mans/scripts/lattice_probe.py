#!/usr/bin/env python3
"""Lattice probe v2 — faithful implementations + the missing controls.

v1 catch (method-honesty note): the hex arm was DEGENERATE (bl=br=up-left
for the sh=1 variant) — its 'wins' were a left-only predictor on flat
classes, not a staggered-lattice effect. v2 fixes the staggered pair and
adds the controls the verdict needs:
  cart-MED   (a=left, b=up, cc=up-left)           [shipping]
  cart-plane LOCO-I plane branch forced (a+b-cc)  [branch-control]
  hex-MED    (a=left, bl=up-left, br=up) with row-parity stagger —
             on the INTEGER hex raster the pair is (y-1, x-parity_off)
             and (y-1, x-parity_off+1) — same causal structure as cart's
             (b, cc) but ONE COLUMN SHIFTED on odd rows.
  tri-MED    (a=left, b=up, d=up-right)
  left-only  pure left predictor (degenerate control — pins what the
             v1 hex arm actually measured)
"""
import zlib
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")


def z9(b):
    return len(zlib.compress(bytes(b), 9))


def shift(rgb, dy, dx):
    """n[y,x] = rgb[y-dy, x-dx] for dy,dx >= 0 causal offsets (dx<0: x-dx
    means to the RIGHT of x? convention: candidate AT (y-dy, x-dx);
    dx negative = column to the right)."""
    H, W = rgb.shape[:2]
    n = np.zeros((H, W, 3), np.int32)
    yy, xx = -dy, -dx
    # n[y,x] = rgb[y+yy, x+xx]; valid where 0<=y+yy<H, 0<=x+xx<W
    ys = slice(max(0, -yy), H) if yy < 0 else slice(0, H - yy)
    xs = slice(max(0, -xx), W) if xx < 0 else slice(0, W - xx)
    yd = slice(max(0, yy), H) if yy > 0 else slice(0, H + yy)
    xd = slice(max(0, xx), W) if xx > 0 else slice(0, W + xx)
    n[yd, xd] = rgb[ys, xs]
    return n


def med_residual(rgb, cands, plane_only=False):
    """cands: [(dy, dx)] causal — first three drive the MED machinery."""
    H, W = rgb.shape[:2]
    a = shift(rgb, *cands[0])
    b = shift(rgb, *cands[1])
    c = shift(rgb, *cands[2])
    if plane_only:
        out = np.clip(a + b - c, 0, 255).astype(np.int32)
    else:
        plane = a + b - c
        flatm = ((a >= b) & (a >= c) & (b >= c)) | ((a <= b) & (a <= c) & (b <= c))
        pa, pb, pc = np.abs(plane - a), np.abs(plane - b), np.abs(plane - c)
        pick = np.where(pa <= pb, np.where(pa <= pc, a, c), np.where(pb <= pc, b, c))
        out = np.where(flatm, plane, pick).astype(np.int32)
    # edges: rows above missing → left; cols left missing → above
    if cands[1][0] > 0:
        out[0, 1:] = rgb[0, :-1]
        out[0, 0] = 127
    if cands[0] == (0, 1):
        out[1:, 0] = rgb[:-1, 0]
    else:
        out[:, 0] = rgb[:, 0]
    return ((rgb - out) % 256).astype(np.uint8)


def hex_staggered(rgb):
    """Faithful hex: stagger = (y % 2) columns; causal pair above pixel
    (y,x): (y-1, x-1+stagger) and (y-1, x+stagger). Implemented per-row.
    MED machinery over (left, above-pair)."""
    H, W = rgb.shape[:2]
    res = np.zeros((H, W, 3), np.uint8)
    a_full = np.zeros((H, W, 3), np.int32)
    a_full[:, 1:] = rgb[:, :-1]
    bl = np.zeros((H, W, 3), np.int32)
    br = np.zeros((H, W, 3), np.int32)
    for y in range(1, H):
        st = y % 2
        # above-pair for row y: columns shifted by st
        # bl = rgb[y-1, x-1+st]; br = rgb[y-1, x+st]
        src = rgb[y - 1]
        bly = np.zeros(W, int)
        bry = np.zeros(W, int)
        bl[y, st:, :] = src[: W - st]
        br[y, st:, :] = src[: W - st]  # placeholder — real pair below
        # true: bl[y,x] = src[(x-1+st) % W-ish]: clamp instead of wrap (causal)
        if st == 0:
            bl[y, 1:] = src[:-1]
            br[y, :] = src[:]
        else:
            bl[y, :] = src[:]
            br[y, 1:] = src[:-1]
    plane = a_full + bl - br
    flatm = ((a_full >= bl) & (a_full >= br) & (bl >= br)) | (
        (a_full <= bl) & (a_full <= br) & (bl <= br))
    pa, pb, pc = np.abs(plane - a_full), np.abs(plane - bl), np.abs(plane - br)
    pick = np.where(pa <= pb, np.where(pa <= pc, a_full, br), np.where(pb <= pc, bl, br))
    out = np.where(flatm, plane, pick).astype(np.int32)
    out[0, 1:] = rgb[0, :-1]
    out[0, 0] = 127
    out[1:, 0] = rgb[:-1, 0]
    return ((rgb - out) % 256).astype(np.uint8)


def main():
    files = sorted((ROOT / "testdata").glob("*.png"))
    files += sorted((ROOT / "testdata/heldout").glob("*.png"))
    files += sorted((ROOT / "testdata/heldout-photos").glob("*.png"))
    files += sorted((ROOT / "testdata/photos").glob("*.png"))
    log = open("/tmp/mnf9_lattice_gate.log", "w")
    print(
        f"{'file':26s} {'cart-med':>9s} {'cart-plano':>10s} {'left-only':>9s} {'tri-med':>8s} {'hex-med':>8s}",
        file=log,
    )
    t = dict.fromkeys(["cart", "cartplane", "left", "tri", "hex"], 0)
    CART = [(0, 1), (1, 0), (1, 1)]
    TRI = [(0, 1), (1, 0), (1, -1)]  # d = up-RIGHT: dx=-1 in shift convention
    for p in files:
        rgb = np.asarray(Image.open(p).convert("RGB"), dtype=np.int32)
        zc = z9(med_residual(rgb, CART).reshape(-1).tobytes())
        zcp = z9(med_residual(rgb, CART, plane_only=True).reshape(-1).tobytes())
        zl = z9(med_residual(rgb, [(0, 1), (1, 0), (1, 0)], plane_only=True).reshape(-1).tobytes())
        zt = z9(med_residual(rgb, TRI).reshape(-1).tobytes())
        zh = z9(hex_staggered(rgb).reshape(-1).tobytes())
        for k, v in zip(t, (zc, zcp, zl, zt, zh)):
            t[k] += v
        print(
            f"{str(p.relative_to(ROOT/'testdata'))[:26]:26s} {zc:>9d} {zcp:>10d} {zl:>9d} {zt:>8d} {zh:>8d}",
            file=log, flush=True,
        )
    print("\nTOTALS:", t, file=log)
    print(f"  hex-vs-cart: {t['hex'] - t['cart']:+d}", file=log)
    print(f"  tri-vs-cart: {t['tri'] - t['cart']:+d}", file=log)
    print(f"  cart-plane vs cart-med: {t['cartplane'] - t['cart']:+d}", file=log)
    print(f"  left-only vs cart-med: {t['left'] - t['cart']:+d}", file=log)
    log.close()
    print(open("/tmp/mnf9_lattice_gate.log").read())


if __name__ == "__main__":
    main()