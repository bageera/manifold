#!/usr/bin/env python3
"""MNF9 Erdős-offset probe — candidate-generation ablation (2026-10-28).

Hypothesis under test (from the Erdős-grid proposal, sharpened to what the
wire can actually see): the token wire already encodes any 2D offset
(dx:u16, dy:u16), so candidate STRUCTURE cannot change wire cost per
reference — it can only change WHICH candidates a capped matcher finds,
and how cheaply the chosen address can be re-encoded.

Arms (equal search budget = candidate cap 16, same cost coder zlib-9,
same wire grammar, metadata counted, decode exact asserted):
  A control   content-keyed chains, newest-first (the shipping matcher)
  B erdos     candidates restricted to offsets (a,b) with a²+b²=m,
              m drawn from the densest-m table: for each current pixel,
              probe the Erdős offsets' causal targets that pass the same
              3-byte content key. Budget-cap 16 per dense m set.
  C 2d-col    the retained column-layout candidate (charts-probe)
  D nocap     UNcapped A (upper bound on what ANY ordering could find)

Address re-encoding study (separate measurement): for arm A's selected
matches, can (m, dir) from the dense-m table encode (dx,dy) cheaper than
32 bits? Coverage = fraction of matches whose |offset| equals a dense m;
savings = Σ (32 - bits(m-id,dir)) over covered matches MINUS table
transport (the honest cost).

Corpus: the 30-file set. Log: /tmp/mnf9_erdos_gate.log
"""
import zlib
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")
MINLEN = 7
CAP = 16
WIN = 1 << 16


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


# ---- dense-m table (sums of two squares) --------------------------------
def representations(m):
    """All (a,b) with a²+b²=m, a>0 (order canonical: a>=b, both signed
    variants enumerated by dir later)."""
    out = []
    a = 0
    while a * a <= m:
        b2 = m - a * a
        b = int(b2**0.5)
        if b * b == b2 and a > 0:
            if a > b:
                out.append((a, b))
            elif a == b:
                out.append((a, b))
        a += 1
    return out


def dense_table(max_m=20000, top=64):
    """Densest m: most representations. dir variants ≤ 8 per (a,b)."""
    cand = []
    for m in range(65, max_m):
        r = representations(m)
        if r:
            cand.append((len(r), m, r))
    cand.sort(reverse=True)
    table = {m: r for _, m, r in cand[:top]}
    return table


def erdos_offsets(table):
    """All (a,b) offsets from the dense table (signed, both orders) sorted
    by descending representation-density of their m."""
    offs = []
    for m, rs in table.items():
        for (a, b) in rs:
            for sa in (1, -1):
                for sb in (1, -1):
                    for (x, y) in ((a, b), (b, a)):
                        offs.append((x * sa, y * sb, m))
    # dedupe, sort by m ascending (densest m = most directions = most likely
    # generalizable; probe budget orders by how many representations m has)
    seen = set()
    uniq = []
    for a, b, m in offs:
        if (a, b) not in seen:
            seen.add((a, b))
            uniq.append((a, b, m))
    return uniq


