#!/usr/bin/env python3
"""MNF16-A1 - acquisition-signature screening (R0-R2) on the frozen traces.

Frozen feature definitions (R0: target-independent BY CONSTRUCTION -
operands are the decoder-known (y, x) of each residual byte via the
record walk; never any image value; framing bytes = fallback):

  A1  2x2 coordinate phase: bucket = (x % 2) * 2 + (y % 2), ids 0..3
      (sensor / demosaicing signatures)
  A2  8x8 coordinate phase: bucket = (x % 8) * 8 + (y % 8), ids 0..63
      (JPEG block signatures)
  A3  block-boundary membership: 1 if (x % 8 == 0) or (y % 8 == 0)
      else 0 (processing discontinuities at block edges), ids 0..1
  A4  negative control: WITHIN-FILE permutation of the A2 array
      (phase counts preserved exactly; seed 17)
  A0  baseline = the shipped production context (trace keys)

R2 measurement (frozen X1 conventions): marginal I(S;A) over residual
bytes; prequential conditional estimate L_base vs L_base+A with the
frozen fallback rule (framing/fallback bytes keep the production model);
shuffled control. Wire-level (R3+) NOT measured here.

Synthetic micro-stream controls run on the AUDIT PATH explicitly
(the record stream is constructed directly; the production picker is
not involved) - recorded separately from natural production selection:
  positive: residuals engineered periodic in x%8 (JPEG-like grid)
  negative: iid uniform residuals (white noise)
Both build one interleaved mode-0 16x16 record (the audit codes any
well-formed stream; recon semantics are not consulted by the
estimator).
"""

import json
import struct
import sys
import time
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

import mnf16_s2_bridge as s2
import mnf16_t1_contract as tc
import mnf16_t1_trace as tt
import mnf16_x1 as x1

SHUF_SEED = 17


def phase_features(tr):
    """A1/A2/A3 arrays on the stream grid from the trace's residual (y, x)."""
    N = len(tr["stream"])
    pos3 = tr["pos"].astype(np.int64)          # residual-indexed (fixed)
    y, xs = pos3[:, 0], pos3[:, 1]
    out = {}
    a1 = np.zeros(N, np.int64); a1[tr["idx"]] = (xs % 2) * 2 + (y % 2)
    a2 = np.zeros(N, np.int64); a2[tr["idx"]] = (xs % 8) * 8 + (y % 8)
    a3 = np.zeros(N, np.int64); a3[tr["idx"]] = ((xs % 8 == 0) | (y % 8 == 0)).astype(np.int64)
    return {"A1": a1, "A2": a2, "A3": a3}


def screen(tr, feats):
    """R2: marginal + prequential conditional on one trace (X1 conventions)."""
    sym = np.frombuffer(tr["stream"].tobytes(), np.uint8).astype(np.int64)
    ctx = tr["keys"].astype(np.int64)
    nctx = int(ctx.max()) + 1
    ad = np.zeros(len(sym), bool)
    ad[tr["idx"]] = True            # features exist on residual bytes only
    out = {}
    for name, fa in feats.items():
        f, s = fa[tr["idx"]], sym[tr["idx"]]
        nfa = int(fa.max()) + 1 if len(fa) else 1
        H = np.zeros((nfa, 256))
        np.add.at(H, (f, s), 1)
        ps = H.sum()
        pj = H / ps if ps else None
        if ps:
            psym = H.sum(axis=1) / ps
            pf = H.sum(axis=0) / ps
            mi = sum(pj[fi, si] * np.log2(pj[fi, si] / (psym[fi] * pf[si]))
                     for fi in range(nfa) for si in range(256) if H[fi, si])
        else:
            mi = 0.0
        cb, ca = x1._x1_cost(sym, ctx, fa, nfa, ad, nctx)
        fs = fa.copy()
        fs[:] = fs[np.random.default_rng(SHUF_SEED).permutation(len(fs))]
        cbs, cas = x1._x1_cost(sym, ctx, fs, nfa, ad, nctx)
        out[name] = {"marginal_bits": round(float(mi * ad.sum()), 1),
                     "gain_B": round(cb - ca, 1),
                     "shuffle_gain_B": round(cbs - cas, 1),
                     "nfa": nfa}
    return out


