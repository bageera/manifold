#!/usr/bin/env python3
"""MNF16-X1 - conditional cross-channel information discovery.

Authorized scope (X1-0 + T0-T5 for X1-A/B): availability census +
prefix-invariance validation + conditional-information measurement of
the same-pixel earlier-lane residual features. NO wire work, NO new
backend, NO parameter search: the aggregate rules and edge lists below
are frozen by this docstring, not tuned.

Feature family (per symbol at position (y, x, lane)):
  operands: the residual bytes of lanes 0..lane-1 AT THE SAME PIXEL
  (same (y, x)) only - all strictly earlier in the stream in BOTH
  layouts (interleaved: per-pixel lane-ascending triples; planar:
  per-tile plane-major). Lane 0 has no same-pixel earlier lane ->
  fallback symbol (unchanged production model). Framing bytes (record
  mode + meta) are never residual operands and stay fallback.
  X1-A magnitude: m = |signed residual| where signed = v if v < 128
  else v - 256; aggregate over available earlier lanes = the value of
  the MAXIMUM-magnitude lane (tie -> lowest lane); frozen buckets
  [0], [1..3], [4..15], [16..63], [>=64] (ids 0..4).
  X1-B sign: 0 = zero, 1 = positive (v in 1..127), 2 = negative
  (v >= 128); aggregate = the max-magnitude lane's sign; ids 0..2.
  X3 joint (byproduct of the same operands, reported not tuned):
  A * 3 + B, ids 0..14.
  Frozen wire rule used by every estimate: fallback symbols keep the
  production base model (no conditioning), so the availability
  partition itself is part of the X1 model; the X4b control (permute
  feature values among admissible positions) isolates CONTENT at fixed
  availability, and the avail_split arm measures the partition alone.

T0 dependency audit (literal): operands = {wire[y, x, lane'] for
lane' < lane at the same (y, x)}. Nothing else: no current byte, no
future bytes, no raster, no cross-pixel reads, no lane >= lane.
Enforced empirically by T1 (exhaustive 256-value target substitution),
T2 (suffix + window mutation), T3 (the reference extractor runs on a
PrefixView that cannot serve lane' >= lane or out-of-prefix data),
T4 (reference == fast path exactly).

T5 estimator conventions = the committed S2 conventions: per-byte
stream grid (framing bytes included, complete accounting), production
contexts via prod_keys (backend 4=8-key, 5=16-key, 6=20-key incl.
refine); 80/20 split; static population = first 80%, evaluation = last
20%; shuffled control seed 17. Levels reported: I(S;A) marginal
(admissible-only, bits x admissible), L_base / L_adv eval-split bytes,
the prequential gain L_base - L_adv (X1 arm), and the controls X4 /
X4b / avail_split. L_wire is explicitly NOT measured at this stage.
"""

import json
import struct
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "crates/mnf-mans/scripts"))
sys.path.insert(0, str(ROOT / "scripts"))

import mnf16_s2_bridge as s2
from mnf13_r9a_gate import bridge, extract_stream

TS = 16
A_BUCKETS = np.array([1, 4, 16, 64])          # frozen: ids 0..4
SHUF_SEED = 17                                 # frozen control seed


def walk_modes(stream, flags, w, h, tile_count):
    """Position map + per-symbol tile record mode (s3's walk + modes)."""
    cols = (w + TS - 1) // TS
    planar = bool(flags & 1)
    pos, wire, idx, modes = [], [], [], []
    off = 0
    for t in range(tile_count):
        gy, gx = divmod(t, cols)
        tw, th = min(TS, w - gx * TS), min(TS, h - gy * TS)
        mode = stream[off]
        off += 1
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
                        modes.append(mode)
        else:
            wb = body.reshape(th, tw, 3)
            for ly in range(th):
                for lx in range(tw):
                    for k in range(3):
                        pos.append((gy * TS + ly, gx * TS + lx, k))
                        wire.append(wb[ly, lx, k])
                        idx.append(idx_base + (ly * tw + lx) * 3 + k)
                        modes.append(mode)
    assert off == len(stream), (off, len(stream))
    return (np.asarray(wire, np.int64), np.asarray(pos, np.int64),
            np.asarray(idx, np.intp), np.asarray(modes, np.int64))


class PrefixView:
    """T3 interface: prefix-only lane state, keyed (y, x, lane).

    `fref` is the only reader. It receives no wire array, no raster,
    no symbol values, and `earlier()` refuses any lane >= cur.
    """

    def __init__(self):
        self.buf = {}

    def expose(self, y, x, lane, byte):
        assert (y, x, lane) not in self.buf, "position exposed twice"
        self.buf[(y, x, lane)] = int(byte)

    def earlier(self, y, x, cur):
        return [self.buf[(y, x, k)]
                for k in range(cur) if (y, x, k) in self.buf]


