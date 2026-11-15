#!/usr/bin/env python3
"""MNF16-S2 — production conditional attribution through the R9 bridge.

Stage 2 of the S1 program (per Jon's contract): measure
I(R; G8 | C6), I(R; D | C6), and their joint, C6 = the SHIPPED
backend-6 production context schedule (16 base keys + heavy-band
prev3 refinement, `refineA-warm2p12-share`, 20 contexts — the exact
key rule from crates/mnf-mans/examples/r9_audit.rs key_of, parity
asserted in docs/mnf13-r9e-record.md).

Reuses the R9 kit without reopening its validation:
  - mnf13_r9a_gate.extract_stream / bridge / bucket / static floor
  - the shipped backend-6 mode string (C0 + refineA + warm2p12 + share)
  - wire-decode anchor path: Maximum law-mixed payloads decode via the
    bridge's 'p' mode (the shipped decoder's own symbol stream,
    non-circular: symbols + constants from the wire + shipped config)
  - stage-1 feature machinery (mnf16_s1a_census.features) with stage-1's
    frozen bucket edges

Arms (static conditional-information layer; coder-level prequential
arms are the r9_audit-extension rung, reported separately):
  S2-1 G8 (primary), S2-2 disagreement, S2-3 joint, S2-4 shuffled
  negative control, S2-5 heavy-context-only split (fragmentation
  comparison).

Decision gates (Jon):
  S2-0  production identity: symbol order, contexts, probability costs.
  decoder causality: every feature input precedes its target symbol in
        the CODED order (same-plane earlier positions, by construction).
  incremental information: prequential-positive + controls + holdout.
"""
import json
import struct
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "crates/mnf-mans/scripts"))
sys.path.insert(0, str(ROOT / "scripts"))
from mnf13_r9a_gate import (  # noqa: E402
    B4, B5, BRIDGE, bucket, bridge, extract_stream, static_floor_bits,
    _walk_check)
import mnf16_s1a_census as s1  # stage-1 features + corpus list (frozen edges)

TS = 16
C0 = "mix32768_1048576-lr15-inc32-ctx16"
B6 = C0 + "-refineA-warm2p12-share"    # shipped backend-6 (r9e record)
NCTX = 20
np.random.seed(16)


def prod_keys(stream, backend):
    """Shipped key schedule, coded order: backend-6 = key_of under
    Refine::Both (share affects table updates, not keys); 5 = 16-key
    base rule; 4 = 8-key rule. Matches r9_audit's key_of."""
    bs = np.frombuffer(stream, np.uint8).astype(np.intp)
    ks = np.empty(len(bs), np.intp)
    prev = p2 = p3 = 0
    if backend == 4:
        for i, b in enumerate(bs):
            ks[i] = bucket(prev)
            p2, prev = prev, b
        return ks
    for i, b in enumerate(bs):
        pb = bucket(prev)
        band = (bucket(p2) >= 3)
        base = pb | band << 3
        if backend == 6 and pb == 7:
            band3 = (bucket(p3) >= 3)
            ks[i] = (16 + band3) if band == 0 else (18 + band3)
        else:
            ks[i] = base
        p3, p2, prev = p2, prev, b
    return ks


def annotate(stream, flags, w, h, tile_count):
    """Wire-decoded stream walk with (y, x, channel) annotation of every
    mode-0-tile symbol, plus its FULL-STREAM index (for exact production
    contexts). Law tiles (6/7) walked but not censused (S1 population
    rule)."""
    cols = (w + TS - 1) // TS
    planar = bool(flags & 1)
    pos, wire, idx = [], [], []
    off = 0
    n0 = 0
    for t in range(tile_count):
        gy, gx = divmod(t, cols)
        tw, th = min(TS, w - gx * TS), min(TS, h - gy * TS)
        mode = stream[off]
        off += 1
        if mode in (6, 7):                       # law: [lb][same-layout nb]
            off += (12 if planar else 9) + tw * th * 3
            continue
        assert mode == 0, (t, mode)
        body = np.frombuffer(stream, np.uint8, tw * th * 3, off)
        idx_base = off
        off += body.size
        n0 += 1
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
            np.asarray(idx, np.intp), n0)


