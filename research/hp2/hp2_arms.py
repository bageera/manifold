#!/usr/bin/env python3
"""MNF-HP2 — compact mathematical corrections over production P0.

Populations (vec = per-image probability-vector prefix, contiguous, exact
model replay; 40k rows/image cap from the HP1 dump):
  P1 all-classes: 19 train images -> 9 heldout-photos (cross-class transfer)
  P2 class-matched: 19 photos -> 9 heldout-photos (the HP1b survivor regime)

Arms:
  A0    unmodified production model (baseline)
  A1    precision floor: LR round 2^24 -> 2^16 (N2 largest-remainder rule)
  A2    scalar temperature P ~ P0^gamma renormalized (1 param)
  A3    per-confidence-bucket temperature (16 params)  [HP1-F probe]
  MIX   probability interpolation P = (1-lam) P0 + lam C(b,F,y), Laplace
        count tables on the HP1b equality/flatness features (X_lu, X_ud,
        X_flat; 3 bins each) and on the census survivor A_U2 (9 bins)
  A8    quantized MIX tables: u8 exact-256ths and row-scaled u4 cells,
        byte sizes CHARGED
  AX/AN permutation / random-feature controls
  NULL  confidence-bucket-only table

Both accounting regimes are reported and never mixed:
  standalone      gain_bytes - model_bytes on the same held-out pool
  installed-codec gain_bytes alone; the table is a shared offline asset
                  whose size is reported separately

Output: research/results/hp2-model-comparison.csv
"""
import csv
import os

import numpy as np
import pandas as pd

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
TRACES = os.path.join(ROOT, "research", "results", "hp1-native-traces")
OUT_CSV = os.path.join(ROOT, "research", "results", "hp2-model-comparison.csv")

NBINS = 8
NF = NBINS + 1
NF_EQ = 3
CONF_BUCKETS = 16
ALPHA = 1.0
LAMS = [0.02, 0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.4]
RNG = np.random.default_rng(31337)

PHOTOS_TRAIN = ["autumn_192x144_s900001", "dusk_160x120_s900003", "forest_224x160_s900002",
                "ocean_192x144_s900007", "sand_160x120_s900000", "slate_192x144_s900004",
                "slate_224x160_s900008", "stone_160x120_s900006", "terrain_160x120_s900009",
                "terrain_224x160_s900005"]
ALL_TRAIN = ["solid_256x256", "gradient_320x240", "repeat_256x256", "screenshot_640x400",
             "noise_256x256", "document_200x280", "plaid_192", "poster_208x144", "blob_224x160"] + PHOTOS_TRAIN
TEST = ["autumn_160x120_s3100006", "dusk_160x120_s3100003", "dusk_224x160_s3100002",
        "forest_192x144_s3100001", "forest_224x160_s3100005", "forest_224x160_s3100008",
        "ocean_192x144_s3100007", "sand_192x144_s3100004", "slate_160x120_s3100009"]


