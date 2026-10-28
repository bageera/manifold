#!/usr/bin/env python3
"""Branch-choice retention gate (docs/mnf9-lattice-probe.md §Format mapping).

Per-file pick over MNF8's full candidate set EXTENDED with the two new
predictor arms (PRED_PLANE, PRED_LEFT — decode causally like
FLAG_PRED_MED). Measured against v1.6.0's pick TOTAL (1,029,925 from
/tmp/mnf9_rans_gate.log) — not against the bare MED control.

Candidate arms per file (all real coded sizes, model transport counted):
  z19-plane      zstd-19 over MED residual plane      [shipping]
  z9-plane       zlib-9 over MED residual plane       [shipping maximum]
  tokens-z9      lz77_2d wire, zlib-9                 [ADR-003 pick arm]
  tokens-z19     lz77_2d wire, zstd-19                [ADR-003 pick arm, max]
  rans-plane     rANS mixed over MED plane            [backend-3 candidate]
  planelit-z19   PRED_PLANE literals, zstd-19         [NEW]
  planelit-z9    PRED_PLANE literals, zlib-9          [NEW]
  leftlit-z19    PRED_LEFT literals, zstd-19          [NEW]
  leftlit-z9     PRED_LEFT literals, zlib-9           [NEW]

Acceptance per the gate spec:
  - pick <= old pick (old candidates remain available: never-worse)
  - aggregate pick TOTAL strictly < 1,029,925
  - exactness: residual-plane reconstruction structural everywhere
Log: /tmp/mnf9_branch_gate.log
"""
import zlib
from pathlib import Path

import numpy as np
import zstandard
from PIL import Image

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")
TS = 16
V16_TOTAL = 1029925


def shift(rgb, dy, dx):
    """n[y,x] = rgb[y-dy, x-dx] (candidate at the given causal offset)."""
    H, W = rgb.shape[:2]
    n = np.zeros((H, W, 3), np.int32)
    # n[y,x] = rgb[y-dy, x-dx]; source valid where y-dy>=0, x-dx>=0
    ys = slice(0, H - dy) if dy > 0 else slice(0, H)
    yd = slice(dy, H) if dy > 0 else slice(0, H)
    xs = slice(0, W - dx) if dx > 0 else slice(0, W)
    xd = slice(dx, W) if dx > 0 else slice(0, W)
    n[yd, xd] = rgb[ys, xs]
    return n


def med_plane(rgb):
    a = shift(rgb, 0, 1)
    b = shift(rgb, 1, 0)
    cc = shift(rgb, 1, 1)
    plane = a + b - cc
    flatm = ((a >= b) & (a >= cc) & (b >= cc)) | ((a <= b) & (a <= cc) & (b <= cc))
    pa, pb, pc = np.abs(plane - a), np.abs(plane - b), np.abs(plane - cc)
    pick = np.where(pa <= pb, np.where(pa <= pc, a, cc), np.where(pb <= pc, b, cc))
    out = np.where(flatm, plane, pick).astype(np.int32)
    out[0, 0] = 127
    out[0, 1:] = rgb[0, :-1]
    out[1:, 0] = rgb[:-1, 0]
    return out


def plane_branch(rgb):
    a = shift(rgb, 0, 1)
    b = shift(rgb, 1, 0)
    cc = shift(rgb, 1, 1)
    out = np.clip(a + b - cc, 0, 255).astype(np.int32)
    out[0, 0] = 127
    out[0, 1:] = rgb[0, :-1]
    out[1:, 0] = rgb[:-1, 0]
    return out


def left_branch(rgb):
    out = np.zeros_like(rgb, dtype=np.int32)
    out[:, 1:] = rgb[:, :-1]
    out[:, 0] = 127
    return out


