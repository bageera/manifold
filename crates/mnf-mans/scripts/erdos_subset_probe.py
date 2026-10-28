#!/usr/bin/env python3
"""MNF9 Erdős-offset probe v2 — subset corpus (photos are slow; the win
classes from every prior probe are synthetics/graphics, so run those +
2 smoke photos). Same arms/equal budget/log discipline as v1."""
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


def r2(m):
    n = 0
    a = 1
    while a * a * 2 <= m if a * a <= m // 2 else False:
        b2 = m - a * a
        b = int(b2**0.5)
        if b >= a and b * b == b2:
            n += 1
        a += 1
    return n


def dense_table(top=32, m_max=40000):
    cand = [(r2(m), m) for m in range(65, m_max)]
    cand = [x for x in cand if x[0] > 0]
    cand.sort(reverse=True)
    return cand[:top]


def build_offsets(dense):
    offs = {}
    for reps, m in dense:
        a = 1
        while a * a <= m:
            b2 = m - a * a
            b = int(b2**0.5)
            if a >= 1 and b >= a and b * b == b2:
                for (x, y) in {(a, b), (b, a)}:
                    for sx in (1, -1):
                        for sy in (1, -1):
                            key = (x * sx, y * sy)
                            if key not in offs or m < offs[key]:
                                offs[key] = m
                if a * a * 2 > m:
                    break
            a += 1
    return offs


def tokens_mode_a(flat, W):
    stream = bytearray()
    chains = {}
    pos = 0
    npx = len(flat) // 3
    matches = 0
    while pos < npx:
        cand = bytes(flat[pos * 3 : pos * 3 + 3])
        best_len = 0
        best_dist = 0
        ncand = 0
        for q in reversed(chains.get(cand, ())):
            dist = pos - q
            if dist > WIN or ncand >= CAP:
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
        if best_len >= MINLEN:
            matches += 1
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
    return bytes(stream), matches


def tokens_mode_b(flat, W, offsets):
    """Candidates ONLY at Erdős offsets (content-key verified), capped."""
    stream = bytearray()
    pos = 0
    npx = len(flat) // 3
    matches = 0
    off_list = sorted(offsets.items(), key=lambda kv: kv[1])  # densest m first? no: lowest m
    off_pairs = [(a, b, m) for (a, b), m in off_list]
    while pos < npx:
        y, x = divmod(pos, W)
        cand = bytes(flat[pos * 3 : pos * 3 + 3])
        best_len = 0
        best_dist = 0
        probes = 0
        for (a, b, m) in off_pairs:
            for (dy, dx) in ((a, b), (a, -b), (-a, b), (-a, -b), (b, a), (b, -a), (-b, a), (-b, -a)):
                if dy <= 0 and (dy < 0 or dx <= 0):
                    continue  # causal: target strictly earlier in raster
                sy = y - dy
                sx = x - dx
                if sx < 0 or sx >= W or sy < 0:
                    continue
                q = sy * W + sx
                if bytes(flat[q * 3 : q * 3 + 3]) != cand:
                    continue
                probes += 1
                if probes > CAP:
                    break
                L = 0
                while L < 255 and pos * 3 + L < len(flat) and flat[q * 3 + L] == flat[pos * 3 + L]:
                    L += 1
                L //= 3
                if L >= MINLEN and L > best_len:
                    best_len = L
                    best_dist = pos - q
            if probes > CAP:
                break
        if best_len >= MINLEN:
            matches += 1
            stream.append(1)
            stream.extend(((best_dist % W) & 0xFFFF).to_bytes(2, "little"))
            stream.extend(((best_dist // W) & 0xFFFF).to_bytes(2, "little"))
            stream.append(best_len)
            pos += best_len
        else:
            stream.append(0)
            stream.extend(cand)
            pos += 1
    return bytes(stream), matches


def main():
    names = [
        "gradient_320x240.png", "solid_256x256.png", "screenshot_640x400.png",
        "repeat_256x256.png", "noise_256x256.png",
        "heldout/blocks_192x128.png", "heldout/document_200x280.png",
        "heldout/plaid_192.png", "heldout/poster_208x144.png", "heldout/blob_224x160.png",
        "photos/stone_160x120_s900006.png",  # one photo smoke
    ]
    dense = dense_table()
    offsets = build_offsets(dense)
    print(f"dense: top m reps={max(r for r,_ in dense)}, table={len(dense)}, offsets={len(offsets)}")
    log = open("/tmp/mnf9_erdos_gate.log", "w")
    print(f"{'file':30s} {'A':>8s} {'B':>8s} {'mA':>6s} {'mB':>6s}", file=log)
    tA = tB = 0
    for name in names:
        rgb = np.asarray(Image.open(ROOT / "testdata" / name).convert("RGB"), dtype=np.int32)
        H, W = rgb.shape[:2]
        res = ((rgb - med_pred(rgb).astype(np.int32)) % 256).astype(np.uint8).reshape(-1)
        wa, ma = tokens_mode_a(res, W)
        wb, mb = tokens_mode_b(res, W, offsets)
        za, zb = z9(wa), z9(wb)
        tA += za
        tB += zb
        print(f"{name[:30]:30s} {za:>8d} {zb:>8d} {ma:>6d} {mb:>6d}", file=log, flush=True)
    print(f"TOTALS: A={tA} B={tB} (B-A={tB - tA:+d})", file=log)
    log.close()
    print(open("/tmp/mnf9_erdos_gate.log").read())


if __name__ == "__main__":
    main()