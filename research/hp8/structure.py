#!/usr/bin/env python3
"""HP8-S — descriptive probability-structure census over production vectors.

On the HP1 .vec prefixes (u32 2^24 rows) + full-stream trace aggregates:
effective alphabet (participation ratio), top-k mass, duplicate-value runs
(adjacent-equal frequencies = run-length compressibility S3), frequency
sparsity above the mandatory floor (S1/S2), symmetry f[x] vs f[255-x] of the
signed mirror (S4), piecewise-constant approximation error (S5), and
temporal stability (per-4096-row-block mean vectors, S6).
"""
import gzip
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from precision import TEST, TRAIN, TRACES, load_prefix

OUT = os.path.join(TRACES, "..", "hp8-structure.csv")


def stats_for(stem):
    vecs, ent = load_prefix(stem)
    if vecs is None:
        return None
    S = float(1 << 24)
    p = vecs / S
    mean_p = p.mean(axis=0)
    srt = np.sort(vecs, axis=1)
    pr = 1.0 / (p ** 2).sum(axis=1)                      # participation ratio
    topk = np.cumsum(srt[:, ::-1], axis=1) / S
    dup_runs = (np.diff(vecs, axis=1) == 0).sum(axis=1)  # adjacent-equal pairs
    floor_cnt = (vecs == 1).sum(axis=1)
    # signed-mirror symmetry: pairs (x, 256-x) for x = 1..127; 0 and 128 are
    # self-paired and excluded
    a = vecs[:, 1:128]
    b = vecs[:, 255:128:-1]
    sym = np.abs(a - b).sum(axis=1) / S
    # piecewise-constant: 16-bin geometric mean vs actual mean
    blocks = mean_p.reshape(16, 16)
    approx = np.repeat(blocks.mean(axis=1, keepdims=True), 16, axis=1).reshape(256)
    pc_err = float(np.abs(mean_p - approx).sum() / mean_p.sum())
    # temporal stability: split-half mean vector L1
    half = vecs.shape[0] // 2
    stab = float(np.abs(p[:half].mean(axis=0) - p[half:2 * half].mean(axis=0)).sum())
    return dict(
        image=stem, rows=vecs.shape[0],
        eff_alphabet_mean=round(float(pr.mean()), 1),
        top1_mass_mean=round(float(topk[:, 0].mean()), 4),
        top8_mass_mean=round(float(topk[:, 7].mean()), 4),
        dup_adjacent_pairs_mean=round(float(dup_runs.mean()), 1),
        floor_1_cells_total=int(floor_cnt.sum()),
        sym_l1_mean=round(float(sym.mean()), 4),
        pc16_l1_mean=round(pc_err, 4),
        temporal_l1_half=round(stab, 4),
    )


def main():
    rows = []
    for stem in TRAIN + TEST:
        r = stats_for(stem)
        if r:
            rows.append(r)
            print({k: v for k, v in list(r.items())[:6]})
    import pandas as pd
    df = pd.DataFrame(rows)
    df.to_csv(OUT, index=False)
    print(df.drop(columns=["image"]).mean(numeric_only=True).round(3).to_string())
    print("wrote", OUT)


if __name__ == "__main__":
    main()
