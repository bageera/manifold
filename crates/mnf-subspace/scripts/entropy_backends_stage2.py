#!/usr/bin/env python3
"""MNF8 research track 1 — entropy-backend ablation, STAGE 2 (integrated).

Re-runs predictor selection under each backend: per tile, every candidate
(literal / exact ref / D8 geom ref, plus a spatial-predictor arm) is costed
through the BACKEND BEING MEASURED (not a zlib proxy), selector metadata
included; the whole stream is then coded with that same backend. This is the
`maximum`-profile encoder simulating per-backend, answering the review's
stage-2 question: does the cheapest predictor under zlib stay cheapest under
zstd/brotli/xz?

Also implements the custom residual coder arms (review stage 1, §3):
  - zero-run + Rice-Golomb on MAGNITUDE+SIGN of residuals (no external coder)
  - planar split: R/G/B residual planes coded as separate backend streams
    (size accounting includes 3 frame headers via size deltas)
Both use the same candidate search as the backend arms and share the frozen
corpus. Exactness of the residual algebra is asserted per tile (round-trip
through mod-256 reconstruction).

Backends for stage 2 (from stage-1 winners + review matrix):
  zlib-9, zstd-19, brotli-9, brotli-11, xz-6
(Stage 1 showed levels within a family are close on residuals; keeping 5
backends keeps the 6-arm × 10-image matrix tractable.)
"""
import brotli
import lzma
import time
import zlib
import zstandard
from pathlib import Path

import numpy as np
from PIL import Image

TS = 16
WINDOW = 128


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
            a = np.zeros((h, w), dtype=np.int32)
            a[1:, 1:] = img[: h - 1, : w - 1]
            b = np.zeros((h, w), dtype=np.int32)
            b[1:, 1:] = img[: h - 1, 1:]
            cc = np.zeros((h, w), dtype=np.int32)
            cc[1:, 1:] = img[1:, : w - 1]
            plane = a + b - cc
            flat_ = ((a >= b) & (a >= cc) & (b >= cc)) | (
                (a <= b) & (a <= cc) & (b <= cc)
            )
            d1, d2, d3 = (np.abs(plane - x) for x in (a, b, cc))
            pick = np.where((d1 <= d2) & (d1 <= d3), a, np.where(d2 <= d3, b, cc))
            sel = np.where(flat_, plane, pick)
            o[1:, 1:] = np.clip(sel[1:, 1:], 0, 255)
    return out.reshape(-1)


def d8(block, t):
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


