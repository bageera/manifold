#!/usr/bin/env python
"""G6 — quaternary (nibble-split / DNA-alphabet) representation probe.

Hypothesis under test (Jon, via ACOUSTICS chat): mapping 8-bit pixels into a
4-symbol (quaternary) alphabet as TWO separate planes might expose a smaller
total description — the theory says a restricted alphabet drops fixed-width
bits per value, and "encoding relationships rather than values" beats per-byte
costs. The DNA framing (4 symbols) is a representation relabeling, not a
mechanism by itself; what could be real is the SPLIT: separating high and low
nibbles into two planes can create intra-plane structure (longer constant
columns, cleaner gradients) that a spatial predictor or the coder exploits.

Arms (same coder everywhere — zlib-9, the repo's same-coder-parity rule):
  raw        : plain zlib over the raster (baseline)
  rgba_bytestride : zlib over the 2 planes packed as raw bytes (nibble-split,
                    no predictor — the pure re-layout arm)
  med_both   : spatial predictor (MED/NIPS-style, the repo's strongest
                 in-plane control) ON each nibble plane separately, planes
                 laid out consecutively in one stream
  med_interleave : nibble-split + MED, planes interleaved row-wise (the
                    planar-vs-interleaved question, known content-dependent)
  plain_med  : MED on the ORIGINAL raster (the established winner — what the
                 shipped pick uses; the split must beat THIS to exist)

Every arm bills its description honestly (stream + any headers). The
description-cost of the nibble mapping itself is 0 (fixed, both sides' code —
charged because it IS both sides' code and identical per arm).

Verdict rule: the split beats `plain_med` on the corpus aggregate → retained
as a pick-arm candidate; otherwise → DENIED (mechanism: re-layout is
incidental; the redundancy lives in prediction, an established lesson).

Run: scripts/g6_quaternary_probe.py
Output: stdout table; gate log written to docs/g6-quaternary-gate.log when
G6_GATE_LOG=1 (convention: committed log beside the probe doc).
"""
import os
import sys
import zlib
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent


def load_gray(path: Path) -> np.ndarray:
    """Decode a PNG to a uint8 gray plane (luminance proxy for the experiment;
    RGB channels processed independently where color matters)."""
    img = Image.open(path)
    if img.mode != "RGB":
        img = img.convert("RGB")
    arr = np.asarray(img, dtype=np.uint8)
    # luminance fold — one channel keeps the probe's byte-budget honest
    r, g, b = arr[..., 0].astype(np.int32), arr[..., 1].astype(np.int32), arr[..., 2].astype(np.int32)
    return ((r * 299 + g * 587 + b * 114) // 1000).astype(np.uint8)


def med_predict(plane: np.ndarray) -> np.ndarray:
    """Vectorized MED-style predictor residual (% 256, int32 math) — the
    repo-standard spatial predictor shape, numpy-vectorized."""
    p = plane.astype(np.int32)
    h, w = p.shape
    out = np.empty_like(p)
    # first row: left-only
    out[0, 0] = p[0, 0]
    out[0, 1:] = (p[0, 1:] - p[0, :-1]) % 256
    # first column: vertical-only
    out[1:, 0] = (p[1:, 0] - p[:-1, 0]) % 256
    # interior MED
    a = p[1:, :-1]   # left
    b = p[:-1, 1:]   # up
    c = p[:-1, :-1]  # up-left
    d = p[1:, 1:]    # target
    med = a + b - c
    lo = np.minimum(a, b)
    hi = np.maximum(a, b)
    pred = np.where(med < lo, lo, np.where(med > hi, hi, med))
    out[1:, 1:] = (d - pred) % 256
    return out.astype(np.uint8)


def split_nibbles(plane: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """8-bit pixels → two 4-bit-valued planes (high nibble plane, low nibble
    plane), SAME raster order each — the quaternary relabeling the hypothesis
    proposes."""
    hi = (plane >> 4).astype(np.uint8)
    lo = (plane & 0xF).astype(np.uint8)
    return hi, lo


def zigzag_costs(stream: bytes) -> int:
    """Order-0-ish description: zlib-9 over the framed stream (the shared
    coder; Adler32 inherent to zlib framing)."""
    return len(zlib.compress(stream, 9))


def main() -> None:
    files = sorted((ROOT / "testdata" / "photos").glob("*.png"))
    files += sorted((ROOT / "testdata" / "heldout").glob("*.png"))
    files += sorted((ROOT / "testdata" / "heldout-photos").glob("*.png"))
    assert len(files) == 25, f"corpus shape changed: {len(files)} files"

    rows = []
    total = {k: 0 for k in ["raw", "split_raw", "split_med", "split_med_int", "plain_med"]}
    wins = 0
    for f in files:
        plane = load_gray(f)
        n = plane.size
        hi, lo = split_nibbles(plane)

        arm_raw = zigzag_costs(plane.tobytes())
        arm_split_raw = zigzag_costs(hi.tobytes() + lo.tobytes())
        arm_split_med = zigzag_costs(med_predict(hi).tobytes() + med_predict(lo).tobytes())
        # interleaved rows variant: row-wise interleave of the two predicted planes
        mh, ml = med_predict(hi), med_predict(lo)
        inter = np.empty((mh.shape[0] * 2, mh.shape[1]), dtype=mh.dtype)
        inter[0::2] = mh
        inter[1::2] = ml
        arm_split_inter = zigzag_costs(inter.tobytes())
        arm_plain_med = zigzag_costs(med_predict(plane).tobytes())

        best_split = min(arm_split_raw, arm_split_med, arm_split_inter)
        if best_split < arm_plain_med:
            wins += 1
        for k, v in [
            ("raw", arm_raw),
            ("split_raw", arm_split_raw),
            ("split_med", arm_split_med),
            ("split_med_int", arm_split_inter),
            ("plain_med", arm_plain_med),
        ]:
            total[k] += v
        rows.append(
            (f.name, n, arm_raw, arm_split_raw, arm_split_med, arm_split_inter, arm_plain_med)
        )

    print("file bytes raw split_raw split_med split_inter plain_med")
    for name, n, a, b, c, d2, e in rows:
        print(f"{name} {n} {a} {b} {c} {d2} {e}")
    print(f"TOTALS: raw={total['raw']} split_raw={total['split_raw']} "
          f"split_med={total['split_med']} split_inter={total['split_med_int']} "
          f"plain_med={total['plain_med']} files={len(rows)} split_beats_plain_med={wins}")

    gate = os.environ.get("G6_GATE_LOG")
    if gate:
        out = ["file bytes raw split_raw split_med split_inter plain_med"]
        for name, n, a, b, c, d2, e in rows:
            out.append(f"{name} {n} {a} {b} {c} {d2} {e}")
        out.append(
            f"TOTALS: raw={total['raw']} split_raw={total['split_raw']} "
            f"split_med={total['split_med']} split_inter={total['split_med_int']} "
            f"plain_med={total['plain_med']} files={len(rows)} split_beats_plain_med={wins}"
        )
        logpath = ROOT / "docs" / "g6-quaternary-gate.log"
        logpath.write_text("\n".join(out) + "\n")
        print(f"gate log written: {logpath}")


if __name__ == "__main__":
    main()