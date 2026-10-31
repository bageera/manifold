#!/usr/bin/env python3
"""MNF14-IF2 in-repo reproduction — fitted photographic illumination fields
vs MED/CD-G causal prediction (work-order memo; per the probe recipe).

Claim (external pilot): a fitted planar illumination field, inferred from a
64x64 reduction and evaluated at full resolution, corrects MED's local
predictions enough to WIN on one real photograph (chelsea −1,079 B at 512²),
losing on the other three (+3,854/+4,027/+159). Per-file min selection
= −1,079 over a 1,505,829-B MED/CD total.

Arms (one process, zstd-19 framing like the pilot, complete records):
  B  baseline   — MED residual plane, CD-G relabel, zstd-19 (the pilot's
                  MED/CD wire)
  P  +planar    — pred = MED ± fitted_planar_correction (params fitted at
                  64x64, evaluated full-res; params honest-transported)
  R  +radial    — radial glare model
  C  +curved    — curved (sine-path) illumination
  G  guide-MED  — gradient-steered MED (plane branch where the field's
                  gradient is steep — the firing surface is the band)
  FLA field-align — field-aligned causal neighbor (left vs top by field)
Controls:
  X  exact-formula fixture — generated from a KNOWN curved formula; the
     family fitted from 64² vs refit at 512² = the memo's precision point
Every arm: EXACT roundtrip (pred + res) % 256 == source, asserted.

Sources: scikit-image sample data (fetched to /tmp/mnf14_if2/):
astronaut PD/NASA · coffee CC0 (Rachel Michetti) · chelsea CC0
(Stefan van der Walt) · hubble PD/NASA.
"""
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image
import zstandard

sys.path.insert(0, "/Users/jonbethea/projects/blacktech/manifold/crates/mnf-mans/scripts")

TD = Path("/tmp/mnf14_if2")
FIT = 64          # the memo's field-fit reduction size
ZC = zstandard.ZstdCompressor(level=19, write_checksum=True)


def med_pred_plane(px):
    """The canonical MED prediction plane — imported from the repo walker's
    own med_pred (never re-derived): (0,0)=127, gy=0 -> left, gx=0 -> top,
    interior = MED over (left, top, topleft), reading TRUE pixels (the
    encode side reads the image, the mod-256 rule keeps decode exact)."""
    from mnf11_x2_dynamics import med_pred  # the canonical implementation
    h, w = px.shape[:2]
    pred = np.empty((h, w, 3), np.int64)
    # vectorized interior; edges per the decode walk's absolute coords
    A = np.concatenate([np.full((h, 1, 3), 127, np.int64), px[:, :-1]], axis=1)
    B = np.concatenate([np.full((1, w, 3), 127, np.int64), px[:-1]], axis=0)
    CC = np.concatenate([np.full((1, w, 3), 127, np.int64), A[:-1]], axis=0)
    plane = A + B - CC
    flat = ((A >= B) & (A >= CC) & (B >= CC)) | ((A <= B) & (A <= CC) & (B <= CC))
    p = np.where(flat, np.clip(plane, 0, 255),
                 np.where((np.abs(plane - A) <= np.abs(plane - B)) & (np.abs(plane - A) <= np.abs(plane - CC)), A,
                          np.where(np.abs(plane - B) <= np.abs(plane - CC), B, CC)))
    p[0, :] = A[0, :]           # gy == 0 → left
    p[:, 0] = B[:, 0]           # gx == 0 → top (row -1), incl. 127 at (0,0)
    return p.astype(np.int64)


def plane_pred_plane(px):
    h, w = px.shape[:2]
    A = np.concatenate([np.full((h, 1, 3), 127, np.int64), px[:, :-1]], axis=1)
    B = np.concatenate([np.full((1, w, 3), 127, np.int64), px[:-1]], axis=0)
    return np.clip(A + B - np.concatenate(
        [np.full((1, w, 3), 127, np.int64), A[:-1]], axis=0), 0, 255).astype(np.int64)


def cd_g_relabel(res):
    return np.stack([(res[..., 1]) % 256,
                     (res[..., 0] - res[..., 1]) % 256,
                     (res[..., 2] - res[..., 1]) % 256], axis=-1)


def fit_field(source_rgb, kind, fit_h, fit_w):
    """Fit a smooth illumination field to a FIT-size reduction; returns
    (kind, coef). Luma-model, offsets by channel mean at eval time."""
    im = Image.fromarray(source_rgb.astype(np.uint8)).resize((fit_w, fit_h), Image.LANCZOS)
    red = np.asarray(im, np.float64).mean(axis=2)  # luma
    yy, xx = np.mgrid[0:fit_h, 0:fit_w].astype(np.float64)
    hn, wn = yy / fit_h, xx / fit_w
    n = fit_h * fit_w
    if kind == "planar":
        M = np.stack([np.ones(n), wn.ravel(), hn.ravel()], axis=1)
        coef, *_ = np.linalg.lstsq(M, red.ravel(), rcond=None)
        return ("planar", coef, n)
    if kind == "radial":
        rr = np.sqrt((wn - 0.5) ** 2 + (hn - 0.5) ** 2)
        M = np.stack([np.ones(n), rr.ravel(), (rr ** 2).ravel()], axis=1)
        coef, *_ = np.linalg.lstsq(M, red.ravel(), rcond=None)
        return ("radial", coef, n)
    if kind == "curved":
        M = np.stack([np.ones(n), wn.ravel(), hn.ravel(),
                      np.sin(2 * np.pi * hn.ravel()).ravel(),
                      np.cos(2 * np.pi * hn.ravel()).ravel()], axis=1)
        coef, *_ = np.linalg.lstsq(M, red.ravel(), rcond=None)
        return ("curved", coef, n)
    raise SystemExit(kind)


