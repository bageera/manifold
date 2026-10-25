#!/usr/bin/env python3
"""MNF v1.2 Atlas Model Evaluation — predictor ablation (experiment matrix from
the review). Uses numpy for the search; every arm shares framing + zlib -6 +
the same canonical RGB8 pixels; exactness is asserted structurally (decode
identities verified in the Rust crate; here the residual algebra is exact by
construction).

Arms:
  A  no spatial prediction, no atlas           [0][tile]
  B  MED only                                  [1][r1]
  C  MED + exact-reference atlas on residuals  [2][j][r2]
  D  MED + D8 geometric atlas on residuals     [3][j,t][r2]
  E  MED + D8 + unaligned (shifted-window)     [4][j,dx,dy][r2]
"""
import zlib
import os
import numpy as np
from PIL import Image

TS = 16
WINDOW = 128


def plane_predict(rgb, w, h):
    # LoCo plane approximation only: p = a + b - cleft (JPEG-LS flat case global) — the "simple" baseline.
    rgb = np.asarray(rgb, dtype=np.int32).reshape(h, w, 3)
    out = np.zeros((h, w, 3), dtype=np.int32)
    for c in range(3):
        img = rgb[:, :, c]
        o = out[:, :, c]
        o[0, 0] = 127
        if w > 1:
            o[0, 1:] = img[0, : w - 1]
        if h > 1:
            o[1:, 0] = img[: h - 1, 0]
        if w > 1 and h > 1:
            a = np.empty((h, w))
            a[1:, 1:] = img[: h - 1, : w - 1]
            b = np.empty((h, w))
            b[1:, 1:] = img[: h - 1, 1:]
            cc = np.empty((h, w))
            cc[1:, 1:] = img[1:, : w - 1]
            o[1:, 1:] = np.clip(a[1:, 1:] + b[1:, 1:] - cc[1:, 1:], 0, 255)
    return out.reshape(-1)


def med_predict(rgb, w, h):
    rgb = np.asarray(rgb, dtype=np.int32).reshape(h, w, 3)
    out = np.zeros((h, w, 3), dtype=np.int32)
    for c in range(3):
        img = rgb[:, :, c]
        o = out[:, :, c]
        o[0, 0] = 127
        if w > 1:
            o[0, 1:] = img[0, : w - 1]
        if h > 1:
            o[1:, 0] = img[: h - 1, 0]
        if w > 1 and h > 1:
            a = np.empty((h, w))
            a[1:, 1:] = img[: h - 1, : w - 1]
            b = np.empty((h, w))
            b[1:, 1:] = img[: h - 1, 1:]
            cc = np.empty((h, w))
            cc[1:, 1:] = img[1:, : w - 1]
            plane = a + b - cc
            flat = ((a >= b) & (a >= cc) & (b >= cc)) | (
                (a <= b) & (a <= cc) & (b <= cc)
            )
            d1, d2, d3 = (np.abs(plane - x) for x in (a, b, cc))
            pick = np.where((d1 <= d2) & (d1 <= d3), a, np.where(d2 <= d3, b, cc))
            sel = np.where(flat, plane, pick)
            o[1:, 1:] = np.clip(sel[1:, 1:], 0, 255)
    return out.reshape(-1)


def tile_grid(w, h):
    return (w + TS - 1) // TS, (h + TS - 1) // TS


