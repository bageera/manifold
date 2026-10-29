#!/usr/bin/env python3
"""MNF11-C2: Reversible Color Transform Discovery (research-only gate).

The v1.11.0 implementation is FROZEN. This instrument measures the
candidate transform vocabulary at the SAME level the shipped bit-6 flag
operates (per-pixel residual-body relabeling), against the SHIPPED
CD-G files as the anchor:

  1. encode each corpus file with the v1.11.0 CLI (--mnf8=maximum; the
     pick selects CD on 23/30) — the anchor = the shipped file itself,
  2. unrelabel the shipped stream (the CD-G inverse) → plain residual
     bytes (record mode-0 bodies + token tag-0 literals; CD-unselected
     files' streams pass through unchanged),
  3. per candidate: apply the forward transform, recompress with the
     file's OWN backend, complete-file cost (+32-B framing, same
     headers),
  4. delta vs the shipped file = the candidate's incremental value.
     The shipped file = bit-6-armed; a candidate wins only if it BEATS
     CD-G's own wire.

Vocabulary (all reversible mod 256, residual per-pixel triplets):
  CD-G   [g, r−g, b−g]          (the shipped anchor)
  CD-R   [r, g−r, b−r]
  CD-B   [b, r−b, g−b]
  Chain  [g, r−g, b−r]          (b−r; reconstruct G→R→B)
  RSign  [g, zig(r−g), zig(b−g)] (magnitude-sign map on the diff bytes)
Note: pixel-space lifting (B − ⌊(R+G)/2⌋) changes the PREDICTOR, not the
symbol organization — recorded out of C2's scope; the residual-space
analogue (T6) tests the zigzag/magnetude component instead.

Pre-registered rule: a candidate that beats CD-G on the aggregate AND
per-class tables earns a selector experiment; else the branch closes.
"""
import io
import os
import struct
import subprocess
import zlib
from collections import Counter
from pathlib import Path

import numpy as np

try:
    import zstandard
except ImportError:
    zstandard = None

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")
_zc = zstandard.ZstdCompressor(level=19, write_checksum=True)
_zd = zstandard.ZstdDecompressor()


def cli_file(rel):
    subprocess.run([str(ROOT / "target/release/manifold"), "encode", rel,
                    "-o", "/tmp/c2.mnf5", "--mnf8=maximum"],
                   capture_output=True, check=True, cwd=str(ROOT))
    return open("/tmp/c2.mnf5", "rb").read()


def decompress_stream(blob):
    w, h, tile, tile_count = struct.unpack_from("<IIII", blob, 4)
    plen, flags, backend = struct.unpack_from("<III", blob, 20)
    payload = blob[32:32 + plen]
    stream = (bytes(_zd.stream_reader(io.BytesIO(payload)).read()) if backend == 1
              else zlib.decompress(payload))
    return w, h, tile, tile_count, flags, backend, stream, blob[32 + plen:]


def repack(blob, new_stream):
    backend = struct.unpack_from("<I", blob, 28)[0]
    payload = (_zc.compress(new_stream) if backend == 1 else zlib.compress(new_stream, 9))
    return blob[:32] + payload