def eval_field(field, h, w):
    kind, coef, _n = field
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float64)
    hn, wn = yy / h, xx / w
    if kind == "planar":
        f = coef[0] + coef[1] * wn + coef[2] * hn
    elif kind == "radial":
        rr = np.sqrt((wn - 0.5) ** 2 + (hn - 0.5) ** 2)
        f = coef[0] + coef[1] * rr + coef[2] * rr ** 2
    elif kind == "curved":
        f = (coef[0] + coef[1] * wn + coef[2] * hn
             + coef[3] * np.sin(2 * np.pi * hn) + coef[4] * np.cos(2 * np.pi * hn))
    else:
        raise SystemExit(kind)
    return f - f.mean()   # zero-mean correction: pure gradient, no luma shift


def field_transport(field):
    kind, coef, n = field
    return 1 + len(coef) * 8   # kind byte + f64 LE coefficients


def arm_bytes(px, pred):
    res = (px - pred) % 256
    rec = (pred + res) % 256
    assert np.array_equal(rec, px % 256), "roundtrip inexact"
    wire = cd_g_relabel(res).astype(np.uint8)
    return len(ZC.compress(wire.tobytes())), res


def main():
    sources = {}
    for name, path, size in [
        ("astronaut", "astronaut.png", (512, 512)),
        ("coffee", "coffee.png", (512, 512)),
        ("chelsea", "chelsea.png", (512, 512)),
        ("hubble", "hubble_deep_field.jpg", (512, 512)),
    ]:
        im = Image.open(TD / path).convert("RGB")
        if im.size != size:
            im = im.resize(size, Image.LANCZOS)
        sources[name] = np.asarray(im, np.int64)

    rows = []
    for name, px in sources.items():
        h, w = px.shape[:2]
        pred_med = med_pred_plane(px)
        n_base, _ = arm_bytes(px, pred_med)
        row = {"image": name, "res": f"{w}x{h}", "baseline": n_base}
        plane_p = plane_pred_plane(px)
        # field fits come from the FULL image's 64² reduction (the memo's
        # protocol: fit at 64, eval at 512, no refit)
        for kind in ("planar", "radial", "curved"):
            fld = fit_field(px.astype(np.uint8), kind, FIT, FIT)
            corr = eval_field(fld, h, w)
            pred = np.clip(pred_med + corr[..., None], 0, 255).astype(np.int64)
            n, _ = arm_bytes(px, pred)
            row[kind] = n + field_transport(fld)
        # gradient-steered MED: band on the fitted planar field's |grad y|
        fldp = fit_field(px.astype(np.uint8), "planar", FIT, FIT)
        f = eval_field(fldp, h, w)
        gy, gx2 = np.gradient(f)
        band = np.abs(gy) > 1.0
        pred_g = np.where(band[..., None], plane_p, pred_med)
        n, _ = arm_bytes(px, pred_g)
        row["guide_med"] = n + field_transport(fldp)
        # field-aligned neighbor: left vs top by |gx| vs |gy|
        trust_left = np.abs(gx2) > np.abs(gy)
        pred_fa = np.where(trust_left[..., None],
                           np.concatenate([np.full((h, 1, 3), 127, np.int64), px[:, :-1]], axis=1),
                           pred_med)
        n, _ = arm_bytes(px, pred_fa)
        row["field_align"] = n + field_transport(fldp)
        rows.append(row)
        print(name, json.dumps(row), flush=True)

    # ===== control: exact-formula fixture =====
    H = W = 512
    yy, xx = np.mgrid[0:H, 0:W].astype(np.float64)
    hn, wn = yy / H, xx / W
    tc = np.array([122.0, 18.0, -8.0, 26.0, -14.0])   # the KNOWN curve
    lum = tc[0] + tc[1] * wn + tc[2] * hn + tc[3] * np.sin(2 * np.pi * hn) + tc[4] * np.cos(2 * np.pi * hn)
    surf = 40.0 * np.sin(2 * np.pi * wn * 3) * np.sin(2 * np.pi * hn * 2)
    img = np.clip(lum[..., None] + surf[..., None] * np.array([0.9, 1.0, 1.1]), 0, 255).round().astype(np.int64)
    pred_med = med_pred_plane(img)
    n_med, _ = arm_bytes(img, pred_med)
    fld64 = fit_field(img.astype(np.uint8), "curved", FIT, FIT)
    pred64 = np.clip(pred_med + eval_field(fld64, H, W)[..., None], 0, 255).astype(np.int64)
    n64, _ = arm_bytes(img, pred64)
    # the 512 refit: fit the field ON the 512 lattice
    fld512 = fit_field(img.astype(np.uint8), "curved", H, W)
    pred512 = np.clip(pred_med + eval_field(fld512, H, W)[..., None], 0, 255).astype(np.int64)
    n512, _ = arm_bytes(img, pred512)
    rows.append({"image": "CTRL", "res": "512x512", "baseline": n_med,
                 "curved": n64 + field_transport(fld64),
                 "curved512_refit": n512 + field_transport(fld512)})
    print("CTRL", json.dumps(rows[-1]), flush=True)

    (TD / "if2_rows.json").write_text(json.dumps(rows, indent=1))
    print("wrote", TD / "if2_rows.json")


if __name__ == "__main__":
    main()