def ctx_cost(sym, ctx, nctx, prequential):
    """Per-symbol conditional cost of R under context ids ctx (mb8 x8):
    static = same-population entropy; prequential = models built on the
    first 80% of symbols, cross-entropy on the final 20%."""
    sym = sym.astype(np.int64)
    ctx = np.asarray(ctx, np.intp)
    a, b = 0, len(sym)
    if prequential:
        split = int(len(sym) * 0.8)
        a = split
        J = np.zeros((nctx, 256))
        np.add.at(J, (ctx[:split], sym[:split]), 1)
        tot = 0.0
        n = 0
        for c in range(nctx):
            occ = (ctx[a:] == c).sum()
            if occ and J[c].sum():
                Jc = J[c] / J[c].sum()
                tot += -(Jc * np.log2(np.maximum(Jc, 1e-12))).sum() * occ
                n += occ
        return tot / max(n, 1) * 8
    J = np.zeros((nctx, 256))
    np.add.at(J, (ctx, sym), 1)
    tot = 0.0
    for c in range(nctx):
        if J[c].sum():
            Jc = J[c] / J[c].sum()
            tot += -(Jc * np.log2(np.maximum(Jc, 1e-12))).sum() * J[c].sum()
    return tot / len(sym) * 8


def joint_eval(sym, ctx, feat, nctx, feat_max):
    """Stage-1 estimator with production contexts: in-sample and
    prequential I(R; F | C) in millibits x8."""
    sym = sym.astype(np.int64)
    cb = np.asarray(ctx, np.intp)
    fb = feat
    s0 = sym >> 4
    del s0
    split = int(len(sym) * 0.8)
    J = np.zeros((nctx, feat_max + 1, 256))
    np.add.at(J, (cb[:split], fb[:split], sym[:split]), 1)
    Je = np.zeros((nctx, feat_max + 1, 256))
    np.add.at(Je, (cb[split:], fb[split:], sym[split:]), 1)
    CR = J.sum(axis=1)
    base_i = sum(CR[c].sum() * s1.xent(CR[c] / CR[c].sum(), CR[c])
                 for c in range(nctx) if CR[c].sum())
    adv_i = sum(J[c, f].sum() * s1.xent(J[c, f] / J[c, f].sum(), J[c, f])
                for c in range(nctx) for f in range(feat_max + 1) if J[c, f].sum())
    ev_base = Je[:, :, :].sum(axis=1)
    base = sum(ev_base[c].sum() * s1.xent(CR[c] / CR[c].sum(), ev_base[c])
               for c in range(nctx) if CR[c].sum() and ev_base[c].sum())
    n_ev = ev_base.sum()
    adv = sum(Je[c, f].sum() * s1.xent(J[c, f] / J[c, f].sum(), Je[c, f])
              for c in range(nctx) for f in range(feat_max + 1)
              if J[c, f].sum() and Je[c, f].sum())
    ins = (base_i - adv_i) / len(sym) * 8
    pre = (base - adv) / max(n_ev, 1) * 8
    return ins, pre


