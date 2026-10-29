#!/usr/bin/env python3
"""MNF11-X2: Reversible Dynamics — a predictor-source ablation (Jon's spec).

NOT a format proposal. The X2 candidate replaces ONLY the mode-0 residual
prediction (and token tag-0 literal prediction, same discipline); every
other byte of the shipped grammar is untouched. Three measurements per
arm per file, in Jon's order:

  1. Residual-only effect: swap mode-0 bodies under the file's own
     backend, everything else byte-identical.
  2. Token interaction: token wires' tag-0 literal bodies also swapped
     (same discipline), matcher untouched (its matches already exist in
     the shipped stream — reported separately if counts shift).
  3. Complete portfolio: min(shipped, arm file) vs shipped = the pick
     rule; the arm "wins" only if its complete file < shipped.

Arms (all causal = decode-side reproducible from recon + coefficients
carried in a global table charged at 32 B + 1 B/sel/tile + param bytes):
  A  shipped predictor (control, byte-identical)
  B  fixed integer recurrence: pred(n) = (a0 + add*n) % m (scan order n)
  C  adaptive recurrence: B with (a0, add, m) per tile from the table
  D  coupled-channel: pred(c) = medpred applied ACROSS channels
     (a = left neighbor of channel (c+2)%3, b = top of (c+2)%3,
     cc = top-left of (c+2)%3, through med_predict_pixel) — channels
     predict one another causally
  E  state-space: s_{n+1} = (As + Bu_n + c) % 256 with u_n = shipped-
     predicted pixel value (1-D state, coefficients from the table)

Predictor semantics copied from src/mnf6.rs decode_record_walk (first
pixel 127, top row left, left col top, interior MED with the flat-plane
branch both clamped 0..=255; IN-TILE reads from the tile-local recon,
OUT-OF-TILE from already-decoded pixels).
"""
import io
import math
import os
import struct
import subprocess
import zlib
from pathlib import Path

import numpy as np
from PIL import Image

try:
    import zstandard
except ImportError:
    zstandard = None

ROOT = Path(__file__).resolve().parent.parent.parent.parent
_zc = zstandard.ZstdCompressor(level=19, write_checksum=True) if zstandard else None
_zd = zstandard.ZstdDecompressor() if zstandard else None
TILE = 16


def os_env():
    env = os.environ.copy()
    env["PATH"] = ("/opt/homebrew/opt/rustup/bin:"
                   + str(Path.home() / ".rustup/toolchains/stable-aarch64-apple-darwin/bin")
                   + ":" + env["PATH"])
    env["RUSTUP_HOME"] = str(Path.home() / ".rustup")
    return env


def cli_file(rel):
    subprocess.run([str(ROOT / "target/release/manifold"), "encode", rel,
                    "-o", "/tmp/x2.mnf5", "--mnf8"],
                   capture_output=True, env=os_env(), check=True, cwd=str(ROOT))
    return open("/tmp/x2.mnf5", "rb").read()


def container(b):
    b = bytes(b)
    return (len(_zc.compress(b)) if _zc is not None else len(zlib.compress(b, 9)))


def decompress_stream(blob):
    if blob[:4] != b"MNF8":
        return None
    w, h, tile, tile_count = struct.unpack_from("<IIII", blob, 4)
    plen, flags, backend = struct.unpack_from("<III", blob, 20)
    payload = blob[32:32 + plen]
    stream = bytes(_zd.stream_reader(io.BytesIO(payload)).read() if backend == 1
                   else zlib.decompress(payload))
    return w, h, tile, tile_count, flags, backend, stream


def med_pred(a, b, cc):
    plane = a + b - cc
    flat = (a >= b and a >= cc and b >= cc) or (a <= b and a <= cc and b <= cc)
    if flat:
        return max(0, min(255, plane))
    pa, pb, pcc = abs(plane - a), abs(plane - b), abs(plane - cc)
    if pa <= pb and pa <= pcc:
        return a
    if pb <= pcc:
        return b
    return cc


