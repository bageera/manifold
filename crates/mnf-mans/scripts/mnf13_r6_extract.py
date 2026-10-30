#!/usr/bin/env python3
"""Residual-plane extraction (as-coded wire order) from the SHIPPED pick.

The v1.13 pick emits record-mode MNF8 files whose payload is the backend-4
mix range coder. For R6's ablation we need the residual byte STREAM in the
mix coder's own coded order WITHOUT re-implementing the range coder
(the hand-rolled-coder ban).

Source of truth = the shipped grammar itself:
  - mode-0 records, tile-major (row-major tile grid), each tile's body in
    wire emission order: planar = per plane [ref][da][db], interleaved =
    per pixel;
  - the CD relabel (ref 1/2/3) applied per PIXEL pre-emission;
  - the predictor residual = (pixel - pred) % 256 with the file's PredKind;
  - token-wire files carry FLAG_TOKENS (MED-only) and are walked by tags;
    tag-0 literals are the emitted bytes.
The mix coder consumes the residual stream in exactly this emission order
(one byte per symbol; mix_encode maps data[i] -> stream symbol i).

Causality/anchor sanity: the recomputed wire plane must re-encode through
the SHIPPED mix coder to the SHIPPED payload size (asserted per file in
the R6 driver before any arm runs — the anchor check).
"""
import struct
import subprocess
from pathlib import Path

import numpy as np
from PIL import Image

from mnf11_x2_dynamics import med_pred, os_env  # noqa: E402  (canonical MED)

ROOT = Path(__file__).resolve().parents[3]
TILE = 16


def cli_encode(png, out="/tmp/r6.mnf5", profile=("mnf8",)):
    args = ["--mnf8"] if profile == ("mnf8",) else ["--mnf8=maximum"]
    subprocess.run([str(ROOT / "target/release/manifold"), "encode", str(png),
                    "-o", out, *args], capture_output=True, check=True,
                   cwd=str(ROOT), env=os_env())
    return Path(out).read_bytes()


def pred_map(kind, px):
    """The shipped per-pixel predictor over causal neighbors, EXACTLY per
    mnf11_x2_dynamics.causal_pixels (the walk proven against the shipped
    decoder): (0,0)->127; gy==0 -> left; gx==0 -> top(recon); else the
    PredKind over (a=left, b=top, cc=topleft) reading ALREADY-DECODED
    pixels — for the residual recompute the 'recon' reads ARE the true
    image (mod-256 rule; R0's causality validator established this)."""
    h, w = px.shape[:2]
    pred = np.empty((h, w, 3), np.int64)
    med_cache = {}

    def at(gy, gx, c):
        return int(px[gy, gx, c])

    for gy in range(h):
        for gx in range(w):
            for c in range(3):
                if gy == 0 and gx == 0:
                    pred[gy, gx, c] = 127
                elif gy == 0:
                    pred[gy, gx, c] = at(gy, gx - 1, c)
                elif gx == 0:
                    pred[gy, gx, c] = at(gy - 1, gx, c)
                else:
                    a = at(gy, gx - 1, c)
                    b = at(gy - 1, gx, c)
                    cc = at(gy - 1, gx - 1, c)
                    if kind == "plane":
                        pred[gy, gx, c] = max(0, min(255, a + b - cc))
                    elif kind == "left":
                        pred[gy, gx, c] = a
                    else:
                        pred[gy, gx, c] = med_pred(a, b, cc)
    return pred


