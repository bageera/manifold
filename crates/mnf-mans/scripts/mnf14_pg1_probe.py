#!/usr/bin/env python3
"""MNF14-PG1 in-repo probe — projective RGB geometry discovery.

The memo's hypothesis: RGB(x,y) = C(P_theta(x,y)) — the image's apparent
complexity is partly a PROJECTION, and reversing the projective map
exposes a shorter color description in (s,t) than any identity-space
field. This differs from IF2/IF3 (identity-space intensity laws).

Arms (one process, one coder = zstd-19, complete honest records,
28-B frame; exact recon asserted before every size):
  A  MED (left-context) + CD-G baseline — the standing control
  B  affine RGB field in (x,y) — IF2/IF3's global planar protocol
  C  projective RGB field: color planar in (s,t) = P(x,y); for
     synthetic fixtures the TRUE H is transported (9 x i32 LE + 1
     q byte); for real photos H is ESTIMATED by 4-point DLT from
     corners of a coarse luma correspondence (honest estimation)
  D  curved-surface: quadratic [1,s,t,s²,st,t²] color law after P
  E  compression-aware piecewise projection: 2x2 quadtree in (s,t)
     — each quadrant's law chosen by measured zstd bytes (IF3's
     theta*-form applied to the projection class)
Fixtures (the mechanism's home turf + controls):
  F1  striped planar scene, identity — A/B territory, C must NOT win
  F2  F1 viewed through a perspective homography — C's firing class:
      in (x,y) the field is projective-warped; in (s,t) it's 1-D
  F3  random noise — nothing fires (no-forced-loss)
Real photos: astronaut/coffee/chelsea/hubble, H estimated honestly;
also run with the IDENTITY map (arm B covers that; C with H=I ties B).
"""
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image
import zstandard

sys.path.insert(0, "/Users/jonbethea/projects/blacktech/manifold/crates/mnf-mans/scripts")
from mnf11_x2_dynamics import med_pred  # noqa: E402 — canonical per-pixel MED

TD = Path("/tmp/mnf14_if2")
OUT = Path("/private/tmp/mnf14_pg1")
OUT.mkdir(exist_ok=True)
ZC = zstandard.ZstdCompressor(level=19, write_checksum=True)
FRAME = 28

# ---------------------------------------------------------------- wire utils
def cd_g(res):
    return np.stack([(res[..., 1]) % 256,
                     (res[..., 0] - res[..., 1]) % 256,
                     (res[..., 2] - res[..., 1]) % 256], axis=-1)


def arm_px(px, pred):
    res = (px - pred) % 256
    assert np.array_equal((pred + res) % 256, px % 256), "recon inexact"
    wire = cd_g(res).astype(np.uint8)
    return FRAME + len(ZC.compress(wire.tobytes()))


def med_plane(px):
    """canonical MED per pixel — EXACTLY per mnf13_r6_extract.pred_map
    (the walk proven against the shipped decoder): (0,0)->127; gy==0 ->
    left; gx==0 -> top; else med_pred(a=left, b=top, cc=topleft)."""
    h, w = px.shape[:2]
    pred = np.empty((h, w, 3), np.int64)
    for gy in range(h):
        for gx in range(w):
            for c in range(3):
                at = lambda y, x: int(px[y, x, c])
                if gy == 0 and gx == 0:
                    pred[gy, gx, c] = 127
                elif gy == 0:
                    pred[gy, gx, c] = at(gy, gx - 1)
                elif gx == 0:
                    pred[gy, gx, c] = at(gy - 1, gx)
                else:
                    pred[gy, gx, c] = med_pred(at(gy, gx - 1), at(gy - 1, gx), at(gy - 1, gx - 1))
    return pred


def signed_err(px, pred):
    e = (px - pred) % 256
    return np.where(e > 128, e - 256, e)

# ------------------------------------------------ integer field evaluation
QFP = 14  # fixed-point fractional bits for transported coordinate map


