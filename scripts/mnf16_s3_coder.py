#!/usr/bin/env python3
"""MNF16-S3 — coder-level cross-scale conditioning through the R9 bridge.

S2 accepted G8's heavy-band refinement (S2-5) as the primary candidate;
S3 runs it through the ACTUAL adaptive backend-6 schedule via the
additively extended r9_audit bridge (`g8child` token: children
20 + (k-16)*8 + feat byte on the heavy band, 36 contexts, children =
plain fresh models replicating the S2-5 static form).

FROZEN SPEC (before the Hubble holdout; Jon's contract):
  - feature: G8 on the same plane, |west-8| + |north-8| mean, edges
    [4, 16, 32, 64, 96, 128, 160] (S1/S2 frozen), buckets 0..7.
  - mapping: heavy key 16..19 -> 20 + (k-16)*8 + g; others unchanged.
  - init: children = fresh Dual models (no parent, no warm mix) — the
    S2-5 replacement form; `share` semantics of the shipped schedule
    retained for base/key parents (children's parent = 0xFF).
  - update: shipped lr 0.15 / inc 32, caps 32768/1048576; 2^24 fixed
    point (the audit instrument's quantization).
  - control S3-2: feature bytes permuted WITHIN FILE, seed 16.

 Arms: S3-0 = shipped b6 audit (frozen); S3-1 = +G8 children;
 S3-2 = +shuffled; S3-H = S3-1 on Hubble, single shot, frozen spec.

 Feature-reconstruction contract (gate: decoder availability): every
 feature byte must be a pure function of the DECODED stream's earlier
 bytes and the record-position math (no image, no future bytes). The
 harness verifies: (a) recompute-from-lut == submitted features,
 (b) every neighbor lut index is strictly earlier in the stream.
"""
import json
import struct
import subprocess
import sys
import time
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "crates/mnf-mans/scripts"))
sys.path.insert(0, str(ROOT / "scripts"))
from mnf13_r9a_gate import (  # noqa: E402
    BRIDGE, bucket, bridge, extract_stream, _walk_check)
from mnf13_r9a_gate import B5  # noqa: E402
import mnf16_s1a_census as s1  # noqa: E402
import mnf16_s2_bridge as s2  # noqa: E402
import residual_census as rc  # noqa: E402


B6 = s2.B6
B6G8 = s2.B6 + "-g8child"
TS = 16
EDGES = np.array([4, 16, 32, 64, 96, 128, 160])


def annotate_all(stream, flags, w, h, tile_count):
    """Position map (y, x, channel) for EVERY residual symbol of the
    decoded stream — mode-0 AND law bodies (the law residual layout is
    identical: tw*th*3 after the law bytes). Returns (wire, pos, idx)."""
    cols = (w + TS - 1) // TS
    planar = bool(flags & 1)
    pos, wire, idx = [], [], []
    off = 0
    for t in range(tile_count):
        gy, gx = divmod(t, cols)
        tw, th = min(TS, w - gx * TS), min(TS, h - gy * TS)
        mode = stream[off]
        off += 1
        # every record mode carries meta bytes then the SAME npix3
        # residual block (META table = the committed record grammar,
        # scripts/residual_census.py; law = 12/9 planar/interleaved)
        meta = {0: 0, 1: 4, 2: 5, 3: 0, 4: 4, 5: 5, 8: 3}.get(mode)
        if meta is None:
            assert mode in (6, 7), (t, mode)
            meta = 12 if planar else 9
        off += meta
        body = np.frombuffer(stream, np.uint8, tw * th * 3, off)
        idx_base = off
        off += body.size
        if planar:
            wp = body.reshape(3, th, tw)
            for k in range(3):
                for ly in range(th):
                    for lx in range(tw):
                        pos.append((gy * TS + ly, gx * TS + lx, k))
                        wire.append(wp[k, ly, lx])
                        idx.append(idx_base + k * th * tw + ly * tw + lx)
        else:
            wb = body.reshape(th, tw, 3)
            for ly in range(th):
                for lx in range(tw):
                    for k in range(3):
                        pos.append((gy * TS + ly, gx * TS + lx, k))
                        wire.append(wb[ly, lx, k])
                        idx.append(idx_base + (ly * tw + lx) * 3 + k)
    assert off == len(stream), (off, len(stream))
    return (np.asarray(wire, np.int64), np.asarray(pos, np.int64),
            np.asarray(idx, np.intp))


def g8_feature_bytes(stream, flags, w, h, tile_count, n_syms):
    """The frozen G8 feature byte per STREAM symbol (0..7), computed from
    same-plane earlier positions only (frozen stage-1 convention: clip to
    image bounds, 128 for positions the stream never annotated)."""
    wire, pos, idx = annotate_all(stream, flags, w, h, tile_count)
    h_, w_ = h, w
    y, x, c = pos[:, 0], pos[:, 1], pos[:, 2]
    lut = -np.ones((h_, w_, 3), np.int64)
    lut[y, x, c] = np.arange(len(wire))            # annotated-subset index

    def sym(dy, dx):
        yy = np.clip(y + dy, 0, h_ - 1)
        xx = np.clip(x + dx, 0, w_ - 1)
        li = lut[yy, xx, c]
        return np.where(li >= 0, wire[li], 128)

    g8 = (np.abs(sym(0, -8) - wire) + np.abs(sym(-8, 0) - wire)) / 2
    fb = np.zeros(n_syms, np.uint8)
    fb[idx] = s1.bucket(g8, EDGES).astype(np.uint8)
    # temporal dependency proof (Jon's gate): every read neighbor is
    # strictly EARLIER in the stream, except border clips that resolve to
    # the symbol's own position (yy,x) == (y,x) — the value there equals
    # the intended "no neighbor" zero-distance convention.
    for dy, dx in ((0, -8), (-8, 0)):
        yy = np.clip(y + dy, 0, h_ - 1)
        xx = np.clip(x + dx, 0, w_ - 1)
        li = lut[yy, xx, c]
        hit = li >= 0
        earlier = idx[li[hit]] <= idx[hit]
        assert earlier.all(), "feature read later than target"
        self_clip = (yy == y) & (xx == x)
        eq = idx[li[hit]] == idx[hit]
        assert (eq & ~self_clip[hit]).sum() == 0, "self-read off the border"
    return fb


