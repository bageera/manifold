#!/usr/bin/env python3
"""MNF11-X1: Mathematical Program Discovery (moonshot lab, experiment 1).

Bounded deterministic integer expression language; per-channel program
search; every candidate accounted at COMPLETE serialized cost
(program bytes + params + residual container + 32-B framing) vs the
RELEASED 1.10.0 CLI's own pick (tokens/near/branch/MNF9 included).

Language (deterministic integer-only, values wrapped mod 256 per node):
  LIN(k)          = ((a*k + b) // d) % 256                  [4 B]
  REC(k)          = ((x0 + add*k) % m)                      [4 B]
  SIN(k)          = (center + LUT[(step*k + phase) % 255] * amp // 255) % 256  [5 B]
  LAT(x,y)        = (ax*x + ay*y) % m                       [4 B]
  MOD(u)          = u % m                                   [+1 B]
  REP(u)          = inner evaluated at k % p                [+1 B]
  SC(u)           = inner evaluated at ((k*num)//den)       [+2 B]
  ADD(u, CONST c) = (u + c) % 256                           [+1 B]
  SIN LUT = decoder-defined 256-entry integer table (no wire cost).
  SKIPPED: MUL, SELECT (no motivating fixture; MNF9 owns piecewise/SELECT).
  SKIPPED: CONST (the shipped MNF9 const arm = 36 B; nothing can beat it).

Screen: per-candidate marginal entropy of the residual (a bound: may miss
slow-burning winners, never inflates a win). Finalists are container-
measured; the complete-file number is the only promoted quantity.

Self-check: demo() asserts the interpreter's arithmetic and residual
reversibility on a couple of known cases.
"""
import io
import os
import struct
import subprocess
import sys
import zlib
from math import log2
from pathlib import Path

import numpy as np
from PIL import Image

try:
    import zstandard
except ImportError:
    zstandard = None

ROOT = Path(__file__).resolve().parent.parent.parent.parent
SIN_LUT = np.array([round(127.5 + 127.5 * np.sin(2 * np.pi * i / 256)) % 256
                    for i in range(256)], np.int64)

_A = list(range(32)) + [48, 64, 96, 128, 192, 255]
_B = [0, 1, 2, 3]
_D = [1, 2, 3, 4, 5, 6, 8, 12, 16, 24, 32]
_M = [2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64, 96, 128, 192, 255]

PROG = {"lin": 4, "rec": 4, "sin": 5, "lat": 4, "mod": 1, "rep": 1, "sc": 2, "add": 1}


def container(b):
    b = bytes(b)
    a = len(zlib.compress(b, 9))
    if zstandard is not None:
        a = min(a, len(zstandard.ZstdCompressor(level=19, write_checksum=True).compress(b)))
    return a


def entropy(vals):
    cnt = np.bincount(vals.reshape(-1), minlength=256).astype(np.float64)
    p = cnt[cnt > 0] / vals.size
    return float(-(p * np.log2(p)).sum())