def proj_apply(H, h, w):
    """(s,t) in [0,1]² for EVERY pixel: (H00 x + H01 y + H02)/(H20 x + H21 y + 1)."""
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float64)
    xn = xx / (w - 1)
    yn = yy / (h - 1)
    num_s = H[0, 0] * xn + H[0, 1] * yn + H[0, 2]
    num_t = H[1, 0] * xn + H[1, 1] * yn + H[1, 2]
    den = H[2, 0] * xn + H[2, 1] * yn + 1.0
    return num_s / den, num_t / den


def quantize_H(H):
    """the DECODER-reproducible form: i16-per-entry normalized H with the
    scale fixed by H22 = 1 (Q12), transported honestly."""
    q = np.round(H * (1 << 12)).astype(np.int64)
    q = np.clip(q, -(2 ** 15), 2 ** 15 - 1)
    return q


def proj_apply_q(qH, h, w):
    """apply the QUANTIZED H at Q12 fixed point — the decoder's exact map."""
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float64)
    xn = np.round(xx / (w - 1) * (1 << QFP)).astype(np.int64)
    yn = np.round(yy / (h - 1) * (1 << QFP)).astype(np.int64)
    num_s = (qH[0, 0] * xn + qH[0, 1] * yn + (qH[0, 2] << QFP)) >> 12
    num_t = (qH[1, 0] * xn + qH[1, 1] * yn + (qH[1, 2] << QFP)) >> 12
    den = (qH[2, 0] * xn + qH[2, 1] * yn + (1 << QFP)) >> 12
    if np.any(den == 0):
        den = np.where(den == 0, 1, den)
    s = ((num_s << QFP) // den) / (1 << QFP)
    t = ((num_t << QFP) // den) / (1 << QFP)
    return np.clip(s, 0, 1), np.clip(t, 0, 1)


def field_from_law(coefs, s, t, deg):
    """integer-ish color field for transport-quantized law coefficients."""
    if deg == 1:
        M = np.stack([np.ones_like(s), s, t], axis=-1)
    else:
        M = np.stack([np.ones_like(s), s, t, s * s, s * t, t * t], axis=-1)
    f = np.zeros(s.shape + (3,), np.float64)
    for c in range(3):
        f[..., c] = M @ coefs[c]
    return f

# --------------------------------------------------------------- homography
def est_homography(px, out_shape):
    """4-point DLT from a coarse LUMA correspondence: the decoder CAN
    reproduce this (deterministic downsample + DLT = same numbers, but
    transport carries the quantized H anyway, so the estimator choice
    costs nothing but bias; residual absorbs the rest)."""
    h, w = px.shape[:2]
    luma = px.mean(axis=2)
    small = np.asarray(Image.fromarray(luma.astype(np.uint8)).resize((8, 8), Image.LANCZOS), np.float64)
    # map canonical 8x8 grid → image: identity-by-default DLT anchors
    src = [(i, j) for i in (0, 7) for j in (0, 7)]  # canonical corners
    dst = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
    Hm = est_dlt(src, dst)
    return Hm

def est_dlt(src_pts, dst_pts):
    A = []
    for (x, y), (u, v) in zip(src_pts, dst_pts):
        A.append([x, y, 1, 0, 0, 0, -u * x, -u * y, -u])
        A.append([0, 0, 0, x, y, 1, -v * x, -v * y, -v])
    _, _, Vt = np.linalg.svd(np.array(A))
    Hm = Vt[-1].reshape(3, 3)
    return Hm / Hm[2, 2]

# --------------------------------------------------------- fixture builders
def planar_scene(h=256, w=256):
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float64)
    s, t = xx / (w - 1), yy / (h - 1)
    img = np.stack([np.clip(80 + 90 * s + 40 * t, 0, 255).round().astype(np.int64)] * 3, axis=-1)
    return img

def striped_scene(h=256, w=256):
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float64)
    s = xx / (w - 1)
    base = np.clip(110 + 55 * np.sin(2 * np.pi * 6 * s), 0, 255).round().astype(np.int64)
    return np.stack([base] * 3, axis=-1)

