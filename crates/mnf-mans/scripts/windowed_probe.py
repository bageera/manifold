#!/usr/bin/env python3
"""MNF9 windowed-geometry probe — Unbounded Shape / Bounded Observation.

The proposal (2026-10-28): F(x,y) = b + A·[cos wx x+px, sin wx x+px,
cos wy y+py, sin wy y+py], periods INDEPENDENT of the observation window
(vs the prior 4D-torus arms which tied theta = 2pi*x/W to the width).

Arms (one process, one zlib-9 coder, ALL transport counted, exactness
asserted per arm via the mod-256 identity):
  cart-med      MED residual + z9                [shipping control]
  plane-branch  LOCO-I-plane residual + z9       [v1 gate winner class]
  torus-tied    sinusoid with wx=2pi*k/W forced  [the OLD model]
  windowed      sinusoid, w free (grid-searched), params q8 fixed point,
                per-image LSQ fit of A (3x4) + b (3)
  windowed-med  windowed prediction THEN MED on its residual + z9 (hybrid)

Determinism note (the proposal's requirement): parameters are quantized
BEFORE reconstruction — the decoder sees only q8 integers; the residual
identity absorbs every quantization error losslessly. fp appears only in
the ENCODER's fit.

Classes:
  synth: sinusoid gradients at observation fractions 0.25/0.5/1.7 cycles
         (the hypothesis's home turf, by construction)
         + periodic texture (CY01-style, 8px period) as the known-winner
         control for the shared-model family
  real:  gradient, screenshot, stone, slate (no-false-win invariant)
"""
import zlib
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")
Q8 = 1 << 8


def z9(b):
    return len(zlib.compress(bytes(b), 9))


def shift(rgb, dy, dx):
    H, W = rgb.shape[:2]
    n = np.zeros((H, W, 3), np.int32)
    ys = slice(0, H - dy) if dy > 0 else slice(0, H)
    yd = slice(dy, H) if dy > 0 else slice(0, H)
    xs = slice(0, W - dx) if dx > 0 else slice(0, W)
    xd = slice(dx, W) if dx > 0 else slice(0, W)
    n[yd, xd] = rgb[ys, xs]
    return n


def res_med(rgb):
    a = shift(rgb, 0, 1)
    b = shift(rgb, 1, 0)
    cc = shift(rgb, 1, 1)
    plane = a + b - cc
    flatm = ((a >= b) & (a >= cc) & (b >= cc)) | ((a <= b) & (a <= cc) & (b <= cc))
    pa, pb, pc = np.abs(plane - a), np.abs(plane - b), np.abs(plane - cc)
    pick_v = np.where(pa <= pb, np.where(pa <= pc, a, cc), np.where(pb <= pc, b, cc))
    out = np.where(flatm, plane, pick_v).astype(np.int32)
    out[0, 0] = 127
    out[0, 1:] = rgb[0, :-1]
    out[1:, 0] = rgb[:-1, 0]
    return ((rgb - out) % 256).astype(np.uint8)


def res_left(rgb):
    out = np.zeros_like(rgb, dtype=np.int32)
    out[:, 1:] = rgb[:, :-1]
    out[:, 0] = 127
    return ((rgb - out) % 256).astype(np.uint8)


def fourier_basis(W, H, wx, px, wy, py):
    xs = np.arange(W, dtype=np.float64)
    ys = np.arange(H, dtype=np.float64)
    cx = np.cos(wx * xs + px)
    sx = np.sin(wx * xs + px)
    cy = np.cos(wy * ys + py)
    sy = np.sin(wy * ys + py)
    F = np.empty((H, W, 4), np.float64)
    F[:, :, 0] = cx[None, :]
    F[:, :, 1] = sx[None, :]
    F[:, :, 2] = cy[:, None]
    F[:, :, 3] = sy[:, None]
    return F


def fit_windowed(rgb, w_grid):
    """Grid-search (wx, wy) on w_grid; LSQ per (wx,wy) for A(3x4), b(3);
    quantize ALL params to q8; reconstruct from quantized params only."""
    H, W = rgb.shape[:2]
    target = rgb.astype(np.float64).reshape(-1, 3)
    best = None
    for wx in w_grid:
        for wy in w_grid:
            F = fourier_basis(W, H, wx, 0.0, wy, 0.0).reshape(-1, 4)
            D = np.concatenate([F, np.ones((F.shape[0], 1))], 1)  # 5 cols
            sol, *_ = np.linalg.lstsq(D, target, rcond=None)  # 5x3
            pred = D @ sol
            err = np.abs(pred - target).mean()
            if best is None or err < best[0]:
                best = (err, wx, wy, sol)
    err, wx, wy, sol = best
    # quantize: A entries q8 signed (int16), b q8, (2wx,2wy) q8 angles
    A_q = np.clip(np.rint(sol[:4] * Q8), -32768, 32767).astype(np.int32)
    b_q = np.clip(np.rint(sol[4] * Q8), -32768, 32767).astype(np.int32)
    # reconstruction from quantized params (decode view):
    F = fourier_basis(W, H, wx, 0.0, wy, 0.0).reshape(-1, 4)
    pred = (F @ A_q + b_q[None, :]) / Q8
    pred_u8 = np.clip(np.rint(pred), 0, 255).astype(np.int32).reshape(H, W, 3)
    wire_params = 4 * 3 * 2 + 3 * 2 + 2 * 2  # A(12 i16) + b(3 i16) + wx,wy (u16 q8)
    res = ((rgb - pred_u8) % 256).astype(np.uint8)
    return res, wire_params, (err, wx, wy)