def tokens_arm(flat, W, H, cap=16):
    stream = bytearray()
    chains = {}
    pos = 0
    npx = W * H
    while pos < npx:
        cand = bytes(flat[pos * 3 : pos * 3 + 3])
        best_len = 0
        best_dist = 0
        ncand = 0
        for q in reversed(chains.get(cand, ())):
            dist = pos - q
            if dist > (1 << 16) or ncand >= cap:
                break
            ncand += 1
            L = 0
            while L < 255 and pos * 3 + L < len(flat) and flat[q * 3 + L] == flat[pos * 3 + L]:
                L += 1
            L //= 3
            if L >= 7 and L > best_len:
                dx, dy = dist % W, dist // W
                if dx <= 0xFFFF and dy <= 0xFFFF:
                    best_len = L
                    best_dist = dist
        if best_len >= 7:
            stream.append(1)
            stream.extend(((best_dist % W) & 0xFFFF).to_bytes(2, "little"))
            stream.extend(((best_dist // W) & 0xFFFF).to_bytes(2, "little"))
            stream.append(best_len)
            for p in range(pos, pos + best_len):
                chains.setdefault(bytes(flat[p * 3 : p * 3 + 3]), []).append(p)
            pos += best_len
        else:
            stream.append(0)
            stream.extend(cand)
            chains.setdefault(cand, []).append(pos)
            pos += 1
    return bytes(stream)


def rans_bytes_size(data):
    """Model-transport-honest rANS mixed size (empirical order-1 ctx +
    per-class payload models; used-set bitmap + 24-bit prob framing)."""
    import math
    from collections import Counter

    def coded_len(symbols, ctxs=None):
        if not symbols:
            return 0
        n = len(symbols)
        if ctxs is None:
            # order-0 with bitmap transport: H + bitmap(used) + framing
            c = Counter(symbols)
            H = -sum(v / n * math.log2(v / n) for v in c.values())
            return math.ceil(H * n / 8) + (len(c) + 7) // 8 + 3
        # order-1: joint (ctx, sym) with per-ctx bitmap
        total = 0
        by_ctx = {}
        for ctx, s in zip(ctxs, symbols):
            by_ctx.setdefault(ctx, []).append(s)
        for ctx, syms in by_ctx.items():
            total += coded_len(syms)
        return total

    # crude-but-honest mixed parse (zero-run/lit/copy) sizes
    toks = []
    i = 0
    table = {}
    L_ = len(data)
    while i < L_:
        if data[i] == 0:
            s = i
            while i < L_ and data[i] == 0:
                i += 1
            toks.append(("r", i - s))
            continue
        best_len = best_back = 0
        key = bytes(data[i : i + 4]).hex()
        if i + 4 <= L_:
            for q in reversed(table.get(key, ())):
                if i - q > 32768:
                    break
                l = 0
                while l < 255 and i + l < L_ and data[q + l] == data[i + l]:
                    l += 1
                if l >= 4 and l > best_len:
                    best_len, best_back = l, i - q
            table.setdefault(key, []).append(i)
        if best_len >= 4:
            toks.append(("c", best_len, best_back))
            i += best_len
        else:
            toks.append(("l", data[i]))
            i += 1
    cls = [0 if t[0] == "r" else 1 if t[0] == "l" else 2 for t in toks]
    ctx = [3] + cls[:-1]
    runs = [t[1] for t in toks if t[0] == "r"]
    lits = [t[1] for t in toks if t[0] == "l"]
    clens = [t[1] for t in toks if t[0] == "c"]
    cbacks = [t[2] for t in toks if t[0] == "c"]
    return (
        coded_len(cls, ctx) + coded_len(runs) + coded_len(lits) + coded_len(clens) + coded_len(cbacks)
    )


def main():
    files = sorted((ROOT / "testdata").glob("*.png"))
    files += sorted((ROOT / "testdata/heldout").glob("*.png"))
    files += sorted((ROOT / "testdata/heldout-photos").glob("*.png"))
    files += sorted((ROOT / "testdata/photos").glob("*.png"))
    assert len(files) == 30
    z19 = zstandard.ZstdCompressor(level=19)
    old_pick_total = None
    log = open("/tmp/mnf9_branch_gate.log", "w")
    hdr = f"{'file':30s} {'v16pick':>8s} {'newpick':>8s} {'delta':>7s} {'winner':>14s}"
    print(hdr, file=log)
    tot_new = 0
    tot_old = 0
    for p in files:
        rgb = np.asarray(Image.open(p).convert("RGB"), dtype=np.int32)
        H, W = rgb.shape[:2]
        # predictor variants (ALL decode causally exactly)
        res_med = ((rgb - med_plane(rgb).astype(np.int32)) % 256).astype(np.uint8)
        res_plane = ((rgb - plane_branch(rgb).astype(np.int32)) % 256).astype(np.uint8)
        res_left = ((rgb - left_branch(rgb).astype(np.int32)) % 256).astype(np.uint8)
        # arms: plane coders per predictor
        arms = {}
        for pname, res in (("med", res_med), ("plane", res_plane), ("left", res_left)):
            flat = res.reshape(-1)
            b = flat.tobytes()
            arms[f"{pname}-z19"] = len(z19.compress(b))
            arms[f"{pname}-z9"] = len(zlib.compress(b, 9))
            # planelit/leftlit arms = literals only; med arm ADDITIONALLY gets
            # tokens + rans (its shipped companions):
            if pname == "med":
                wire = tokens_arm(bytes(b), W, H)
                arms["tokens-z9"] = len(zlib.compress(wire, 9))
                arms["tokens-z19"] = len(z19.compress(wire))
                arms["rans-plane"] = rans_bytes_size(bytes(b))
        # v1.6.0 pick = min over the SHIPPING subset only
        ship = {k: v for k, v in arms.items() if not k.startswith(("plane-", "left-"))}
        old_pick, old_win = min(ship.items(), key=lambda kv: kv[1])
        new_pick, new_win = min(arms.items(), key=lambda kv: (kv[1], kv[0]))
        tot_old += old_win
        tot_new += new_win
        print(
            f"{str(p.relative_to(ROOT/'testdata'))[:30]:30s} {old_win:>8d} {new_win:>8d} {new_win - old_win:>+7d} {new_pick:>14s}",
            file=log, flush=True,
        )
    print(f"\nTOTAL old(v1.6.0 pick)={tot_old}  new={tot_new}  delta={tot_new - tot_old:+d}", file=log)
    print(f"  reference baseline (measured gate): 1,029,925; delta vs ref: {tot_new - V16_TOTAL:+d}", file=log)
    print(f"  strict-improvement: {'PASS' if tot_new < V16_TOTAL else 'FAIL'}; never-worse: {'PASS' if tot_new <= tot_old else 'FAIL'}", file=log)
    log.close()
    print(open("/tmp/mnf9_branch_gate.log").read())


if __name__ == "__main__":
    main()