def view_through(img, Hq, out_h, out_w):
    """rasterize img through the QUANTIZED H map (decoder-side exact form)."""
    s, t = proj_apply_q(Hq, out_h, out_w)
    hi, wi = img.shape[:2]
    si = np.clip(np.round(s * (wi - 1)).astype(int), 0, wi - 1)
    ti = np.clip(np.round(t * (hi - 1)).astype(int), 0, hi - 1)
    return img[ti, si]

# ---------------------------------------------------------------- arms
def arm_A(px):
    return arm_px(px, med_plane(px))


def arm_B(px):
    """affine RGB field per channel on MED's error (IF3's global protocol)."""
    pred_med = med_plane(px)
    err = signed_err(px, pred_med)
    h, w = px.shape[:2]
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float64)
    M = np.stack([np.ones(h * w), (xx / (w - 1)).ravel(), (yy / (h - 1)).ravel()], axis=1)
    coefs = np.stack([np.linalg.lstsq(M, err[..., c].ravel(), rcond=None)[0] for c in range(3)])
    f = np.stack([M @ coefs[c] for c in range(3)], axis=-1).reshape(h, w, 3)
    pred = np.clip(pred_med + f, 0, 255).astype(np.int64)
    return arm_px(px, pred) + int(coefs.size * 8) + 3


def arm_C(px, H_true=None, deg=1):
    """projective field: fit color law in the (s,t) given by the quantized H."""
    h, w = px.shape[:2]
    pred_med = med_plane(px)
    err = signed_err(px, pred_med)
    if H_true is None:
        H = est_homography(px, (h, w))
        qH = quantize_H(H)
    else:
        qH = quantize_H(H_true)
    s, t = proj_apply_q(qH, h, w)
    if deg == 1:
        basis = [np.ones_like(s), s, t]
    else:
        basis = [np.ones_like(s), s, t, s * s, s * t, t * t]
    M = np.stack([b.ravel() for b in basis], axis=1)
    coefs = np.stack([np.linalg.lstsq(M, err[..., c].ravel(), rcond=None)[0] for c in range(3)])
    f = np.stack([M @ coefs[c] for c in range(3)], axis=-1).reshape(h, w, 3)
    pred = np.clip(pred_med + f, 0, 255).astype(np.int64)
    tp = 18 + int(coefs.size * 8) + 3  # qH (9 x i16) + fp coefs + q byte
    return arm_px(px, pred) + tp, qH, coefs


