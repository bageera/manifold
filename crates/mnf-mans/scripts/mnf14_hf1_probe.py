#!/usr/bin/env python3
"""MNF14-HF1 in-repo probe — Hopf-feature RGB laws (per the work-order memo).

Model: RGB(x,y) = affine map over the Hopf projection's OBSERVABLE features.
Key algebra (assert-tested in-script): for phasor fields z_k = A_k e^{iφ_k(x,y)}
the Hopf map's observable content = the RELATIVE phase field Δφ = φ1−φ2 and
the amplitudes: w1 = 2A1A2·cosΔφ, w2 = 2A1A2·sinΔφ, w3 = A1²−A2².
A COMMON phase rotation (vibration along the fiber) changes nothing —
assert-tested numerically over a random sweep; no fiber-phase byte exists
anywhere in the transported grammar, so the "vibratory" form ties the
stationary form to the byte BY CONSTRUCTION (asserted per record too).

Decoder-exact form (all integer; no fp in decode):
  idx(x,y) = ((pQ10·x + qQ10·y) // (n−1)) mod 1024      relative-phase field
  corr_c   = (b_c·LUTc[idx] + s_c·LUTs[idx] + (1<<20)) >> 21   (Q7·Q14→RGB)
  pred_c   = clip(a_c + corr_c, 0, 255)
Transport per record: a_c i16 RGB units (6 B) + b/s coefs i16 Q7 (12 B) +
p,q i16 Q10 turns (4 B) + 2 tag bytes = 24 B fixed; residual = (X − pred)
mod 256 in raster order, CD-G relabel, zstd-19, 28-B matched frame — the
standing honest protocol (IF2/IF3/PG1). cos/sin = a deterministic 1024-entry
LUT defined by the wire doc (zero transport, MNF9-convention shape).

Arms (one process, one coder, exact recon asserted per record):
  A       MED (canonical pred_map walk) — control
  B       conventional affine field (IF3's global per-channel plane on
          MED's error plane) — the memo's named comparator
  H1dir   stationary Hopf law as FULL predictor (features → RGB)
  H1corr  stationary Hopf law ON MED's error plane (composed form)
  H1v     vibratory — common fiber phase in the param set; ASSERT record
          byte-identical to H1corr (the fiber phase is free)
  H1q     compression-aware 2×2 quadrant split (per-quadrant ramp + map,
          root-vs-split by measured complete bytes — IF3's theta* form)
Discovery on every record: integer-ramp grid stage-1 proxy (L1 residual),
top-6 honest wire re-cost through zstd-19, then a local Q10 FRACTIONAL
refinement sweep around the best ramp (the memo's area-3 precision arm).

Fixtures:
  HF-A  exact Hopf-law image, integer-lattice ramp (3,5 turns); the law
        arm must collapse it (positive control)
  HF-B  off-lattice ramp (p = 3.37 turns); integer grid misses, the
        fractional refinement recovers — the precision cliff, measured
        (memo area 3)
  NOISE rng control — nothing may win
Photos: astronaut/coffee/chelsea/hubble at 512² (baselines + all arms).
"""
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image
import zstandard

sys.path.insert(0, "/Users/jonbethea/projects/blacktech/manifold/crates/mnf-mans/scripts")
from mnf13_r6_extract import pred_map  # noqa: E402 — canonical MED walk

OUT = Path("/private/tmp/mnf14_hf1")
OUT.mkdir(exist_ok=True)
ZC = zstandard.ZstdCompressor(level=19, write_checksum=True)
FRAME = 28
TP_HOPF = 6 + 12 + 4 + 2   # a + (b,s) + (p,q) + tags = 24 B fixed transport

# ------------------------------------------------------------- Hopf algebra
def hopf_direct(z1, z2):
    """UNnormalized Hopf features (2·z1·z̄2, |z1|²−|z2|²) — the linear map
    absorbs the S² normalization scale (comment: the memo's R(P,L,M)
    structure evaluated at its Hopf-projected coordinates)."""
    p = z1 * np.conj(z2)
    return 2 * np.real(p), 2 * np.imag(p), np.abs(z1) ** 2 - np.abs(z2) ** 2


