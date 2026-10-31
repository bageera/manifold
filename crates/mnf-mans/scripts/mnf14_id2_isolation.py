#!/usr/bin/env python3
"""ID2 isolation A/B on the CLI's exact hubble pixels: the shipped pick
priced 1,479,904 B for THIS RGB8 (JPEG-decoded by image-crate). The probe's
512² estimate doesn't transfer — re-measure the const arm + baseline here,
same pixels, same honest protocol."""
import sys
sys.path.insert(0, "/private/tmp/mnf14_id1")
sys.path.insert(0, "/Users/jonbethea/projects/blacktech/manifold/crates/mnf-mans/scripts")
import numpy as np
from PIL import Image

# load the CLI's exact pixels
px_img = Image.open("/tmp/hf1_hubble_decoded.png").convert("RGB")
W, H = px_img.size
px = np.asarray(px_img, np.int64)
print("fixture:", W, "x", H)

# downscale factor for tractability of the python MED walk: measure on the
# FULL frame is minutes/file in python. The shipped pick's per-TILE grammar
# is what matters; run the python arms on a CROPPED CENTER 512x512 (the
# pick's per-tile choices don't depend on frame size for mode bytes) AND
# ALSO the full-frame tile-choice census via the CLI itself.
# Const arm vs MED arm vs the shipped file on the crop:
import zstandard
ZC = zstandard.ZstdCompressor(level=19, write_checksum=True)
FRAME = 28

def cd_g(res):
    return np.stack([(res[..., 1]) % 256,
                     (res[..., 0] - res[..., 1]) % 256,
                     (res[..., 2] - res[..., 1]) % 256], axis=-1)

def arm(px, pred, tp):
    res = (px - pred) % 256
    assert np.array_equal((pred + res) % 256, px % 256), "recon inexact"
    return FRAME + len(ZC.compress(cd_g(res).astype(np.uint8).tobytes())) + tp

def med_vec(Wc, Hc):
    """vectorized canonical MED (same edge semantics as pred_map)."""
    p = px[:Hc, :Wc].astype(np.int64)
    h, w = p.shape[:2]
    A = np.concatenate([np.full((h, 1, 3), 127, np.int64), p[:, :-1]], axis=1)
    B = np.concatenate([np.full((1, w, 3), 127, np.int64), p[:-1]], axis=0)
    CC = np.concatenate([np.full((1, w, 3), 127, np.int64), A[:-1]], axis=0)
    plane = A + B - CC
    flat = ((A >= B) & (A >= CC) & (B >= CC)) | ((A <= B) & (A <= CC) & (B <= CC))
    out = np.where(flat, np.clip(plane, 0, 255),
                   np.where((np.abs(plane - A) <= np.abs(plane - B)) & (np.abs(plane - A) <= np.abs(plane - CC)), A,
                            np.where(np.abs(plane - B) <= np.abs(plane - CC), B, CC)))
    out[0, :] = A[0, :]
    out[:, 0] = B[:, 0]
    return out

crop = 512
med = med_vec(crop, crop)
sub = px[:crop, :crop]
base = arm(sub, med, 0)
med_rgb = [int(np.median(sub[..., c])) for c in range(3)]
const_pred = np.full_like(sub, 0)
const_pred[..., 0] = med_rgb[0]; const_pred[..., 1] = med_rgb[1]; const_pred[..., 2] = med_rgb[2]
const_cost = arm(sub, const_pred, 4)
print("crop512 baseline(MED):", base, " const:", const_cost, " delta:", const_cost - base)
# per-tile census: how many 16x16 tiles would the const arm WIN there?
tiles_x, tiles_y = crop // 16, crop // 16
wins = 0
import zlib
for ty in range(tiles_y):
    for tx in range(tiles_x):
        t = sub[ty*16:(ty+1)*16, tx*16:(tx+1)*16]
        m = med[ty*16:(ty+1)*16, tx*16:(tx+1)*16]
        mc = [int(np.median(t[..., c])) for c in range(3)]
        cp = np.zeros_like(t); cp[...,0]=mc[0]; cp[...,1]=mc[1]; cp[...,2]=mc[2]
        c_m = len(zlib.compress(cd_g((t-m)%256).astype(np.uint8).tobytes(), 9))
        c_c = 4 + len(zlib.compress(cd_g((t-cp)%256).astype(np.uint8).tobytes(), 9))
        wins += (c_c < c_m)
print("per-tile const wins:", wins, "of", tiles_x*tiles_y)