def _smag(v):
    return v if v < 128 else 256 - v


def feature_from_earlier(vals):
    """(A_bucket, B_class, J_id) from the earlier-lane bytes, or
    (None, None, None) for fallback symbols (no earlier lane)."""
    if not vals:
        return None, None, None
    mags = [_smag(v) for v in vals]
    j = int(np.argmax(mags))            # tie -> lowest lane
    m, v = mags[j], vals[j]
    a = int(np.searchsorted(A_BUCKETS, m, side="right"))
    b = 0 if v == 0 else (1 if v < 128 else 2)
    return a, b, a * 3 + b


def fref(view, y, x, cur):
    """The reference extractor under the T3 interface."""
    return feature_from_earlier(view.earlier(y, x, cur))


def features_fast(wire, pos, idx, n_stream):
    """Vectorized same-operand fast path (T4-paired with the view).
    Returns stream-grid arrays; framing bytes stay fallback (id 0)."""
    h, w = pos[:, 0].max() + 1, pos[:, 1].max() + 1
    lut = np.full((h, w, 3), -1, np.int64)
    lut[pos[:, 0], pos[:, 1], pos[:, 2]] = np.arange(len(wire))
    y, x, k = pos[:, 0], pos[:, 1], pos[:, 2]
    n = len(wire)
    best_m = np.zeros(n, np.int64)
    best_a = np.zeros(n, np.int64)
    best_b = np.zeros(n, np.int64)
    best_j = np.zeros(n, np.int64)
    have = np.zeros(n, bool)
    for lane in (0, 1):
        li = lut[y, x, lane]
        ok = np.where((k > lane) & (li >= 0))[0]
        if not len(ok):
            continue
        v = wire[li[ok]]
        m = np.where(v < 128, v, 256 - v)
        a = np.searchsorted(A_BUCKETS, m, side="right")
        b = np.where(v == 0, 0, np.where(v < 128, 1, 2))
        upd = ~have[ok] | (m > best_m[ok])
        oi = ok[upd]
        best_m[oi] = m[upd]
        best_a[oi] = a[upd]
        best_b[oi] = b[upd]
        best_j[oi] = a[upd] * 3 + b[upd]
        have[ok] = True
    As = np.zeros(n_stream, np.int64)
    Bs = np.zeros(n_stream, np.int64)
    Js = np.zeros(n_stream, np.int64)
    As[idx] = best_a
    Bs[idx] = best_b
    Js[idx] = best_j
    have_s = np.zeros(n_stream, bool)
    have_s[idx] = have
    return As, Bs, Js, have_s


def _x1_cost(sym, ctx, feat, fids, avail, nctx):
    """Wire-honest estimate with the frozen fallback rule: fallback
    symbols keep the production base model (trained on everything
    pre-split); admissible eval symbols use per-(ctx, feat>0) models
    trained on admissible pre-split symbols (cold slots inherit base).
    Returns (base_eval_bytes, adv_eval_bytes) over the final 20%."""
    n = len(sym)
    split = int(n * 0.8)
    ev = np.arange(split, n)
    JB = np.zeros((nctx, 256))
    np.add.at(JB, (ctx[:split], sym[:split]), 1)
    base_bits = np.zeros(n)
    for c in range(nctx):
        occ = ctx == c
        if not occ.any() or JB[c].sum() == 0:
            continue
        p = JB[c] / JB[c].sum()
        bit = -np.log2(np.maximum(p, 1e-12))
        base_bits[occ] = bit[sym[occ]]
    JBsum = JB.sum(axis=1)
    JA = np.zeros((nctx, fids, 256))
    tr = (np.arange(n) < split) & avail
    np.add.at(JA, (ctx[tr], feat[tr], sym[tr]), 1)
    adv_bits = base_bits.copy()
    ii = np.arange(n)
    for c in range(nctx):
        if JBsum[c] == 0:
            continue
        for f in range(1, fids):
            ev_m = (ii >= split) & (ctx == c) & (feat == f) & avail
            if not ev_m.any():
                continue
            tot = JA[c, f].sum()
            if tot == 0:
                continue          # frozen rule: cold slot -> base model
            p = JA[c, f] / tot
            bit = -np.log2(np.maximum(p, 1e-12))
            adv_bits[ev_m] = bit[sym[ev_m]]
    return float(base_bits[ev].sum() / 8), float(adv_bits[ev].sum() / 8)