def main():
    rows = []
    for png in s1.corpus():
        rel = str(png)
        backend, flags, blob, _recompute = extract_stream(ROOT / png)
        if blob[:4] != b"MNF8" or (flags & 0x8):
            rows.append({"file": rel, "skip": "token/magic"})
            print(json.dumps({k: v for k, v in rows[-1].items()}), flush=True)
            continue
        w, h, _tile, tile_count = struct.unpack_from("<IIII", blob, 4)
        plen = struct.unpack_from("<I", blob, 20)[0]
        payload = bytes(blob[32:32 + plen])
        if backend == 4:
            mode, nctx = B4, 8
        elif backend == 5:
            mode, nctx = B5, 16
        elif backend == 6:
            mode, nctx = B6, NCTX
        else:
            rows.append({"file": rel, "skip": f"backend{backend}"})
            print(json.dumps({k: v for k, v in rows[-1].items()}), flush=True)
            continue
        # S2-0 anchor: wire-decode (the kit's fallback path; the recompute
        # direction cannot reproduce law-mode picks — ADR-010b)
        stream = bridge(mode, payload, "p")
        _walk_check(stream, w, h)
        audit = bridge(mode, stream, "a")
        n = struct.unpack_from("<I", audit, 0)[0]
        assert n == len(stream), (rel, n, len(stream))
        bits = np.frombuffer(audit[4:4 + 8 * n], np.float64)
        ks = prod_keys(stream, backend)
        L_model = float(bits.sum())
        L_static = float(static_floor_bits(np.frombuffer(stream, np.uint8),
                                           ks, nctx))
        row = {"file": rel, "anchor": "wire-decode", "n_syms": n,
               "be": backend,
               "L_model_B": round(L_model / 8, 1),
               "L_static_B": round(L_static / 8, 1),
               "adapt_regret_B": round((L_model - L_static) / 8, 1),
               "L_entropy_wire_B": plen - 4,
               "coder_overhead_B": round(plen - 4 - L_model / 8, 1),
               "ctx_pop": {str(c): int((ks == c).sum()) for c in range(nctx)
                          if int((ks == c).sum())}}
        # feature annotations for the mode-0-tile population
        wire, pos, idx, n0 = annotate(stream, flags, w, h, tile_count)
        row["mode0_tiles"] = n0
        row["mode0_syms"] = int(len(wire))
        if len(wire) >= 1000:
            sub_ctx = ks[idx] + (NCTX if backend == 6 and False else 0)  # EXACT production keys
            D, g8 = s1.features(wire, pos, (h, w))
            gd = s1.bucket(g8, np.array([4, 16, 32, 64, 96, 128, 160]))
            dd = s1.bucket(D, np.array([0, 1, 2, 4, 8, 16, 32, 64]))
            arms = {
                "S2-1_G8": joint_eval(wire, sub_ctx, gd, nctx, 7),
                "S2-2_D": joint_eval(wire, sub_ctx, dd, nctx, 8),
                "S2-3_joint": joint_eval(wire, sub_ctx, gd * 8 + dd, nctx, 71),
                "S2-4_shuffled": joint_eval(wire, sub_ctx,
                                            gd[np.random.permutation(len(gd))], nctx, 7),
            }
            # S2-5: fragment ONLY the heavy band by G8 (r9d child style)
            # child context space: 16..19 × G8 buckets; parent = else base
            if nctx == NCTX and backend == 6:
                c5 = np.where(sub_ctx >= 16, sub_ctx * 8 + gd, sub_ctx)
                arms["S2-5_heavy_split"] = (
                    ctx_cost(wire, sub_ctx, NCTX, False) - ctx_cost(wire, c5, 180, False),
                    ctx_cost(wire, sub_ctx, NCTX, True) - ctx_cost(wire, c5, 180, True))
            for name, (ins, pre) in arms.items():
                row[name] = {"in_sample": round(ins, 3),
                             "prequential": round(pre, 3)}
        rows.append(row)
        print(json.dumps({k: v for k, v in row.items()}), flush=True)
    agg = {}
    for r in rows:
        for name in ("S2-1_G8", "S2-2_D", "S2-3_joint", "S2-4_shuffled",
                     "S2-5_heavy_split"):
            if name in r and isinstance(r[name], dict):
                a = agg.setdefault(name, [0, 0, 0])
                a[0] += (r[name]["in_sample"] / 8) * r["mode0_syms"] / 8
                a[1] += (r[name]["prequential"] / 8) * r["mode0_syms"] / 8
                a[2] += r["mode0_syms"]
    print("\n== AGG (bytes; per-symbol mb8) ==", flush=True)
    for name, (ins_b, pre_b, n) in sorted(agg.items()):
        if n:
            print(f"{name}: in-sample {ins_b:,.0f} B ({ins_b / n * 8:.3f}) "
                  f"prequential {pre_b:,.0f} B ({pre_b / n * 8:.3f}) N={n:,}",
                  flush=True)


if __name__ == "__main__":
    main()
