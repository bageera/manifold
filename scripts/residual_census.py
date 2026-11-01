#!/usr/bin/env python3
"""MNF14-ID4 — production residual-cost census of the shipped MNF8 pick.

The ADR-010.4 order of operations, after ID3: measure WHERE the release
codec spends bytes BEFORE proposing another vocabulary. The five research
questions (high-entropy texture / flat+sparse exceptions / repeated
patterns / predictor transitions / quantized-math transport) map to the
buckets below.

Production-true method:
  1. release CLI encode --mnf8 (--profile maximum AND balanced) per file
     — the shipped pick, never a python reimplementation.
  2. header parse + container decompress + record-grammar walk with
     per-tile (mode, body). Desync detector: consumed == stream len on
     EVERY file (the check that caught the v1.9 walker's near-token bug).
     MNF9-magic files record as whole-frame-law and skip the walk;
     FLAG_TOKENS files get the exact §3.1 tag census.
  3. WIRE-EXACTNESS validator: for firing modes 0 and 8 the tile is
     reconstructed FROM the stream bytes (REF-field inverse + planar
     unpack + predictor/const add) and asserted == source pixels per
     tile. Walk offsets, REF semantics, and the predictor map are all
     proved by the same pass. Modes 6/7 (IF4) are length-walked only.
  4. per-tile semantic class from the FILE'S OWN stored residual
     (per mode: literal -> predictor-delta; const-8 -> (x - const);
     refs/ycocg -> predictor-delta approximation), byte-priced with the
     zlib-9 proxy (ID2 census convention; labeled estimate).
     Classes: zero / sparse-small (<=25% nonzero, amp <= 8) /
     texture-heavy (>=40% amp >= 16) / edge (any |d| >= 32) / mixed.
  5. aggregation answers the five questions with numbers.

Usage: residual_census.py [--out /tmp/id4_census.csv] [--only substr]
"""
import argparse
import csv
import io
import json
import struct
import subprocess as sp
import zlib
from pathlib import Path

import numpy as np
from PIL import Image

try:
    import zstandard
except ImportError:
    zstandard = None

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")
BIN = ROOT / "target/release/manifold"
TS = 16

# metadata bytes after the mode byte, per record grammar (docs/format.md)
META = {0: 0, 1: 4, 2: 5, 3: 0, 4: 4, 5: 5, 6: 12, 7: 12, 8: 3}


def record_extra(mode, npix3):
    """Bytes after the mode byte for one record (npix3 = 3*tw*th)."""
    if mode == 3:
        n1 = npix3 // 3
        n9 = (9 * n1 + 7) // 8
        return n1 + 2 * n9  # Y plane (1 B/px) + co9/cg9 bit-packed
    return META[mode] + npix3


def parse_records(stream, w, h, tile_count):
    cols = (w + TS - 1) // TS
    recs = []
    pos = 0
    for t in range(tile_count):
        gy, gx = divmod(t, cols)
        tw = min(TS, w - gx * TS)
        th = min(TS, h - gy * TS)
        nb = 3 * tw * th
        mode = stream[pos]
        pos += 1
        extra = record_extra(mode, nb)
        recs.append((t, mode, pos, extra, nb))
        pos += extra
    return recs, pos


def token_walk(stream, w, h):
    """Exact §3.1 token census: tag0 [tag+3B]=4, tag1 [tag+dx2+dy2+len]=6,
    tag2 [tag+dx1+dy1+len]=4; len is the record's last byte."""
    npx = w * h
    pos = 0
    tags = {0: 0, 1: 0, 2: 0}
    while pos < len(stream):
        tag = stream[pos]
        pos += 1
        tags[tag] += 1
        if tag == 0:
            npx -= 1
            pos += 3
        elif tag == 1:
            ln = stream[pos + 4]
            pos += 5
            npx -= ln
        else:
            ln = stream[pos + 2]
            pos += 3
            npx -= ln
    if npx != 0:
        raise SystemExit(f"token pixel-account desync: leftover {npx}")
    return tags, pos


def pred_map_for(flags):
    if flags & 0x4:
        return "med"
    if flags & 0x10:
        return "plane"
    if flags & 0x20:
        return "left"
    if flags & 0x8:
        return "med"  # token payloads ARE the MED residual wire
    return None


