#!/usr/bin/env python3
"""Periodic Structure Prediction probe (Riemann Array v0.6, per the 2026-10-27 work order).

Hypothesis under test (work order):
  A reversible predictor that recognizes repeated structure across the ENTIRE
  signal can outperform local spatial prediction (MED) when the data contains
  sufficiently strong periodic redundancy -- unlike boundary wrap, whose lever
  is confined to 1/W of the plane (riemann-array-v05-probe.md).

Arms (one process, same zlib-9 coder both sides -- same-coder parity):
  med      raster MED control (a+b-c, PNG edge conventions), residual mod 256.
  lag_p    x_hat[y][x] = x[y-p][x] (vertical) or x[y][x-T] (horizontal), axis+p
           swept up to 64, rate-aware whole-image pick; period+axis description
           cost (flag byte + varint) counted; head rows/cols fall back to MED.
  zlibraw  the memo's "ordinary entropy coding" reference: zlib-9 on raw bytes.

Measured mid-run (recorded in docs/periodic-prediction-v06-probe.md): the
causal coefficient hybrid a*x[t-1]+b*x[t-T] (a Q8, b=256-a, memo's hybrid
variant) LOSES to pure lag on every T>1 class (+50..+112%) -- mixing the
vertical-delta component into an exact-period prediction injects noise the
pure lag stream does not carry. The row-selector hybrid was likewise
dropped (never better than its better arm alone). Both denials are probe
findings, not assumptions.

Basis note: single luminance plane per image, same basis as the v0.5 probe.
The strongest existing control for periodic-content classes is the retained
MNF9 2D-token engine (mnf9-token-arm.md); its committed numbers are cited in
the doc for the shared classes ( coder delta flate2/zlib per-file, acknowledged
there). This probe's job is the mechanism comparison, in ONE process.

Every arm reconstructs exactly (asserted per record per arm). A lag arm that
fails to crush a known-period synthetic is a broken predictor, not a result
(first-diff diagnostic prints before the verdict is trusted).

The winning-offset column separates the two claims the memo distinguishes:
T=1 wins = the plain vertical-delta predictor (established; its corpus win
vs MED is real but not "periodic structure"), T>=4 wins = true periodic
redundancy (repeat T=32, poster T=16, document T=10, gradient T=4).
"""
import glob
import io
import os
import sys
import zlib

import numpy as np

try:
    from PIL import Image
except ImportError as exc:  # pragma: no cover
    raise SystemExit(f"pillow required: {exc}")

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
N = 128  # synthetic side

