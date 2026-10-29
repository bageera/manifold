#!/usr/bin/env python3
"""MNF10-G5: identity-sparse portfolio-arm economics (Jon's directive).

The question this gate answers: can approximate template reuse with
compactly-coded sparse corrections earn its own production representation?

Three competing sparse-residual representations, one per arm, ALL with:
  - one identity template per file (aligned 16/32 grid, min-cost or
    most-common discovery — the frozen G4 budget),
  - full per-ref byte accounting in BOTH representations,
  - z19 + z9 containers both measured (smaller charged),
  - the released 1.10.0 CLI output as the baseline.

Arms:
  S1  position+RGB delta : per ref, (u16 x, u16 y, u8 T) header + FULL
  residual plane for the changed pixels (the G4 baseline form, but only
  CORRECTED pixels transmitted, position-listed)
  S2  mask + packed deltas: per ref, header + zlib-compressed CHANGED-PIXEL
  bitmap + correction stream (positions factored through the mask)
  S3  mask + channel-aware deltas: per ref (channels split: R/G/B
  corrections coded per channel stream, per-channel masks)

A fourth form, S4 = S3 with per-channel delta VALUES also packed as their
own stream.
Decision rule (pre-registered): a representation earns the wire only if
its COMPLETE file beats 1.10.0's portfolio on the representative class
fixtures AND the mandatory 30-file corpus control shows zero regressions
(every 1.10.0 candidate stays available to the pick by construction).
"""
import os
import struct
import subprocess
import time
import zlib
from collections import Counter
from pathlib import Path

import numpy as np
from PIL import Image

try:
    import zstandard
except ImportError:
    zstandard = None

ROOT = Path(__file__).resolve().parent.parent.parent.parent


def container919(s):
    a = len(zlib.compress(s, 9))
    if zstandard is not None:
        a = min(a, len(zstandard.ZstdCompressor(level=19, write_checksum=True).compress(s)))
    return a


def cli_size(path):
    env = os.environ.copy()
    env["PATH"] = ("/opt/homebrew/opt/rustup/bin:"
                   + str(Path.home() / ".rustup/toolchains/stable-aarch64-apple-darwin/bin")
                   + ":" + env["PATH"])
    env["RUSTUP_HOME"] = str(Path.home() / ".rustup")
    out = Path("/tmp/g5.mnf5")
    subprocess.run([str(ROOT / "target/release/manifold"), "encode", str(path),
                    "-o", str(out), "--mnf8"], capture_output=True, env=env, check=True)
    return out.stat().st_size


def discover_template(img, T):
    """One identity template per file: most-common exact block; else min-cost
    sampled (the frozen G4 discovery). Returns (tkey, template | None)."""
    h, w = img.shape[:2]
    blocks, keys = {}, []
    for by in range(0, h - T + 1, T):
        for bx in range(0, w - T + 1, T):
            blocks[(by, bx)] = img[by:by + T, bx:bx + T].astype(np.int64)
            keys.append((by, bx))
    sig = {k: blocks[k].tobytes() for k in keys}
    cnt = Counter(sig.values())
    common, n = cnt.most_common(1)[0]
    if n >= 3:
        tkey = [k for k in keys if sig[k] == common][0]
        return tkey, blocks[tkey], blocks, keys
    # min-cost sampled identity-only
    best = None
    for cand in keys[:60]:
        tot = 0
        for k in keys:
            if k == cand:
                continue
            rb = (blocks[k] - blocks[cand]).clip(-255, 255)
            tot += int(np.abs(((blocks[k] - blocks[cand]) % 256 + 128) % 256 - 128).sum())
        if best is None or tot < best[0]:
            best = (tot, cand)
    if best is None:
        return None, None, blocks, keys
    tkey = best[1]
    return tkey, blocks[tkey], blocks, keys


def sparse_refs(img, T, boundary=1.1):
    """Template + references whose per-block sum|resid| < the boundary;
    identity-only. Returns (template record pieces, refs) or None."""
    tkey, template, blocks, keys = discover_template(img, T)
    if template is None:
        return None
    refs = {}
    for k in keys:
        if k == tkey:
            continue
        rb = ((blocks[k] - template) % 256 + 128) % 256 - 128
        cost = int(np.abs(rb).sum())
        if cost < T * T * 3 * boundary:
            refs[k] = rb
    if not refs:
        return None
    return tkey, template, refs


def arm_s1(img, T):
    """S1: full per-ref residual plane (G4's identity+sparse form)."""
    r = sparse_refs(img, T)
    if r is None:
        return None, 0, 0, 0.0
    tkey, template, refs = r
    h, w = img.shape[:2]
    t0 = time.time()
    out = bytearray()
    out.append(7)
    out += struct.pack("<HH", T, len(refs) + 1)
    out += template.astype(np.uint8).tobytes()
    out += struct.pack("<2H", *tkey)
    m = np.zeros((h, w), bool)
    m[tkey[0]:tkey[0] + T, tkey[1]:tkey[1] + T] = True
    for k, rb in refs.items():
        out += struct.pack("<2HB", k[0], k[1], 0)
        out += np.clip(rb, -127, 127).astype(np.int8).tobytes()
        m[k[0]:k[0] + T, k[1]:k[1] + T] = True
    lit = np.where(m[..., None], 0, left_res(img).astype(np.uint8))
    out += lit.tobytes()
    dt = time.time() - t0
    return 32 + container919(bytes(out)), len(refs), 1, dt


