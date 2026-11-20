#!/usr/bin/env python3
"""MNF-HP1b — class-matched + equality-feature census variant.

Bounded follow-up before closing HP1: (1) train on PHOTOS ONLY, evaluate on
heldout-photos (class-matched transfer, no cross-class pooling); (2) add the
relation-shaped features the width-32 value bins could not express:
  X_lu  = (l1 == u1) equality indicator (0/1)
  X_ud  = (u1 == dg) equality indicator
  X_ed  = |e| <= 2 and |s| <= 2 ("flat neighborhood") indicator
  X_e8  = signed e = u1 - dg bucketed at width 8 (fine, 32 bins via v//8 % 25)
Feature bins are direct bucket ids (not value//32); Table NF sizing per arm.
"""
import csv
import os
import sys

import numpy as np
import pandas as pd

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from census import (ALPHA, CONF_BUCKETS, LAMS, NBINS, ROOT, TEST, TRAIN, Table,
                    conf_bucket, TRACES)

NF_WIDE = NBINS + 1        # 9 (value width-32 bins + invalid sentinel)
NF_EQ = 2 + 1              # 3 bins: eq / neq / invalid
NF_FLAT = 2 + 1
NF_FINE = 32 + 1           # signed e in [-128,127] -> (v+128)//8 -> 32 bins


def feature_bins_eq(stem):
    df = pd.read_csv(os.path.join(TRACES, stem + ".csv.gz"))
    n = len(df)
    b = conf_bucket(df["ent"].to_numpy())
    y = df["y"].to_numpy(np.int64)
    py = df["py"].to_numpy(np.float64)
    vi = df["vi"].to_numpy(np.int64)
    vu = df["vu"].to_numpy(np.int64)
    l1 = df["l1"].to_numpy(np.int64)
    u1 = df["u1"].to_numpy(np.int64)
    dg = df["dg"].to_numpy(np.int64)
    ok = ((vi & 1) != 0) & ((vu & 1) != 0) & ((vi >> 6) != 0)
    inv = NF_EQ - 1
    F = {}
    F["X_lu"] = np.where(ok, (l1 == u1).astype(np.int64), inv)
    F["X_ud"] = np.where(ok, (u1 == dg).astype(np.int64), inv)
    e, s = u1 - dg, l1 - dg
    F["X_flat"] = np.where(ok, ((np.abs(e) <= 2) & (np.abs(s) <= 2)).astype(np.int64), inv)
    F["X_e8"] = np.where(ok, np.clip((e + 128) // 8, 0, 31), NF_FINE - 1)
    F["_inv"] = np.where(ok, np.zeros(n, np.int64), NF_FINE - 1)
    return dict(b=b, y=y, p0row=py / float(2 ** 24), F=F, n=n)


def main():
    photos_train = [s for s in TRAIN if not s.endswith(("_256x256", "_320x240", "_640x400"))
                    and "heldout" not in s and s not in ("document_200x280", "plaid_192",
                                                         "poster_208x144", "blob_224x160")]
    photos_test = TEST
    fr = {s: feature_bins_eq(s) for s in photos_train + photos_test}
    F = {}
    for s in photos_train + photos_test:
        for k, v in fr[s]["F"].items():
            F.setdefault(k, []).append((s, v))
    names = [n for n in sorted(F) if n != "_inv"]
    # per-feature tables: photos-train only
    rows_out = []
    for name in names:
        nf = {"X_lu": NF_EQ, "X_ud": NF_EQ, "X_flat": NF_FLAT, "X_e8": NF_FINE}[name]
        t = Table(nf)
        for s in photos_train:
            v = fr[s]["F"][name]
            v = np.clip(v, 0, nf - 1)
            t.add(fr[s]["b"], v, fr[s]["y"])
        t.freeze()
        # lambda on 5% of train rows
        rng = np.random.default_rng(5)
        dev_gain = {}
        for lam in LAMS:
            g = 0.0
            for s in photos_train:
                f = fr[s]
                ii = rng.choice(f["n"], size=max(1, f["n"] // 20), replace=False)
                v = np.clip(f["F"][name], 0, nf - 1)[ii]
                mix = (1 - lam) * f["p0row"][ii] + lam * t.tbl[f["b"][ii], v, f["y"][ii]]
                g += float(np.sum(np.log2(np.maximum(mix, 1e-12)
                                         / np.maximum(f["p0row"][ii], 1e-12))))
            dev_gain[lam] = g
        lam_star = max(dev_gain, key=dev_gain.get)
        agg = 0.0
        tot = 0
        per_img = {}
        for s in photos_test:
            f = fr[s]
            v = np.clip(f["F"][name], 0, nf - 1)
            mix = (1 - lam_star) * f["p0row"] + lam_star * t.tbl[f["b"], v, f["y"]]
            g = float(np.sum(np.log2(np.maximum(mix, 1e-12)
                                    / np.maximum(f["p0row"], 1e-12))))
            per_img[s] = g / f["n"] * 1e3
            agg += g
            tot += f["n"]
        vals = np.array(list(per_img.values()))
        rows_out.append(dict(feature=name, nf=nf, lam=lam_star,
                             test_gain_bits=round(agg, 1), test_rows=tot,
                             gain_per_krows=round(agg / tot * 1e3, 2),
                             per_img_min=round(vals.min(), 2),
                             per_img_max=round(vals.max(), 2),
                             per_img_std=round(vals.std(), 2)))
        print(f"{name:8s} lam {lam_star:4.2f} test {agg:9.1f} "
              f"({rows_out[-1]['gain_per_krows']:7.2f}/krow) "
              f"img[min/max/std] {rows_out[-1]['per_img_min']}/"
              f"{rows_out[-1]['per_img_max']}/{rows_out[-1]['per_img_std']}", flush=True)

    out = os.path.join(ROOT, "research", "results", "hp1b-classmatched-census.csv")
    with open(out, "w", newline="") as h:
        w = csv.DictWriter(h, fieldnames=list(rows_out[0].keys()))
        w.writeheader()
        w.writerows(rows_out)
    print("wrote", out)


if __name__ == "__main__":
    main()
