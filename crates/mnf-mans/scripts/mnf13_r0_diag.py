#!/usr/bin/env python3
"""MNF13-R0 — conditional-structure diagnostics on the ACTUAL v1.12
streams (per paste: R0 matrix merged with SI1's zero-index gate).

Pre-registered questions (measurement only, no format change):
  Q0  H(R)              — the v1.12 residual symbol entropy, per REF mode
  Q1  H(R | activity)   — 4-class q = floor(log2(1+|L-T|+|T-TL|)) context
  Q2  H(R | act, dir)   — +direction class (|L-T| vs |T-TL| dominance)
  Q3  H(R | prev res)   — order-1 on the coded residual order
  Q4  I(dR; dB | G)     — conditional joint-channel dependence post-CD
  Q5  H(R | causal offsets) — SI1-B's zero-index gate: fixed causal
      offsets {-1,-W,-2,-2W,-W-1,-W+1}, per-offset conditional gains vs
      H(R) — the decision gate for any shared-index arm.

All quantities measured on the residual bodies the v1.12 RELEASE pick
actually coded (CLI --mnf8=maximum per file; MNF9-magic files skipped;
token wires = tag-0 literals only). Per REF mode: the shipped file's
bodies are unrelabelled to the plain residual plane, then relabelled
per reference r in {0=none,1=G,2=R,3=B} — the exact wire the REF=r
candidate would carry. Held-out split: train/heldout = photos/
heldout-photos, results reported separately (paste's reproducibility
gate).
"""
import io
import os
import struct
import subprocess
import sys
import zlib
from math import log2
from pathlib import Path

import numpy as np
from PIL import Image

try:
    import zstandard as _zstd
    _zc = _zstd.ZstdCompressor(level=19)
except ImportError:
    _zc = None

ROOT = Path(__file__).resolve().parents[3]
TILE = 16
sys.path.insert(0, str(Path(__file__).parent))
from mnf11_x2_dynamics import causal_pixels, decompress_stream, med_pred  # noqa: E402

os_env = None


def cli_encode_max(rel):
    subprocess.run([str(ROOT / "target/release/manifold"), "encode", rel,
                    "-o", "/tmp/r0.mnf5", "--mnf8=maximum"],
                   capture_output=True, check=True, cwd=str(ROOT))
    blob = open("/tmp/r0.mnf5", "rb").read()
    return blob