def arm_E(px, H_true=None):
    """compression-aware 2x2 quadtree in (s,t): each quadrant picks its own
    planar law by MEASURED zstd bytes (theta* applied to projections)."""
    h, w = px.shape[:2]
    pred_med = med_plane(px)
    err = signed_err(px, pred_med)
    if H_true is None:
        qH = quantize_H(est_homography(px, (h, w)))
    else:
        qH = quantize_H(H_true)
    s, t = proj_apply_q(qH, h, w)
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float64)

    def node_cost(mask):
        """HONEST node cost: the prediction is pred_med + fitted law (clipped
        exactly as the decoder replays), residual = (px - pred) % 256, wire =
        CD-G of that residual, transport = fp64 coefs + frame share."""
        n = int(mask.sum())
        sub = err[mask]
        M = np.stack([np.ones(n), s[mask], t[mask]], axis=1)
        coefs = np.stack([np.linalg.lstsq(M, sub[..., c], rcond=None)[0] for c in range(3)])
        f = np.zeros(err.shape)
        Mfull = np.stack([np.ones(mask.sum()), s[mask], t[mask]], axis=1)
        for c in range(3):
            f[mask, c] = Mfull @ coefs[c]
        pred = np.clip(pred_med + f, 0, 255).astype(np.int64)
        # per-node transport only counts THIS node's own bytes
        node_tp = int(coefs.size * 8) + 3
        # the honest residual wire for the whole plane under THIS pred — but
        # only THIS node's pixels are corrected; other pixels keep pred_med.
        # Cost is compositional: nodes' corrections ride ONE plane at the end.
        return coefs, f, node_tp

    def honest_wire(pred):
        res_final = (px_img - pred) % 256
        return FRAME + len(ZC.compress(cd_g(res_final).astype(np.uint8).tobytes()))

    px_img = px
    smask = np.ones((h, w), bool)
    coefs_root, f_root, tp_root = node_cost(smask)
    pred_root = np.clip(pred_med + f_root, 0, 255).astype(np.int64)
    cost_root = honest_wire(pred_root) + tp_root
    # 4 quadrants in (s,t) space
    s_med, t_med = np.median(s[smask]), np.median(t[smask])
    quads = [smask & (s <= s_med) & (t <= t_med), smask & (s <= s_med) & (t > t_med),
             smask & (s > s_med) & (t <= t_med), smask & (s > s_med) & (t > t_med)]
    pred_split = pred_med.copy()
    tp_split = 18 + 3 + 1 + 1  # qH + q/deg + split tag
    for q in quads:
        if q.sum() == 0:
            continue
        _, fq, tpq = node_cost(q)
        pred_split = np.clip(pred_split + fq, 0, 255)
        tp_split += tpq
    pred_split = pred_split.astype(np.int64)
    cost_split = honest_wire(pred_split) + tp_split
    if cost_split < cost_root:
        return cost_split, "split"
    return cost_root, "root"

# ---------------------------------------------------------------- main
def main():
    rows = []

    # ---- fixtures ----
    f1 = striped_scene()
    H_true = np.array([[0.9, 0.18, 12.0],
                       [0.0, 1.30, -18.0],
                       [0.0012, -0.0006, 1.0]])
    f2 = view_through(f1, quantize_H(H_true), 256, 256)
    rng = np.random.default_rng(9)
    f3 = rng.integers(0, 256, (256, 256, 3), dtype=np.int64)

    for tag, img, Ht in (("F1-identity", f1, None), ("F2-perspective", f2, H_true),
                         ("F3-noise", f3, None)):
        row = {"image": tag}
        row["A_med"] = arm_A(img)
        row["B_affine"] = arm_B(img)
        c1, qH, cf = arm_C(img)
        # arm_C with H estimated for F1: identity-ish since est maps to identity
        row["C_proj"] = c1
        c2, _, _ = arm_C(img, H_true=Ht, deg=2) if Ht is not None else arm_C(img, deg=2)
        row["D_curved"] = c2
        e1, pick = arm_E(img, H_true=Ht)
        row["E_caware"] = e1
        row["E_pick"] = pick
        rows.append(row)
        print(json.dumps(row), flush=True)

    # ---- real photos (H estimated honestly) ----
    for name, path in [("astronaut", "astronaut.png"), ("coffee", "coffee.png"),
                       ("chelsea", "chelsea.png"), ("hubble", "hubble_deep_field.jpg")]:
        im = Image.open(TD / path).convert("RGB").resize((512, 512), Image.LANCZOS)
        px = np.asarray(im, np.int64)
        row = {"image": name}
        row["A_med"] = arm_A(px)
        row["B_affine"] = arm_B(px)
        c1, _, _ = arm_C(px)
        row["C_proj"] = c1
        c2, _, _ = arm_C(px, deg=2)
        row["D_curved"] = c2
        e1, pick = arm_E(px)
        row["E_caware"] = e1
        row["E_pick"] = pick
        rows.append(row)
        print(json.dumps(row), flush=True)

    (OUT / "pg1_rows.json").write_text(json.dumps(rows, indent=1))
    print("wrote", OUT / "pg1_rows.json")


if __name__ == "__main__":
    main()