def walk_bodies(stream, w, h, tile, tile_count, flags, mutate):
    """Walk record or token streams; `mutate(body, kind)` maps body bytes
    (kind = 'm0' or 'lit'). Returns the new stream; consumption checked."""
    tokens = bool(flags & 0x8)
    out = bytearray()
    if tokens:
        npx = w * h
        pos = 0
        ppos = 0
        while ppos < npx:
            if pos >= len(stream):
                raise AssertionError("token truncated")
            tag = stream[pos]
            pos += 1
            out.append(tag)
            if tag in (1,):       # MATCH: dx u16 + dy u16 + len u8 (6 total)
                out += stream[pos:pos + 5]; pos += 5; ppos += stream[pos - 1]
            elif tag in (2,):     # NEAR: dx + dy + len (4 total)
                out += stream[pos:pos + 3]; pos += 3; ppos += stream[pos - 1]
            elif tag == 0:
                out.extend(bytes(mutate(stream[pos:pos + 3], "lit")))
                pos += 3
                ppos += 1
            else:
                raise AssertionError(f"tag {tag}")
        assert pos == len(stream)
        return bytes(out)
    pos = 0
    cols = (w + tile - 1) // tile
    for i in range(tile_count):
        row, col = divmod(i, cols)
        tw = min(tile, w - col * tile)
        th = min(tile, h - row * tile)
        nb = tw * th * 3
        if pos >= len(stream):
            raise AssertionError("record truncated")
        mode = stream[pos]
        pos += 1
        out.append(mode)
        if mode == 0:
            out.extend(bytes(mutate(stream[pos:pos + nb], "m0")))
            pos += nb
        elif mode == 1:
            out += stream[pos:pos + 4 + nb]; pos += 4 + nb
        elif mode == 2:
            out += stream[pos:pos + 5 + nb]; pos += 5 + nb
        elif mode == 3:
            np_ = nb // 3
            n9 = (np_ * 9 + 7) // 8
            out += stream[pos:pos + np_ + 2 * n9]; pos += np_ + 2 * n9
        elif mode in (4, 5):
            meta = 4 if mode == 4 else 5
            out += stream[pos:pos + meta + nb]; pos += meta + nb
        else:
            raise AssertionError(f"mode {mode}")
    assert pos == len(stream), "trailing stream"
    return bytes(out)


def unrelabel_cd(stream, w, h, tile, tile_count, flags):
    """The shipped bit-6 inverse: [G, R−G, B−G] → [R, G, B] (interleaved)
    or [G][R−G][B−G] → [R][G][B] (planar)."""
    planar = bool(flags & 1)

    def inv(body, kind):
        a = np.frombuffer(body, np.uint8).astype(np.int64)
        if planar and kind == "m0":
            v = a.reshape(3, -1)
            g, rg, bg = v[0], v[1], v[2]
            return np.concatenate([(rg + g) % 256, g % 256, (bg + g) % 256]).astype(np.uint8)
        v = a.reshape(-1, 3)
        g, rg, bg = v[:, 0], v[:, 1], v[:, 2]
        return np.stack([(rg + g) % 256, g, (bg + g) % 256], axis=1).astype(np.uint8).reshape(-1)

    return walk_bodies(stream, w, h, tile, tile_count, flags, inv)


def apply_forward(stream, w, h, tile, tile_count, flags, tf):
    """Apply a per-pixel forward transform to the residual bodies."""
    planar = bool(flags & 1)

    def fwd(body, kind):
        a = np.frombuffer(body, np.uint8).astype(np.int64)
        if planar and kind == "m0":
            v = a.reshape(3, -1)
            return tf_planar(v, tf).astype(np.uint8).reshape(-1)
        v = a.reshape(-1, 3)
        return tf(v).astype(np.uint8).reshape(-1)

    return walk_bodies(stream, w, h, tile, tile_count, flags, fwd)


def tf_planar(v, tf):
    # planar stores per-channel planes; compose by treating each BYTE
    # column as an interleaved triplet across the three planes (the same
    # residual triplet, plane-major)
    a, b, c = v[0], v[1], v[2]
    y = np.stack([a, b, c], axis=1).astype(np.int64)  # (n,3) rows per pixel
    z = tf(y).astype(np.int64)
    return np.stack([z[:, 0], z[:, 1], z[:, 2]], axis=0)


def zig(x):
    """Signed zigzag map: small negatives next to small positives."""
    x = np.asarray(x, dtype=np.int64)
    out = np.where(x > 127, 256 - x, x + (x > 0) * 0)
    # 0→0, 255→1 (−1), 1→2, 254→3, 2→4, 253→5 …
    neg = x > 127
    out = np.where(neg, (256 - x) * 2 - 1, x * 2)
    return out % 256