def run(png_rel):
    backend, flags, blob, _ = extract_stream(ROOT / png_rel)
    if blob[:4] != b"MNF8" or (flags & 0x8) or backend not in (4, 5, 6):
        return {"file": png_rel, "skip": f"be{backend}/token"}
    w, h, _ts, tc = struct.unpack_from("<IIII", blob, 4)
    plen = struct.unpack_from("<I", blob, 20)[0]
    payload = bytes(blob[32:32 + plen])
    mode_tbl = {4: s2.B4, 5: s2.B5, 6: s2.B6}
    nctx = {4: 8, 5: 16, 6: 20}[backend]
    stream = bridge(mode_tbl[backend], payload, "p")
    # the bridge stamps len(input) as the header n (recorded gotcha):
    # the audit input IS the decoded stream; words are per stream byte
    out = bridge(mode_tbl[backend], stream, "a")
    nn = int.from_bytes(out[:4], "little")
    bits = np.frombuffer(out[4:4 + 8 * nn], np.float64)
    wire, pos, idx, modes = walk_modes(stream, flags, w, h, tc)
    N = len(stream)
    assert len(wire) == len(idx) and len(bits) == N, (len(wire), len(bits), N)
    sym = np.frombuffer(stream, np.uint8).astype(np.int64)
    ctx = s2.prod_keys(stream, backend)
    idx_row = {int(v): r for r, v in enumerate(idx.tolist())}
    cand_idx = sorted(idx_row)

    # ---- T0-T4: reference path (T3 interface) on the stream grid --
    view = PrefixView()
    A_ref = np.zeros(N, np.int64)
    B_ref = np.zeros(N, np.int64)
    J_ref = np.zeros(N, np.int64)
    have_ref = np.zeros(N, bool)
    rng = np.random.default_rng(101)     # frozen gate seed
    for i in range(N):
        if i in idx_row:
            r = idx_row[i]
            y, x, k = int(pos[r, 0]), int(pos[r, 1]), int(pos[r, 2])
            a = fref(view, y, x, k)
            have_ref[i] = a[0] is not None
            if have_ref[i]:
                A_ref[i], B_ref[i], J_ref[i] = a
            view.expose(y, x, k, int(stream[i]))
        # framing bytes: not residual operands -> nothing to expose

    # ---- T1: exhaustive 256-value target substitution -------------
    sub = rng.choice(cand_idx, size=min(1024, len(cand_idx)), replace=False)
    t1_ok = 0
    for i in sub:
        r = idx_row[int(i)]
        y, x, k = int(pos[r, 0]), int(pos[r, 1]), int(pos[r, 2])
        before = fref(view, y, x, k)
        prev = view.buf.get((y, x, k))
        for tval in range(256):
            # mutate ONLY the target's own slot; earlier() must not read it
            if prev is not None:
                view.buf[(y, x, k)] = tval
            after = fref(view, y, x, k)
            t1_ok += int(before == after)
        if prev is not None:
            view.buf[(y, x, k)] = prev

    # ---- T2: suffix + window mutations (fast path) ----------------
    A_f, B_f, J_f, have_f = features_fast(wire, pos, idx, N)
    for i0 in (N // 2, 3 * N // 4):
        smut = bytearray(stream)
        for ipos in cand_idx:
            if ipos >= i0:
                smut[ipos] = 0xAA
        wmut = np.frombuffer(bytes(smut), np.uint8)[idx].astype(np.int64)
        A2, B2, J2, _ = features_fast(wmut, pos, idx, N)
        assert (A2[:i0] == A_f[:i0]).all() and (J2[:i0] == J_f[:i0]).all()
    j0 = N // 3
    shuf = np.random.default_rng(SHUF_SEED)
    smut = bytearray(stream)
    tail = [ip for ip in cand_idx if ip >= j0]
    vals = [smut[ip] for ip in tail]
    perm = shuf.permutation(len(tail))
    for ip, pv in zip(tail, [vals[q] for q in perm]):
        smut[ip] = pv
    wsh = np.frombuffer(bytes(smut), np.uint8)[idx].astype(np.int64)
    A3, _, J3, _ = features_fast(wsh, pos, idx, N)
    head = idx[np.asarray(cand_idx) < j0]
    assert (A3[head] == A_f[head]).all() and (J3[head] == J_f[head]).all()

    # ---- T4: parity fast == reference -----------------------------
    t4 = bool((A_f == A_ref).all() and (B_f == B_ref).all()
              and (J_f == J_ref).all() and (have_f == have_ref).all())
    ad = have_f

    # ---- X1-0: availability census --------------------------------
    l6 = float(bits.sum() / 8.0)
    planar = bool(flags & 1)
    ref = (flags >> 6) & 3
    by_mode = {}
    ad_res = ad[idx]
    for m in np.unique(modes):
        mm = modes == m
        by_mode[int(m)] = [int((ad_res & mm).sum()), int(mm.sum())]
    by_ctx = {}
    for c in range(nctx):
        cc = ctx == c
        by_ctx[int(c)] = [int((ad & cc).sum()), int(cc.sum())]

    # ---- T5: conditional information ------------------------------
    rng2 = np.random.default_rng(SHUF_SEED)
    feats = {"A": A_f, "B": B_f, "J": J_f}
    fids = {"A": 5, "B": 3, "J": 15}
    out5 = {}
    for name, fa in feats.items():
        f, s = fa[idx][ad_res], sym[idx][ad_res]
        nfa = fids[name]
        H = np.zeros((nfa, 256))
        np.add.at(H, (f, s), 1)
        ps = H.sum()
        if ps:
            psym = H.sum(axis=1) / ps
            pj = H / ps
            pf = H.sum(axis=0) / ps
            mi = sum(pj[fi, si] * np.log2(pj[fi, si] / (psym[fi] * pf[si]))
                     for fi in range(nfa) for si in range(256) if H[fi, si])
        else:
            mi = 0.0
        cb, ca = _x1_cost(sym, ctx, fa, nfa, ad, nctx)
        fs = fa.copy()
        fs[:] = fs[rng2.permutation(len(fs))]          # X4: global
        cbs, cas = _x1_cost(sym, ctx, fs, nfa, ad, nctx)
        f2 = fa.copy()
        ai = np.where(ad)[0]
        f2[ai] = f2[ai][rng2.permutation(len(ai))]     # X4b: within-adm
        cb2, ca2 = _x1_cost(sym, ctx, f2, nfa, ad, nctx)
        cbv, cav = _x1_cost(sym, ctx, ad.astype(np.int64), 2, ad, nctx)
        out5[name] = {
            "marginal_bits": round(float(mi * ad.sum()), 1),
            "L_base_ev_B": round(cb, 1), "L_adv_ev_B": round(ca, 3),
            "gain_B": round(cb - ca, 1),
            "X4_gain_B": round(cbs - cas, 1),
            "X4b_gain_B": round(cb2 - ca2, 1),
            "avail_split_gain_B": round(cbv - cav, 1),
        }
    return {"file": png_rel, "backend": int(backend), "planar": planar,
            "ref": int(ref), "n_syms": N, "n_residuals": int(len(wire)),
            "admissible": int(ad.sum()),
            "admissible_share": round(float(ad.mean()), 4),
            "L6_B": round(l6, 1), "coverage_B": round(float(bits[ad].sum() / 8), 1),
            "t1_target_subst": int(t1_ok), "t1_total": int(len(sub) * 256),
            "t4_parity": t4, "by_mode": by_mode, "by_ctx": by_ctx,
            "t5": out5}


def main():
    paths = [a for a in sys.argv[1:] if not a.startswith("-")]
    if not paths:
        import mnf16_s1a_census as s1
        fn = s1.files if hasattr(s1, "files") else s1.corpus
        paths = [str(pp) for pp in fn()]
    rows = []
    for rel in paths:
        try:
            rows.append(run(rel))
        except Exception as e:  # a census must not die on one file
            rows.append({"file": rel, "skip": str(e)[:200]})
        print(json.dumps({k: v for k, v in rows[-1].items()
                          if k not in ("by_mode", "by_ctx")}), flush=True)
    ok = [r for r in rows if "skip" not in r]
    if ok:
        tot_syms = sum(r["n_syms"] for r in ok)
        tot_ad = sum(r["admissible"] for r in ok)
        print(f"== AGG == files {len(ok)}  stream bytes {tot_syms:,}  "
              f"admissible {tot_ad:,} ({tot_ad / max(tot_syms, 1):.1%})  "
              f"T4 all-parity {all(r['t4_parity'] for r in ok)}  "
              f"T1 {sum(r['t1_target_subst'] for r in ok)}/"
              f"{sum(r['t1_total'] for r in ok)}")
        for name in ("A", "B", "J"):
            g = sum(r["t5"][name]["gain_B"] for r in ok)
            g4 = sum(r["t5"][name]["X4_gain_B"] for r in ok)
            g4b = sum(r["t5"][name]["X4b_gain_B"] for r in ok)
            ga = sum(r["t5"][name]["avail_split_gain_B"] for r in ok)
            mi = sum(r["t5"][name]["marginal_bits"] for r in ok)
            print(f"{name}: marginal {mi:,.0f} bits  |  gain {g:+,.1f} B  |  "
                  f"X4 {g4:+,.1f} B  X4b(content) {g4b:+,.1f} B  "
                  f"avail-only {ga:+,.1f} B")
    with open("/tmp/mnf16_x1_census.json", "w") as fo:
        json.dump(rows, fo, indent=1)
    print("wrote /tmp/mnf16_x1_census.json")


if __name__ == "__main__":
    main()