def compute_pred(name, px):
    """Vectorized image-wide predictors with the SHIPPED edge semantics
    (mnf6.rs med_predict / mnf8.rs image_predict): (0,0)=127, top row=left
    neighbour, left col=ABOVE neighbour, interior=kind."""
    h, w = px.shape[:2]
    out = np.empty_like(px)
    out[0, 0] = 127
    out[1:, 0] = px[:-1, 0]  # x==0 pred = above (true pixels)
    if w > 1:
        out[0, 1:] = px[0, :-1]  # y==0 pred = left
    if h > 1 and w > 1:
        a = np.concatenate([px[1:, :1], px[1:, :-1]], axis=1)  # left
        b = px[:-1, :]                                          # above
        cc = np.concatenate([px[:-1, :1], px[:-1, :-1]], axis=1)  # top-left
        if name == "left":
            out[1:, 1:] = a[:, 1:]
        elif name == "plane":
            out[1:, 1:] = np.clip(a[:, 1:] + b[:, 1:] - cc[:, 1:], 0, 255)
        else:  # med (LOCO-I edge-sense)
            plane = a[:, 1:] + b[:, 1:] - cc[:, 1:]
            flat = (((a[:, 1:] >= b[:, 1:]) & (a[:, 1:] >= cc[:, 1:])
                     & (b[:, 1:] >= cc[:, 1:]))
                    | ((a[:, 1:] <= b[:, 1:]) & (a[:, 1:] <= cc[:, 1:])
                       & (b[:, 1:] <= cc[:, 1:])))
            dA, dB, dC = (np.abs(plane - x[:, 1:]) for x in (a, b, cc))
            pick = np.where((dA <= dB) & (dA <= dC), a[:, 1:],
                            np.where(dB <= dC, b[:, 1:], cc[:, 1:]))
            out[1:, 1:] = np.where(flat, np.clip(plane, 0, 255), pick)
    return out


def classify(resid):
    """resid: signed per-tile residual HxWx3 (mod NOT applied)."""
    a = np.abs(resid)
    nz = float((a > 0).mean())
    if nz == 0:
        return "zero"
    if int(a.max()) >= 32:
        return "edge"
    if nz <= 0.25 and int(a.max()) <= 8:
        return "sparse-small"
    if float((a >= 16).mean()) >= 0.40:
        return "texture-heavy"
    return "mixed"


def inv_cd_px(wb, reference):
    """Interleaved CD inverse: wire per-pixel (refc, d1, d2) -> (R, G, B)."""
    if reference == 1:      # [G, R-G, B-G]
        return np.stack([(wb[..., 0] + wb[..., 1]) % 256, wb[..., 0],
                         (wb[..., 0] + wb[..., 2]) % 256], -1)
    if reference == 2:      # [R, G-R, B-R]
        return np.stack([wb[..., 0], (wb[..., 0] + wb[..., 1]) % 256,
                         (wb[..., 0] + wb[..., 2]) % 256], -1)
    if reference == 3:      # [B, R-B, G-B]
        return np.stack([(wb[..., 0] + wb[..., 1]) % 256,
                         (wb[..., 0] + wb[..., 2]) % 256, wb[..., 0]], -1)
    return wb


def inv_cd_planar(wp, reference):
    """Planar CD inverse: wire planes [pref][d1][d2] -> [R][G][B] planes
    (unpermute_wire_to_planar semantics)."""
    p0, p1, p2 = wp[0], wp[1], wp[2]
    if reference == 1:      # [G][R-G][B-G] -> R=d1+ref, G=ref, B=d2+ref
        return np.stack([(p1 + p0) % 256, p0, (p2 + p0) % 256], 0)
    if reference == 2:      # [R][G-R][B-R]
        return np.stack([p0, (p1 + p0) % 256, (p2 + p0) % 256], 0)
    if reference == 3:      # [B][R-B][G-B]
        return np.stack([(p1 + p0) % 256, (p2 + p0) % 256, p0], 0)
    return wp


