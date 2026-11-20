#!/usr/bin/env python3
"""MNF-HP1 — native conditional-information census.

Measures, against the PRODUCTION probability model P0 (backends 4-7, replayed
in-crate by mnf_hp1_census), the held-out predictive gain of decoder-available
feature families:

  HP1-A extended spatial (left/upper at distances 1,2,4,8,16,32, diagonal)
  HP1-B geometric/differential (LOCO switch, MED proxy, Laplacian, curvature)
  HP1-C cross-channel (other lanes' same-position bytes)
  HP1-D regional statistics (record mode, tile-local row band, rolling model
        surprise over the past 8/64 positions)
  HP1-E nonlocal recurrence (causal patch match distance buckets)
  (null/AX/AN controls: confidence-bucket-only, shuffled-feature,
   random-feature)

Corrector = interpolated mixture  P(y) = (1-lam)*P0(y) + lam*C(b,F,y)
with C the Laplace-smoothed (confidence-bucket, feature-bin, symbol) count
table. Counts come from TRAIN images only; lam from a random 20% train
subsample; gain reported per image and aggregated over the held-out
heldout-photos/* TEST pool as sum log2(P/P0) bits. Positive gain = the
feature carries incremental information about Y beyond the production
byte-trigram + lane-class context.

Run:  ~/.hermes/cache/scratch/hp-venv/bin/python3 research/hp1/census.py
Output: research/results/hp1-feature-census.csv
"""
import csv
import os

import numpy as np
import pandas as pd

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
TRACES = os.path.join(ROOT, "research", "results", "hp1-native-traces")
OUT_CSV = os.path.join(ROOT, "research", "results", "hp1-feature-census.csv")

DIST = [1, 2, 4, 8, 16, 32]
NBINS = 8                # value bins of width 32 (+1 invalid-sentinel bin)
NF = NBINS + 1
CONF_BUCKETS = 16
ALPHA = 1.0
LAMS = [0.0, 0.02, 0.05, 0.1, 0.15, 0.25, 0.4, 0.6]
RNG = np.random.default_rng(20261119)

TRAIN = ["solid_256x256", "gradient_320x240", "repeat_256x256", "screenshot_640x400",
         "noise_256x256", "document_200x280", "plaid_192", "poster_208x144", "blob_224x160",
         "autumn_192x144_s900001", "dusk_160x120_s900003", "forest_224x160_s900002",
         "ocean_192x144_s900007", "sand_160x120_s900000", "slate_192x144_s900004",
         "slate_224x160_s900008", "stone_160x120_s900006", "terrain_160x120_s900009",
         "terrain_224x160_s900005"]
TEST = ["autumn_160x120_s3100006", "dusk_160x120_s3100003", "dusk_224x160_s3100002",
        "forest_192x144_s3100001", "forest_224x160_s3100005", "forest_224x160_s3100008",
        "ocean_192x144_s3100007", "sand_192x144_s3100004", "slate_160x120_s3100009"]