def cd_relabel(res, ref, planar, w, h):
    """Per-pixel channel-diff relabel (ADR-008) EXACTLY as shipped
    (src/mnf8.rs channel_diff_relabel): interleaved = per-pixel triplet
    [refc, a-refc, b-refc] with ref_pick ordering; planar = per-tile
    [ref-plane][a-ref plane][b-ref plane] with the SAME ref_pick pair
    semantics per reference id (1: g,(r-g),(b-g); 2: r,(g-r),(b-r);
    3: b,(r-b),(g-b))."""
    if ref == 0:
        return res
    if planar:
        cols = (w + TILE - 1) // TILE
        out = np.empty_like(res)
        for ti in range(((w + TILE - 1) // TILE) * ((h + TILE - 1) // TILE)):
            row, col = divmod(ti, cols)
            tw = min(TILE, w - col * TILE)
            th = min(TILE, h - row * TILE)
            t = res[row * TILE:row * TILE + th, col * TILE:col * TILE + tw]
            if ref == 1:
                g = t[..., 1]
                ra = (t[..., 0] - g) % 256
                ba = (t[..., 2] - g) % 256
                o = np.stack([g, ra, ba], axis=-1)
            elif ref == 2:
                r0 = t[..., 0]
                ga = (t[..., 1] - r0) % 256
                ba = (t[..., 2] - r0) % 256
                o = np.stack([r0, ga, ba], axis=-1)
            else:
                b0 = t[..., 2]
                ra = (t[..., 0] - b0) % 256
                ga = (t[..., 1] - b0) % 256
                o = np.stack([b0, ra, ga], axis=-1)
            out[row * TILE:row * TILE + th, col * TILE:col * TILE + tw] = o
        return out
    if ref == 1:
        return np.stack([(res[..., 1]) % 256,
                         (res[..., 0] - res[..., 1]) % 256,
                         (res[..., 2] - res[..., 1]) % 256], axis=-1)
    if ref == 2:
        return np.stack([(res[..., 0]) % 256,
                         (res[..., 1] - res[..., 0]) % 256,
                         (res[..., 2] - res[..., 0]) % 256], axis=-1)
    return np.stack([(res[..., 2]) % 256,
                     (res[..., 0] - res[..., 2]) % 256,
                     (res[..., 1] - res[..., 2]) % 256], axis=-1)


def residual_wire(png, profile=("mnf8",)):
    """(backend, flags, kind, ref, planar, wire_bytes, blob) — the residual
    byte stream in the mix coder's coded order, or stream=None when the
    pick's file carries no mix-addressable record stream (MNF9 magic,
    token wire)."""
    blob = cli_encode(png, profile=profile)
    if blob[:4] != b"MNF8":
        return 0, 0, "", 0, False, None, blob  # MNF9 / other magic: no MNF8 record stream
    w, h, tile, tile_count = struct.unpack_from("<IIII", blob, 4)
    plen, flags, backend = struct.unpack_from("<III", blob, 20)
    assert tile == TILE
    img = Image.open(png).convert("RGB")
    px = np.asarray(img, np.int64)
    assert (w, h) == (px.shape[1], px.shape[0]), px.shape
    kind = ("med" if flags & 4 else "plane" if flags & 16 else
            "left" if flags & 32 else "none")
    ref = (flags >> 6) & 3
    planar = bool(flags & 1)
    tok = bool(flags & 8)
    pred = pred_map(kind, px)
    res = (px - pred) % 256
    wire3 = cd_relabel(res, ref, planar, w, h)
    if tok:
        # token wire: the mix payload codes the tag-0 literal bytes plus
        # tag structure — the R6 gate SKIPS backend-4 token files (measure
        # the record arms; token files keep their shipped wire by the pick).
        return backend, flags, kind, ref, planar, None, blob
    n_bytes = h * w * 3
    wire = np.empty(n_bytes, np.int64)
    pos = 0
    cols = (w + TILE - 1) // TILE
    for ti in range(tile_count):
        row, col = divmod(ti, cols)
        tw = min(TILE, w - col * TILE)
        th = min(TILE, h - row * TILE)
        t = wire3[row * TILE:row * TILE + th, col * TILE:col * TILE + tw]
        if planar:
            wire[pos:pos + t.size] = t.transpose(2, 0, 1).ravel()
        else:
            wire[pos:pos + t.size] = t.ravel()
        pos += t.size
    assert pos == n_bytes
    wire_bytes = wire.astype(np.uint8).tobytes()
    # The record stream codes per-tile [mode byte][body]; the shipped pick
    # on record arms emits mode 0 for every tile. Re-encoding this stream
    # through the shipped mix coder MUST reproduce the shipped payload
    # byte-for-byte (the anchor check in the R6 driver).
    stream = bytearray()
    pos = 0
    for ti in range(tile_count):
        row, col = divmod(ti, cols)
        tw = min(TILE, w - col * TILE)
        th = min(TILE, h - row * TILE)
        nb = tw * th * 3
        stream.append(0)  # mode 0 (assert: every shipped record is mode 0 here)
        stream += wire_bytes[pos:pos + nb]  # bytes slice: no ndarray rebind
        pos += nb
    return backend, flags, kind, ref, planar, bytes(stream), blob


if __name__ == "__main__":
    import sys
    be, fl, kind, ref, planar, wire, blob = residual_wire(sys.argv[1]
                                                          if len(sys.argv) > 1
                                                          else "testdata/photos/autumn_192x144_s900001.png")
    plen = struct.unpack_from("<I", blob, 20)[0]
    print(f"backend={be} flags={fl:#04x} kind={kind} ref={ref} "
          f"planar={planar} wire={len(wire) if wire else None} plen={plen}")