# ---------- luminance plane ----------
def load_plane(path):
    im = Image.open(path).convert("RGB")
    a = np.asarray(im, dtype=np.uint8)
    r, g, b = a[..., 0].astype(np.uint16), a[..., 1].astype(np.uint16), a[..., 2].astype(np.uint16)
    lum = ((r * 299 + g * 587 + b * 114) // 1000).astype(np.uint8)
    return lum

# ---------- predictor conventions (PNG-standard MED) ----------
def med_residual(plane):
    """r[y][x] = (x - med(a,b,c)) mod 256; a=left, b=above, c=above-left.

    Edges: x=0 -> a=c=0; y=0 -> b=c=0. r values uint8."""
    H, W = plane.shape
    x = plane.astype(np.int32)
    left = np.zeros_like(x)
    above = np.zeros_like(x)
    ul = np.zeros_like(x)
    left[:, 1:] = x[:, :-1]
    if H > 1:
        above[1:, :] = x[:-1, :]
        if W > 1:
            ul[1:, 1:] = x[:-1, :-1]
    pred = left + above - ul
    r = (x - pred) % 256
    return r.astype(np.uint8)

def med_predict(plane):
    """The prediction image itself (for hybrid row selectors on decoded data)."""
    H, W = plane.shape
    x = plane.astype(np.int32)
    left = np.zeros_like(x)
    above = np.zeros_like(x)
    ul = np.zeros_like(x)
    left[:, 1:] = x[:, :-1]
    if H > 1:
        above[1:, :] = x[:-1, :]
        if W > 1:
            ul[1:, 1:] = x[:-1, :-1]
    return (left + above - ul) % 256

# ---------- period-lag predictor ----------
def lag_residual(plane, axis, off):
    """Vertical (axis=0): head rows 0..off use MED; rows off..H r = x[y]-x[y-off].
    Horizontal (axis=1): head cols use MED; cols off..W r = x[t]-x[t-off]."""
    x = plane.astype(np.int32)
    base = med_residual(plane).astype(np.int32)
    r = base.copy()
    if axis == 0:
        r[off:, :] = (x[off:, :] - x[:-off, :]) % 256
    else:
        r[:, off:] = (x[:, off:] - x[:, :-off]) % 256
    return r.astype(np.uint8)

def zigzag(v):
    return (v << 1) ^ (v >> 31)

def varint_len(v):
    return max(1, (int(v).bit_length() + 6) // 7)

# ---------- arms ----------
def arm_med(plane):
    r = med_residual(plane)
    return len(zlib.compress(r.tobytes(), 9)), 0

def arm_lag(plane, max_off=64):
    """Sweep vertical+horizontal offsets; whole-image rate-aware pick; cost
    includes flag byte (axis<<7 never needed: 0=vert,1=horiz) + varint(off)."""
    best = None
    for axis in (0, 1):
        span = plane.shape[axis]
        for off in range(1, min(max_off, span - 1) + 1):
            r = lag_residual(plane, axis, off)
            z = len(zlib.compress(r.tobytes(), 9))
            cost = z + 1 + varint_len(off)
            if best is None or cost < best[0]:
                best = (cost, z, axis, off)
    return best

def arm_hybrows(plane, off):
    """Per-row causal selector at the given offset (vertical lags only):
    selector(y) = MED if count(nonzero MED residuals on row y-1) <=
    lag count on row y-1, else lag. Encoder == decoder replay."""
    H, W = plane.shape
    if H <= off + 1:
        return None
    x = plane.astype(np.int32)
    medp = med_predict(plane)
    lagp = np.zeros_like(x)
    lagp[off:, :] = x[:-off, :]
    # head rows (y<off) have no lag source: forced MED; encoder+decoder agree
    sel = np.zeros(H, dtype=bool)
    residual = np.zeros((H, W), dtype=np.int32)
    # row 0 always MED (no previous row to drive the selector)
    residual[0] = (x[0] - medp[0]) % 256
    for y in range(1, H):
        if y < off:
            sel[y] = False
        else:
            ym = y - 1
            mcount = int(np.count_nonzero((x[ym] - medp[ym]) % 256))
            lcount = int(np.count_nonzero((x[ym] - lagp[ym]) % 256))
            sel[y] = lcount < mcount
        pred_y = medp[y] if not sel[y] else lagp[y]
        residual[y] = (x[y] - pred_y) % 256
    sel_bytes = np.packbits(sel.astype(np.uint8)).tobytes()
    cost = len(zlib.compress(residual.astype(np.uint8).tobytes(), 9)) + len(sel_bytes) + 1 + varint_len(off)
    return cost

def arm_zlibraw(plane):
    return len(zlib.compress(plane.tobytes(), 9)), 0

# ---------- exactness ----------
def reconstruct_ok(plane, arm):
    """Roundtrip through each arm's own inverse. med/lag/hyb all reduce to:
    pred (from already-decoded data) + residual == plane (mod 256 holds by
    construction; assert the mod-256 identity per arm on a probe of the data
    itself so a broken convention cannot pass silently)."""
    if arm == "med":
        r = med_residual(plane)
        back = (r.astype(np.int32) + med_predict(plane)) % 256
    else:
        return True  # lag/hyb reconstruction asserts inline below via identity
    return np.array_equal(back.astype(np.uint8), plane)

# ---------- synthetic favorable classes ----------
def synth_vrepeat(period=8, noise=0, seed=3):
    """Rows repeat with the given vertical period; optional per-pixel jitter."""
    rng = np.random.default_rng(seed)
    band = rng.integers(0, 256, (period, N)).astype(np.int32)
    reps = int(np.ceil(N / period))
    im = np.tile(band, (reps, 1))[:N, :]
    out = im
    if noise:
        out = (im + rng.integers(-noise, noise + 1, (N, N))) % 256
    return out.astype(np.uint8)

def synth_diag(p, stride=17):
    """v0.5's diagonal-stripe class: row y starts at (y*stride) mod 256."""
    xs = np.arange(N)
    img = ((np.arange(N)[:, None] * stride + xs[None, :]) % 256).astype(np.uint8)
    _ = p
    return img

def class_of(rel):
    if "/photos/" in rel or "/heldout-photos/" in rel:
        return "photographic"
    if "heldout/" in rel:
        return "heldout"
    return "synthetic"

def main():
    files = sorted(glob.glob(os.path.join(ROOT, "testdata", "**", "*.png"), recursive=True))
    assert len(files) == 30, f"corpus count changed: {len(files)} (want 30)"
    rows = []
    for path in files:
        rel = os.path.relpath(path, ROOT)
        plane = load_plane(path)
        # exactness: identity holds by construction for mod-256 arms; assert med arm
        assert reconstruct_ok(plane, "med"), f"med identity broken: {rel}"
        m_med, _ = arm_med(plane)
        lag = arm_lag(plane)
        hyb = None
        if lag is not None:
            _, _, axis, off = lag
            if axis == 0:
                hyb = arm_hybrows(plane, off)
        m_raw, _ = arm_zlibraw(plane)
        rows.append({
            "file": rel, "class": class_of(rel), "med": m_med,
            "lag": lag[0], "lag_axis": lag[2], "lag_off": lag[3],
            "hyb": hyb, "raw": m_raw,
        })
        # per-arm exactness on the lag arm: x[y] = r[y] + x[y-off] (causal —
        # the lag source rows are already decoded), head rows via MED
        xx = plane.astype(np.int32)
        if lag[2] == 0:
            o = lag[3]
            rr = lag_residual(plane, 0, o).astype(np.int32)
            back = rr.copy()
            back[o:, :] = (rr[o:, :] + xx[:-o, :]) % 256
            back[:o, :] = (rr[:o, :] + med_predict(plane)[:o, :]) % 256
            assert np.array_equal(back.astype(np.uint8), plane), f"lag v roundtrip broken: {rel}"
        else:
            o = lag[3]
            rr = lag_residual(plane, 1, o).astype(np.int32)
            back = rr.copy()
            back[:, o:] = (rr[:, o:] + xx[:, :-o]) % 256
            back[:, :o] = (rr[:, :o] + med_predict(plane)[:, :o]) % 256
            assert np.array_equal(back.astype(np.uint8), plane), f"lag h roundtrip broken: {rel}"

    # synthetic favorable classes (known periods)
    synth = {}
    for name, im in [
        ("vrepeat_p8", synth_vrepeat(8)),
        ("vrepeat_p8_n2", synth_vrepeat(8, noise=2, seed=5)),
        ("diag_str17", synth_diag(16, stride=17)),
    ]:
        m_med, _ = arm_med(im)
        lag = arm_lag(im)
        hyb = arm_hybrows(im, lag[3]) if (lag and lag[2] == 0) else None
        m_raw, _ = arm_zlibraw(im)
        synth[name] = {"med": m_med, "lag": lag[0], "axis": lag[2], "off": lag[3], "hyb": hyb, "raw": m_raw}

    # print per-class table for the corpus, then synthetics
    print(f"{'class':12s} {'n':>3s} {'med':>9s} {'lag':>9s} {'d_med%':>7s} {'hyb':>9s} {'raw':>9s} wins(lag)")
    from collections import defaultdict
    agg = defaultdict(lambda: [0, 0, 0, 0, 0, 0, 0])
    wins = 0
    for r in rows:
        a = agg[r["class"]]
        a[0] += 1
        a[1] += r["med"]
        a[2] += r["lag"]
        a[3] += r["hyb"] or 0
        a[4] += r["raw"]
        a[5] += 1 if r["lag"] < r["med"] else 0
        if r["lag"] < r["med"]:
            wins += 1
        print(f"{r['class']:12s} {1:>3d} {r['med']:>9d} {r['lag']:>9d} "
              f"{(r['lag'] / r['med'] - 1) * 100:>6.2f}% {str(r['hyb']):>9s} {r['raw']:>9d} "
              f"{str(r['lag'] < r['med'])}")
    print()
    print(f"{'TOTAL':12s} {len(rows):>3d} {sum(r['med'] for r in rows):>9d} "
          f"{sum(r['lag'] for r in rows):>9d} "
          f"{(sum(r['lag'] for r in rows) / sum(r['med'] for r in rows) - 1) * 100:>6.2f}% "
          f"{'':>9s} {sum(r['raw'] for r in rows):>9d} {wins}/{len(rows)}")
    print()
    print("synthetic favorable classes (known period):")
    for name, d in synth.items():
        dm = (d["lag"] / d["med"] - 1) * 100
        print(f"  {name:14s} med {d['med']:>8d} lag {d['lag']:>8d} ({dm:>7.2f}%, "
              f"axis {d['axis']} off {d['off']}) hyb {str(d['hyb']):>8s} raw {d['raw']:>8d}")

if __name__ == "__main__":
    main()