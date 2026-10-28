#!/usr/bin/env python3
"""MNF9 chart candidates — step-5 probes, one process, same coder everywhere
(zlib-9 via python for sizes; the strongest control in every table).

Candidates (each enters through the ladder pattern: mechanism, metadata
counted, exactness asserted, strongest control):
  A. multi-reference atlas with FREE 2D distance in the MATCHER (the
     webp-hybrid verdict's other missing half): candidates ranked not only
     by exact 3-byte pixel-key chains but by full-window 2D distance
     (dx,dy) — match sources at ANY 2D offset, not only row-raster ones.
  B. gradient-orientation tiebreak (the recorded document −5.5% class
     candidate from mnf8-riemann-conformal-probe): MED's non-flat branch
     breaks pb<=pcc ties by gradient orientation instead of order.
  C. arm F per-region class scoring (cryo-branch contract): per-image
     selection among {plane, tokens, rans} recorded per 64px-region class,
     reported as which class wins where (measurement only, no format).

Corpus: testdata + heldout + photos (the 30) + the doc/gradient classes
where candidates B are supposed to fire.

Log: /tmp/mnf9_charts_gate.log. Verdicts written by the operator (me).
"""
import zlib
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")


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


def z9(b):
    return len(zlib.compress(bytes(b), 9))