def conf_bucket(ent):
    return np.clip((ent.astype(np.int64) // 128), 0, CONF_BUCKETS - 1)


def bin_f(vals, valid):
    v = np.where(valid, vals.astype(np.int64), 0)
    return np.where(valid, (v // 32) % NBINS, NF - 1).astype(np.int64)


def load_rows(stem):
    df = pd.read_csv(os.path.join(TRACES, stem + ".csv.gz"))
    vp = os.path.join(TRACES, stem + ".vec")
    n = int(np.fromfile(vp, dtype=np.uint32, count=1)[0])
    df = df.iloc[:n]
    vecs = np.fromfile(vp, dtype=np.uint32, offset=4).reshape(n, 256)
    r = dict(
        b=conf_bucket(df["ent"].to_numpy()),
        y=df["y"].to_numpy(np.int64),
        p0=vecs.astype(np.float64) / float(2 ** 24),
        n=n,
    )
    vi = df["vi"].to_numpy(np.int64)
    vu = df["vu"].to_numpy(np.int64)
    l1 = df["l1"].to_numpy(np.int64)
    u1 = df["u1"].to_numpy(np.int64)
    dg = df["dg"].to_numpy(np.int64)
    ok = ((vi & 1) != 0) & ((vu & 1) != 0) & ((vi >> 6) != 0)
    inv = NF_EQ - 1
    r["F"] = {
        "X_lu": np.where(ok, (l1 == u1).astype(np.int64), inv),
        "X_ud": np.where(ok, (u1 == dg).astype(np.int64), inv),
        "X_flat": np.where(ok, ((np.abs(u1 - dg) <= 2) & (np.abs(l1 - dg) <= 2)).astype(np.int64), inv),
        "A_U2": bin_f(df["u2"].to_numpy(), (vu >> 1) != 0),
    }
    return r


class Table:
    def __init__(self, nf):
        self.nf = nf
        self.cnt = np.zeros(CONF_BUCKETS * nf * 256, np.float64)

    def add(self, b, fb, y):
        self.cnt += np.bincount((b * self.nf + fb) * 256 + y, minlength=self.cnt.size)

    def freeze(self):
        t = self.cnt.reshape(CONF_BUCKETS, self.nf, 256) + ALPHA
        self.tbl = t / t.sum(axis=2, keepdims=True)
        return self

    def gains(self, b, fb, y, p0y, lam):
        mix = (1.0 - lam) * p0y + lam * self.tbl[b, fb, y]
        return np.log2(np.maximum(mix, 1e-12) / np.maximum(p0y, 1e-12))


def model_bytes_u8(tbl):
    q = np.clip(np.round(tbl.reshape(-1, 256) * 256), 0, 255)
    return int(q.nbytes)


def model_bytes_u4(tbl):
    rows = tbl.reshape(-1, 256)
    mx = rows.max(axis=1, keepdims=True)
    q = np.clip(np.round(rows / np.maximum(mx, 1e-9) * 15), 0, 15).astype(np.uint8)
    return int(q.shape[0] * 129)


def bits(p, y):
    """Per-row -log2 of the observed symbol's probability from a (n,256) table."""
    return -np.log2(np.maximum(p[np.arange(len(y)), y], 1e-12))


def p0y(r):
    """P0's probability of the observed symbol, per row."""
    return r["p0"][np.arange(r["n"]), r["y"]]


def gain(new_p, r):
    """Held-out coding improvement of `new_p` over P0, in BITS.

    Convention (same as the hp1 census): sum of log2(P_new(y)/P0(y)) —
    POSITIVE = the corrected distribution codes the observed stream for
    FEWER bits than production P0. Every arm and every lambda/gamma picker
    in this script goes through this single definition.
    """
    new_y = new_p[np.arange(r["n"]), r["y"]]
    base = p0y(r)
    return float(np.sum(np.log2(np.maximum(new_y, 1e-12) / np.maximum(base, 1e-12))))


def main():
    os.chdir(ROOT)
    data = {}
    for stem in ALL_TRAIN + TEST:
        try:
            data[stem] = load_rows(stem)
        except (FileNotFoundError, OSError):
            print("missing", stem)
    dev = {}
    rngall = np.random.default_rng(47)
    for stem in ALL_TRAIN:
        r = data[stem]
        dev[stem] = rngall.choice(r["n"], size=max(1, r["n"] // 20), replace=False)

    out_rows = []

    def eval_pop(pop, name, stem_bits, model_bytes=0, note=""):
        stems = TEST
        agg = sum(stem_bits.get(s, 0.0) for s in stems)
        rows = sum(data[s]["n"] for s in stems)
        gb = agg / 8.0
        out_rows.append(dict(
            population=pop, arm=name, model_bytes=model_bytes,
            test_gain_bits=round(agg, 1), test_rows=rows,
            gain_per_krows=round(agg / rows * 1e3, 2),
            gain_bytes=round(gb, 1),
            standalone_net_bytes=round(gb - model_bytes, 1),
            installed_net_bytes=round(gb, 1), note=note))
        print(f"[{pop}] {name:14s} model {model_bytes:7d} B  gain {agg:10.1f} bits "
              f"({out_rows[-1]['gain_per_krows']:7.2f}/krow) "
              f"standalone {out_rows[-1]['standalone_net_bytes']:9.1f} B", flush=True)

    # ---------- P1: all-classes, calibration + census-survivor ----------
    t_au2 = Table(NF)
    tnull = Table(1)
    tax = Table(NF)
    tan = Table(NF)
    for stem in ALL_TRAIN:
        r = data[stem]
        fb = r["F"]["A_U2"]
        t_au2.add(r["b"], fb, r["y"])
        tnull.add(r["b"], np.zeros(r["n"], np.int64), r["y"])
        perm = rngall.permutation(r["n"])
        tax.add(r["b"], fb[perm], r["y"])
        tan.add(r["b"], rngall.integers(0, NF, r["n"]), r["y"])
    for t in (t_au2, tnull, tax, tan):
        t.freeze()

    lam = None
    best = -1e18
    for cand in LAMS:
        g = 0.0
        for stem in ALL_TRAIN:
            r = data[stem]
            ii = dev[stem]
            p0y = r["p0"][np.arange(len(ii)), r["y"][ii]]
            g += float(np.sum(t_au2.gains(r["b"][ii], r["F"]["A_U2"][ii], r["y"][ii], p0y, cand)))
        if g > best:
            best, lam = g, cand
    print(f"P1 lam* {lam}")

    per = {s: 0.0 for s in TEST}
    eval_pop("P1", "A0", per, 0, "baseline (0 by definition)")
    per = {}
    for stem in TEST:
        r = data[stem]
        p24 = np.rint(r["p0"] * (1 << 24)).astype(np.int64)
        fs = p24 >> 8
        rem = (p24 & 255).astype(np.float64)
        kk = np.minimum((1 << 16) - fs.sum(axis=1), 256)
        order = np.argsort(-rem, axis=1, kind="stable")
        mask = np.zeros(fs.shape, bool)
        for i in range(fs.shape[0]):
            mask[i, order[i, : kk[i]]] = True
        fs2 = fs + mask
        for i, j in zip(*np.where(fs2 == 0)):
            fs2[i, int(fs2[i].argmax())] -= 1
            fs2[i, j] = 1
        per[stem] = gain(fs2 / 65536.0, r)
    eval_pop("P1", "A1", per, 0, "LR 2^24->2^16 precision floor")

    # A2 scalar temperature: gamma picked on the TRAIN dev subsample (never
    # on test), one global parameter.
    gams = (0.85, 0.95, 1.0, 1.1, 1.25)
    dev_curves = {}
    for gam in gams:
        g = 0.0
        for stem in ALL_TRAIN:
            r = data[stem]
            ii = dev[stem]
            pg = np.power(r["p0"][ii], gam)
            pg /= pg.sum(axis=1, keepdims=True)
            new_y = pg[np.arange(len(ii)), r["y"][ii]]
            base = r["p0"][ii][np.arange(len(ii)), r["y"][ii]]
            g += float(np.sum(np.log2(np.maximum(new_y, 1e-12) / np.maximum(base, 1e-12))))
        dev_curves[gam] = g
    gam_star = max(dev_curves, key=dev_curves.get)
    print(f"P1 gamma* {gam_star} (dev curve {[(k, round(v)) for k, v in dev_curves.items()]})")
    per = {}
    for stem in TEST:
        r = data[stem]
        if gam_star == 1.0:
            per[stem] = 0.0
            continue
        p = np.power(r["p0"], gam_star)
        p /= p.sum(axis=1, keepdims=True)
        per[stem] = gain(p, r)
    eval_pop("P1", "A2", per, 1, f"scalar temperature (dev-picked gamma={gam_star})")

    # A3 per-bucket temperature: gammas fit on TRAIN rows per bucket.
    gam_by_b = {}
    for stem in ALL_TRAIN:
        r = data[stem]
        for bb in range(CONF_BUCKETS):
            if bb in gam_by_b:
                continue
            m = r["b"] == bb
            if m.sum() < 512:
                continue
            pg, yg = r["p0"][m], r["y"][m]
            best_g, best_gam = 0.0, 1.0
            for gam in gams:
                if gam == 1.0:
                    continue
                p = np.power(pg, gam)
                p /= p.sum(axis=1, keepdims=True)
                g = float(np.sum(np.log2(np.maximum(p[np.arange(len(yg)), yg], 1e-12)
                                        / np.maximum(pg[np.arange(len(yg)), yg], 1e-12))))
                if g > best_g:
                    best_g, best_gam = g, gam
            gam_by_b[bb] = (best_gam, best_g)
        # bucket-fit gains must be charged on TRAIN; report the sum too
    fit_gain = sum(g for _, g in gam_by_b.values())
    per = {}
    for stem in TEST:
        r = data[stem]
        gams_row = np.array([gam_by_b.get(bb, (1.0, 0.0))[0] for bb in r["b"]])
        if np.all(gams_row == 1.0):
            per[stem] = 0.0
            continue
        p = r["p0"].copy()
        for gam in np.unique(gams_row):
            if gam == 1.0:
                continue
            m = gams_row == gam
            pg = np.power(r["p0"][m], gam)
            pg /= pg.sum(axis=1, keepdims=True)
            p[m] = pg
        per[stem] = gain(p, r)
    eval_pop("P1", "A3", per, CONF_BUCKETS,
             f"per-bucket temperature (HP1-F probe; train-fit gain {fit_gain:.0f} bits)")

    per = {}
    for stem in TEST:
        r = data[stem]
        p0y = bits(r["p0"], r["y"])
        g = t_au2.gains(r["b"], r["F"]["A_U2"], r["y"], r["p0"][np.arange(r["n"]), r["y"]], lam)
        tot = r["p0"][np.arange(r["n"]), r["y"]]
        mix = (1 - lam) * tot + lam * t_au2.tbl[r["b"], r["F"]["A_U2"], r["y"]]
        per[stem] = float(np.sum(np.log2(np.maximum(mix, 1e-12) / np.maximum(tot, 1e-12))))
    eval_pop("P1", "MIX-A_U2", per, model_bytes_u8(t_au2.tbl), f"A_U2 table lam={lam}")
    per = {}
    for stem in TEST:
        r = data[stem]
        tot = r["p0"][np.arange(r["n"]), r["y"]]
        mix = (1 - lam) * tot + lam * tnull.tbl[r["b"], np.zeros(r["n"], np.int64), r["y"]]
        per[stem] = float(np.sum(np.log2(np.maximum(mix, 1e-12) / np.maximum(tot, 1e-12))))
    eval_pop("P1", "NULL", per, 16 * 256, "confidence-bucket-only")
    per = {}
    for stem in TEST:
        r = data[stem]
        tot = r["p0"][np.arange(r["n"]), r["y"]]
        fb = r["F"]["A_U2"][rngall.permutation(r["n"])]
        mix = (1 - lam) * tot + lam * tax.tbl[r["b"], fb, r["y"]]
        per[stem] = float(np.sum(np.log2(np.maximum(mix, 1e-12) / np.maximum(tot, 1e-12))))
    eval_pop("P1", "AX", per, model_bytes_u8(t_au2.tbl), "shuffled-feature control")
    per = {}
    for stem in TEST:
        r = data[stem]
        tot = r["p0"][np.arange(r["n"]), r["y"]]
        fb = rngall.integers(0, NF, r["n"])
        mix = (1 - lam) * tot + lam * tan.tbl[r["b"], fb, r["y"]]
        per[stem] = float(np.sum(np.log2(np.maximum(mix, 1e-12) / np.maximum(tot, 1e-12))))
    eval_pop("P1", "AN", per, model_bytes_u8(t_au2.tbl), "random-feature control")

    # ---------- P2: class-matched photos, X-family tables ----------
    tables = {}
    for fname, nf in (("X_lu", NF_EQ), ("X_ud", NF_EQ), ("X_flat", NF_EQ)):
        t = Table(nf)
        for stem in PHOTOS_TRAIN:
            r = data[stem]
            t.add(r["b"], r["F"][fname], r["y"])
        tables[fname] = t.freeze()
    lams_pick = {}
    for fname, t in tables.items():
        best, ls = -1e18, 0.0
        for cand in LAMS:
            g = 0.0
            for stem in PHOTOS_TRAIN:
                r = data[stem]
                ii = dev[stem]
                p0y = r["p0"][np.arange(len(ii)), r["y"][ii]]
                g += float(np.sum(t.gains(r["b"][ii], r["F"][fname][ii], r["y"][ii], p0y, cand)))
            if g > best:
                best, ls = g, cand
        lams_pick[fname] = ls
        print(f"P2 {fname} lam* {ls}")
    for fname, t in tables.items():
        per = {}
        ls = lams_pick[fname]
        for stem in TEST:
            r = data[stem]
            tot = r["p0"][np.arange(r["n"]), r["y"]]
            mix = (1 - ls) * tot + ls * t.tbl[r["b"], r["F"][fname], r["y"]]
            per[stem] = float(np.sum(np.log2(np.maximum(mix, 1e-12) / np.maximum(tot, 1e-12))))
        eval_pop("P2", f"MIX-{fname}", per, model_bytes_u8(t.tbl), f"lam={ls}")
        eval_pop("P2", f"MIX-{fname}-u4", per, model_bytes_u4(t.tbl), "u4 row-scaled")
        eval_pop("P2", f"MIX-{fname}-installed", per, 0,
                 "installed-codec regime (asset separate)")

    with open(OUT_CSV, "w", newline="") as h:
        w = csv.DictWriter(h, fieldnames=list(out_rows[0].keys()))
        w.writeheader()
        w.writerows(out_rows)
    print("wrote", OUT_CSV)


if __name__ == "__main__":
    main()
