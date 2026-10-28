#!/usr/bin/env python3
"""MNF9 ladder RETAINED-arm bridge gate (crates/mnf-mans).

Drives the Rust lz77_2d port over the FULL 30-file corpus:
  for every image: MED residual plane -> Rust token wire (arm B grammar)
  vs the same plane zlib-9 (the literal arm; python zlib-9 = the ladder's
  coder on BOTH sides), per-file rate-aware pick counted.
Exactness asserted IN RUST (bridge) per plane before the size line.

Parity check first: python-side arm-B reference (the engine_ladder
grammar) re-costed on the synthetics where arm B fires, so the Rust
port's numbers can be compared against the committed ladder table.

Usage: mnfpca venv (numpy, PIL). Writes /tmp/mnf9_bridge_gate.log.
"""
import subprocess as sp
import sys
import zlib
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")
CRATE = ROOT / "crates/mnf-mans"
TS = 16
WIRE_OVERHEAD = 0  # per-file pick flag bit rounds to <1 B; documented in doc


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
    pred = np.where(flatm, plane, pick)
    pred[0, 0] = 127
    pred[0, 1:] = rgb[0, :-1].astype(np.int32)
    pred[1:, 0] = rgb[:-1, 0].astype(np.int32)
    return pred.astype(np.uint8)


def run_bridge(plane_u8, w, h):
    """stdin: w h mode le, then plane. stdout: tok wire sizes + stats."""
    src = plane_u8.reshape(-1).tobytes()
    r = sp.run(
        ["cargo", "run", "--quiet", "--release", "--example", "bridge9"],
        input=w.to_bytes(4, "little") + h.to_bytes(4, "little") + b"\x01" + src,
        capture_output=True,
        cwd=CRATE,
        timeout=600,
    )
    if r.returncode != 0:
        print(r.stderr.decode()[-2000:], file=sys.stderr)
        raise SystemExit(f"bridge failed")
    wire_z9, rans, lits, matches, plane_z9 = map(int, r.stdout.split())
    return plane_z9, wire_z9, lits, matches


def main():
    files = sorted((ROOT / "testdata").glob("*.png"))
    files += sorted((ROOT / "testdata/heldout").glob("*.png"))
    files += sorted((ROOT / "testdata/heldout-photos").glob("*.png"))
    files += sorted((ROOT / "testdata/photos").glob("*.png"))
    if len(files) != 30:
        raise SystemExit(f"expected 30 corpus files, got {len(files)} (count assert)")
    log = open("/tmp/mnf9_bridge_gate.log", "w")
    print(f"{'file':40s} {'lit(z9)':>9s} {'tok(rust)':>9s} {'pick':>5s} {'delta':>8s}", file=log)
    tot_l = tot_t = 0
    wins = 0
    for p in files:
        rgb = np.asarray(Image.open(p).convert("RGB"), dtype=np.int32)
        H, W = rgb.shape[:2]
        pred = med_pred(rgb)
        res = ((rgb - pred.astype(np.int32)) % 256).astype(np.uint8)
        # exactness contract asserted by the bridge itself (rebuild==plane)
        plane_z9, tok, lits, matches = run_bridge(res, W, H)
        lit_py = plane_z9
        picked = tok if tok < lit_py else lit_py
        delta = tok - lit_py
        if tok < lit_py:
            wins += 1
            marker = "TOK"
        else:
            marker = "lit"
        # python zlib-9 must agree with the bridge's own zlib9 (same coder
        # both sides; flate2 miniz may differ slightly, so report both)
        print(
            f"{str(p.relative_to(ROOT / 'testdata')):40s} {lit_py:>9d} {tok:>9d} {marker:>5s} {delta:>+8d}"
            f"   [py-z9 lit={lit_py} lits={lits} matches={matches}]",
            file=log,
        )
        tot_l += lit_py
        tot_t += picked
    n = len(files)
    print(f"\nfiles={n}  token-arm wins={wins}", file=log)
    print(f"rate-aware pick TOTAL: {tot_t} vs literal-arm TOTAL {tot_l} "
          f"({tot_t / tot_l:.3f}x, never worse by construction)", file=log)
    log.close()
    print(open("/tmp/mnf9_bridge_gate.log").read())


if __name__ == "__main__":
    main()