def micro_stream(resid, planar_layout=False):
    """One interleaved mode-0 16x16 record: mode byte + 768 residuals."""
    assert len(resid) == 768
    return bytes([0]) + resid.tobytes()


def micro_controls():
    """Synthetic audit-path controls (separate from production picks)."""
    from mnf13_r9a_gate import bridge  # noqa: F401 - ident only via BRIDGE
    rng = np.random.default_rng(11)
    yy, xx = np.mgrid[0:16, 0:16]
    yy = yy.ravel(); xx = xx.ravel()
    # interleaved layout: for pixel p the 3 lanes occupy 3p..3p+2; the A2
    # phase of lane bytes at pixel p (all lanes share (y,x)) = xx*8+yy
    phase = np.repeat((xx % 8) * 8 + (yy % 8), 3)
    pos_ = (phase * 29) % 256          # strongly phase-dependent residuals
    pos_ = pos_.astype(np.uint8)
    white = rng.integers(0, 256, 768, dtype=np.uint8)
    keys = s2.prod_keys(micro_stream(pos_), 6)
    nctx = int(keys.max()) + 1
    sym = np.frombuffer(micro_stream(pos_), np.uint8).astype(np.int64)
    # features: A2 on this micro stream (all bytes are residuals here)
    a2 = np.tile(phase.astype(np.int64), 1)
    fa2 = np.zeros(len(sym), np.int64)
    fa2[0] = 0                          # mode byte: structural, no phase
    fa2[1:] = a2                        # 768 residual bytes
    tr = {"stream": np.frombuffer(micro_stream(pos_), np.uint8),
          "keys": keys.astype(np.int16),
          "idx": np.arange(len(sym), dtype=np.int64),
          "meta": {"file": "micro-positive", "n": len(sym)}}
    ad = np.ones(len(sym), bool)
    cb, ca = x1._x1_cost(sym, keys.astype(np.int64), fa2, 64, ad, nctx)
    fs = fa2.copy(); fs[:] = fs[np.random.default_rng(SHUF_SEED).permutation(len(fs))]
    cbs, cas = x1._x1_cost(sym, keys.astype(np.int64), fs, 64, ad, nctx)
    tr_n = dict(tr, stream=np.frombuffer(micro_stream(white), np.uint8))
    sym_n = tr_n["stream"].astype(np.int64)
    keys_n = s2.prod_keys(micro_stream(white), 6)
    cbn, can = x1._x1_cost(sym_n, keys_n.astype(np.int64), fa2, 64, ad,
                           int(keys_n.max()) + 1)
    return {"micro_positive_gain_B": round(cb - ca, 1),
            "micro_positive_shuffle_B": round(cbs - cas, 1),
            "micro_white_gain_with_A2_B": round(cbn - can, 1)}


def main():
    paths = sys.argv[1:] or json.loads(
        (ROOT / "research" / "manifests" / "screening.lock").read_text())["files"]
    rows = []
    for rel in paths:
        tr = tc.load_trace(rel)
        feats = phase_features(tr)
        shuf = feats["A2"].copy()
        shuf[:] = shuf[np.random.default_rng(SHUF_SEED).permutation(len(shuf))]
        feats["A4"] = shuf
        rep = screen(tr, {"A1": feats["A1"], "A2": feats["A2"],
                          "A3": feats["A3"], "A4": shuf})
        rows.append({"file": rel, **rep})
        print(json.dumps(rows[-1]), flush=True)
    agg = {}
    for arm in ("A1", "A2", "A3", "A4"):
        agg[arm] = {k: round(sum(r[arm][k] for r in rows), 1)
                    for k in ("marginal_bits", "gain_B", "shuffle_gain_B")}
        print(f"{arm}: marginal {agg[arm]['marginal_bits']:,.0f} bits  "
              f"gain {agg[arm]['gain_B']:+,.1f} B  "
              f"shuffle {agg[arm]['shuffle_gain_B']:+,.1f} B")
    ctrl = micro_controls()
    print(json.dumps(ctrl, indent=1))
    out = ROOT / "research" / "results" / "a1_screening.json"
    out.write_text(json.dumps({"files": rows, "agg": agg, "controls": ctrl},
                              indent=1))
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