def reconstruct_mode0(body, pred3, planar, reference, th, tw):
    """body = the record's value bytes (nb). Returns reconstructed px."""
    nb = 3 * th * tw
    if planar:
        wp = np.frombuffer(body, np.uint8).reshape(3, th, tw).astype(np.int64)
        res3 = np.stack([inv_cd_planar(wp, reference)[c] for c in range(3)], -1) \
            if reference else np.stack([wp[0], wp[1], wp[2]], -1)
        if pred3 is None:
            return res3 % 256
        return (pred3 + res3) % 256
    wb = np.frombuffer(body, np.uint8).reshape(th, tw, 3).astype(np.int64)
    res3 = inv_cd_px(wb, reference)
    if pred3 is None:
        return res3 % 256
    return (pred3 + res3) % 256


def reconstruct_mode8(body, planar, reference, th, tw):
    """body = [3 B const][nb B residual]. Returns reconstructed px."""
    const = np.frombuffer(body[:3], np.uint8).astype(np.int64)  # (R,G,B) const
    rb = body[3:]
    nb = 3 * th * tw
    if planar:
        wp = np.frombuffer(rb, np.uint8).reshape(3, th, tw).astype(np.int64)
        res3 = np.stack([inv_cd_planar(wp, reference)[c] for c in range(3)], -1) \
            if reference else np.stack([wp[0], wp[1], wp[2]], -1)
    else:
        wb = np.frombuffer(rb, np.uint8).reshape(th, tw, 3).astype(np.int64)
        res3 = inv_cd_px(wb, reference)
    return (const[None, None, :] + res3) % 256