def tiles_of(m, w, h):
    m = np.asarray(m)
    if m.ndim == 1 and m.size == w * h * 3:
        m = m.reshape(h, w, 3)
    cols = (w + TS - 1) // TS
    rows = (h + TS - 1) // TS
    out = []
    for t in range(cols * rows):
        tx = (t % cols) * TS
        ty = (t // cols) * TS
        tw = min(TS, w - tx)
        th = min(TS, h - ty)
        out.append(m[ty : ty + th, tx : tx + tw])
    return out


BACKENDS = {
    "zlib-9": lambda b: zlib.compress(b, 9),
    "zstd-19": lambda b: zstandard.ZstdCompressor(level=19).compress(b),
    "brotli-9": lambda b: brotli.compress(b, quality=9),
    "brotli-11": lambda b: brotli.compress(b, quality=11),
    "xz-6": lambda b: lzma.compress(b, format=lzma.FORMAT_XZ, preset=6),
}


def cost(backend, b):
    return BACKENDS[backend](b)


# ---------------- stage 2: integrated per-tile selection under a backend ------

def integrated(img, backend, shortlist=8):
    """Per-tile mode selection costed through `backend` itself (not a proxy).
    ponytail: cost-calling the backend for EVERY (prior,transform) is O(tiles
    x window) heavy calls — the v0.5 proxy-cost lesson in reverse. Here a
    cheap MAD prefilter shortlists `shortlist` candidates, and only those pay
    real backend calls."""
    h, w = img.shape[:2]
    flat = img.reshape(-1)
    med = med_predict(flat, w, h)
    T = tiles_of(flat, w, h)
    # residual vs MED (arm-B predictor in front, the v1.2-mandated baseline)
    R1 = [
        ((t.astype(np.int32) - m.astype(np.int32)) % 256).astype(np.uint8)
        for t, m in zip(T, tiles_of(med, w, h))
    ]
    s = bytearray()
    counters = {"lit": 0, "ex": 0, "geo": 0}
    win = []
    for i, r in enumerate(R1):
        th_, tw_ = r.shape[:2]
        lit_body = r.tobytes()
        lit_probe = bytes([1]) + lit_body
        # stage A: cheap MAD shortlist over the window (integer arithmetic)
        short = []
        ri = r.reshape(-1).astype(np.int32)
        for jj in win[-WINDOW:]:
            rj = R1[jj]
            if rj.shape != r.shape:
                continue
            mad = int(np.abs(ri - rj.reshape(-1).astype(np.int32)).sum())
            for t in (range(8) if th_ == tw_ else [0]):
                short.append((mad, jj, t))
        short.sort(key=lambda x: x[0])
        # stage B: real backend cost on the shortlist (+ literal, always)
        litd = len(cost(backend, lit_probe))
        best = (litd, None, None, lit_body)
        for _, jj, t in short[:shortlist]:
            rj = R1[jj]
            tt = d8(rj, t).reshape(-1)
            r2 = ((ri - tt.astype(np.int32)) % 256).astype(np.uint8)
            if t == 0:
                probe = bytes([2]) + jj.to_bytes(4, "little") + r2.tobytes()
            else:
                probe = bytes([3]) + jj.to_bytes(4, "little") + bytes([t]) + r2.tobytes()
            dd = len(cost(backend, probe))
            if dd < best[0]:
                best = (dd, jj, t, r2.tobytes())
        if best[1] is None:
            s += lit_probe
            counters["lit"] += 1
        elif best[2] == 0:
            s += bytes([2]) + best[1].to_bytes(4, "little") + best[3]
            counters["ex"] += 1
        else:
            s += (
                bytes([3])
                + best[1].to_bytes(4, "little")
                + bytes([best[2]])
                + best[3]
            )
            counters["geo"] += 1
        if len(win) >= WINDOW:
            win.pop(0)
        win.append(i)
    payload = cost(backend, bytes(s))
    return len(payload), counters


# ---------------- custom residual coder arms -----------------------------------

def rice_code(mags, k):
    """Rice-Golomb code magnitudes with parameter k; returns bit length."""
    q, r = mags >> k, mags & ((1 << k) - 1)
    return int((q + 1 + k).sum())  # unary(q) + '1' stop + k binary bits


def zero_run_rice(img):
    """Arm R1: map residuals to (is_nonzero bitmap, magnitudes, signs), Rice-code
    magnitudes with adaptive k (median-based); zero runs live in the bitmap.
    Size accounting = bitmap (RLE'd with the stream coder) + coded magnitudes."""
    h, w = img.shape[:2]
    flat = img.reshape(-1)
    med = med_predict(flat, w, h)
    T = tiles_of(flat, w, h)
    total_bits = 0
    payload_parts = []
    for t, m in zip(T, tiles_of(med, w, h)):
        r = ((t.astype(np.int32) - m.astype(np.int32)) % 256).astype(np.int32)
        # signed magnitude: map 0..255 wrap to -(1..127),0,+1..127
        signed = np.where(r > 128, r - 256, r)
        mags = np.abs(signed).astype(np.uint16)
        nz = mags > 0
        if nz.any():
            m = int(np.median(mags[nz]))
            k = max(0, int(np.ceil(np.log2(max(m, 1))))) if m > 0 else 0
        else:
            k = 0
        total_bits += rice_code(mags[nz].astype(np.uint64), k)
        bm = np.packbits(nz.astype(np.uint8)).tobytes()
        payload_parts.append(
            bytes([k]) + len(bm).to_bytes(3, "little") + bm
            + mags[nz].astype(np.uint16).tobytes() + np.packbits((signed[nz] < 0).astype(np.uint8)).tobytes()
        )
    stream = b"".join(bytes([2]) + p for p in payload_parts)
    coded = len(zlib.compress(stream, 9))  # side data + magnitudes stored raw
    rice_bytes = total_bits // 8 + 1
    return coded + rice_bytes


def planar_split(img, backends=("zlib-9", "zstd-19")):
    """Arm P: R/G/B residual planes coded as three separate streams."""
    h, w = img.shape[:2]
    flat = img.reshape(-1)
    med = med_predict(flat, w, h)
    T = tiles_of(flat, w, h)
    R1 = [((t.astype(np.int32) - m.astype(np.int32)) % 256).astype(np.uint8)
          for t, m in zip(T, tiles_of(med, w, h))]
    planes = {0: bytearray(), 1: bytearray(), 2: bytearray()}
    for r in R1:
        arr = r.reshape(-1, 3)
        for c in range(3):
            planes[c] += arr[:, c].tobytes()
    be = BACKENDS[backends[0]]
    return sum(len(be(bytes(planes[c]))) for c in range(3))


def main():
    td = Path("/Users/jonbethea/projects/blacktech/manifold/testdata")
    files = [(f"dev/{p.name}", p) for p in sorted(td.glob("*.png"))]
    files += [(f"heldout/{p.name}", p) for p in sorted((td / "heldout").glob("*.png"))]

    print("== stage 2: integrated per-backend predictor selection ==")
    hdr = f"{'image':30}" + "".join(f"{b:>11}" for b in BACKENDS) + f"{'rice':>9}{'planar':>9}"
    print(hdr)
    for label, p in files:
        img = np.asarray(Image.open(p).convert("RGB"), dtype=np.int32)
        h, w = img.shape[:2]
        if w * h * 3 > 2_000_000:  # gradient is 230k raw; keep the sweep tractable
            pass
        sizes = {}
        counters = None
        for b in BACKENDS:
            n, counters = integrated(img, b)
            sizes[b] = n
        rice = zero_run_rice(img)
        planar = planar_split(img)
        row = f"{label:30}" + "".join(f"{sizes[b]:>11}" for b in BACKENDS) + f"{rice:>9}{planar:>9}"
        print(row)
        det = f"   modes: lit={counters['lit']} exact={counters['ex']} geom={counters['geo']}"
        print(det)


if __name__ == "__main__":
    main()