def eval_fam(fam, prm, kk, x, y):
    """kk = the wrap axis (1-D index grid); returns recon (H,W) int64."""
    h, w = x.shape
    if fam == "lin":
        a, b, d = prm
        base = ((a * kk + b) // d) % 256
    elif fam == "rec":
        x0, add, m = prm
        base = (x0 + add * kk) % m
    elif fam == "sin":
        step, phase, amp, center = prm
        base = (center + (SIN_LUT[(step * kk + phase) % 256] * amp) // 255) % 256
    elif fam == "lat":
        ax, ay, m = prm
        base = (ax * x + ay * y) % m
    elif fam == "mod":
        inner, m = prm
        base = eval_fam(inner[0], inner[1], kk, x, y) % m
    elif fam == "rep":
        inner, p = prm
        base = eval_fam(inner[0], inner[1], kk % p, x, y)
    elif fam == "sc":
        inner, num, den = prm
        base = eval_fam(inner[0], inner[1], (kk * num) // den, x, y)
    elif fam == "add":
        e1, e2 = prm
        base = eval_fam(e1[0], e1[1], kk, x, y) + eval_fam(e2[0], e2[1], kk, x, y)
    else:
        raise ValueError(fam)
    return base % 256


def prog_bytes(fam, prm):
    if fam in ("mod",):
        return PROG[fam] + prog_bytes(prm[0][0], prm[0][1])
    if fam in ("rep", "sc", "add"):
        return PROG[fam] + prog_bytes(prm[0][0], prm[0][1])
    return PROG[fam]


def show(fam, prm):
    if fam == "lin":
        return f"lin(k)=({prm[0]}*k+{prm[1]})//{prm[2]}"
    if fam == "rec":
        return f"rec(k)=({prm[0]}+{prm[1]}*k)%{prm[2]}"
    if fam == "sin":
        return f"sin(k)={prm[3]}+LUT[({prm[0]}k+{prm[1]})%256]*{prm[2]}//255"
    if fam == "lat":
        return f"lat=({prm[0]}x+{prm[1]}y)%{prm[2]}"
    if fam == "mod":
        return f"{show(prm[0][0], prm[0][1])}%{prm[1]}"
    if fam == "rep":
        return f"{show(prm[0][0], prm[0][1])}[k%{prm[1]}]"
    if fam == "sc":
        return f"{show(prm[0][0], prm[0][1])}[(k*{prm[1]})//{prm[2]}]"
    if fam == "add":
        return f"({show(prm[0][0], prm[0][1])}+{show(prm[1][0], prm[1][1])})"
    return str(fam)


def wrap_lin_grids(a_hi=15, b_hi=1, d_list=(1, 2, 4, 8)):
    return (range(a_hi + 1), range(b_hi + 1), list(d_list) if isinstance(d_list, tuple) else d_list)


def families(kk, x, y, W, H):
    """Yield (fam, params, recon). kk = the wrap index grid."""
    k = kk
    # lin, both axes
    for axis_name, grid in (("x", x), ("y", y)):
        for a in _A:
            for b in _B:
                for d in _D:
                    yield ("lin", (a, b, d), ((a * grid + b) // d) % 256)
        # rec (short grids)
        for x0 in range(8):
            for add in range(1, 10):
                for m in (16, 32, 64, 96, 128, 192, 255):
                    yield ("rec", (x0, add, m), (x0 + add * grid) % m)
        # sin
        for step in (1, 2, 3, 4, 5, 6, 7, 8, 12, 16):
            for phase in (0, 64, 128, 192):
                for amp in (128, 255):
                    for center in (64, 128, 192):
                        yield ("sin", (step, phase, amp, center),
                               (center + (SIN_LUT[(step * grid + phase) % 256] * amp) // 255) % 256)
        if axis_name == "x":
            # 2-D lattice
            for ax in range(12):
                for ay in range(12):
                    for m in (3, 4, 6, 8, 12, 16, 24, 32, 48, 64, 128, 255):
                        yield ("lat", (ax, ay, m), (ax * x + ay * y) % m)
            # wraps over lin(x)
            aa = list(range(16))
            for a in aa:
                for b in (0, 1):
                    for d in (1, 2, 3, 4, 6, 8):
                        core = ("lin", (a, b, d))
                        for m in (2, 3, 4, 6, 8, 12):
                            yield ("mod", (core, m), (eval_lin(a, b, d, k) % m))
                        for p in (2, 3, 4, 6, 8, 12, 16, 24, 32, 48):
                            yield ("rep", (core, p), eval_lin(a, b, d, k % p))
                        for num in (1, 2, 3, 4):
                            for den in (1, 2, 3, 4):
                                if (num, den) != (1, 1):
                                    yield ("sc", (core, num, den),
                                           eval_lin(a, b, d, (k * num) // den))
    # add composites
    for a1 in range(8):
        for d1 in (1, 2, 3, 4):
            u = eval_lin(a1, 0, d1, x)
            for c in (0, 1, 2, 3):
                yield ("add", (("lin", (a1, 0, d1)), ("constlike", c)), (u + c) % 256)
            for a2 in range(8):
                for d2 in (1, 2, 3, 4):
                    v = eval_lin(a2, 0, d2, y)
                    yield ("add", (("lin", (a1, 0, d1)), ("lin", (a2, 0, d2))), (u + v) % 256)


def eval_lin(a, b, d, kk):
    return ((a * kk + b) // d) % 256


def search_channel(chimg, x, y, kk):
    """One generator pass; bounded top-24 entropy heap; container-measure
    only finalists. Ponytail: single-pass heap, exhaustive but bounded."""
    import heapq
    heap = []
    for fam, prm, recon in families(kk, x, y, x.shape[1], x.shape[0]):
        res = ((chimg - recon) % 256).astype(np.uint8)
        e = entropy(res)
        item = (-e, fam, prm, res.tobytes())
        if len(heap) < 24:
            heapq.heappush(heap, item)
        elif item < heap[0]:
            heapq.heapreplace(heap, item)
    if not heap:
        return None
    winner = None
    for neg_e, fam, prm, resb in heap:
        c = container(resb)
        L = prog_bytes(fam, prm) + c
        if winner is None or L < winner[0]:
            winner = (L, fam, prm, c)
    return winner if winner else None


def cli_size(rel):
    env = os.environ.copy()
    env["PATH"] = ("/opt/homebrew/opt/rustup/bin:"
                   + str(Path.home() / ".rustup/toolchains/stable-aarch64-apple-darwin/bin")
                   + ":" + env["PATH"])
    env["RUSTUP_HOME"] = str(Path.home() / ".rustup")
    subprocess.run([str(ROOT / "target/release/manifold"), "encode", rel,
                    "-o", "/tmp/x1.mnf5", "--mnf8"],
                   capture_output=True, env=env, check=True, cwd=str(ROOT))
    return Path("/tmp/x1.mnf5").stat().st_size


def measure(rel):
    img = np.asarray(Image.open(Path(rel) if os.path.isabs(rel) else ROOT / rel)
                     .convert("RGB"), dtype=np.int64)
    h, w = img.shape[:2]
    x, y = np.meshgrid(np.arange(w), np.arange(h))
    kk = x
    shipped = cli_size(str(rel) if os.path.isabs(rel) else rel)
    ch_progs, L_tot = [], 32
    for c in range(3):
        win = search_channel(img[:, :, c], x, y, kk)
        if win is None:
            return None, shipped, None
        L, fam, prm, cc = win
        L_tot += L
        ch_progs.append((fam, prm, L, cc))
    return L_tot, shipped, ch_progs


def demo():
    # interpreter sanity: lin identity, rec arithmetic, residual reversibility
    x, y = np.meshgrid(np.arange(16), np.arange(8))
    assert int(eval_fam("lin", (1, 5, 2), x, x, y)[2, 10]) == ((1 * 10 + 5) // 2) % 256
    assert int(eval_fam("rec", (3, 7, 32), x, x, y)[0, 5]) == (3 + 7 * 5) % 32
    img = np.full((8, 16, 3), 200, np.int64)
    recon = np.stack([eval_fam("lin", (0, 200, 1), x, x, y)] * 3, axis=2)
    assert np.array_equal((img[..., 0] - recon[..., 0]) % 256, np.zeros((8, 16), np.int64))
    print("x1 self-check OK")


def main():
    demo()
    fixtures = ["testdata/heldout/document_200x280.png",
                "testdata/heldout/poster_208x144.png",
                "testdata/screenshot_640x400.png",
                "testdata/gradient_320x240.png",
                "testdata/solid_256x256.png",
                "testdata/heldout/blob_224x160.png",
                "testdata/heldout/plaid_192.png",
                "testdata/heldout/repeat_256x256.png"]
    for d in ("photos", "heldout-photos"):
        p = ROOT / "testdata" / d
        if p.exists():
            fixtures += [str(f.relative_to(ROOT)) for f in sorted(p.glob("*.png"))]
    gen = Path("/tmp/mnf10-gen")
    if gen.exists():
        fixtures += [str(f) for f in sorted(gen.glob("*.png"))]
    wins = misses = fires = 0
    for rel in fixtures:
        L, shipped, progs = measure(rel)
        name = Path(rel).name
        if L is None:
            print(f"{name:42s} shipped={shipped:>7d}  no-fire (some channel unmotivated)")
            continue
        d = L - shipped
        fires += 1
        tag = "WIN " if d < 0 else "lose"
        wins += d < 0
        misses += d >= 0
        ps = " | ".join(show(f, p) for f, p, _, _ in progs)
        print(f"{name:42s} shipped={shipped:>7d}  x1={L:>7d} ({d:+6d}) {tag}")
        print(f"    {ps}")
    print(f"\nfixtures {len(fixtures)}  fires {fires}  wins {wins}  {misses} no-win")


if __name__ == "__main__":
    main()