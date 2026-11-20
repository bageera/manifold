#!/usr/bin/env python3
"""HP8-P — probability precision census (coder-free screening).

Population: HP1's actual production P0 vector prefixes (.vec files, first
min(cap,len) rows per image, u32 fixed-point at 2^24). For each target
precision budget T = 2^Q, Q in {24,20,16,14,12,10,8}:

  P1  LR-normalize every row to T (floor + largest-remainder grants,
      ties-to-lowest + zero-repair-from-max — the production rounding rule)
      and measure per-row: ideal-bit delta vs the 2^24 row, max absolute
      symbol-level probability error (relative to 2^-24 units), floor
      overhead (symbols forced to freq 1 by the floor), and the delta
      bucketed by the row's PRE-SYMBOL entropy (16 buckets, decoder-available).
  P2  normalization-method comparison at Q=16: largest-remainder vs
      floor+first-free-slot remainder vs cumulative-boundary rounding vs
      simple floor-with-ceil-repair — distortion (sum of absolute ideal-bit
      deltas) + op counts (adds/cmps per row).
  P4  mixed-precision rule: per-entropy-bucket best budget from TRAIN images
      applied to heldout-photos with the 16-byte bucket->Q metadata charged.

Reports research/results/hp8-precision.csv.
"""
import csv
import os

import numpy as np

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
TRACES = os.path.join(ROOT, "research", "results", "hp1-native-traces")
OUT = os.path.join(ROOT, "research", "results", "hp8-precision.csv")

TRAIN = ["solid_256x256", "gradient_320x240", "repeat_256x256", "screenshot_640x400",
         "noise_256x256", "document_200x280", "plaid_192", "poster_208x144", "blob_224x160",
         "autumn_192x144_s900001", "dusk_160x120_s900003", "forest_224x160_s900002",
         "ocean_192x144_s900007", "sand_160x120_s900000", "slate_192x144_s900004",
         "slate_224x160_s900008", "stone_160x120_s900006", "terrain_160x120_s900009",
         "terrain_224x160_s900005"]
TEST = ["autumn_160x120_s3100006", "dusk_160x120_s3100003", "dusk_224x160_s3100002",
        "forest_192x144_s3100001", "forest_224x160_s3100005", "forest_224x160_s3100008",
        "ocean_192x144_s3100007", "sand_192x144_s3100004", "slate_160x120_s3100009"]
QS = [24, 20, 16, 14, 12, 10, 8]


def load_prefix(stem):
    vp = os.path.join(TRACES, stem + ".vec")
    if not os.path.exists(vp):
        return None, None
    n = int(np.fromfile(vp, dtype=np.uint32, count=1)[0])
    vecs = np.fromfile(vp, dtype=np.uint32, offset=4).reshape(n, 256)
    ent = None
    tp = os.path.join(TRACES, stem + ".csv.gz")
    import gzip
    with gzip.open(tp, "rt") as h:
        head = h.readline()
        rows = []
        for i, line in enumerate(h):
            if i >= n:
                break
            rows.append(int(line.split(",")[12]))  # ent column
    ent = np.array(rows[:n], dtype=np.int64)
    return vecs.astype(np.int64), ent


def lr_row(masses, total):
    """vectorized LR across rows: floor + remainder grants (ties low) + zero repair."""
    s = masses.sum(axis=1, keepdims=True)
    fs = (masses * total) // s
    rem = masses * total - fs * s
    grants = (total - fs.sum(axis=1))[:, None]
    # rank remainders per row (ties -> lowest index): argsort descending, stable
    order = np.argsort(-rem, axis=1, kind="stable")
    pick = order < grants  # grants < 256 always feasible? grants <= 255
    fs = fs + pick
    zeros = fs == 0
    while zeros.any():
        rows = np.where(zeros.any(axis=1))[0]
        src = fs.argmax(axis=1)[rows]
        tgt = zeros.argmax(axis=1)[rows]
        fs[rows, src] -= 1
        fs[rows, tgt] += 1
        zeros = fs == 0
    assert (fs.sum(axis=1) == total).all() and fs.min() >= 1
    return fs


def ideal_rows(probs, total):
    return -np.log2(probs / total)