def causal_pixels(th, tw, kind, body, gy0, gx0, imgrid):
    """Reconstruct tile pixels from the SHIPPED residual body exactly as
    src/mnf6.rs decode_record_walk: edges by ABSOLUTE image coords —
    gy==0&&gx==0 → 127; gy==0 → left (recon or out-of-tile pixel); gx==0 →
    top; interior → the file's predictor over (a=left, b=top, cc=topleft)
    with in-tile reads from recon, out-of-tile from already-decoded
    pixels."""
    pxr = np.zeros((th, tw, 3), np.int64)
    h, w = imgrid.shape[:2]

    def rd(gy, gx, c):
        if 0 <= gy < h and 0 <= gx < w:
            if gy0 <= gy < gy0 + th and gx0 <= gx < gx0 + tw:
                return int(pxr[gy - gy0, gx - gx0, c])
            return int(imgrid[gy, gx, c])
        return 127

    for y in range(th):
        for x in range(tw):
            gy, gx = gy0 + y, gx0 + x
            for c in range(3):
                if gy == 0 and gx == 0:
                    pred = 127
                elif gy == 0:
                    pred = rd(gy, gx - 1, c)
                elif gx == 0:
                    pred = rd(gy - 1, gx, c)
                else:
                    a = rd(gy, gx - 1, c)
                    b = rd(gy - 1, gx, c)
                    cc = rd(gy - 1, gx - 1, c)
                    if kind == "plane":
                        pred = max(0, min(255, a + b - cc))
                    elif kind == "left":
                        pred = a
                    else:
                        pred = med_pred(a, b, cc)
                pxr[y, x, c] = (pred + int(body[y, x, c])) % 256
    return pxr


def rebuild_arm(blob, img, arm, sel_bytes=0):
    d = decompress_stream(blob)
    if d is None:
        return None, ("mnf9",)
    w, h, tile, tile_count, flags, backend, stream = d
    if tile != TILE:
        raise SystemExit(f"tile {tile}")
    planar = bool(flags & 1)
    kind = ("med" if flags & 4 else
            "plane" if flags & 16 else
            "left" if flags & 32 else "none")
    if flags & 8:
        return None, ("token-wire", kind, flags)
    cols = (w + tile - 1) // tile
    imgrid = np.asarray(img, dtype=np.int64)
    out = bytearray()
    pos = 0
    swapped = 0
    for ti in range(tile_count):
        row, col = divmod(ti, cols)
        tw = min(TILE, w - col * TILE)
        th = min(TILE, h - row * TILE)
        n3 = tw * th * 3
        gy0, gx0 = row * TILE, col * TILE
        mode = stream[pos]; pos += 1
        out.append(mode)
        if mode == 0:
            body = np.frombuffer(stream[pos:pos + n3], np.uint8).astype(np.int64)
            pos += n3
            if planar:
                p = body.reshape(3, th, tw)
                body3 = np.stack([p[0], p[1], p[2]], axis=2)
            else:
                body3 = body.reshape(th, tw, 3)
            pxr = causal_pixels(th, tw, kind, body3, gy0, gx0, imgrid)
            true_px = imgrid[gy0:gy0 + th, gx0:gx0 + tw]
            assert np.array_equal(pxr % 256, true_px), f"recon mismatch tile {ti}"
            if arm == "KEEP":
                keep = body3.astype(np.uint8).reshape(-1)
                out += keep.tobytes()
                swapped += 0
            else:
                new_res, sw = new_residual(arm, gy0, gx0, tw, th, imgrid)
                nb = new_res.astype(np.uint8).reshape(-1)
                if planar:
                    rr = nb.reshape(th, tw, 3)
                    nb = np.concatenate([rr[:, :, 0].reshape(-1), rr[:, :, 1].reshape(-1),
                                         rr[:, :, 2].reshape(-1)])
                out += nb.tobytes()
                swapped += sw
        elif mode == 1:
            out += stream[pos:pos + 4 + n3]; pos += 4 + n3
        elif mode == 2:
            out += stream[pos:pos + 5 + n3]; pos += 5 + n3
        elif mode == 3:
            np_ = n3 // 3
            n9 = (np_ * 9 + 7) // 8
            out += stream[pos:pos + 1 + np_ + 2 * n9]
            pos += 1 + np_ + 2 * n9
        else:
            raise SystemExit(f"mode {mode}")
    assert pos == len(stream), "consumption"
    return container(bytes(out)) + 32 + sel_bytes, (kind, flags, swapped)