def bin_f(vals, valid):
    """8 value bins of width 32; sentinel bin NF-1 when invalid."""
    v = np.where(valid, vals.astype(np.int64), 0)
    return np.where(valid, (v // 32) % NBINS, NF - 1).astype(np.int64)


def conf_bucket(ent):
    """Decoder-available pre-symbol confidence bucket.

    ent describes the WHOLE P0 vector before the symbol is known, so
    conditioning on it leaks nothing about y. 16 buckets over 0..8 bits
    of model entropy (ent is the trace's entropy x256).
    """
    return np.clip((ent.astype(np.int64) // 128), 0, CONF_BUCKETS - 1)


class Table:
    """Laplace-smoothed (b, F, y) count table + mixture gain evaluation."""

    def __init__(self, nf=NF):
        self.nf = nf
        self.cnt = np.zeros(CONF_BUCKETS * nf * 256, np.float64)

    def add(self, b, fb, y):
        idx = (b * self.nf + fb) * 256 + y
        self.cnt += np.bincount(idx, minlength=self.cnt.size)

    def freeze(self):
        t = self.cnt.reshape(CONF_BUCKETS, self.nf, 256) + ALPHA
        self.tbl = t / t.sum(axis=2, keepdims=True)
        return self

    def gains(self, b, fb, y, p0row, lam):
        c = self.tbl[b, fb, y]
        mix = (1.0 - lam) * p0row + lam * c
        with np.errstate(divide="ignore"):
            return np.log2(mix / p0row)


class PatchRec:
    """Causal first-repeat distance of the (l2,l1,dg,u1,u2) patch."""

    def __init__(self):
        self.last = {}

    def run(self, l2, l1, dg, u1, u2, pos):
        key = (l1.astype(np.uint64) << 40) | (l2.astype(np.uint64) << 32) \
            | (dg.astype(np.uint64) << 24) | (u1.astype(np.uint64) << 16) | u2.astype(np.uint64)
        out = np.full(len(key), -1, np.int64)
        last = self.last
        for i in range(len(key)):
            k = key[i]
            r = last.get(k, -1)
            out[i] = r
            last[k] = pos + i
        return out


def dist_bucket(d):
    """0 = unseen; 1..7 else log-scaled distance buckets (8 = invalid sentinel)."""
    b = np.zeros(len(d), np.int64)
    for i, v in enumerate(d):
        if v > 0:
            gap = 1
            while (1 << (2 * (gap - 1))) < v and gap < 7:
                gap += 1
            b[i] = gap
    return b


def features_of(stem):
    path = os.path.join(TRACES, stem + ".csv.gz")
    if not os.path.exists(path):
        return None
    df = pd.read_csv(path)
    n = len(df)
    py = df["py"].to_numpy(np.float64)
    y = df["y"].to_numpy(np.int64)
    b = conf_bucket(df["ent"].to_numpy())
    p0row = py / float(2 ** 24)
    vi = df["vi"].to_numpy(np.int64)
    vu = df["vu"].to_numpy(np.int64)
    F = {}
    for i, d in enumerate(DIST):
        lv = (vi >> i) != 0
        uv = (vu >> i) != 0
        F[f"A_L{d}"] = bin_f(df[f"l{d}"].to_numpy(), lv)
        F[f"A_U{d}"] = bin_f(df[f"u{d}"].to_numpy(), uv)
    l1 = df["l1"].to_numpy(np.int64)
    u1 = df["u1"].to_numpy(np.int64)
    dg = df["dg"].to_numpy(np.int64)
    l2 = df["l2"].to_numpy(np.int64)
    u2 = df["u2"].to_numpy(np.int64)
    base_ok = ((vi & 1) != 0) & ((vu & 1) != 0) & ((vi >> 6) != 0)
    e, s = u1 - dg, l1 - dg
    F["B_e"] = bin_f(np.abs(e) % 256, base_ok)
    F["B_s"] = bin_f(np.abs(s) % 256, base_ok)
    F["B_med"] = bin_f((l1 + u1 - dg) % 256, base_ok)
    F["B_lap"] = bin_f(np.abs(l1 + u1 - 2 * dg) % 64, base_ok)
    F["B_loco"] = bin_f(np.abs(np.where(np.abs(s) > np.abs(e), e, s)) % 256, base_ok)
    F["C_pa"] = bin_f(df["pa"].to_numpy(), (vi >> 7) != 0)
    F["C_pb"] = bin_f(df["pb"].to_numpy(), (vu >> 7) != 0)
    F["D_mode"] = bin_f(df["mode"].to_numpy(), np.ones(n, bool))
    F["D_ty"] = bin_f(df["ty"].to_numpy() * 16, ((vi & 1) != 0))
    bits = 24.0 - np.log2(py)
    for w in (8, 64):
        cs = np.concatenate([[0.0], np.cumsum(bits)])
        rolled = np.zeros(n)
        rolled[w:] = cs[w:-1] - cs[1:n - w + 1] if n > w else 0.0
        F[f"D_sur{w}"] = bin_f(np.clip(rolled, 0, 255), np.ones(n, bool))
    okE = ((vi & 3) == 3) & ((vu & 3) == 3) & ((vi >> 6) != 0)
    pr = PatchRec()
    d = pr.run(l2, l1, dg, u1, u2, 0)
    eb = np.zeros(n, np.int64)
    eb[okE] = dist_bucket(d[okE])
    F["E_patch"] = eb
    return dict(b=b, y=y, p0row=p0row, F=F, n=n, okE=okE, bits=bits)


def main():
    frames = {}
    for stem in TRAIN + TEST:
        fr = features_of(stem)
        if fr is None:
            print("missing trace:", stem)
            continue
        frames[stem] = fr
        print(f"{stem}: rows {fr['n']} a0 {fr['bits'].sum():.0f} bits", flush=True)

    feats = sorted({k for fr in frames.values() for k in fr["F"]})
    # ---- counts from TRAIN (+ small DEV subsample), all features ----
    tables = {f: Table() for f in feats}
    dev = {}   # stem -> subsample index arrays (for lam selection)
    for stem in TRAIN:
        fr = frames[stem]
        idx = np.arange(fr["n"])
        rng = np.random.default_rng(31 + len(stem))
        dev[stem] = rng.choice(idx, size=max(1, fr["n"] // 20), replace=False)
        for f in feats:
            tables[f].add(fr["b"], fr["F"][f], fr["y"])
    # ---- controls ----
    null_t = Table(nf=1)
    ax_t = {f: Table() for f in feats}
    rng_all = np.random.default_rng(777)
    for stem in TRAIN:
        fr = frames[stem]
        for f in feats:
            # permutation control: shuffle FEATURE VALUES across rows while
            # keeping every (b, y) pair in place — the count table then has
            # identical marginals with the F<->Y association broken.
            fb = fr["F"][f][rng_all.permutation(fr["n"])]
            ax_t[f].add(fr["b"], fb, fr["y"])
        null_t.add(fr["b"], np.zeros(fr["n"], np.int64), fr["y"])
    for t in tables.values():
        t.freeze()
    for t in ax_t.values():
        t.freeze()
    null_t.freeze()

    print("counts built; evaluating", flush=True)
    rows_out = []
    for f in feats:
        per_img = {}
        agg = 0.0
        tot = 0
        # lam on the train-dev subsample
        def dev_gain(t, lam):
            g = 0.0
            for stem in TRAIN:
                fr = frames[stem]
                ii = dev[stem]
                gg = t.gains(fr["b"][ii], fr["F"][f][ii], fr["y"][ii], fr["p0row"][ii], lam)
                g += float(np.sum(gg))
            return g
        lam_star = max(LAMS, key=lambda L: dev_gain(tables[f], L))
        for stem in TEST:
            fr = frames[stem]
            gg = tables[f].gains(fr["b"], fr["F"][f], fr["y"], fr["p0row"], lam_star)
            cov = float(np.mean(fr["F"][f] != NF - 1))
            per_img[stem] = (float(np.sum(gg)), int(fr["n"]), cov)
            agg += float(np.sum(gg))
            tot += fr["n"]
        null_g = 0.0
        ax_g = 0.0
        for stem in TEST:
            fr = frames[stem]
            null_g += float(np.sum(null_t.gains(fr["b"], np.zeros(fr["n"], np.int64),
                                                fr["y"], fr["p0row"], lam_star)))
            ax_g += float(np.sum(ax_t[f].gains(fr["b"], fr["F"][f], fr["y"],
                                               fr["p0row"], lam_star)))
        vals = np.array([a / max(1, r) * 1e3 for a, r, _ in per_img.values()])  # bits per 1k rows
        rows_out.append(dict(
            feature=f, lam=lam_star,
            test_gain_bits=round(agg, 1), test_rows=tot,
            gain_per_krows=round(agg / max(1, tot) * 1e3, 2),
            coverage=round(np.mean([c for _, _, c in per_img.values()]), 4),
            per_img_min=round(vals.min(), 2), per_img_max=round(vals.max(), 2),
            per_img_std=round(vals.std(), 2),
            null_gain_per_krows=round(null_g / max(1, tot) * 1e3, 2),
            perm_gain_per_krows=round(ax_g / max(1, tot) * 1e3, 2),
        ))
        print(f"{f:12s} lam {lam_star:4.2f} test {agg:9.1f} bits "
              f"({rows_out[-1]['gain_per_krows']:7.2f}/krow) "
              f"null {rows_out[-1]['null_gain_per_krows']:7.2f} "
              f"perm {rows_out[-1]['perm_gain_per_krows']:7.2f}", flush=True)

    with open(OUT_CSV, "w", newline="") as h:
        w = csv.DictWriter(h, fieldnames=list(rows_out[0].keys()))
        w.writeheader()
        w.writerows(rows_out)
    print("wrote", OUT_CSV)


if __name__ == "__main__":
    main()