def census_file(png, out_dir, truth=None, tag=""):
    rel = tag + png.name
    if truth is not None:
        cls = "hubble"
    elif "/photos/" in str(png) or "heldout-photos" in str(png):
        cls = "photo"
    elif "heldout" in str(png):
        cls = "heldout"
    else:
        cls = "synthetic"
    fmax = out_dir / f"{rel}.max.mnf8"
    fbal = out_dir / f"{rel}.bal.mnf8"
    for prof, outp in (("maximum", fmax), ("balanced", fbal)):
        r = sp.run([str(BIN), "encode", str(png), "-o", str(outp), "--mnf8",
                    "--profile", prof], capture_output=True, text=True)
        assert r.returncode == 0, (rel, prof, r.stderr[-400:])
    blob = fmax.read_bytes()
    size_bal = fbal.stat().st_size
    if blob[:4] == b"MNF9":
        _, w, h, _, tc, plen, flags, backend = struct.unpack("<4sIIIIIII", blob[:32])
        return {"file": rel, "class": cls, "magic": "MNF9", "w": w, "h": h,
                "size_max": len(blob), "size_bal": size_bal,
                "regret": len(blob) - size_bal, "mode_hist": {}, "tokens": None,
                "tiles": tc, "classes": {}, "validated": 0,
                "flags": hex(flags), "backend": backend}
    w, h, tile, tile_count, plen, flags, backend = struct.unpack("<IIIIIII", blob[4:32])
    size_max_var = len(blob)  # the maximum-profile file's true size (regret basis)
    payload = blob[32:32 + plen]
    walk_src = "maximum"
    if backend not in (0, 1):
        # mix backends (4/5) carry a range-coder stream — no python decode.
        # Walk the zstd-pinned balanced file instead, LABELED: stage-2
        # re-costing can move per-tile picks, so this is the honest
        # walk source, never a silent substitution.
        blob = fbal.read_bytes()
        assert blob[:4] == b"MNF8", (rel, "balanced magic", blob[:4])
        w, h, tile, tile_count, plen, flags, backend = struct.unpack(
            "<IIIIIII", blob[4:32])
        payload = blob[32:32 + plen]
        walk_src = "balanced"
    if backend == 1:
        assert zstandard is not None, "zstd payload but no zstandard module"
        stream = zstandard.ZstdDecompressor().stream_reader(io.BytesIO(payload)).read()
    else:
        stream = zlib.decompress(payload)
    if flags & 0x8:
        tags, consumed = token_walk(stream, w, h)
        assert consumed == len(stream), (rel, "token desync", consumed, len(stream))
        return {"file": rel, "class": cls, "magic": "MNF8", "w": w, "h": h,
                "size_max": size_max_var, "size_bal": size_bal,
                "regret": size_max_var - size_bal, "mode_hist": {}, "tokens": tags,
                "tiles": 0, "classes": {}, "validated": 0,
                "flags": hex(flags), "backend": backend, "walk_src": walk_src}
    recs, consumed = parse_records(stream, w, h, tile_count)
    assert consumed == len(stream), (rel, "record desync", consumed, len(stream))
    px = np.asarray(Image.open(truth or png).convert("RGB"), np.int64)
    assert px.shape[:2] == (h, w), (rel, "truth dims", px.shape, (h, w))
    pred_name = pred_map_for(flags)
    pmap = compute_pred(pred_name, px) if pred_name else None
    cols = (w + TS - 1) // TS
    planar = bool(flags & 0x1)
    reference = (flags >> 6) & 0b11
    mode_hist, class_stats = {}, {}
    n_valid = 0
    for t, mode, metapos, extra, nb in recs:
        gy, gx = divmod(t, cols)
        tw = min(TS, w - gx * TS)
        th = min(TS, h - gy * TS)
        mode_hist[mode] = mode_hist.get(mode, 0) + 1
        tpx = px[gy * TS:gy * TS + th, gx * TS:gx * TS + tw]
        pred3 = pmap[gy * TS:gy * TS + th, gx * TS:gx * TS + tw] if pmap is not None else None
        bodyf = stream[metapos:metapos + extra]
        # the file's own stored residual, per mode (classification basis)
        if mode == 0:
            stored = (tpx - (pred3 if pred3 is not None else 0)) % 256
            rec = reconstruct_mode0(bodyf, pred3, planar, reference, th, tw)
        elif mode == 8:
            const = np.frombuffer(bodyf[:3], np.uint8).astype(np.int64)
            stored = (tpx - const[None, None, :]) % 256
            rec = reconstruct_mode8(bodyf, planar, reference, th, tw)
        else:
            stored = (tpx - (pred3 if pred3 is not None else 0)) % 256
            rec = None
        if rec is not None:
            assert np.array_equal(rec.astype(np.uint8),
                                  tpx.astype(np.uint8)), (rel, t, f"mode {mode}")
            n_valid += 1
        ck = classify(stored.astype(np.int64))
        st = class_stats.setdefault(ck, {"tiles": 0, "z9": 0, "nz_sum": 0.0})
        st["tiles"] += 1
        st["z9"] += len(zlib.compress(stored.astype(np.uint8).tobytes(), 9))
        st["nz_sum"] += float((np.abs(stored.astype(np.int64)) > 0).mean())
    return {"file": rel, "class": cls, "magic": "MNF8", "w": w, "h": h,
            "size_max": size_max_var, "size_bal": size_bal,
            "regret": size_max_var - size_bal, "mode_hist": mode_hist,
            "tokens": None,
            "classes": {k: {"tiles": v["tiles"], "z9": v["z9"],
                            "nz_mean": round(v["nz_sum"] / v["tiles"], 3)}
                        for k, v in sorted(class_stats.items())},
            "validated": n_valid, "flags": hex(flags), "backend": backend,
            "walk_src": walk_src}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="/tmp/id4_census.csv")
    ap.add_argument("--only", default="")
    a = ap.parse_args()
    out_dir = Path("/tmp/id4_files")
    out_dir.mkdir(exist_ok=True)
    named = [(p, None, "") for p in sorted((ROOT / "testdata").glob("**/*.png"))
             if a.only in p.name]
    hub_src = Path("/tmp/mnf14_if2/hubble_deep_field.jpg")
    hub_truth = Path("/tmp/hf1_hubble_decoded.png")
    if hub_src.exists() and hub_truth.exists():
        named.append((hub_src, hub_truth, "hub_"))
    rows = []
    for png, truth, tag in named:
        r = census_file(png, out_dir, truth, tag)
        rows.append(r)
        print(json.dumps(r), flush=True)
    thirty = [r for r in rows if not r["file"].startswith("hub_")]
    tot = sum(r["size_max"] for r in thirty)
    print(f"\n30-corpus total (maximum): {tot} B  [anchor: 812,180]")
    with open(a.out, "w", newline="") as f:
        keys = []
        for r in rows:
            for k in r:
                if k not in keys:
                    keys.append(k)
        wr = csv.DictWriter(f, fieldnames=keys)
        wr.writeheader()
        wr.writerows(rows)
    print(f"csv -> {a.out}")


if __name__ == "__main__":
    main()