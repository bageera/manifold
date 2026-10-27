#!/usr/bin/env python3
"""Win-arms probe on a real cryo-EM plane: reproduce the 10/27 numbers.

Corpus: EMPIAR-10061 averages (one file fetched by fetch_cryoem.sh).
Renders the u8 plane (per-plane min-max), measures:
  zlib-9 base (0.5406 on 10/27 plane_0000 = 30787735/56,947,320)
  vertical delta (0.6226)
  row-LOCO MED (0.6495)
  solid-tile fraction (0.0)
Verdict recorded in docs/dimensional-fold-probe.md: real cryo-EM u8 plane
is detector-noise dominated — the photographic single-image arms don't
transfer; structural wins (slice-rule/phase) still need an EVENT-stack
corpus (EER reader pending). No format item.
"""
import numpy as np, zlib, sys, struct, os

MRC_HDR = 1024

def load_plane(path, nx, ny):
    P = np.fromfile(path, dtype=np.float32, count=nx*ny, offset=MRC_HDR).reshape(ny, nx)
    lo, hi = float(P.min()), float(P.max())
    if hi <= lo:
        raise SystemExit("flat plane")
    return ((P.astype(np.float64) - lo) / (hi - lo) * 255).round().astype(np.uint8)

def med_image(P):
    H, W = P.shape
    R = np.zeros_like(P, dtype=np.uint8)
    R[0, :] = P[0, :]
    R[0, 1:] = ((P[0, 1:].astype(np.int32) - P[0, :-1].astype(np.int32)) % 256).astype(np.uint8)
    for y in range(1, H):
        aa = P[y, :-1].astype(np.int16)
        b = P[y-1, 1:]
        c = P[y-1, :-1]
        pv = np.clip(aa + b - c, 0, 255)
        R[y, 0] = P[y, 0]
        R[y, 1:] = ((P[y, 1:].astype(np.int32) - pv) % 256).astype(np.uint8)
    return R

def main(path):
    h = open(path, "rb").read(MRC_HDR)
    nx, ny, nz, mode = struct.unpack("<iiii", h[0:16])
    dt = {0: np.int8, 1: np.int16, 2: np.float32, 6: np.uint16}[mode]
    P = np.fromfile(path, dtype=dt, count=nx*ny, offset=MRC_HDR).reshape(ny, nx)
    lo, hi = float(P.min()), float(P.max())
    if hi <= lo:
        raise SystemExit("flat plane")
    plane = ((P.astype(np.float64) - lo) / (hi - lo) * 255).round().astype(np.uint8)
    raw = plane.tobytes()
    print(f"plane {nx}x{ny} mode {mode} u8-render")
    b = len(zlib.compress(raw, 9)); print(f"zlib-9: {b} ({b/len(raw):.4f})")
    a = np.zeros_like(plane)
    a[1:, :] = ((plane[1:, :].astype(np.int32) - plane[:-1, :].astype(np.int32)) % 256).astype(np.uint8)
    da = len(zlib.compress(a.tobytes(), 9)); print(f"vertical-delta: {da} ({da/len(raw):.4f})")
    dm = len(zlib.compress(med_image(plane).tobytes(), 9)); print(f"row-LOCO: {dm} ({dm/len(raw):.4f})")
    T = 16
    flat = plane[:(ny//T)*T, :(nx//T)*T].reshape(ny//T, T, nx//T, T).transpose(0, 2, 1, 3).reshape(ny//T, nx//T, T*T)
    solids = int((flat.max(axis=2).astype(np.int16) - flat.min(axis=2) == 0).sum())
    print(f"solid 16x16 tiles: {solids}/{(ny//T)*(nx//T)} ({solids/((ny//T)*(nx//T)):.4f})")

if __name__ == "__main__":
    main(sys.argv[1])