def tiles_2d(mat, w, h):
    # accept (h,w[,3]) or flat (h*w[,3]); normalize to 2-D with trailing 3 channels packed
    mat = np.asarray(mat)
    if mat.ndim == 1:
        if mat.size == w * h * 3:
            mat = mat.reshape(h, w, 3)
        else:
            mat = mat.reshape(h, w)
    cols, rows = tile_grid(w, h)
    out = []
    dims = []
    for t in range(cols * rows):
        tx = (t % cols) * TS
        ty = (t // cols) * TS
        tw = min(TS, w - tx)
        th = min(TS, h - ty)
        out.append(mat[ty : ty + th, tx : tx + tw])
        dims.append((th, tw))
    return out, dims, cols, rows


def d8(block, t):
    # block: (th, tw, 3) integer array; codec-consistent transforms
    th, tw = block.shape[:2]
    if t == 0:
        return block
    if t == 1:
        return block[:, ::-1].copy()
    if t == 2:
        return block[::-1].copy()
    if t == 3:
        return block[::-1, ::-1].copy()
    if t == 4:
        return np.transpose(block, (1, 0, 2)).copy()
    if t == 5:
        return d8(d8(block, 4), 1)
    if t == 6:
        return d8(d8(block, 4), 3)
    if t == 7:
        return d8(d8(d8(block, 4), 3), 1)
    raise ValueError(t)


def zlen(b):
    return len(zlib.compress(bytes(b), 6))


def ablate(path):
    rgb = np.asarray(Image.open(path).convert("RGB"), dtype=np.int32)
    h, w = rgb.shape[:2]
    flat = rgb.reshape(-1)
    med = med_predict(flat, w, h)

    # A: literal tiles
    tiles, dims, cols, rows = tiles_2d(rgb, w, h)
    stream_a = bytearray()
    for t in tiles:
        stream_a += bytes([0])
        stream_a += t.astype(np.uint8).tobytes()
    A = zlen(stream_a)

    # residual layer 1 (spatial): x - med
    r1, _, _, _ = tiles_2d((flat - med) % 256, w, h)  # int in 0..255
    mdims = dims

    # B: MED only
    stream_b = bytearray()
    for r in r1:
        stream_b += bytes([1])
        stream_b += r.astype(np.uint8).tobytes()
    B = zlen(stream_b)

    # B' : plane-only predictor (no edge-sense)
    plane = plane_predict(flat, w, h)
    r1p, _, _, _ = tiles_2d((flat - plane) % 256, w, h)
    stream_bp = bytearray()
    for r in r1p:
        stream_bp += bytes([1])
        stream_bp += r.astype(np.uint8).tobytes()
    BP = zlen(stream_bp)

    # C/D: atlas over residual tiles: r2 = r1 - T_t(r1_j) (mod 256)
    # decoder identity:  x = med + T_t(r1_j) + r2  (T_t^(-1) consistent: T applied to base residual)
    def atlas_stream(geom):
        s = bytearray()
        win = []  # ring: last WINDOW candidates (absolute tile indices)
        for i, ri in enumerate(r1):
            ti, twi = mdims[i]
            ri_flat = ri.reshape(-1).astype(np.int32)
            lit_fallback = bytes([1]) + ri_flat.astype(np.uint8).tobytes()
            if i > 0:
                best = None
                for jj in win[-WINDOW:]:
                    if mdims[jj] != (ti, twi):
                        continue
                    rj = r1[jj].reshape(-1).astype(np.int32)
                    ts_list = range(8) if (geom and ti == twi) else [0]
                    for t in ts_list:
                        tt = d8(rj.reshape(ti, twi, 3), t).reshape(-1).astype(np.int32)
                        r2 = (ri_flat - tt) % 256
                        # zlib-proxy cost of THIS residual candidate (bytes)
                        probe = bytes([3 if t else 2]) + jj.to_bytes(4, "little") + (
                            (bytes([t]) if t else b"") + r2.astype(np.uint8).tobytes()
                        )
                        d = zlen(probe)
                        if best is None or d < best[0]:
                            best = (d, jj, t, r2)
                lit_cost = zlen(lit_fallback)
                if best and best[0] < lit_cost:
                    _, jj, t, r2 = best
                    if t == 0:
                        s += bytes([2]) + jj.to_bytes(4, "little")
                    else:
                        s += bytes([3]) + jj.to_bytes(4, "little") + bytes([t])
                    s += r2.astype(np.uint8).tobytes()
                else:
                    s += lit_fallback
            else:
                s += lit_fallback
            if len(win) >= WINDOW:
                win.pop(0)
            win.append(i)
        return zlen(s)

    C = atlas_stream(geom=False)
    D = atlas_stream(geom=True)
    return {"A": A, "B": B, "BP": BP, "C": C, "D": D, "N": len(r1)}, (w, h)


if __name__ == "__main__":
    import sys

    TD = "/Users/jonbethea/projects/blacktech/manifold/testdata"
    files = sorted(
        os.path.join(TD, f)
        for f in os.listdir(TD)
        if f.endswith(".png") and "expected" not in f and "_rt" not in f
    ) + sorted(
        os.path.join(TD, "heldout", f)
        for f in os.listdir(f"{TD}/heldout")
        if f.endswith(".png")
    )
    print(
        f"{'image':28} {'A':>7} {'B(MED)':>7} {"B'(plane)":>9} {'C':>7} {'D':>7} {'best MNF7':>10}"
    )
    for path in files:
        res, (w, h) = ablate(path)
        mnf7 = None
        try:
            import subprocess

            BIN = "/Users/jonbethea/projects/blacktech/manifold/target/release/manifold"
            r = subprocess.run(
                [BIN, "encode", path, "-o", "/tmp/ab.mnf6", "--mnf6"],
                capture_output=True,
                timeout=600,
            )
            if r.returncode == 0:
                import os

                mnf7 = os.path.getsize("/tmp/ab.mnf6")
        except Exception:
            pass
        row = f"{res['A']:>7} {res['B']:>7} {res['BP']:>9} {res['C']:>7} {res['D']:>7}"
        name = path.split("/")[-2] + "/" + path.split("/")[-1]
        print(f"{name:28} {row} {str(mnf7):>10}")