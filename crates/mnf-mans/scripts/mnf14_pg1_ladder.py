#!/usr/bin/env python3
"""MNF14-PG1 scale ladder — the decisive test for projective RGB laws.

The FP64-transport draft lost everywhere (fixture transport tax at 256²).
Two fixes, per the shipped laws.rs precedent (u8/i8 quantized wire params):
  - color-law coefs quantized to i16 Q12 (6 B/ch-law) BEFORE eval —
    the decoder replays the quantized law exactly
  - the projection map H quantized to i16 Q12 (18 B) as before
Arms at 256/512/1024 on F2 (perspective stripes) + F1 (identity control):
  A   MED left-context (raster, no model)
  C   q12 projective law (C in (s,t) after quantized P)
  D   q12 quadratic law (6 basis terms)
  E   q12 compression-aware 2x2 split in (s,t)
plus the NEGATIVE controls at every scale (F1 identity, F3 noise).
Growth exponent per arm = OLS log2(bytes) vs log2(width) — CS1's shape.
A constant-cost law arm shows exponent ~0 vs A's ~2.0.
"""
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image
import zstandard

sys.path.insert(0, "/Users/jonbethea/projects/blacktech/manifold/crates/mnf-mans/scripts")
from mnf11_x2_dynamics import med_pred  # noqa: E402

OUT = Path("/private/tmp/mnf14_pg1")
OUT.mkdir(exist_ok=True)
ZC = zstandard.ZstdCompressor(level=19, write_checksum=True)
FRAME = 28
QFP = 14   # coordinate fixed point for s,t transport
QCO = 12   # law-coefficient fixed point (i16)

# ---------------------------------------------------------------- wire utils
def cd_g(res):
    return np.stack([(res[..., 1]) % 256,
                     (res[..., 0] - res[..., 1]) % 256,
                     (res[..., 2] - res[..., 1]) % 256], axis=-1)


def med_plane(px):
    """canonical MED per pixel — exactly per pred_map (proven walk)."""
    h, w = px.shape[:2]
    pred = np.empty((h, w, 3), np.int64)
    for gy in range(h):
        for gx in range(w):
            at = lambda y, x, c: int(px[y, x, c])
            for c in range(3):
                if gy == 0 and gx == 0:
                    pred[gy, gx, c] = 127
                elif gy == 0:
                    pred[gy, gx, c] = at(gy, gx - 1, c)
                elif gx == 0:
                    pred[gy, gx, c] = at(gy - 1, gx, c)
                else:
                    pred[gy, gx, c] = med_pred(at(gy, gx - 1, c), at(gy - 1, gx, c),
                                               at(gy - 1, gx - 1, c))
    return pred


def arm_px(px, pred):
    res = (px - pred) % 256
    assert np.array_equal((pred + res) % 256, px % 256), "recon inexact"
    return FRAME + len(ZC.compress(cd_g(res).astype(np.uint8).tobytes()))

# ---------------------------------------------------------------- projection
def quantize_H(H):
    q = np.round(H * (1 << QCO)).astype(np.int64)
    return np.clip(q, -(2 ** 15), 2 ** 15 - 1)


