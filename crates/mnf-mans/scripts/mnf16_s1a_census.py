#!/usr/bin/env python3
"""MNF16-S1-A/S1-B census on the SHIPPED v1.16 balanced record streams.

The shipped balanced wire (backend 1 = zstd container) carries the raw
per-tile record stream; residual_census.py's walker recovers it exactly
(zstd-decompress, per-tile [mode][body], lossless recon anchors for
modes 0/8). S1-A question: do residual symbols carry information beyond
their stream context — specifically predictor disagreement D (max-min
over {a, b, cc, plane, MED} of the SAME plane's causal recon) and the
S1-B coarse-scale gradient G8 (8-px west/north, same plane)?

  I(R; F | C) = H(R|C) - H(R|C, F)

C = bucket(prev wire symbol, 16 bins) in stream order. Histograms are
built on the stream prefix (first 80%); improvements are evaluated
causally on the final 20% (prequential) and in-sample (upper bound).
Wire symbols are the SHIPPED bytes; features are decoder-causal
same-plane recon reads (the recon == the image; lossless). Law-mixed
(6/7) tiles are walked for stream structure but excluded from the
population; mode-0 tiles are included (their symbols are the plain
residual wire the census reads).

Usage: python3 mnf16_s1a_census.py [name-filter]
Writes /tmp/mnf16_s1a_census.csv; prints per-file rows + AGG summary.
"""
import csv
import io
import struct
import subprocess
import sys
from pathlib import Path

import numpy as np
import PIL.Image
import zstandard

TS = 16
ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "scripts"))
import residual_census as rc  # noqa: E402


def corpus():
    return [ln.split(",")[0].strip()
            for ln in (ROOT / ".research/r9/r9e_rows.csv").read_text().strip().splitlines()[1:]]


def h_bits(counts):
    n = counts.sum()
    if n == 0:
        return 0.0
    p = counts[counts > 0] / n
    return float(-(p * np.log2(p)).sum())


def bucket(x, edges):
    return np.searchsorted(edges, x)


def xent(mod, occ):
    occ = occ.astype(np.float64)
    n = occ.sum()
    if n == 0:
        return 0.0
    return float(-(occ * np.log2(np.maximum(mod, 1e-12))).sum() / n)


def census_file(png):
    """(wire, pos, dims, counts) for the balanced file's record stream —
    symbols restricted to mode-0 tiles; positions = (y, x, channel)."""
    out = "/tmp/mnf16_s1a_work.mnf8"
    r = subprocess.run([str(ROOT / "target/release/manifold"), "encode",
                        str(png), "-o", out, "--mnf8", "--profile", "balanced"],
                       capture_output=True, text=True, cwd=str(ROOT))
    assert r.returncode == 0, (png, r.stderr[-400:])
    blob = Path(out).read_bytes()
    w, h, tile, tile_count, plen, flags, backend = struct.unpack("<IIIIIII", blob[4:32])
    payload = blob[32:32 + plen]
    if backend not in (0, 1):
        # plain zlib/zstd-container payloads code the zstd level, not the
        # pick's own stream; the mix backends need the Rust bridge (stage 2)
        return (None, None, (h, w), 0, 0, False, f"backend{backend}", 0)
    import zlib
    stream = (zstandard.ZstdDecompressor()
              .stream_reader(io.BytesIO(payload)).read()
              if backend == 1 else zlib.decompress(payload))
    if flags & 0x8:
        # token wire: the payload is tag-structured literals, not the
        # residual record stream — outside the record census.
        return (None, None, (h, w), 0, 0, False, "token", 0)
    recs, consumed = rc.parse_records(stream, w, h, tile_count)
    assert consumed == len(stream), (png, "desync", consumed, len(stream))
    px = np.asarray(PIL.Image.open(png).convert("RGB"), np.int64)
    assert px.shape[:2] == (h, w)
    pred_name = rc.pred_map_for(flags)
    pmap = rc.compute_pred(pred_name, px) if pred_name else None
    cols = (w + TS - 1) // TS
    planar = bool(flags & 0x1)
    reference = (flags >> 6) & 0b11
    wire_l, pos_l = [], []
    mode_mode0 = 0
    for t, mode, metapos, extra, nb in recs:
        gy, gx = divmod(t, cols)
        tw = min(TS, w - gx * TS)
        th = min(TS, h - gy * TS)
        if mode != 0:
            continue  # law/const/token records: walked, not censused
        mode_mode0 += 1
        bodyf = stream[metapos:metapos + extra]
        # recon validation (the kit's mode-0 lossless anchor)
        pred3 = pmap[gy * TS:gy * TS + th, gx * TS:gx * TS + tw] \
            if pmap is not None else None
        rec = rc.reconstruct_mode0(bodyf, pred3, planar, reference, th, tw)
        assert np.array_equal(rec.astype(np.uint8),
                              px[gy * TS:gy * TS + th, gx * TS:gx * TS + tw]
                              .astype(np.uint8)), (png, t, "mode-0 recon")
        if planar:
            wp = np.frombuffer(bodyf, np.uint8).reshape(3, th, tw)
            for k in range(3):
                for ly in range(th):
                    for lx in range(tw):
                        wire_l.append(wp[k, ly, lx])
                        pos_l.append((gy * TS + ly, gx * TS + lx, k))
        else:
            wb = np.frombuffer(bodyf, np.uint8).reshape(th, tw, 3)
            for ly in range(th):
                for lx in range(tw):
                    for k in range(3):
                        wire_l.append(wb[ly, lx, k])
                        pos_l.append((gy * TS + ly, gx * TS + lx, k))
    return (np.asarray(wire_l, np.int64), np.asarray(pos_l, np.int64),
            (h, w), mode_mode0, len(recs), planar, pred_name, reference)