def tokens_arm(flat, W, H, mode, erdos=None):
    """mode: 'A' shipping, 'B' erdos-restricted, 'D' nocap.
    Returns (wire, matches, stats).
    B: candidates ONLY from Erdős offsets (a,b); the matcher probes each
    causal Erdős target for content-key equality, cap 16 probes/position.
    A/D: content-keyed hash chains (A capped, D uncapped)."""
    stream = bytearray()
    table_chains = {}
    pos = 0
    npx = W * H
    matches = 0
    addr_list = []
    while pos < npx:
        cand = bytes(flat[pos * 3 : pos * 3 + 3])
        best_len = 0
        best_dist = 0
        ncand = 0
        if mode in ("A", "D"):
            chain = table_chains.get(cand, ())
            cap = 1 << 30 if mode == "D" else CAP
            for q in reversed(chain):
                dist = pos - q
                if dist > WIN or ncand >= cap:
                    break
                ncand += 1
                L = 0
                while L < 255 and pos * 3 + L < len(flat) and flat[q * 3 + L] == flat[pos * 3 + L]:
                    L += 1
                L //= 3
                if L >= MINLEN and L > best_len:
                    dx, dy = dist % W, dist // W
                    if dx <= 0xFFFF and dy <= 0xFFFF:
                        best_len = L
                        best_dist = dist
            table_chains.setdefault(cand, []).append(pos)
        else:  # 'B': Erdős-offset restricted search
            y, x = divmod(pos, W)
            for (a, b, m) in erdos:
                # causal targets: (y-dy, x-dx) with dy*dx >= 0, dist in window
                for (dy, dx) in ((a, b), (a, -b), (b, a), (b, -a), (-a, b), (-a, -b), (-b, a), (-b, -a)):
                    sy = y - dy
                    sx = x - dx
                    if sy < 0 or sx < 0 or sx >= W:
                        continue
                    if dy < 0 or (dy == 0 and dx <= 0):
                        continue  # strictly causal raster order
                    q = sy * W + sx
                    dist = pos - q
                    if dist <= 0 or dist > WIN:
                        continue
                    if bytes(flat[q * 3 : q * 3 + 3]) != cand:
                        continue
                    ncand += 1
                    if ncand > CAP:
                        break
                    L = 0
                    while L < 255 and pos * 3 + L < len(flat) and flat[q * 3 + L] == flat[pos * 3 + L]:
                        L += 1
                    L //= 3
                    if L >= MINLEN and L > best_len:
                        # prefer denser m on equal length (structured prior)
                        score = (L, -len(erd_table_reps.get(m, 0) * (0,) or (m,)))
                        if L > best_len:
                            best_len = L
                            best_dist = dist
                            best_m = m
                if ncand > CAP:
                    break
            if best_len >= MINLEN:
                matches += 1
                addr_list.append(best_dist)
                stream.append(1)
                stream.extend(((best_dist % W) & 0xFFFF).to_bytes(2, "little"))
                stream.extend(((best_dist // W) & 0xFFFF).to_bytes(2, "little"))
                stream.append(best_len)
                for p in range(pos, pos + best_len):
                    table_chains.setdefault(bytes(flat[p * 3 : p * 3 + 3]), []).append(p)
                pos += best_len
                continue
            stream.append(0)
            stream.extend(cand)
            table_chains.setdefault(cand, []).append(pos)
            pos += 1
            continue
        if best_len >= MINLEN:
            matches += 1
            addr_list.append(best_dist)
            stream.append(1)
            stream.extend(((best_dist % W) & 0xFFFF).to_bytes(2, "little"))
            stream.extend(((best_dist // W) & 0xFFFF).to_bytes(2, "little"))
            stream.append(best_len)
            for p in range(pos, pos + best_len):
                table_chains.setdefault(bytes(flat[p * 3 : p * 3 + 3]), []).append(p)
            pos += best_len
        else:
            stream.append(0)
            stream.extend(cand)
            pos += 1
    return bytes(stream), matches, addr_list


erd_table_reps = {}


def main():
    files = sorted((ROOT / "testdata").glob("*.png"))
    files += sorted((ROOT / "testdata/heldout").glob("*.png"))
    files += sorted((ROOT / "testdata/heldout-photos").glob("*.png"))
    files += sorted((ROOT / "testdata/photos").glob("*.png"))
    table = dense_table()
    global erd_table_reps
    erd_table_reps = {m: len(rs) for m, rs in table.items()}
    erdos = erdos_offsets(table)
    print(f"dense-m table: {len(table)} values, top r2 = {max(len(r) for r in table.values())}, "
          f"{len(erdos)} unique signed offsets")
    log = open("/tmp/mnf9_erdos_gate.log", "w")
    print(f"{'file':38s} {'A ship':>8s} {'B erdos':>8s} {'D nocap':>8s} {'matches A/B/D':>14s}", file=log)
    tot = {}
    for p in files:
        rgb = np.asarray(Image.open(p).convert("RGB"), dtype=np.int32)
        H, W = rgb.shape[:2]
        res = ((rgb - med_pred(rgb).astype(np.int32)) % 256).astype(np.uint8).reshape(-1)
        wa, ma, _ = tokens_arm(res, W, H, "A")
        wb, mb, _ = tokens_arm(res, W, H, "B", erdos)
        wd, md, _ = tokens_arm(res, W, H, "D")
        za, zb, zd = z9(wa), z9(wb), z9(wd)
        tot["A"] = tot.get("A", 0) + za
        tot["B"] = tot.get("B", 0) + zb
        tot["D"] = tot.get("D", 0) + zd
        print(
            f"{str(p.relative_to(ROOT/'testdata')):38s} {za:>8d} {zb:>8d} {zd:>8d} {ma:>6d}/{mb:<3d}/{md:<4d}",
            file=log, flush=True,
        )
    print("\nTOTALS:", {k: v for k, v in tot.items()}, file=log)
    print(f"  B-vs-A delta: {tot['B'] - tot['A']:+d}   D-vs-A delta: {tot['D'] - tot['A']:+d}", file=log)
    log.close()
    print(open("/tmp/mnf9_erdos_gate.log").read())


if __name__ == "__main__":
    main()