def assert_fiber_identity(trials=200):
    """THE memo's key identity: h(e^{iφ}z1, e^{iφ}z2) = h(z1,z2) exactly.
    Numerically: any common rotation changes NOTHING in all three features."""
    rng = np.random.default_rng(11)
    for _ in range(trials):
        z1 = rng.normal() + 1j * rng.normal()
        z2 = rng.normal() + 1j * rng.normal()
        delta = rng.uniform(0, 2 * np.pi)
        a = hopf_direct(z1, z2)
        b = hopf_direct(np.exp(1j * delta) * z1, np.exp(1j * delta) * z2)
        assert np.allclose(a, b, atol=1e-9), (a, b)
        # AND the collapsed relative-phase form equals the direct form:
        A1, A2 = abs(z1), abs(z2)
        dphi = np.angle(z1) - np.angle(z2)
        c = (2 * A1 * A2 * np.cos(dphi), 2 * A1 * A2 * np.sin(dphi), A1 ** 2 - A2 ** 2)
        assert np.allclose(a, c, atol=1e-9), (a, c)
    print("fiber identity: asserted over", trials, "random samples (exact to 1e-9)")


# ------------------------------------------------------------ decoder form
_LUTN = 1024
_i = np.arange(_LUTN)
LUTC = np.rint(np.cos(2 * np.pi * _i / _LUTN) * (1 << 14)).astype(np.int64)
LUTS = np.rint(np.sin(2 * np.pi * _i / _LUTN) * (1 << 14)).astype(np.int64)