def proj_apply_q(qH, h, w):
    """decoder-exact (s,t) at fixed point from the quantized H (Q12)."""
    yy, xx = np.mgrid[0:h, 0:w].astype(np.int64)
    xn = (xx << QFP) // (w - 1)
    yn = (yy << QFP) // (h - 1)
    num_s = (qH[0, 0] * xn + qH[0, 1] * yn + (qH[0, 2] << QFP)) >> QCO
    num_t = (qH[1, 0] * xn + qH[1, 1] * yn + (qH[1, 2] << QFP)) >> QCO
    den = (qH[2, 0] * xn + qH[2, 1] * yn + (1 << QFP)) >> QCO
    den = np.where(den == 0, 1, den)
    s = ((num_s << QFP) // den).astype(np.float64) / (1 << QFP)
    t = ((num_t << QFP) // den).astype(np.float64) / (1 << QFP)
    return np.clip(s, 0.0, 1.0), np.clip(t, 0.0, 1.0)


def eval_law_q(coefs_q, s, t, deg):
    """decoder-exact law: coefs are i16 Q12; terms evaluated at integer
    Q14 coordinates; sums >> QCO. Terms: 1, s, t (deg1) + s²,st,t² (deg2)."""
    s_q = np.rint(s * (1 << QFP)).astype(np.int64)
    t_q = np.rint(t * (1 << QFP)).astype(np.int64)
    if deg == 1:
        terms = [1 << QFP, s_q, t_q]
    else:
        terms = [1 << QFP, s_q, t_q,
                 (s_q * s_q) >> QFP, (s_q * t_q) >> QFP, (t_q * t_q) >> QFP]
    f = np.zeros(s.shape + (3,), np.float64)
    for c in range(3):
        acc = np.zeros(s.shape, np.int64)
        for k, tk in enumerate(terms):
            acc += (coefs_q[c, k] * tk) >> QCO
        f[..., c] = acc / (1 << QFP)
    return f

# -------------------------------------------------------- fixtures / ladder
def striped_scene(h, w):
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float64)
    s = xx / (w - 1)
    base = np.clip(110 + 55 * np.sin(2 * np.pi * 6 * s), 0, 255).round().astype(np.int64)
    return np.stack([base] * 3, axis=-1)

H_TRUE = np.array([[0.9, 0.18, 12.0],
                   [0.0, 1.30, -18.0],
                   [0.0012, -0.0006, 1.0]])


def view_through(img, qH, out_h, out_w):
    s, t = proj_apply_q(qH, out_h, out_w)
    hi, wi = img.shape[:2]
    si = np.clip(np.round(s * (wi - 1)).astype(int), 0, wi - 1)
    ti = np.clip(np.round(t * (hi - 1)).astype(int), 0, hi - 1)
    return img[ti, si]

# ---------------------------------------------------------------- arms
def arm_A(px):
    return arm_px(px, med_plane(px))


def fit_law(px, pred_med, s, t, deg, qcoefs=True):
    h, w = px.shape[:2]
    err = (px - pred_med) % 256
    err = np.where(err > 128, err - 256, err)
    s_q = np.rint(s * (1 << QFP))
    t_q = np.rint(t * (1 << QFP))
    if deg == 1:
        terms = [np.ones_like(s), s, t]
    else:
        terms = [np.ones_like(s), s, t, s * s, s * t, t * t]
    M = np.stack([b.ravel() for b in terms], axis=1)
    coefs = np.stack([np.linalg.lstsq(M, err[..., c].ravel(), rcond=None)[0] for c in range(3)])
    if qcoefs:
        coefs_q = np.clip(np.round(coefs * (1 << QCO)), -(2 ** 15), 2 ** 15 - 1).astype(np.int64)
    else:
        coefs_q = coefs
    f = eval_law_q(coefs_q, s, t, deg)
    pred = np.clip(pred_med + f, 0, 255).astype(np.int64)
    tp = (18 + 1  # qH + deg byte
          + int(coefs_q.size * 2) + 1)  # i16 law coefs + q byte
    return pred, tp, coefs_q


def arm_C_q(px, qH, deg=1):
    pred_med = med_plane(px)
    s, t = proj_apply_q(qH, *px.shape[:2])
    pred, tp, _ = fit_law(px, pred_med, s, t, deg)
    return arm_px(px, pred) + tp


def arm_A_ff(px):
    """A + one q12 planar law in (x,y) — the identity-map fair-match arm
    (B rewritten with the SAME quantized transport as C; no fp64 tax)."""
    h, w = px.shape[:2]
    pred_med = med_plane(px)
    s = np.tile(np.linspace(0, 1, w), (h, 1))
    t = np.tile(np.linspace(0, 1, h), (w, 1)).T
    pred, tp, _ = fit_law(px, pred_med, s, t, 1)
    return arm_px(px, pred) + tp


def arm_D_q(px, qH):
    return arm_C_q(px, qH, deg=2)


def arm_E_q(px, qH):
    """compression-aware 2x2 in (s,t), q12 transport, honest whole-plane wire."""
    h, w = px.shape[:2]
    pred_med = med_plane(px)
    s, t = proj_apply_q(qH, h, w)
    s_med, t_med = np.median(s), np.median(t)
    m = np.ones((h, w), bool)
    quads = [m & (s <= s_med) & (t <= t_med), m & (s <= s_med) & (t > t_med),
             m & (s > s_med) & (t <= t_med), m & (s > s_med) & (t > t_med)]
    # root
    pred_root, tp_root, _ = fit_law(px, pred_med, s, t, 1)
    cost_root = arm_px(px, pred_root) + tp_root
    # split
    pred_split = pred_med.copy()
    tp_split = 18 + 1 + 1 + 1  # qH + deg + split tag + q
    for q in quads:
        if q.sum() < 64:
            continue
        sm, tm = s[q], t[q]
        err = (px[q] - pred_med[q]) % 256
        err = np.where(err > 128, err - 256, err)
        M = np.stack([np.ones(int(q.sum())), sm, tm], axis=1)
        cq = np.stack([np.linalg.lstsq(M, err[..., c], rcond=None)[0] for c in range(3)])
        cq = np.clip(np.round(cq * (1 << QCO)), -(2 ** 15), 2 ** 15 - 1).astype(np.int64)
        f = eval_law_q(cq, s, t, 1)
        pred_split = np.clip(pred_split + f * q[..., None], 0, 255)
        tp_split += int(cq.size * 2) + 4  # coefs + per-quad s/t medians i16
    cost_split = arm_px(px, pred_split.astype(np.int64)) + tp_split + 1
    return (cost_split, "split") if cost_split < cost_root else (cost_root, "root")

# ---------------------------------------------------------------- main
def main():
    rows = []
    qH = quantize_H(H_TRUE)
    for tag, build in (("F1-identity", lambda n: striped_scene(n, n)),
                       ("F2-perspective", lambda n: view_through(striped_scene(max(n // 2, 16), max(n // 2, 16)), qH, n, n)),
                       ("F3-noise", lambda n: np.random.default_rng(9).integers(0, 256, (n, n, 3), dtype=np.int64))):
        for n in (256, 512, 1024):
            img = build(n)
            row = {"image": tag, "n": n}
            row["A_med"] = arm_A(img)
            row["A_xy_law_q12"] = arm_A_ff(img)
            row["C_proj_q12"] = arm_C_q(img, qH)
            row["D_quad_q12"] = arm_D_q(img, qH)
            e, pick = arm_E_q(img, qH)
            row["E_caware_q12"] = e
            row["E_pick"] = pick
            rows.append(row)
            print(json.dumps(row), flush=True)

    (OUT / "pg1_ladder.json").write_text(json.dumps(rows, indent=1))
    print("wrote", OUT / "pg1_ladder.json")


if __name__ == "__main__":
    main()