def features(wire, pos, dims):
    h, w = dims
    y, x, c = pos[:, 0], pos[:, 1], pos[:, 2]
    lut = -np.ones((h, w, 3), np.int64)
    lut[y, x, c] = np.arange(len(wire))

    def sym(dy, dx):
        yy = np.clip(y + dy, 0, h - 1)
        xx = np.clip(x + dx, 0, w - 1)
        li = lut[yy, xx, c]
        return np.where(li >= 0, wire[li], 128)

    a, b, cc = sym(0, -1), sym(-1, 0), sym(-1, -1)
    plane = (a + b - cc) % 256
    flatm = (((a >= b) & (a >= cc) & (b >= cc)) |
             ((a <= b) & (a <= cc) & (b <= cc)))
    pa, pb, pc = np.abs(plane.astype(np.int64) - a), \
        np.abs(plane.astype(np.int64) - b), np.abs(plane.astype(np.int64) - cc)
    pick = np.where(pa <= pb, np.where(pa <= pc, a, cc),
                    np.where(pb <= pc, b, cc))
    med = np.where(flatm, plane, pick)
    D = (np.maximum.reduce([a, b, cc, plane, med]) -
         np.minimum.reduce([a, b, cc, plane, med]))
    g8 = (np.abs(sym(0, -8) - wire) + np.abs(sym(-8, 0) - wire)) / 2
    return D.astype(np.int64), g8


def main():
    filt = sys.argv[1] if len(sys.argv) > 1 else None
    rows = []
    for png in corpus():
        if filt and filt not in png:
            continue
        try:
            wire, pos, dims, n0, nrec, planar, pred, ref = census_file(ROOT / png)
        except Exception as e:  # noqa: BLE001 — a census must not die on one file
            rows.append((png, "skip-error", "", "", str(e)[:120], "0"))
            print(png, "ERROR", str(e)[:160], flush=True)
            continue
        if wire is None:
            kind = "skip-token" if pred == "token" else pred
            rows.append((png, f"skip-{kind}", "", "", "", "0"))
            continue
        if len(wire) < 1000:
            rows.append((png, "skip-small", "", "", str(len(wire)), "0"))
            continue
        D, g8 = features(wire, pos, dims)
        s0 = np.concatenate([[128], wire[:-1]]) >> 4
        split = int(len(wire) * 0.8)
        for fname, feat in (("D", bucket(D, np.array([0, 1, 2, 4, 8, 16, 32, 64]))),
                            ("G8", bucket(g8, np.array([4, 16, 32, 64, 96, 128, 160])))):
            CB = 16
            J = np.zeros((CB, feat.max() + 1, 256))
            np.add.at(J, (s0[:split], feat[:split], wire[:split]), 1)
            CR = J.sum(axis=1)
            Je = np.zeros((CB, feat.max() + 1, 256))
            np.add.at(Je, (s0[split:], feat[split:], wire[split:]), 1)
            Jr = CR
            base_i = sum(Jr[c].sum() * xent(CR[c] / CR[c].sum(), Jr[c])
                         for c in range(CB) if CR[c].sum() and Jr[c].sum())
            adv_i = sum(J[c, f].sum() * xent(J[c, f] / J[c, f].sum(), J[c, f])
                        for c in range(CB) for f in range(J.shape[1]) if J[c, f].sum())
            evC = Je[:, :, :].sum(axis=1)
            base = sum(evC[c].sum() * xent(CR[c] / CR[c].sum(), evC[c])
                       for c in range(CB) if CR[c].sum() and evC[c].sum())
            n_ev = evC.sum()
            adv = sum(Je[c, f].sum() * xent(J[c, f] / J[c, f].sum(), Je[c, f])
                      for c in range(CB) for f in range(J.shape[1])
                      if J[c, f].sum() and Je[c, f].sum())
            rows.append((png, fname, f"{h_bits(CR):.3f}",
                         f"{(base_i - adv_i) / len(wire) * 8:.3f}",
                         f"{(base - adv) / max(n_ev, 1) * 8:.3f}", str(len(wire))))
        print(png, f"mode-0 tiles {n0}/{nrec} wire {len(wire):,} "
                   f"planar={planar} pred={pred} ref={ref}", flush=True)
    with open("/tmp/mnf16_s1a_census.csv", "w", newline="") as fo:
        wtr = csv.writer(fo)
        wtr.writerow(["file", "feat", "H_Rc", "in_sample_mb8",
                      "prequential_mb8", "N"])
        wtr.writerows(rows)
    tot = {}
    for r in rows:
        if r[1] in ("D", "G8"):
            f = tot.setdefault(r[1], [0.0, 0.0, 0.0])
            f[0] += float(r[3]) * int(r[5]) / 8.0
            f[1] += float(r[4]) * int(r[5]) / 8.0
            f[2] += int(r[5])
    for fname, (ins_b, pres_b, n) in sorted(tot.items()):
        print(f"AGG {fname}: in-sample {ins_b:,.0f} B "
              f"({ins_b / n * 8:,.3f} mb8/sym)  prequential {pres_b:,.0f} B "
              f"({pres_b / n * 8:,.3f} mb8/sym)  N={n:,}")
    print("wrote /tmp/mnf16_s1a_census.csv")


if __name__ == "__main__":
    main()
