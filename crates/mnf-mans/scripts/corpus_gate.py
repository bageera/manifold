#!/usr/bin/env python3
"""Corpus gate for the mixed run/LZ+rANS coder (crates/mnf-mans bridge):
MED-residual streams of the 10-image corpus -> mixed (Rust prototype) vs
zlib-9 (same bridge, same bytes) vs zstd-19 (library). Exactness asserted in
Rust (rebuild(parse) == input) per stream."""
import subprocess as sp
from pathlib import Path

import numpy as np
import zstandard
from PIL import Image

TS = 16
CRATE = Path("/Users/jonbethea/projects/blacktech/manifold/crates/mnf-mans")


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


def stream_med(img):
    h, w = img.shape[:2]
    flat = img.reshape(-1)
    med = med_predict(flat, w, h)
    parts = bytearray()
    for ty in range(0, h, TS):
        for tx in range(0, w, TS):
            th_ = min(TS, h - ty)
            tw_ = min(TS, w - tx)
            m_t = med.reshape(h, w, 3)[ty : ty + th_, tx : tx + tw_]
            x_t = img[ty : ty + th_, tx : tx + tw_]
            rr = ((x_t - m_t) % 256).astype(np.uint8).reshape(-1)
            parts += bytes([1]) + rr.tobytes()
    return bytes(parts)


def main():
    td = Path("/Users/jonbethea/projects/blacktech/manifold/testdata")
    files = sorted(td.glob("*.png")) + sorted((td / "heldout").glob("*.png"))
    print(f"{'image':26} {'mixed':>9} {'zlib9':>9} {'zstd19':>9} {'winner':>10}")
    z19 = zstandard.ZstdCompressor(level=19)
    for p in files:
        img = np.asarray(Image.open(p).convert("RGB"), dtype=np.int32)
        s = stream_med(img)
        r = sp.run(
            ["cargo", "run", "--quiet", "--release", "--example", "bridge"],
            input=s, capture_output=True, cwd=CRATE, timeout=600,
        )
        mixed, zlib9 = map(int, r.stdout.split())
        z = len(z19.compress(s))
        win = min((mixed, "mixed"), (zlib9, "zlib9"), (z, "zstd19"))[1].strip("()'\" ")
        print(f"{p.name:26} {mixed:>9} {zlib9:>9} {z:>9} {win:>10}")


if __name__ == "__main__":
    main()