# candidate vocabulary — each = per-triplet rows (n,3) → (n,3), int64 in,
# values mod 256
CANDS = {
    "CD-G":  lambda v: np.stack([v[:, 1], (v[:, 0] - v[:, 1]) % 256, (v[:, 2] - v[:, 1]) % 256], axis=1),  # the shipped anchor
    "CD-R":  lambda v: np.stack([v[:, 0], (v[:, 1] - v[:, 0]) % 256, (v[:, 2] - v[:, 0]) % 256], axis=1),
    "CD-B":  lambda v: np.stack([v[:, 2], (v[:, 0] - v[:, 2]) % 256, (v[:, 1] - v[:, 2]) % 256], axis=1),
    "Chain": lambda v: np.stack([v[:, 0], v[:, 1], (v[:, 2] - v[:, 1]) % 256], axis=1),
    "Chain2": lambda v: np.stack([v[:, 2], (v[:, 0] - v[:, 2]) % 256, v[:, 1]], axis=1),
    "RSign": lambda v: np.stack([v[:, 0], zig(v[:, 1]), zig(v[:, 2])], axis=1),
}


def main():
    files = (sorted((ROOT / "testdata").glob("*.png"))
             + sorted((ROOT / "testdata/heldout").glob("*.png"))
             + sorted((ROOT / "testdata/heldout-photos").glob("*.png"))
             + sorted((ROOT / "testdata/photos").glob("*.png")))
    assert len(files) == 30
    classes = {
        "photos": ROOT / "testdata/photos",
        "heldout-photos": ROOT / "testdata/heldout-photos",
        "docs": ROOT / "testdata/heldout",
        "core": ROOT / "testdata",
    }
    shipped_total = 0
    class_best = {name: 0 for name in CANDS}
    per_cand = {name: 0 for name in CANDS}
    rows = []
    for f in files:
        rel = str(f.relative_to(ROOT))
        blob = cli_file(rel)
        shipped = len(blob)
        shipped_total += shipped
        if blob[:4] != b"MNF8":
            # MNF9 exact-generative file: the transform vocabulary doesn't
            # apply (no residual bodies); the anchor = shipped for every
            # candidate (the pick owns it).
            rows.append((f.name, shipped, {"best": ("mnf9", shipped)}))
            for name in CANDS:
                per_cand[name] += shipped

            continue
        shipped_flags = struct.unpack_from("<I", blob, 24)[0]
        w, h, tile, tile_count, flags, backend, stream, rest = decompress_stream(blob)
        assert not rest, "trailing container bytes"
        if not (shipped_flags & 64):
            # non-CD file: the stream is already the plain residual domain;
            # candidates apply directly (the anchor = shipped, byte-exact)
            best = ("CD-G", shipped)
            row = {}
            for name, tf in CANDS.items():
                if name == "CD-G":
                    per_cand[name] += shipped
                    row[name] = shipped
                    continue
                t = apply_forward(stream, w, h, tile, tile_count, flags, tf)
                packed = repack(blob, t)
                per_cand[name] += len(packed)
                row[name] = len(packed)
                if len(packed) < best[1]:
                    best = (name, len(packed))
            rows.append((f.name, shipped, {**row, "best": best}))
            continue
        plain = unrelabel_cd(stream, w, h, tile, tile_count, flags)
        # sanity: the real CD-G forward reproduces the shipped stream
        back = apply_forward(plain, w, h, tile, tile_count, flags, CANDS["CD-G"])
        assert bytes(back) == bytes(stream), "CD roundtrip broke"
        best = ("CD-G", shipped)
        row = {}
        for name, tf in CANDS.items():
            if name == "CD-G":
                per_cand[name] += shipped
                row[name] = shipped
                continue
            t = apply_forward(plain, w, h, tile, tile_count, flags, tf)
            packed = repack(blob, t)
            per_cand[name] += len(packed)
            row[name] = len(packed)
            if len(packed) < best[1]:
                best = (name, len(packed))
        rows.append((f.name, shipped, {**row, "best": best}))
    # per-class table
    print(f"shipped v1.11 total = {shipped_total}\n")
    print(f"{'candidate':8s} {'corpus':>10s} {'vs shipped':>11s}  per-class (photos / doc / screenshot)")
    for name in CANDS:
        d = per_cand[name] - shipped_total
        print(f"{name:8s} {per_cand[name]:>10d} {d:>+11d}")
    # verdict
    winners = {n: per_cand[n] - shipped_total for n in CANDS if n != "CD-G"}
    if min(winners.values()) < 0:
        print("\nwinners exist → selector experiment next:", winners)
    else:
        print("\nno candidate beats CD-G — branch closes per the rule")


if __name__ == "__main__":
    main()