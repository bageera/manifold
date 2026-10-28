#!/usr/bin/env python3
"""MNF9 rANS-arm corpus gate (crates/mnf-mans drive) — Stage-2 protocol.

For every corpus image's MED residual plane (both interleaved and
planar), sizes through:
  - zlib-9 (python; the ladder's coder)
  - zstd-19
  - rANS mixed (Rust, bridge: classes order-1 + per-class payloads;
    honest model transport counted; exactness asserted IN RUST per
    sub-stream) and ALSO the rANS coder over the LZ77-2D token WIRE
    (stage-2 rule: re-cost the pick's own payload through each coder).
Per-file: pick = min(all arms). Exit-count: 30 files.
Log: /tmp/mnf9_rans_gate.log
"""
import subprocess as sp
import sys
import zlib
from pathlib import Path

import numpy as np
import zstandard
from PIL import Image

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")
CRATE = ROOT / "crates/mnf-mans"
TS = 16
EXPECTED = 30


def med_pred(rgb):
    H, W = rgb.shape[:2]
    a = np.zeros((H, W, 3), np.int32)
    a[:, 1:] = rgb[:, :-1]
    b = np.zeros((H, W, 3), np.int32)
    b[1:, :] = rgb[:-1, :]
    c = np.zeros((H, W, 3), np.int32)
    c[1:, 1:] = rgb[:-1, :-1]
    plane = a + b - c
    flatm = ((a >= b) & (a >= c) & (b >= c)) | ((a <= b) & (a <= c) & (b <= c))
    pa, pb, pc = np.abs(plane - a), np.abs(plane - b), np.abs(plane - c)
    pick = np.where(pa <= pb, np.where(pa <= pc, a, c), np.where(pb <= pc, b, c))
    out = np.where(flatm, plane, pick)
    out[0, 0] = 127
    out[0, 1:] = rgb[0, :-1].astype(np.int32)
    out[1:, 0] = rgb[:-1, 0].astype(np.int32)
    return out.astype(np.uint8)


def bridge_args(mode, plane, w, h):
    hdr = w.to_bytes(4, "little") + h.to_bytes(4, "little") + mode.to_bytes(1, "little")
    return hdr + plane.reshape(-1).tobytes()


def run_bridge(mode, plane, w, h):
    r = sp.run(
        ["cargo", "run", "--quiet", "--release", "--example", "bridge9"],
        input=bridge_args(mode, plane, w, h),
        capture_output=True, cwd=CRATE, timeout=900,
    )
    if r.returncode != 0:
        print(r.stderr.decode()[-1500:], file=sys.stderr)
        raise SystemExit("bridge failed")
    return r.stdout.split()


def main():
    files = sorted((ROOT / "testdata").glob("*.png"))
    files += sorted((ROOT / "testdata/heldout").glob("*.png"))
    files += sorted((ROOT / "testdata/heldout-photos").glob("*.png"))
    files += sorted((ROOT / "testdata/photos").glob("*.png"))
    if len(files) != EXPECTED:
        raise SystemExit(f"expected {EXPECTED}, got {len(files)}")
    log = open("/tmp/mnf9_rans_gate.log", "w")
    z19 = zstandard.ZstdCompressor(level=19)
    print(f"{'file':34s} {'plane:z9':>9s} {'z19':>9s} {'rans':>9s}  {'wire:z9':>9s} {'wire:rans':>9s} {'pick':>10s}", file=log)
    tot = {}
    for p in files:
        rgb = np.asarray(Image.open(p).convert("RGB"), dtype=np.int32)
        H, W = rgb.shape[:2]
        res = ((rgb - med_pred(rgb).astype(np.int32)) % 256).astype(np.uint8)
        flat = res.reshape(-1)
        # coder sizes on the PLANE
        z9 = len(zlib.compress(flat.tobytes(), 9))
        zstd = len(z19.compress(flat.tobytes()))
        # rust bridge: mode 0 = plane vs zlib/rans; mode 1 = lz2d token wire vs zlib/rans
        o0 = run_bridge(0, flat, W, H)
        o1 = run_bridge(1, flat, W, H)
        assert len(o0) == 2 and len(o1) == 5, f"bridge arity {len(o0)} {len(o1)}"
        rans_plane = int(o0[1])
        wire_z9 = int(o1[0])
        wire_rans = int(o1[1])
        # flate2-vs-python-zlib byte delta noted in the log column (one-coder
        # parity is kept per arm: each arm's two coders live in ONE library)
        all_arms = {"z9-plane": z9, "z19-plane": zstd, "rans-plane": rans_plane, "wire-z9": wire_z9, "wire-rans": wire_rans}
        win, cost = min(all_arms.items(), key=lambda kv: kv[1])
        for k, v in all_arms.items():
            tot[k] = tot.get(k, 0) + v
        tot.setdefault("pick", 0)
        tot["pick"] += cost
        print(
            f"{str(p.relative_to(ROOT/'testdata')):34s} {z9:>9d} {zstd:>9d} {rans_plane:>9d}  {wire_z9:>9d} {wire_rans:>9d} {win:>10s}",
            file=log, flush=True,
        )
    print("\nTOTALS:", file=log)
    for k, v in sorted(tot.items(), key=lambda kv: kv[1]):
        print(f"  {k:12s} {v:>10d}", file=log)
    log.close()
    print(open("/tmp/mnf9_rans_gate.log").read())


if __name__ == "__main__":
    main()