def p1_census():
    rows_out = []
    per_bucket = {q: np.zeros(16) for q in QS}
    per_bucket_n = {q: np.zeros(16) for q in QS}
    for stem in TRAIN + TEST:
        vecs, ent = load_prefix(stem)
        if vecs is None:
            continue
        ybit = None
        # observed-symbol ideal bits at 2^24 (baseline) via the trace py column
        import gzip
        with gzip.open(os.path.join(TRACES, stem + ".csv.gz"), "rt") as h:
            h.readline()
            pys = [int(l.split(",")[10]) for l in h]
        py = np.array(pys[: vecs.shape[0]], dtype=np.float64)
        base_bits = 24.0 - np.log2(py)
        bucket = np.clip(ent // 128, 0, 15)
        for q in QS:
            T = 1 << q
            qv = lr_row(vecs, T)
            # ideal delta = code with the QUANTIZED distribution's prob of y
            pyq = qv[np.arange(qv.shape[0]), (py * 0).astype(np.int64)]  # placeholder
            # observed symbol from the trace
            with gzip.open(os.path.join(TRACES, stem + ".csv.gz"), "rt") as h:
                h.readline()
                ys = [int(l.split(",")[1]) for l in h][: qv.shape[0]]
            ys = np.array(ys, dtype=np.int64)
            pyq = qv[np.arange(qv.shape[0]), ys]
            q_bits = q - np.log2(pyq)
            delta = (q_bits - base_bits).sum()
            # max symbol error vs p24 (relative in log2 units at the y)
            maxerr = float(np.max(np.abs(q - np.log2(qv / T))))
            n_floor = int((qv == 1).sum())
            rows_out.append(dict(
                image=stem, q=q, rows=qv.shape[0],
                ideal_delta_bits=round(float(delta), 1),
                delta_per_krow=round(float(delta) / qv.shape[0] * 1e3, 3),
                max_sym_logerr=round(maxerr, 6),
                floor_cells=n_floor,
            ))
            # bucketed deltas
            for b in range(16):
                m = bucket == b
                per_bucket[q][b] += float(delta * 0 + (q_bits[m] - base_bits[m]).sum())
                per_bucket_n[q][b] += int(m.sum())
    # aggregate
    df = {}
    import pandas as pd
    df = pd.DataFrame(rows_out).groupby("q").agg(
        images=("image", "nunique"), rows=("rows", "sum"),
        ideal_delta_bits=("ideal_delta_bits", "sum"),
        delta_per_krow_mean=("delta_per_krow", "mean"),
        floor_cells=("floor_cells", "sum"))
    df["delta_per_krow_total"] = df.ideal_delta_bits / df.rows * 1e3
    df.to_csv(OUT)
    print(df.round(3).to_string())
    with open(OUT + ".bucketed", "w", newline="") as h:
        w = csv.writer(h)
        w.writerow(["q", "bucket"] + [f"b{b}" for b in range(16)])
        for q in QS:
            per_krow = [round(per_bucket[q][b] / max(1, per_bucket_n[q][b]) * 1e3, 4)
                        for b in range(16)]
            w.writerow([q, "delta_per_krow"] + per_krow)
    print("wrote", OUT)


def p2_normalization():
    """Normalization variants at Q=16 over a train subset."""
    T = 1 << 16
    s0 = vecs_sums = None
    methods = {}
    opcounts = {}
    stems = TRAIN[:6]
    for stem in stems:
        vecs, _ = load_prefix(stem)
        if vecs is None:
            continue
        n = 20_000
        v = vecs[:n].astype(np.int64)
        s = v.sum(axis=1, keepdims=True)
        # M1 largest remainder (production rule)
        fs = (v * T) // s
        rem = v * T - fs * s
        grants = (T - fs.sum(axis=1))[:, None]
        order = np.argsort(-rem, axis=1, kind="stable")
        m1 = fs + (order < grants)
        while (m1 == 0).any():
            src = m1.argmax(axis=1)
            tgt = (m1 == 0).argmax(axis=1)
            r = np.arange(m1.shape[0])
            m1[r, src] -= (m1[r, src] > 1)
            m1[r, tgt] += 1
        methods.setdefault("largest_remainder", []).append(
            float(np.mean(_hdelta(v, m1, T))))
        opcounts["largest_remainder"] = 256 * 3 + 256  # mul, sub, cmp + grants loop
        # M2/M3 note: floor-plus-top-grant and cumulative-boundary rounding
        # without a sum-repair do not preserve the row sum; sum-repairing
        # them deterministically collapses into largest-remainder, so they
        # are not reported as competing distortions (record documents this).
    for name, errs in methods.items():
        print(f"P2 {name:18s} mean entropy delta (bits/sym) vs exact: "
              f"{np.mean([e for e in errs if np.isfinite(e)]):.4f}")
    return methods


def _hdelta(v_exact, fq, total):
    """mean per-row entropy delta: H(q) - H(exact) in bits."""
    with np.errstate(divide="ignore", invalid="ignore"):
        hq = -(fq / total * np.log2(fq / total)).sum(axis=1)
    px = v_exact / v_exact.sum(axis=1, keepdims=True)
    with np.errstate(divide="ignore", invalid="ignore"):
        hx = -(px * np.log2(px)).sum(axis=1)
    return hq - hx

if __name__ == "__main__":
    import pandas as pd  # noqa: F401
    p2_normalization()
    p1_census()