def cd_unrelabel_stream(w, h, flags, stream):
    """v1.12 body unrelabel: REF in {1,2,3} maps [ref, a-ref, b-ref] back
    to [r, g, b]; planar bodies reorder planes in wire emission order."""
    ref = (flags >> 6) & 3
    planar = bool(flags & 1)
    if ref == 0:
        return stream
    cols = (w + TILE - 1) // TILE
    tile_count = ((w + TILE - 1) // TILE) * ((h + TILE - 1) // TILE)
    out = bytearray()
    pos = 0
    for ti in range(tile_count):
        row, col = divmod(ti, cols)
        tw = min(TILE, w - col * TILE)
        th = min(TILE, h - row * TILE)
        nb = tw * th * 3
        mode = stream[pos]; pos += 1
        out.append(mode)
        if mode != 0:
            raise SystemExit("non-mode-0 under a REF arm")
        body = stream[pos:pos + nb]; pos += nb
        if planar:
            third = nb // 3
            pref, pa, pb = body[:third], body[third:2 * third], body[2 * third:]
            prefa = np.frombuffer(pa, np.uint8).astype(np.int64)
            prefb = np.frombuffer(pb, np.uint8).astype(np.int64)
            f = np.frombuffer(pref, np.uint8).astype(np.int64)
            if ref == 1:    # [g][r-g][b-g]
                r = (prefa + f) % 256
                g = f
                b = (prefb + f) % 256
            elif ref == 2:  # [r][g-r][b-r]
                r = f
                g = (prefa + f) % 256
                b = (prefb + f) % 256
            else:           # [b][r-b][g-b]
                b = f
                r = (prefa + f) % 256
                g = (prefb + f) % 256
            out += bytes(np.concatenate([r, g, b]).astype(np.uint8))
        else:
            t = np.frombuffer(body, np.uint8).astype(np.int64).reshape(-1, 3)
            refc, da, db = t[:, 0], t[:, 1], t[:, 2]
            if ref == 1:
                g = refc; r = (da + refc) % 256; b_ = (db + refc) % 256
            elif ref == 2:
                r = refc; g = (da + refc) % 256; b_ = (db + refc) % 256
            else:
                b_ = refc; r = (da + refc) % 256; g = (db + refc) % 256
            out += bytes(np.stack([r, g, b_], axis=1).astype(np.uint8).reshape(-1))
    return bytes(out)


def cd_relabel_stream(w, h, stream, ref):
    """per-pixel relabel to REF=ref (ref 0 = none) on PLAIN record streams."""
    if ref == 0:
        return stream
    cols = (w + TILE - 1) // TILE
    tile_count = ((w + TILE - 1) // TILE) * ((h + TILE - 1) // TILE)
    out = bytearray()
    pos = 0
    for ti in range(tile_count):
        row, col = divmod(ti, cols)
        tw = min(TILE, w - col * TILE)
        th = min(TILE, h - row * TILE)
        nb = tw * th * 3
        out.append(stream[pos]); 
        if stream[pos] != 0:
            raise SystemExit("non-mode-0")
        pos += 1
        body = stream[pos:pos + nb]; pos += nb
        t = np.frombuffer(body, np.uint8).astype(np.int64).reshape(-1, 3)
        r, g, b = t[:, 0], t[:, 1], t[:, 2]
        if ref == 1:
            wire = np.stack([g, (r - g) % 256, (b - g) % 256], axis=1)
        elif ref == 2:
            wire = np.stack([r, (g - r) % 256, (b - r) % 256], axis=1)
        else:
            wire = np.stack([b, (r - b) % 256, (g - b) % 256], axis=1)
        out += bytes(wire.astype(np.uint8).reshape(-1))
    return bytes(out)


# ---------------- residual extraction from the ACTUAL v1.12 files -------

def extract_residuals(blob, img, recon_check=False):
    """Return (ref, predictor_kind, planar, token_wire, residual_plane,
    token_wire_bytes or None). Residual plane = th×tw×3 int64 residuals
    AS CODED (post-relabel), assembled tile by tile from mode-0 bodies or
    from tag-0 literals of a token wire; the causal walker asserts recon
    == true image per tile (the causality validator, pre-registered)."""
    d = decompress_stream(blob)
    if d is None:
        return None
    w, h, tile, tile_count, flags, backend, stream = d
    if tile != TILE:
        raise SystemExit("tile")
    planar = bool(flags & 1)
    ref = (flags >> 6) & 3
    kind = ("med" if flags & 4 else
            "plane" if flags & 16 else
            "left" if flags & 32 else "none")
    token_wire = bool(flags & 8)
    imgrid = np.asarray(img, dtype=np.int64)
    res = np.zeros((h, w, 3), np.int64)      # residual AS-CODED
    pos = 0
    if token_wire:
        # §3.1 token wire: walk tags; tag-0 literals = coded residuals
        # (POST-relabel: tag-0 payload = the emitted bytes).
        npix = w * h
        ppos = 0
        while ppos < npix:
            tag = stream[pos]
            if tag == 0:
                t = np.frombuffer(stream[pos + 1:pos + 4], np.uint8).astype(np.int64)
                y, x = divmod(ppos, w)
                res[y, x] = t
                pos += 4; ppos += 1
            elif tag == 1:
                ppos += int(stream[pos + 5]); pos += 6
            elif tag == 2:
                ppos += int(stream[pos + 3]); pos += 4
            else:
                raise SystemExit(f"token tag {tag}")
        debug_assert_size = pos
        assert pos == len(stream), f"token wire unconsumed {pos}/{len(stream)}"
    else:
        cols = (w + TILE - 1) // TILE
        for ti in range(tile_count):
            row, col = divmod(ti, cols)
            tw = min(TILE, w - col * TILE)
            th = min(TILE, h - row * TILE)
            nb = tw * th * 3
            gy0, gx0 = row * TILE, col * TILE
            mode = stream[pos]; pos += 1
            if mode != 0:
                raise SystemExit(f"non-mode-0 record mode {mode}")
            body = np.frombuffer(stream[pos:pos + nb], np.uint8).astype(np.int64)
            pos += nb
            if planar:
                p = body.reshape(3, th, tw)
                body3 = np.stack([p[0], p[1], p[2]], axis=2)
            else:
                body3 = body.reshape(th, tw, 3)
            res[gy0:gy0 + th, gx0:gx0 + tw] = body3
        assert pos == len(stream), "record stream unconsumed"
        plain_stream = cd_unrelabel_stream(w, h, flags, stream)
        plain = np.zeros((h, w, 3), np.int64)
        pos = 0
        cols = (w + TILE - 1) // TILE
        for ti in range(tile_count):
            row, col = divmod(ti, cols)
            tw = min(TILE, w - col * TILE)
            th = min(TILE, h - row * TILE)
            nb = tw * th * 3
            gy0, gx0 = row * TILE, col * TILE
            assert plain_stream[pos] == 0
            pos += 1
            body = np.frombuffer(plain_stream[pos:pos + nb], np.uint8).astype(np.int64)
            pos += nb
            if planar:
                q = body.reshape(3, th, tw)
                body3 = np.stack([q[0], q[1], q[2]], axis=2)
            else:
                body3 = body.reshape(th, tw, 3)
            if recon_check:
                pxr = causal_pixels(th, tw, kind, body3, gy0, gx0, imgrid)
                assert np.array_equal(pxr % 256, imgrid[gy0:gy0+th, gx0:gx0+tw]), f"recon tile {ti}"
            plain[gy0:gy0+th, gx0:gx0+tw] = body3
        res = plain                    # canonical plane = plain residuals
    return ref, kind, planar, token_wire, res, stream


# ---------------- the diagnostic matrix --------------------------------

def entropy(a):
    """a = int64 array of symbol values in [0,256)."""
    c = np.bincount(np.asarray(a).astype(np.int64).ravel(), minlength=256)
    p = c[c > 0].astype(np.float64) / c.sum()
    return float(-(p * np.log2(p)).sum())


def cond_entropy(a, ctx, nctx=256):
    a = np.asarray(a).astype(np.int64).ravel()
    ctx = np.asarray(ctx).astype(np.int64).ravel()
    assert a.shape == ctx.shape
    total = a.shape[0]
    actx = a * nctx + ctx                 # joint key
    cc = np.bincount(actx, minlength=nctx * 256).astype(np.float64)
    p = cc[cc > 0] / total
    hjoint = -(p * np.log2(p)).sum()
    pc = np.bincount(ctx, minlength=nctx).astype(np.float64)
    pc = pc[pc > 0] / total
    hctx = -(pc * np.log2(pc)).sum()
    return float(hjoint - hctx)


def entropy_pairs(a, b):
    a = np.asarray(a).astype(np.int64).ravel()
    b = np.asarray(b).astype(np.int64).ravel()
    n = a.shape[0]
    key = a.astype(np.int64) * 256 + b
    c = np.bincount(key, minlength=256 * 256).astype(np.float64)
    p = c[c > 0] / n
    return float(-(p * np.log2(p)).sum())


def activity_class(img, ref_mode_plane):
    """q = floor(log2(1+|L-T|+|T-TL|)) clamped to a 4-class histogram:
    classes {0,1,2,3+} = per the paste's four-activity-class proposal.
    img = the TRUE image grid; context reads = TRUE pixels (encode-side
    reads are the true ones — same rule the encoder would get)."""
    a = img.astype(np.int64)
    h, w = a.shape[:2]
    # per-channel magnitude sums of the two neighbors + diagonal:
    Ld = np.zeros((h, w, 3), np.int64); Ld[:, 1:] = np.abs(a[:, 1:, :] - a[:, :-1, :])
    Td = np.zeros((h, w, 3), np.int64); Td[1:] = np.abs(a[1:, :, :] - a[:-1, :, :])
    TLd = np.zeros((h, w, 3), np.int64); TLd[1:, 1:] = np.abs(a[1:, 1:, :] - a[:-1, :-1, :])
    mag = Ld + Td + TLd
    mag = mag.sum(axis=2)
    q = np.floor(np.log2(1 + mag)).astype(np.int64)
    q = np.minimum(q, 3)                  # 4 classes
    return q


def direction_class(img):
    """2-class: is the horizontal gradient magnitude the larger one?
    dir = (|L| >= |T|) as a 0/1 context (encode-side true pixels)."""
    a = img.astype(np.int64)
    h, w = a.shape[:2]
    Lm = np.zeros((h, w), np.int64)
    Ld = np.abs(np.diff(a, axis=1, prepend=a[:, :1, :])).sum(axis=2)
    Tm = np.zeros((h, w), np.int64)
    Td = np.abs(np.diff(a, axis=0, prepend=a[:1, :, :])).sum(axis=2)
    return (Ld >= Td).astype(np.int64)


def prev_res_ctx(plane, w):
    """order-1 context on the coded residual order: the immediately
    previous residual byte within the wire order (flat)."""
    r = np.asarray(plane).astype(np.int64).ravel()
    ctx = np.concatenate([[0], r[:-1]])
    return ctx


def spatial_offsets(plane, img_shape):
    """R0-E / SI1-B gate: conditional entropy of each causal offset's
    residual at the same pixel position — offsets in PIXELS on the
    residual plane (raster order, interleaved RGB: offset in bytes = 3*px).
    Offsets (pixels): {-1,-W,-2,-2W,-W-1,-W+1} and a few texture lags
    {+? no: causal only}. Reads = the residual plane itself (the coder's
    input alphabet)."""
    h, w = img_shape[:2]
    flat = np.asarray(plane).astype(np.int64)
    if flat.ndim == 3:
        flat = flat.reshape(-1)      # h*w*3 byte order (coded order)
    out = {}
    Wb = w * 3
    for px_off, name in [(-3, "left"), (-Wb, "up"), (-2 * Wb, "up2"),
                         (-4 * Wb, "up4"), (-(Wb + 3), "upleft"),
                         (-(Wb - 3), "upright"), (-8 * Wb, "up8"),
                         (-16 * Wb, "up16"), (-32 * Wb, "up32")]:
        if len(flat) <= abs(px_off):
            continue
        ctx = np.concatenate([np.zeros(-px_off, np.int64), flat[:px_off]])[:len(flat) - abs(px_off)]
        val = flat[abs(px_off):]
        assert ctx.shape == val.shape, (ctx.shape, val.shape)
        gain = entropy(val) - cond_entropy(val, ctx)
        out[name] = (entropy(val), gain)
    return out


# ---------------- per REF-mode wire planes ------------------------------

def wire_plane_for_ref(plain, ref, planar):
    """Return the residual plane AS the REF=ref candidate would code it,
    in the SAME shape the container sees (planar = [plane_ref][plane_a]
    [plane_b]; interleaved = per-pixel [ref, a-ref, b-ref]) for the
    entropy measurements (byte order matters for prev-res contexts)."""
    t = plain.astype(np.int64)
    h, w = t.shape[:2]
    if ref == 0:
        wire = t
    elif ref == 1:
        g = t[:, :, 1]
        wire = np.stack([g, (t[:, :, 0] - g) % 256, (t[:, :, 2] - g) % 256], axis=2)
    elif ref == 2:
        r = t[:, :, 0]
        wire = np.stack([r, (t[:, :, 1] - r) % 256, (t[:, :, 2] - r) % 256], axis=2)
    else:
        b = t[:, :, 2]
        wire = np.stack([b, (t[:, :, 0] - b) % 256, (t[:, :, 1] - b) % 256], axis=2)
    if planar:
        return wire[:, :, 0].reshape(-1).tolist()[:0] or np.stack([
            wire[:, :, 0], wire[:, :, 1], wire[:, :, 2]], axis=1).reshape(-1)
    return wire.reshape(-1)


def diag_file(rel, img, verbose=False):
    blob = cli_encode_max(rel)
    got = extract_residuals(blob, img)
    if got is None:
        return None
    ref, kind, planar, token_wire, plain, stream = got
    rows = []
    # Q4: I(dR; dB | G) — only meaningful post-CD: measure on the shipped
    # wire channels (c1, c2 given c0 as the condition class)
    # per REF mode {0,1,2,3}: H(R), H|act, H|act+dir, H|prev
    q = activity_class(img, plain)
    dclass = direction_class(img)
    prev = None
    for rmode in (0, 1, 2, 3):
        wire = wire_plane_for_ref(plain, rmode, planar)   # bytes in coded order
        warr = np.asarray(wire, np.int64)
        h0 = entropy(warr)
        # contexts per PIXEL (not per byte): expand to byte contexts
        qpx = q.reshape(-1)
        dpx = dclass.reshape(-1)
        qbyte = np.repeat(qpx, 3)
        dbyte = np.repeat(dpx, 3)
        if warr.shape == qbyte.shape:
            h_act = cond_entropy(warr, qbyte, nctx=4)
            key = qbyte * 2 + dbyte
            h_act_dir = cond_entropy(warr, key, nctx=8)
        else:
            h_act = h_act_dir = float("nan")
        hprev = cond_entropy(warr, np.concatenate([[0], warr[:-1]]), nctx=256)
        rows.append((rmode, h0, h_act, h_act_dir, hprev))
    # Q4 conditional cross-channel: from the SHIPPED ref's wire channels
    shipped = wire_plane_for_ref(plain, ref, planar)
    if not planar:
        t = np.asarray(shipped, np.int64).reshape(-1, 3)
        c0, c1, c2 = t[:, 0], t[:, 1], t[:, 2]
    else:
        t = np.asarray(shipped, np.int64).reshape(3, -1).T
        c0, c1, c2 = t[:, 0], t[:, 1], t[:, 2]
    # I(c1; c2 | c0) = H(c1|c0) + H(c2|c0) − H(c1c2 pair | c0)
    def hcond(a, ctx):
        return cond_entropy(a, ctx, nctx=256)
    pairkey = c1 * 256 + c2
    h_c1c2_c0 = cond_entropy(pairkey, c0, nctx=256 * 256)
    ii = hcond(c1, c0) + hcond(c2, c0) - h_c1c2_c0
    # R0-E offsets on the SHIPPED wire:
    offs = spatial_offsets(shipped, (plain.shape[0], plain.shape[1]))
    return dict(rows=rows, i_c1c2_g0=ii, offs=offs, ref=ref, kind=kind,
                planar=planar, token_wire=token_wire)


# ---------------- main --------------------------------------------------

CORPUS = [
    ("photos", [
        "photos/autumn_192x144_s900001.png",
        "photos/dusk_160x120_s900003.png",
        "photos/forest_224x160_s900002.png",
        "photos/ocean_192x144_s900007.png",
        "photos/sand_160x120_s900000.png",
        "photos/slate_192x144_s900004.png",
        "photos/slate_224x160_s900008.png",
        "photos/stone_160x120_s900006.png",
        "photos/terrain_160x120_s900009.png",
        "photos/terrain_224x160_s900005.png",
    ]),
    ("heldout", [
        "heldout-photos/autumn_160x120_s3100006.png",
        "heldout-photos/dusk_160x120_s3100003.png",
        "heldout-photos/dusk_224x160_s3100002.png",
        "heldout-photos/forest_192x144_s3100001.png",
        "heldout-photos/forest_224x160_s3100005.png",
        "heldout-photos/ocean_192x144_s3100007.png",
        "heldout-photos/sand_192x144_s3100004.png",
        "heldout-photos/slate_160x120_s3100009.png",
        "heldout-photos/terrain_160x120_s3100000.png",
    ]),
]


def fmt_offs(offs):
    return "  ".join(f"{n}:{g:+.3f}" for n, (a, g) in offs.items() if abs(g) >= 0.01)


def main():
    print(f"{'file':34s} {'ref':>3s} {'kind':5s} {'shipped':>8s}  "
          "H0  H|act  H|act+dir  H|prev   I(d1;d2|c0)   offsets>=0.01")
    agg = {"photos": [], "heldout": []}
    for part, rels in CORPUS:
        for rel in rels:
            full = str(ROOT / "testdata" / rel)
            img = np.asarray(Image.open(full).convert("RGB"))
            out = diag_file("testdata/" + rel, img)
            if out is None:
                print(f"{rel:34s} MNF9-magic — skipped")
                continue
            ref, kind, planar, token_wire = out["ref"], out["kind"], out["planar"], out["token_wire"]
            import subprocess as _sp
            shipped = _sp.run([str(ROOT / "target/release/manifold"), "encode",
                               "testdata/" + rel, "-o", "/tmp/r0sz.mnf5", "--mnf8=maximum"],
                              capture_output=True, check=True, cwd=str(ROOT))
            sz = Path("/tmp/r0sz.mnf5").stat().st_size
            row = out["rows"][ref]
            tag = "tok" if token_wire else ("plnr" if planar else "ilev")
            print(f"{rel:34s} {ref:>3d} {kind[:5]:5s}{tag} {sz:>8d}  "
                  f"{row[1]:.2f} {row[2]:.2f} {row[3]:.2f} {row[4]:.2f}   "
                  f"{out['i_c1c2_g0']:.3f}   {fmt_offs(out['offs'])}")
            agg[part].append(out)
    # aggregate held-out reproducibility: mean gains per diagnostic
    for part, outs in agg.items():
        if not outs:
            continue
        import numpy as _np
        def meancol(k, idx):
            vals = [o["rows"][o["ref"]][idx] for o in outs if o["rows"][o["ref"]][idx] == o["rows"][o["ref"]][idx]]
            return sum(vals) / len(vals) if vals else float("nan")
        off_best = _np.mean([
            max((g for _, g in o["offs"].values()), default=0.0) for o in outs])
        i_mean = sum(o["i_c1c2_g0"] for o in outs) / len(outs)
        print(f"[{part}] n={len(outs)}  mean H0={meancol(None,1):.3f} "
              f"H|act={meancol(None,2):.3f} H|act+dir={meancol(None,3):.3f} "
              f"H|prev={meancol(None,4):.3f}  mean I(c1;c2|c0)={i_mean:.3f}  "
              f"mean best-offset gain={off_best:.3f}")


if __name__ == "__main__":
    main()