def keys_g8(stream, backend, feats):
    """Python mirror of feat_key (for per-context accounting)."""
    ks = s2.prod_keys(stream, backend).astype(np.intp)
    out = np.where((ks >= 16) & (ks <= 19), (ks - 16) * 8 + 20 + feats, ks)
    return out.astype(np.intp)


def bridge_feat(mode, stream, feats, cmd):
    r"""r9a's bridge() stamps len(input) as n; feat modes need
    n = symbol count with n feature bytes after the stream."""
    hdr = f"{len(stream)} {mode}\n".encode()
    r = subprocess.run([str(BRIDGE), cmd, mode], input=hdr + stream + feats,
                       capture_output=True)
    if r.returncode != 0:
        raise RuntimeError(f"bridge {cmd} {mode}: {r.stderr[:200]!r}")
    return r.stdout


def audit_bits(mode, stream, feats=None):
    audit = (bridge_feat(mode, stream, feats.tobytes(), "a") if feats is not None
             else bridge(mode, stream, "a"))
    n = int.from_bytes(audit[:4], "little")
    assert n == len(stream), (n, len(stream))
    return np.frombuffer(audit[4:4 + 8 * n], np.float64)


def wire_len(mode, stream, feats=None):
    """Entropy-coded length under the mode (the 'e' direction)."""
    out = (bridge_feat(mode, stream, feats.tobytes(), "e") if feats is not None
           else bridge(mode, stream, "e"))
    n = int.from_bytes(out[:4], "little")
    return (len(out) - 4), n


def per_ctx(bits, ks, nctx=52):
    acc = {}
    b8 = bits
    for c in range(nctx):
        m = ks == c
        if m.sum():
            tot = float(b8[m].sum())
            first = float(b8[m][:64].mean())
            acc[c] = {"n": int(m.sum()), "B": round(tot / 8, 1),
                      "first64_B": round(first / 8, 2)}
    return acc


def main():
    only_hubble = "--hubble" in sys.argv
    rng = np.random.default_rng(16)
    if only_hubble:
        files = [("hubble", Path("/tmp/hf1_hubble_dec.png"))]
    else:
        files = [(png, ROOT / png) for png in s1.corpus()]
    out_rows = []
    for name, path in files:
        be, flags, blob, _ = extract_stream(path)
        if be not in (4, 5, 6) or (flags & 0x8):
            out_rows.append({"file": name, "skip": f"be{be}/token"})
            print(json.dumps(out_rows[-1]), flush=True)
            continue
        w, h, _t, tile_count = struct.unpack_from("<IIII", blob, 4)
        plen = struct.unpack_from("<I", blob, 20)[0]
        payload = bytes(blob[32:32 + plen])
        mode = {4: s2.B4, 5: s2.B5, 6: s2.B6}[be]
        t0 = time.perf_counter()
        stream = bridge(mode, payload, "p")
        _walk_check(stream, w, h)
        n = len(stream)
        fb = g8_feature_bytes(stream, flags, w, h, tile_count, n)
        t_feat = time.perf_counter() - t0
        ks0 = s2.prod_keys(stream, be)
        s3_0 = audit_bits(mode, stream)
        s3_1 = audit_bits(B6G8, stream, fb)
        shuf = fb[rng.permutation(n)]
        s3_2 = audit_bits(B6G8, stream, shuf)
        wire0, _n0 = wire_len(mode, stream)
        wire1, _n1 = wire_len(B6G8, stream, fb)
        ks1 = keys_g8(stream, be, fb)
        row = {"file": name, "be": be, "n_syms": n,
               "L0_B": round(float(s3_0.sum()) / 8, 1),
               "L1_B": round(float(s3_1.sum()) / 8, 1),
               "L2_B": round(float(s3_2.sum()) / 8, 1),
               "gain_preq_B": round(float(s3_0.sum() - s3_1.sum()) / 8, 1),
               "shuf_preq_B": round(float(s3_0.sum() - s3_2.sum()) / 8, 1),
               "entropy_wire0_B": wire0, "entropy_wire1_B": wire1,
               "feat_ms": round(t_feat * 1000, 1),
               "ctx52_pop": {str(c): int((ks1 == c).sum())
                             for c in range(52) if (ks1 == c).sum()}
               }
        # adaptation detail: per-child populations and cold-start cost
        row["children"] = {k: v for k, v in per_ctx(s3_1, ks1).items()
                           if int(k) >= 20}
        out_rows.append(row)
        print(json.dumps(row), flush=True)

    if only_hubble:
        with open("/tmp/mnf16_s3_hubble.json", "w") as f:
            json.dump(out_rows, f, indent=1)
    tot0 = sum(float(r.get("L0_B", 0)) for r in out_rows)
    tot1 = sum(float(r.get("L1_B", 0)) for r in out_rows)
    tot2 = sum(float(r.get("L2_B", 0)) for r in out_rows)
    print(f"\n== AGG == S3-0 {tot0:,.1f} B | S3-1 {tot1:,.1f} B "
          f"(gain {tot0 - tot1:,.1f} B) | S3-2 {tot2:,.1f} B "
          f"(gain {tot0 - tot2:,.1f} B)", flush=True)


if __name__ == "__main__":
    main()