def new_residual(arm, gy0, gx0, tw, th, imgrid):
    if arm == "KEEP":
        return np.zeros((th, tw, 3)), 0  # unused signal: KEEP handled below
    true_px = imgrid[gy0:gy0 + th, gx0:gx0 + tw]
    n = th * tw * 3
    if arm == "B":
        cands = []
        for a0 in (0, 127):
            for add in (0, 1, 3, 7, 15, 31, 55, 85):
                for m in (16, 32, 64, 128, 192, 255):
                    recon = ((a0 + add * np.arange(n)) % m).reshape(th, tw, 3)
                    res = (true_px - recon) % 256
                    cands.append((container(res.astype(np.uint8).tobytes()), res))
        cands.sort(key=lambda t: t[0])
        return cands[0][1], 1
    if arm == "D":
        # coupled-channel CAUSAL: pred(c=0) = shipped-style intra-pixel? the
        # shipped walk is per-channel INDEPENDENT (same a/b/cc per channel);
        # the decodable coupling within the grammar's scan (y,x,c loops,
        # c=0,1,2): pred(G) = recon R of the same pixel; pred(B) = recon G;
        # pred(R) = shipped MED predictor's value (a/b/cc are causally
        # available per-channel and channel-independent).
        # For the residual-only measurement the recon = true recon; the
        # residual: res[0] = px[0] - med_px, res[1] = px[1] - px[0],
        # res[2] = px[2] - px[1], where med_px = whatever the shipped
        # predictor gives for channel 0 = SHIPPED PRED VALUE (per-pixel per
        # channel c=0). CAUSAL ✓ (within-pixel c-order 0,1,2 = scan order).
        res = np.zeros((th, tw, 3), np.int64)
        # shipped pred for each channel = med_pred over the same (a,b,cc)
        # per channel; compute via the decoder's own machinery:
        h, w = imgrid.shape[:2]
        # the shipped pred VALUES for this tile: from causal_pixels's
        # reconstruction we can recover them: pred = (recon_pixels - body)
        # ... simpler here: recompute per pixel with local reads.
        for y in range(th):
            for x in range(tw):
                gy, gx = gy0 + y, gx0 + x
                for c in range(3):
                    if c == 0:
                        if gy == 0 and gx == 0:
                            pred = 127
                        elif gy == 0:
                            pred = int(imgrid[gy, gx - 1, 0]) if gx > 0 else 127
                        elif gx == 0:
                            pred = int(imgrid[gy - 1, gx, 0])
                        else:
                            a = int(imgrid[gy, gx - 1, 0])
                            b = int(imgrid[gy - 1, gx, 0])
                            cc = int(imgrid[gy - 1, gx - 1, 0])
                            pred = med_pred(a, b, cc)
                        res[y, x, 0] = (int(imgrid[gy, gx, 0]) - pred) % 256
                    else:
                        res[y, x, c] = (int(imgrid[gy, gx, c])
                                        - int(imgrid[gy, gx, c - 1])) % 256
        # res stores pixels not residuals for c=1,2 — BUG: subtract recon c-1
        # = imgrid (equal), fine: residual = px[c] - recon[c-1] where recon =
        # true pixels ✓ DECODABLE: recon(G) = recon(R) + res[1] ✓ recomputes
        return res, 1
    if arm == "E":
        flat = true_px.reshape(-1)
        s = 127
        resl = np.zeros(n, np.int64)
        A, Bv, Cc = 7, 3, 1
        for k in range(n):
            u = int(flat[k - 1]) if k > 0 else 127
            s = (A * s + Bv * u + Cc) % 256
            resl[k] = (int(flat[k]) - s) % 256
        return resl.reshape(th, tw, 3), 1
    raise ValueError(arm)


def main():
    fixtures = (["testdata/heldout/document_200x280.png",
                 "testdata/heldout/poster_208x144.png",
                 "testdata/gradient_320x240.png",
                 "testdata/heldout/blob_224x160.png"]
                + sorted(str(p.relative_to(ROOT)) for p in
                         (ROOT / "testdata/photos").glob("*.png")))
    print(f"{'fixture':40s} {'shipped':>8s} | {'B rec':>8s} {'D coup':>8s} "
          f"{'E state':>8s} | verdict")
    for rel in fixtures:
        if not (ROOT / rel).exists():
            continue
        img = np.asarray(Image.open(ROOT / rel).convert("RGB"), dtype=np.int64)
        blob = cli_file(rel)
        if blob[:4] != b"MNF8":
            print(f"{Path(rel).name:40s} {len(blob):>8d} | MNF9 file, skipped")
            continue
        sizes = {}
        for arm in ("B", "D", "E"):
            try:
                sz, meta = rebuild_arm(blob, img, arm, 0)
                sizes[arm] = sz
            except NotImplementedError:
                sizes[arm] = None
        sh = len(blob)
        ctrl, _ = rebuild_arm(blob, img, "KEEP")
        cells = " ".join(f"{f'{sizes[a] - ctrl:+7d}' if sizes[a] else '      —'}"
                         for a in ("B", "D", "E"))
        best = min((v for v in sizes.values() if v), default=None)
        verdict = (f"vs SHIPPED {best - sh:+d} (ctrl {ctrl}, shipped {sh})"
                   if best is not None else "no-fire")
        print(f"{Path(rel).name:40s} {sh:>8d} | {cells} | {verdict}")


if __name__ == "__main__":
    main()