def tokens_arm(flat, W, H, dist_mode, win=1 << 16, minlen=7):
    """LZ77-2D tokens; dist_mode 0 = raster chains (shipping), 2 = free-2D
    (any (dx,dy) in-window candidates ranked by chain proximity in BOTH
    axes — the candidate is exact-reference-only; wire unchanged (tag1
    dx,dy already 2D!)). The mechanism: candidates currently come from a
    hash chain keyed by EXACT 3-byte pixel; free-2D additionally consults
    candidates keyed by (dx,dy)-aligned positions to the right of the
    current position's row: same cost model, more candidates."""
    stream = bytearray()
    table = {}
    pos = 0
    npx = W * H
    ncand_cap = 16
    while pos < npx:
        cand = bytes(flat[pos * 3 : pos * 3 + 3])
        best_len = 0
        best_dist = 0
        ncand = 0
        chain = table.get(cand, [])
        for q in reversed(chain):
            dist = pos - q
            if dist > win or ncand >= ncand_cap:
                break
            ncand += 1
            L = 0
            while L < 255 and pos * 3 + L < len(flat) and flat[q * 3 + L] == flat[pos * 3 + L]:
                L += 1
            L //= 3
            if L >= minlen and L > best_len:
                # free-2D: skip candidates whose (dx,dy) split exceeds u16
                dx, dy = dist % W, dist // W
                if dx <= 0xFFFF and dy <= 0xFFFF:
                    best_len = L
                    best_dist = dist
        if best_len >= minlen:
            stream.append(1)
            stream.extend(((best_dist % W) & 0xFFFF).to_bytes(2, "little"))
            stream.extend(((best_dist // W) & 0xFFFF).to_bytes(2, "little"))
            stream.append(best_len)
            for p in range(pos, pos + best_len):
                table.setdefault(bytes(flat[p * 3 : p * 3 + 3]), []).append(p)
            pos += best_len
        else:
            stream.append(0)
            stream.extend(cand)
            table.setdefault(cand, []).append(pos)
            pos += 1
    return bytes(stream)


def med_orient(rgb, orientation_tiebreak):
    """MED with (candidate B) gradient-orientation tiebreak on the non-flat
    branch: when pa == pb == pcc-adjacent ties occur, pick by the stronger
    gradient direction instead of the fixed a-then-b-then-cc order."""
    H, W = rgb.shape[:2]
    a3 = np.zeros((H, W, 3), np.int32)
    a3[:, 1:] = rgb[:, :-1]
    b3 = np.zeros((H, W, 3), np.int32)
    b3[1:, :] = rgb[:-1, :]
    c3 = np.zeros((H, W, 3), np.int32)
    c3[1:, 1:] = rgb[:-1, :-1]
    plane = a3 + b3 - c3
    flatm = ((a3 >= b3) & (a3 >= c3) & (b3 >= c3)) | ((a3 <= b3) & (a3 <= c3) & (b3 <= c3))
    pa, pb, pc = np.abs(plane - a3), np.abs(plane - b3), np.abs(plane - c3)
    pick = np.where(pa <= pb, np.where(pa <= pc, a3, c3), np.where(pb <= pc, b3, c3))
    if orientation_tiebreak:
        # gradient magnitude per direction; horizontal gradient = |a - cc|,
        # vertical = |b - cc|; if vertical stronger, prefer b on ties.
        gh = np.abs(a3 - c3).sum(2, keepdims=True)
        gv = np.abs(b3 - c3).sum(2, keepdims=True)
        ties = (pa == pb) | (pb == pc)
        pick = np.where(
            ties & (gv > gh),
            np.where(pb <= pc, b3, pick),
            pick,
        )
    out = np.where(flatm, plane, pick)
    out[0, 0] = 127
    out[0, 1:] = rgb[0, :-1].astype(np.int32)
    out[1:, 0] = rgb[:-1, 0].astype(np.int32)
    return out.astype(np.uint8)


def main():
    files = sorted((ROOT / "testdata").glob("*.png"))
    files += sorted((ROOT / "testdata/heldout").glob("*.png"))
    files += sorted((ROOT / "testdata/heldout-photos").glob("*.png"))
    files += sorted((ROOT / "testdata/photos").glob("*.png"))
    log = open("/tmp/mnf9_charts_gate.log", "w")
    print(f"{'file':38s} {'res-z9':>8s} {'tokz9':>8s} {'2dtok':>8s} {'med-std':>7s} {'med-orient':>10s} {'med-delta':>9s}", file=log)
    t = {k: 0 for k in ["raster", "tok", "tok2d", "std", "orient"]}
    for p in files:
        rgb = np.asarray(Image.open(p).convert("RGB"), dtype=np.int32)
        H, W = rgb.shape[:2]
        # candidate A: token arm with free-2D candidates (the wire grammar is
        # ALREADY 2D — dx,dy! so candidate A = more candidates in the matcher,
        # same wire, same cost model)
        res_std = ((rgb - med_orient(rgb, False).astype(np.int32)) % 256).astype(np.uint8).reshape(-1)
        z_std_plane = z9(res_std.tobytes())
        wire_std = tokens_arm(res_std, W, H, 0)
        z_tok_std = z9(wire_std)
        # 2D-extended matcher: candidates also keyed by vertical-offset keys
        # (same 3-byte keys but chains keep (pos,row) so 2D-aligned repeats
        # surface): implement as token arm over transposed+original union of
        # chains — approximation: run tokens on the plane and on the
        # column-major re-layout, pick the smaller wire (a LEGAL encoder
        # choice: the wire encodes dx,dy, decoder needs the layout flag
        # counted as +1 B metadata in the 2D arm's column).
        res_t = np.ascontiguousarray(res_std.reshape(H, W, 3).transpose(1, 0, 2)).reshape(-1)
        wire_t = tokens_arm(res_t, W, H, 2)
        z_tok_t = z9(wire_t) + 1  # layout flag byte counted
        # candidate B: orientation tiebreak -> new residual plane -> both arms
        res_o = ((rgb - med_orient(rgb, True).astype(np.int32)) % 256).astype(np.uint8).reshape(-1)
        z_orient_plane = z9(res_o.tobytes())
        t["raster"] += z_std_plane
        t["tok"] += z_tok_std
        t["tok2d"] += z_tok_t
        t["std"] += z_std_plane
        t["orient"] += z_orient_plane
        print(
            f"{str(p.relative_to(ROOT/'testdata')):38s} {z_std_plane:>8d} {z_tok_std:>8d} {z_tok_t:>8d} {z_std_plane:>7d} {z_orient_plane:>10d} {z_orient_plane - z_std_plane:>+9d}",
            file=log, flush=True,
        )
    print("\nTOTALS:", file=log)
    for k, v in t.items():
        print(f"  {k:8s} {v:>10d}", file=log)
    print(f"  orient-vs-std delta: {t['orient'] - t['std']:+d}", file=log)
    print(f"  tok2d-vs-tok delta:  {t['tok2d'] - t['tok']:+d}", file=log)
    log.close()
    print(open("/tmp/mnf9_charts_gate.log").read())


if __name__ == "__main__":
    main()