def left_res(img):
    h, w = img.shape[:2]
    a = img.astype(np.int64)
    res = np.zeros((h, w, 3), np.int64)
    res[:, 1:] = (a[:, 1:] - a[:, :-1]) % 256
    res[:, 0] = a[:, 0]
    return res % 256


def arm_s2(img, T):
    """S2: per-ref compressed CHANGED-pixel mask + packed correction stream."""
    r = sparse_refs(img, T)
    if r is None:
        return None, 0, 0, 0.0
    tkey, template, refs = r
    h, w = img.shape[:2]
    t0 = time.time()
    out = bytearray()
    out.append(7)
    out += struct.pack("<HH", T, len(refs) + 1)
    out += template.astype(np.uint8).tobytes()
    out += struct.pack("<2H", *tkey)
    m = np.zeros((h, w), bool)
    m[tkey[0]:tkey[0] + T, tkey[1]:tkey[1] + T] = True
    for k, rb in refs.items():
        changed = (rb != 0).any(axis=2)          # (T,T) bool
        mask_bits = np.packbits(changed.reshape(-1))
        deltas = rb[changed]                     # (n_changed, 3) int64
        out += struct.pack("<2HB", k[0], k[1], len(mask_bits))
        out += zlib.compress(mask_bits.tobytes(), 9)
        packed = deltas.astype(np.int8).tobytes()
        out += struct.pack("<I", len(packed))
        out += packed
        m[k[0]:k[0] + T, k[1]:k[1] + T] = True
    lit = np.where(m[..., None], 0, left_res(img).astype(np.uint8))
    out += lit.tobytes()
    dt = time.time() - t0
    return 32 + container919(bytes(out)), len(refs), len(refs), dt


def arm_s3(img, T):
    """S3: channel-aware — per-channel mask + per-channel corrections."""
    r = sparse_refs(img, T)
    if r is None:
        return None, 0, 0, 0.0
    tkey, template, refs = r
    h, w = img.shape[:2]
    t0 = time.time()
    out = bytearray()
    out.append(7)
    out += struct.pack("<HH", T, len(refs) + 1)
    out += template.astype(np.uint8).tobytes()
    out += struct.pack("<2H", *tkey)
    m = np.zeros((h, w), bool)
    m[tkey[0]:tkey[0] + T, tkey[1]:tkey[1] + T] = True
    for k, rb in refs.items():
        out += struct.pack("<2HB", k[0], k[1], 0)
        for c in range(3):
            changed_c = (rb[:, :, c] != 0)
            mask_bits = np.packbits(changed_c.reshape(-1))
            deltas_c = rb[:, :, c][changed_c].astype(np.int8)
            out += struct.pack("<BH", len(mask_bits), len(deltas_c))
            out += zlib.compress(mask_bits.tobytes(), 9)
            out += deltas_c.tobytes()
        m[k[0]:k[0] + T, k[1]:k[1] + T] = True
    lit = np.where(m[..., None], 0, left_res(img).astype(np.uint8))
    out += lit.tobytes()
    dt = time.time() - t0
    return 32 + container919(bytes(out)), len(refs), 2, dt


def main():
    fixtures = [
        ("document", ROOT / "testdata/heldout/document_200x280.png"),
        ("poster", ROOT / "testdata/heldout/poster_208x144.png"),
        ("hero-pair", Path("/tmp/g4_hero_pair.png")),
        ("guard-strip", Path("/tmp/g4_guard_strip.png")),
        ("screenshot", ROOT / "testdata/screenshot_640x400.png"),
        ("gradient", ROOT / "testdata/gradient_320x240.png"),
        ("solid", ROOT / "testdata/solid_256x256.png"),
        ("blob", ROOT / "testdata/heldout/blob_224x160.png"),
        ("plaid", ROOT / "testdata/heldout/plaid_192.png"),
    ]
    print(f"{'fixture':12s} {'A=CLI':>7s} | {'S1':>8s} {'n1':>3s} | {'S2':>8s} "
          f"{'n2':>3s} | {'S3':>8s} {'n3':>3s} | best-delta vs CLI")
    for name, path in fixtures:
        if not path.exists():
            print(f"{name}: missing")
            continue
        img = np.asarray(Image.open(path).convert("RGB"), dtype=np.uint8)
        A = cli_size(path)
        rows = []
        for fn, label in [(arm_s1, "S1"), (arm_s2, "S2"), (arm_s3, "S3")]:
            best = None
            for T in (16, 32):
                r = fn(img, T)
                if r and r[0] and (best is None or r[0] < best[0]):
                    best = (r[0], r[1])
            rows.append(best if best else (None, 0))
        s1, n1 = rows[0]
        s2, n2 = rows[1]
        s3, n3 = rows[2]
        vals = [(v, lab) for v, lab in [(s1, "S1"), (s2, "S2"), (s3, "S3")] if v]
        if vals:
            bv, blab = min(vals)
            delta = f"{bv - A:+d} ({blab})"
        else:
            delta = "no-fire"
        fmt = lambda v: f"{v:>8d}" if v else "     non"
        print(f"{name:12s} {A:>7d} | {fmt(s1):>8s} {n1:>3d} | {fmt(s2):>8s} {n2:>3d} | "
              f"{fmt(s3):>8s} {str(n3):>3s} | {delta}")


if __name__ == "__main__":
    main()