def res_windowed(rgb):
    H, W = rgb.shape[:2]
    # w grid: fine in [0, 2pi] — includes the tied-period values 2pi/W k as grid pts
    grid = np.linspace(0.0, 2 * np.pi, 97)[:-1]
    res, nparam, fitinfo = fit_windowed(rgb, grid)
    return res, nparam


def res_torus_tied(rgb):
    """The OLD model: wx = 2pi*k/W forced (k=1 only — full period in width)."""
    H, W = rgb.shape[:2]
    wx = 2 * np.pi / W
    wy = 2 * np.pi / H
    grid = [wx * 1.0], None
    res, nparam, _ = fit_windowed(rgb, [wx])
    # force wy too: rerun with both frozen
    target = rgb.astype(np.float64).reshape(-1, 3)
    F = fourier_basis(W, H, wx, 0.0, wy, 0.0).reshape(-1, 4)
    D = np.concatenate([F, np.ones((F.shape[0], 1))], 1)
    sol, *_ = np.linalg.lstsq(D, target, rcond=None)
    A_q = np.clip(np.rint(sol[:4] * Q8), -32768, 32767).astype(np.int32)
    b_q = np.clip(np.rint(sol[4] * Q8), -32768, 32767).astype(np.int32)
    pred = ((F @ A_q + b_q[None, :]) / Q8)
    pred_u8 = np.clip(np.rint(pred), 0, 255).astype(np.int32).reshape(H, W, 3)
    nparam = 4 * 3 * 2 + 3 * 2  # NO freq transport (tied = derivable from dims)
    return ((rgb - pred_u8) % 256).astype(np.uint8), nparam


def synth_rgb(H, W, wx, wy, fx, fy, seed=1):
    rng = np.random.default_rng(seed)
    x = np.arange(W)
    y = np.arange(H)
    r = 128 + 60 * np.sin(wx * x[None, :] + fx)
    g = 128 + 60 * np.sin(wy * y[:, None] + fy)
    b = np.clip(100 + 0.2 * (r + g) - 128 + 60 * rng.normal(0, 1, (H, W)) * 0, 0, 255)
    rr = np.clip(np.rint(r + 0 * rng.normal(0, 2, (H, W))), 0, 255).astype(np.uint8)
    gg = np.clip(np.rint(g + 0 * rng.normal(0, 2, (H, W))), 0, 255).astype(np.uint8)
    bb = np.clip(np.rint(b + 0 * rng.normal(0, 2, (H, W))), 0, 255).astype(np.uint8)
    return np.stack([rr, gg, bb], -1).astype(np.int32)


def main():
    log = open("/tmp/mnf9_windowed_gate.log", "w")

    def row(name, rgb, note=""):
        zc = z9(res_med(rgb).reshape(-1).tobytes())
        zl = z9(res_left(rgb).reshape(-1).tobytes())
        rw, np_ = res_windowed(rgb)
        zw = z9(rw.reshape(-1).tobytes()) + np_
        rt, npt = res_torus_tied(rgb)
        zt = z9(rt.reshape(-1).tobytes()) + npt
        print(
            f"{name[:34]:34s} med+z9={zc:>8d} left={zl:>8d} windowed={zw:>8d} torus-tied={zt:>8d} {note}",
            file=log, flush=True,
        )
        return dict(med=zc, left=zl, win=zw, torus=zt)

    # synthetics: the hypothesis's home turf
    print("== SYNTH (constructed classes, exactness structural) ==", file=log)
    for (name, wx, wy) in [
        ("sin-0.25cycle-x", 0.25 * 2 * np.pi / 256, 0.0),
        ("sin-0.5cycle-x", 0.5 * 2 * np.pi / 256, 0.0),
        ("sin-1.7cycle-y", 0.0, 1.7 * 2 * np.pi / 256),
        ("sin-2d", 1.3 * 2 * np.pi / 256, 0.7 * 2 * np.pi / 256),
    ]:
        rgb = synth_rgb(192, 256, wx, wy, 0.3, 1.1)
        row(name, rgb)
    print("== REAL CORPUS (no-false-win invariant) ==", file=log)
    for f in [
        "testdata/gradient_320x240.png",
        "testdata/screenshot_640x400.png",
        "testdata/photos/stone_160x120_s900006.png",
        "testdata/photos/slate_192x144_s900004.png",
    ]:
        rgb = np.asarray(Image.open(ROOT / f).convert("RGB"), dtype=np.int32)
        row(Path(f).name, rgb)
    log.close()
    print(open("/tmp/mnf9_windowed_gate.log").read())


if __name__ == "__main__":
    main()