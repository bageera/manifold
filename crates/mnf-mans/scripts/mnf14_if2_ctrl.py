"""CTRL rebuilt per the memo: a photograph generated from an EXACT curved
illumination formula = the quantized GRAY illumination curve itself (no
surface, no tint — the memo's 101-B exact-record row). The 64²-inferred
field vs the 512² refit on the same family = the precision-transfer rows."""
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image
import zstandard

TD = Path("/tmp/mnf14_if2")
FIT = 64
ZC = zstandard.ZstdCompressor(level=19, write_checksum=True)

H = W = 512
yy, xx = np.mgrid[0:H, 0:W].astype(np.float64)
hn, wn = yy / H, xx / W
tc = np.array([122.0, 18.0, -8.0, 26.0, -14.0])   # the KNOWN curve
lum = tc[0] + tc[1] * wn + tc[2] * hn + tc[3] * np.sin(2 * np.pi * hn) \
    + tc[4] * np.cos(2 * np.pi * hn)
img = np.repeat(np.clip(lum, 0, 255).round().astype(np.int64)[..., None],
                3, axis=-1)   # gray illumination photograph


def med_pred_plane(px):
    h, w = px.shape[:2]
    A = np.concatenate([np.full((h, 1, 3), 127, np.int64), px[:, :-1]], axis=1)
    B = np.concatenate([np.full((1, w, 3), 127, np.int64), px[:-1]], axis=0)
    CC = np.concatenate([np.full((1, w, 3), 127, np.int64), A[:-1]], axis=0)
    plane = A + B - CC
    flat = ((A >= B) & (A >= CC) & (B >= CC)) | ((A <= B) & (A <= CC) & (B <= CC))
    p = np.where(flat, np.clip(plane, 0, 255),
                 np.where((np.abs(plane - A) <= np.abs(plane - B)) & (np.abs(plane - A) <= np.abs(plane - CC)), A,
                          np.where(np.abs(plane - B) <= np.abs(plane - CC), B, CC)))
    p[0, :] = A[0, :]
    p[:, 0] = B[:, 0]
    return p.astype(np.int64)


def arm_bytes(px, pred):
    res = ((px - pred) % 256).astype(np.int64)
    rec = (pred + res) % 256
    assert np.array_equal(rec, px % 256)
    # CD-G on a gray image: all three wire channels = the gray residual
    # (r=r, r-r=0) — the wire degenerates to a plane + zeros; honest bytes
    wire = cd_g = np.stack([(res[..., 1]) % 256,
                            (res[..., 0] - res[..., 1]) % 256,
                            (res[..., 2] - res[..., 1]) % 256], axis=-1)
    return len(ZC.compress(wire.astype(np.uint8).tobytes())), res


def fit_on(freq_h, freq_w):
    red = np.asarray(Image.fromarray(img.astype(np.uint8)).resize(
        (freq_w, freq_h), Image.LANCZOS), np.float64).mean(axis=2)
    y2, x2 = np.mgrid[0:freq_h, 0:freq_w].astype(np.float64)
    n2, w2 = y2 / freq_h, x2 / freq_w
    M = np.stack([np.ones(freq_h * freq_w), w2.ravel(), n2.ravel(),
                  np.sin(2 * np.pi * n2).ravel(),
                  np.cos(2 * np.pi * n2).ravel()], axis=1)
    coef, *_ = np.linalg.lstsq(M, red.ravel(), rcond=None)
    return ("curved", coef)


def eval_at(coef, h, w):
    return (coef[0] + coef[1] * wn[:h, :w] + coef[2] * hn[:h, :w]
            + coef[3] * np.sin(2 * np.pi * hn[:h, :w])
            + coef[4] * np.cos(2 * np.pi * hn[:h, :w]))


# baseline: MED on the exact illumination
pred_med = med_pred_plane(img)
n_med, res_med = arm_bytes(img, pred_med)
# MED's error on a smooth curve: the MED clamp picks neighbors — the residual
# structure IS the curve's second difference; the fitted field fixes the LUMA
c64 = fit_on(FIT, FIT)[1]
f64 = eval_at(c64, H, W)
pred64 = f64.round().clip(0, 255).astype(np.int64).repeat(3, axis=-1)[..., :3] \
    if f64.ndim < 3 else None
pred64 = np.repeat(np.clip(f64.round(), 0, 255).astype(np.int64)[..., None], 3, axis=-1)
n64, _ = arm_bytes(img, pred64)
c512 = fit_on(H, W)[1]
f512 = eval_at(c512, H, W)
pred512 = np.repeat(np.clip(f512.round(), 0, 255).astype(np.int64)[..., None], 3, axis=-1)
n512, _ = arm_bytes(img, pred512)
print("CTRL-b       :", n_med, "(memo: 101 exact / 66,673 inferred)")
print("CTRL 64-fit  :", n64, "+ 41 field =", n64 + 41)
print("CTRL 512-fit :", n512, "+ 41 field =", n512 + 41, "(memo: 4,713)")
json.dump({"baseline": n_med, "fit64": n64 + 41, "fit512": n512 + 41,
           "c64": c64.tolist(), "c512": c512.tolist()},
          open(TD / "if2_ctrl.json", "w"), indent=1)