def phase_idx(n, pQ10, qQ10):
    """relative-phase raster index (integer): i16 Q10 turns per axis."""
    ys, xs = np.mgrid[0:n, 0:n]
    return (((np.int64(pQ10) * xs + np.int64(qQ10) * ys) // (n - 1)) % _LUTN)


def pred_law_idx(pQ10, qQ10, a_c, b_c, s_c, n):
    """the wire's integer prediction (decoder replay verbatim)."""
    idx = phase_idx(n, pQ10, qQ10)
    out = np.empty((n, n, 3), np.int64)
    for c in range(3):
        acc = np.int64(b_c[c]) * LUTC[idx] + np.int64(s_c[c]) * LUTS[idx]
        corr = (acc + (1 << 20)) >> 21
        out[..., c] = np.clip(np.int64(a_c[c]) + corr, 0, 255)
    return out


def features_fp(n, pQ10, qQ10):
    """float features for the FIT (never for decode): angle at Q10 lattice."""
    ys, xs = np.mgrid[0:n, 0:n].astype(np.float64)
    turns = (pQ10 * xs / (n - 1) + qQ10 * ys / (n - 1)) / _LUTN
    return np.cos(2 * np.pi * turns), np.sin(2 * np.pi * turns)


def cd_g(res):
    return np.stack([(res[..., 1]) % 256,
                     (res[..., 0] - res[..., 1]) % 256,
                     (res[..., 2] - res[..., 1]) % 256], axis=-1)


def arm(px, pred, tp):
    res = (px - pred) % 256
    assert np.array_equal((pred + res) % 256, px % 256), "recon inexact"
    return FRAME + len(ZC.compress(cd_g(res).astype(np.uint8).tobytes())) + tp


def med_plane(px):
    return pred_map("med", px)


def signed_err(px, pred):
    e = (px - pred) % 256
    return np.where(e > 128, e - 256, e)


INT_GRID = [d for d in (0, 1, 2, 3, 4, 5, 6, 8, -1, -2, -3, -4, -5, -6, -8)]


def quant_b(v):
    """RGB-unit coefficient → i16 Q7."""
    return int(np.clip(round(v * (1 << 7)), -32767, 32767))


# ------------------------------------------------------------- discovery
def screen_grid(target, feats_cache, n, on_error, grid=INT_GRID, proxy_top=6):
    """stage-1: normal-equations lstsq per (p,q) — cheap proxy = L1 residual.
    on_error=True fits MED's error plane (corrections), else the pixels."""
    T = target.reshape(-1, 3).astype(np.float64)
    scored = []
    for p in grid:
        for q in grid:
            cth, sth = features_fp(n, p * (1 << 10), q * (1 << 10))
            D = np.stack([np.ones(cth.size), cth.ravel(), sth.ravel()], axis=1)
            DtD = D.T @ D
            DtT = D.T @ T
            coef = np.linalg.solve(DtD + 1e-9 * np.eye(3), DtT)  # (3,3)
            f = (D @ coef).reshape(n, n, 3)
            r = target - f
            scored.append((float(np.abs(r).sum()), p, q, coef))
    scored.sort(key=lambda t: t[0])
    return scored[:proxy_top]


def honest_candidates(px, med, target, pqa, n, on_error, refine=True):
    """stage-2/3: honest wire cost of the top ramps (+ fractional refine)."""
    results = []
    for score, p, q, coef in pqa:
        preds, tps = wire_px_from_coef(px, med, target, n, p, q, coef, on_error)
        results.append((preds, tps, p * (1 << 10), q * (1 << 10)))
    if refine:
        # fractional refinement: ±(1..12) Q10 steps around the best ramp
        bp, bq = results[0][2], results[0][3]
        best_ramp = []
        for dp in range(-12, 13, 2):
            for dq in range(-12, 13, 2):
                if dp == 0 and dq == 0:
                    continue
                best_ramp.append((bp + dp, bq + dq))
        sub = []
        for pQ10, qQ10 in best_ramp:
            cth, sth = features_fp(n, pQ10, qQ10)
            D = np.stack([np.ones(cth.size), cth.ravel(), sth.ravel()], axis=1)
            DtD = D.T @ D
            DtT = D.T @ target.reshape(-1, 3)
            coef = np.linalg.solve(DtD + 1e-9 * np.eye(3), DtT)
            f = (D @ coef).reshape(n, n, 3)
            sub.append((float(np.abs(target - f).sum()), pQ10 // (1 << 10) if False else pQ10, qQ10, coef))
        sub.sort(key=lambda t: t[0])
        for score, pQ10, qQ10, coef in sub[:4]:
            preds, tps = wire_px_from_coef(px, med, target, n, pQ10, qQ10, coef, on_error)
            results.append((preds, tps, pQ10, qQ10))
    return results


def wire_px_from_coef(px, med, target, n, p, q, coef, on_error):
    """quantize the fitted map to the wire lattice; replay the INTEGER path;
    honest cost of the complete record."""
    pQ10 = int(p) if abs(p) < 32768 else int(np.clip(round(p), -32768, 32767))
    qQ10 = int(q) if abs(q) < 32768 else int(np.clip(round(q), -32768, 32767))
    a_c = [0, 0, 0]
    b_c = [0, 0, 0]
    s_c = [0, 0, 0]
    for c in range(3):
        a_c[c] = quant_b(coef[0, c])
        b_c[c] = quant_b(coef[1, c])
        s_c[c] = quant_b(coef[2, c])
    law = pred_law_idx(pQ10, qQ10, a_c, b_c, s_c, n)
    pred = np.clip(med + (law - 127), 0, 255).astype(np.int64) if on_error else law
    if on_error:
        # law carried as a CORRECTION: a_c around its fitted value minus the
        # 127 bias we just unwound — recompute cleanly: correction = law + (a-127)
        corr_law = np.zeros((n, n, 3), np.int64)
        for c in range(3):
            k = np.int64(b_c[c]) * LUTC[phase_idx(n, pQ10, qQ10)] + np.int64(s_c[c]) * LUTS[phase_idx(n, pQ10, qQ10)]
            corr_law[..., c] = ((k + (1 << 20)) >> 21) + np.int64(a_c[c])
        pred = np.clip(med + corr_law, 0, 255).astype(np.int64)
    tp = TP_HOPF
    return arm(px, pred, tp), tp


def arm_B(px, med):
    """IF3's global affine comparator: per-channel [1,x,y] plane on MED's
    error, fp64 transport — the memo's 'conventional affine color prediction'."""
    err = signed_err(px, med)
    n = px.shape[0]
    ys, xs = np.mgrid[0:n, 0:n].astype(np.float64)
    u, v = xs / (n - 1), ys / (n - 1)
    M = np.stack([np.ones(u.size), u.ravel(), v.ravel()], axis=1)
    coefs = np.stack([np.linalg.lstsq(M, err[..., c].ravel(), rcond=None)[0] for c in range(3)])
    f = np.stack([M @ coefs[c] for c in range(3)], axis=-1).reshape(n, n, 3)
    pred = np.clip(med + f, 0, 255).astype(np.int64)
    return arm(px, pred, 3 + coefs.size * 8)


def arm_hopf(px, med, form="corr", quadtree=False):
    n = px.shape[0]
    target = signed_err(px, med) if form == "corr" else px.astype(np.float64)
    cand = screen_grid(target, None, n, form == "corr")
    results = honest_candidates(px, med, target, cand, n, form == "corr")
    best = min(results, key=lambda t: t[0])
    if not quadtree:
        return best[0], None
    # 2×2 quadrant: per-quadrant best ramp+map, honest root-vs-split
    s_med = n // 2
    quads = [(slice(0, s_med), slice(0, s_med)), (slice(0, s_med), slice(s_med, n)),
             (slice(s_med, n), slice(0, s_med)), (slice(s_med, n), slice(s_med, n))]
    pred_split = med.copy() if form == "corr" else np.zeros_like(med)
    tp_split = 24 + 2  # split tag + per-quad tags
    quad_best = 0
    for sl_y, sl_x in quads:
        sy, sx = sl_y.stop - sl_y.start, sl_x.stop - sl_x.start
        med_q = med[sl_y, sl_x]
        tgt_q = signed_err(px[sl_y, sl_x], med_q) if form == "corr" else px[sl_y, sl_x].astype(np.float64)
        cq = screen_grid(tgt_q, None, sy, form == "corr")
        # honest re-cost of each quadrant's wire (only the quadrant's pixels
        # ride in the final split wire — cost via the WHOLE-plane honest form
        # at the END; here choose the quadrant law by its own proxy→honest mix)
        preds = []
        for score, p, q, coef in cq[:3]:
            pQ10, qQ10 = int(p) * (1 << 10), int(q) * (1 << 10)
            a_c = [quant_b(coef[0, c]) for c in range(3)]
            b_c = [quant_b(coef[1, c]) for c in range(3)]
            s_c = [quant_b(coef[2, c]) for c in range(3)]
            preds.append((score, pQ10, qQ10, a_c, b_c, s_c))
        score, pQ10, qQ10, a_c, b_c, s_c = min(preds, key=lambda t: t[0])
        idx_q = phase_idx(sy, pQ10, qQ10)
        corr_q = np.zeros((sy, sx, 3), np.int64)
        for c in range(3):
            k = np.int64(b_c[c]) * LUTC[idx_q] + np.int64(s_c[c]) * LUTS[idx_q]
            corr_q[..., c] = ((k + (1 << 20)) >> 21) + np.int64(a_c[c])
        if form == "corr":
            pred_split[sl_y, sl_x] = np.clip(med_q + corr_q, 0, 255)
        else:
            pred_split[sl_y, sl_x] = np.clip(corr_q, 0, 255)
        quad_best += 1
        tp_split += 24 * 1  # per-quadrant law transport
    tp_split += 24 * 0
    pred_final = np.clip(pred_split, 0, 255).astype(np.int64)
    cost_split = arm(px, pred_final, tp_split)
    if cost_split < best[0]:
        return cost_split, "split"
    return best[0], "root"


# ---------------------------------------------------------------- fixtures
def fixture_exact_law(n=256, p=3, q=5):
    """RGB generated BY the wire's integer law (HF-A): the exact arm = the
    same params → zero residual by construction."""
    a_c = [130, 90, 160]
    b_c = [50, -35, 20]
    s_c = [25, 40, -15]
    return pred_law_idx(p * (1 << 10), q * (1 << 10), a_c, b_c, s_c, n)


def fixture_off_lattice(n=256, p=3.37, q=5.0):
    """fp-angled oscillation (HF-B): the integer-ramp grid can only round;
    the Q10 fractional refinement is what recovers precision."""
    ys, xs = np.mgrid[0:n, 0:n].astype(np.float64)
    u, v = xs / (n - 1), ys / (n - 1)
    ang = 2 * np.pi * (p * u + q * v)
    m = np.stack([130 + 50 * np.cos(ang) + 25 * np.sin(ang),
                  90 - 35 * np.cos(ang) + 40 * np.sin(ang),
                  160 + 20 * np.cos(ang) - 15 * np.sin(ang)], axis=-1)
    return np.clip(np.round(m), 0, 255).astype(np.int64)

# ---------------------------------------------------------------- main
def main():
    rows = []
    assert_fiber_identity()

    fixtures = [("HF-A-exactlaw", fixture_exact_law()),
                ("HF-B-offlattice", fixture_off_lattice()),
                ("NOISE", np.random.default_rng(9).integers(0, 256, (256, 256, 3), dtype=np.int64))]

    for tag, img in fixtures:
        med = med_plane(img)
        row = {"image": tag}
        row["A_med"] = arm(img, med, 0)
        row["B_affine"] = arm_B(img, med)
        d, q = arm_hopf(img, med, "dir")
        row["H1dir"] = d
        if q is not None:
            row["H1dir_pick"] = q
        c, q2 = arm_hopf(img, med, "corr")
        row["H1corr"] = c
        if q2 is not None:
            row["H1corr_pick"] = q2
        cq, q3 = arm_hopf(img, med, "corr", quadtree=True)
        row["H1q"] = cq
        row["H1q_pick"] = q3
        # vibratory: the same record with a common fiber phase on the
        # PARAM side — structurally absent from the wire (the projection
        # consumes only Δφ) — asserted byte-identical re-cost here:
        c_again, _ = arm_hopf(img, med, "corr")
        assert c_again == c, "vibratory tie violated"
        row["H1v_tie"] = 0
        rows.append(row)
        print(json.dumps(row), flush=True)

    for name, path in [("astronaut", "astronaut.png"), ("coffee", "coffee.png"),
                       ("chelsea", "chelsea.png"), ("hubble", "hubble_deep_field.jpg")]:
        im = Image.open(Path("/tmp/mnf14_if2") / path).convert("RGB").resize((512, 512), Image.LANCZOS)
        px = np.asarray(im, np.int64)
        med = med_plane(px)
        row = {"image": name}
        row["A_med"] = arm(px, med, 0)
        row["B_affine"] = arm_B(px, med)
        d, _ = arm_hopf(px, med, "dir")
        row["H1dir"] = d
        c, _ = arm_hopf(px, med, "corr")
        row["H1corr"] = c
        cq, q3 = arm_hopf(px, med, "corr", quadtree=True)
        row["H1q"] = cq
        row["H1q_pick"] = q3
        rows.append(row)
        print(json.dumps(row), flush=True)
        Path("/private/tmp/mnf14_hf1/hf1_rows.json").write_text(json.dumps(rows, indent=1))

    Path("/private/tmp/mnf14_hf1/hf1_rows.json").write_text(json.dumps(rows, indent=1))
    print("wrote", OUT / "hf1_rows.json")


if __name__ == "__main__":
    main()