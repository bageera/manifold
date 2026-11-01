#!/usr/bin/env python3
"""MNF14-ID3 — tile-scale div-form law family (firing-surface + inversion probe).

The proposed law (per channel, per 16x16 tile, ALL-INTEGER — no fp anywhere):

    pred(x, y) = clamp(v00 + (tau + vx*x + vy*y) // D, 0, 255)

Wire (mode 9 LAW-DIV proposal, ADR-010(a) doc block — frozen before harness):
    record = [1 B mode][21 B params][n B residual]
    params per channel (7 B): [v00 u8][tau u8][vx i16 LE][vy i16 LE][D u8]
    - v00 = pred(0,0) (in [0,255]); tau in [0, D) — so tau//D = 0 and
      pred(0,0) = clamp(v00) = v00 EXACTLY (the redundant-degree resolution:
      the origin value is v00; per-pixel constant offsets reprice inside the
      residual body, never transported twice).
    - FLOOR division incl. negatives (div_euclid wire rule; the refdecode
      lesson: Rust `/` truncates toward zero and breaks steep negative
      slopes — the freeze test pins one).
    - clamp is part of the wire law: clipped ramps (saturated plateaus) are
      EXACTLY generative; mode 6 (D=32, unclamped, i8 rates) cannot express
      them.
    - Canonicalization: (vx,vy,D,tau) -> (g·vx, g·vy, g·D, g·tau) is
      observationally identical, so discovery enumerates PRIMITIVE triples;
      the tau-interval pick lands in [0, D) which is already reduced.

Discovery (ADR-010(b) bounded, decoder never reruns):
  Interior-pair rate intervals. Clamp outputs are only ever 0 or 255, so a
  free (formula-exact) pixel is any in 1..254. For any same-row free pair
  (a, b), v00 and the row's vy·y term cancel, and floor arithmetic gives
      vx*(b-a) in [D*(dv-1)+1, D*(dv+1)-1],  dv = v(b) - v(a)
  (and symmetrically for vy down columns). The intersection of these closed
  intervals per D is a candidate set CONTAINING the true rate for every
  wire-law member (complete), cheap, and clip-aware: saturated plateaus
  exclude themselves from the pairs. 14-entry bounded D table, <= CAND_CAP
  candidates per axis per D. tau then comes from the clamp-aware interval
  intersection (v >= 1 pins the lower bound, v <= 254 the upper; saturated
  pixels drop one side — how clipped ramps stay solvable).

Stages (pre-registered):
  A  exact inversion on GENERATED tiles (fixtures from the wire law's own
     replay — membership by construction; ID1-A discipline), incl. negative
     slopes, clip saturation, off-lattice tau (folds into v00), and ONE
     family-boundary decline recorded as expected.
  B  fp control: fp-LSQ plane + mode-6-lattice replay on a steep generated
     tile (the ID1 rounding floor, tile-scale confirmation).
  F  firing census: per-tile arms {MED, const(8), planar(6 analog), div(9),
     literal} on named-class files — where the div arm's complete-record
     cost beats every shipped arm. zlib-9 per-tile proxy (ID2 census
     convention), isolation-estimate precision per R1.
  C  composition: a 256x256 frame of per-tile laws (mixed families + noise
     tiles) through the EXISTING per-tile pick; totals with/without the div
     arm + a CLI (--mnf8=maximum, current binary) anchor.
"""
import json
import sys
import zlib
from pathlib import Path

import numpy as np
from PIL import Image

sys.path.insert(0, "/Users/jonbethea/projects/blacktech/manifold/crates/mnf-mans/scripts")
from mnf13_r6_extract import pred_map  # noqa: E402 — canonical MED walk

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")
OUT = Path("/private/tmp/mnf14_id3")
OUT.mkdir(exist_ok=True)
rows = []

D_TABLE = (1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 24, 32, 48, 64)
CAND_CAP = 16  # bounded budget: max rate candidates per axis per D


# ------------------------------------------------------------------ wire law
def div_pred(params, w, h):
    """The WIRE law replayed independently (decode-form): clamp + floor div."""
    v00, tau, vx, vy, D = params
    xs, ys = np.meshgrid(np.arange(w, dtype=np.int64), np.arange(h, dtype=np.int64))
    u = v00 + (tau + vx * xs + vy * ys) // D  # python // IS floor (incl neg)
    return np.clip(u, 0, 255)


def _pair_stats(lines):
    """D-independent interior-pair table: per line, (dist, dv) for the span
    pair and every consecutive free pair. Lines with <2 free pixels skip."""
    out = []
    for line in lines:
        free = np.flatnonzero((line >= 1) & (line <= 254))
        if free.size < 2:
            continue
        i0, i1 = int(free[0]), int(free[-1])
        prs = [(i0, i1)]
        prs += [(int(a), int(b)) for a, b in zip(free[:-1], free[1:])]
        out.append([(b - a, int(line[b]) - int(line[a])) for a, b in prs])
    return out


def solve_div_channel(plane, budget_log):
    """Exact/best div-law fit for ONE channel plane (h*w ints 0..255).

    Returns (params, pred, D, exact) — best-effort (min SAD) when not
    exact — or None when no (D, vx, vy) candidate survives the bounds.
    """
    h, w = plane.shape
    v00 = int(plane[0, 0])
    p = plane.astype(np.int64)
    if p.size < 2:
        budget_log["no_shape"] = budget_log.get("no_shape", 0) + 1
        return None
    xs, ys = np.meshgrid(np.arange(w, dtype=np.int64), np.arange(h, dtype=np.int64))
    lo_mask = p >= 1        # X in 1..255 pins a one/two-sided LOWER bound
    hi_mask = p <= 254      # X in 0..254 pins a one/two-sided UPPER bound
    px_pairs = _pair_stats([p[r] for r in range(h)])
    py_pairs = _pair_stats([p[:, c] for c in range(w)])
    best = None

    def axis_cands(pair_table):
        """Integer vx candidates per D: intersection of all interior-pair
        closed intervals. Unconstrained axis (no free pairs) -> [0]."""
        if not pair_table:
            return [0]
        lo_c, hi_c = -(10 ** 15), 10 ** 15
        for line in pair_table:
            for dist, dv in line:
                lo_c = max(lo_c, -((-(D * (dv - 1) + 1)) // dist))  # ceil
                hi_c = min(hi_c, (D * (dv + 1) - 1) // dist)        # floor
        if lo_c > hi_c:
            return []
        c = list(range(lo_c, hi_c + 1))
        if len(c) > CAND_CAP:
            budget_log["cand_cap"] = budget_log.get("cand_cap", 0) + 1
            return None  # skip this D (only non-member shapes get here)
        return c

    for D in D_TABLE:
        vx_c = axis_cands(px_pairs)
        vy_c = axis_cands(py_pairs)
        if vx_c is None or vy_c is None:
            continue
        for vx in vx_c:
            for vy in vy_c:
                z = vx * xs + vy * ys
                lo = ((p - v00) * D - z)[lo_mask].max() if lo_mask.any() else -(1 << 62)
                hi = ((p - v00 + 1) * D - 1 - z)[hi_mask].min() if hi_mask.any() else (1 << 62)
                if lo > hi:
                    continue
                tau = max(lo, 0)
                if tau > min(hi, D - 1):
                    continue
                params = (v00, int(tau), vx, vy, D)
                pred = div_pred(params, w, h)
                exact = bool(np.array_equal(pred, p))
                if best is None or (exact, -int(np.abs(pred - p).sum())) > \
                        (bool(best[3]), -int(np.abs(best[1] - p).sum())):
                    best = (params, pred, D, exact)
                if exact:
                    return best
    return best


def solve_div_tile(tile):
    """Per-channel solve; wire params = 21 B ([v00][tau][vx i16][vy i16][D])x3."""
    bl = {"diff_guard": 0, "cand_cap": 0}
    preds, params_all, exact = [], [], True
    for c in range(3):
        r = solve_div_channel(tile[..., c], bl)
        if r is None:
            return None, bl
        params, pred, _, ex = r
        preds.append(pred)
        params_all.append(params)
        exact &= ex
    pred = np.stack(preds, axis=-1)
    v00s = bytes(pp[0] for pp in params_all)
    taus = bytes(pp[1] for pp in params_all)
    vxvy = b"".join(
        int(pp[2]).to_bytes(2, "little", signed=True)
        + int(pp[3]).to_bytes(2, "little", signed=True)
        for pp in params_all
    )
    Ds = bytes(pp[4] for pp in params_all)
    wire = v00s + taus + vxvy + Ds
    assert len(wire) == 21
    return {"pred": pred, "wire": wire, "exact": exact}, bl


# ------------------------------------------------------------- mode-6 analog
def _tdiv(num, den):
    """i64 division semantics: truncation toward zero (Rust `/`)."""
    q = abs(num) // abs(den)
    return -q if (num < 0) != (den < 0) else q


def planar_seed_tile(plane):
    """Faithful port of src/laws.rs planar_seed_tile (end-point rates, t=0;
    i64 trunc division on the rate, then clamp into i8)."""
    th, tw = plane.shape
    at = lambda x, y: int(plane[y, x])  # single-channel plane
    a = at(0, 0)
    dx = _tdiv((at(tw - 1, 0) - a) * 32, tw - 1) if tw > 1 else 0
    dy = _tdiv((at(0, th - 1) - a) * 32, th - 1) if th > 1 else 0
    return [max(-128, min(127, v)) for v in (a, dx, dy)]


def planar_arm(plane, tw, th):
    """mode-6 analog at the shipped lattice: seed + sweep db/dc in +-2,
    dt in 0..3. Candidates are charged at their own zlib residual cost
    (proxy_cost, as the pick does). The mode-6 WIRE pred is UNCLAMPED —
    clamp is mode-9's new semantics; conflating them would falsify F."""
    seed = planar_seed_tile(plane)
    xs, ys = np.meshgrid(np.arange(tw, dtype=np.int64), np.arange(th, dtype=np.int64))
    best = None
    for db in (-2, -1, 0, 1, 2):
        for dc in (-2, -1, 0, 1, 2):
            for dt in (0, 1, 2, 3):
                b = max(-128, min(127, seed[1] + db))
                c = max(-128, min(127, seed[2] + dc))
                pred = seed[0] + (b * xs + c * ys + dt) // 32
                r = ((plane - pred) % 256).astype(np.uint8).tobytes()
                zc = len(zlib.compress(r, 9))
                if best is None or zc < best[0]:
                    best = (zc, pred)
    return best[1]


# ------------------------------------------------------------------- arms
def resid_bytes(pred, tile):
    return ((tile.astype(np.int32) - pred.astype(np.int32)) % 256).astype(np.uint8).tobytes()


def census_arms(tile, med_tile):
    """Complete-record costs (isolation proxy): 1 B mode + params + z9 body."""
    h, w, _ = tile.shape
    const = np.stack([np.full((h, w), int(np.median(tile[..., c]))) for c in range(3)], -1)
    planar = np.stack([planar_arm(tile[..., c], w, h) for c in range(3)], -1)
    div, bl = solve_div_tile(tile)
    arms = {
        "med": 1 + len(zlib.compress(resid_bytes(med_tile, tile), 9)),
        "const": 1 + 3 + len(zlib.compress(resid_bytes(const, tile), 9)),
        "planar": 1 + 12 + len(zlib.compress(resid_bytes(planar, tile), 9)),
        "literal": 1 + len(zlib.compress(tile.astype(np.uint8).tobytes(), 9)),
    }
    if div is None:
        arms["div"] = None
        arms["div_exact"] = None
    else:
        arms["div"] = 1 + 21 + len(zlib.compress(resid_bytes(div["pred"], tile), 9))
        arms["div_exact"] = div["exact"]
    arms["_guard"] = dict(bl)
    costs = {k: v for k, v in arms.items()
             if k in ("med", "const", "planar", "literal", "div") and isinstance(v, int)}
    arms["_costs"] = costs
    arms["_argmin"] = min((v, k) for k, v in costs.items())[1] if costs else None
    return arms


# ------------------------------------------------------- stage A: inversion
def gen_law_tile(params, w=16, h=16):
    """Fixture = the WIRE law's own replay (membership by construction)."""
    plane = div_pred(params, w, h).astype(np.uint8)
    return np.stack([plane] * 3, -1)


def stage_a():
    print("== A: exact inversion on generated tiles (incl. clip + neg slopes)")
    fixtures = [
        ("steep-int vx", (40, 0, 9, 0, 1)),
        ("steep-int vy", (10, 3, 0, -7, 1)),
        ("steep-neg both", (200, 0, 6, -11, 1)),
        ("frac D=2", (0, 1, 9, 0, 2)),
        ("frac D=3 (stair 1/3)", (0, 0, 1, 0, 3)),
        ("frac D=5", (128, 2, 3, -1, 5)),
        ("frac D=6", (77, 4, 5, 0, 6)),
        ("frac D=24", (5, 7, 25, 13, 24)),
        ("frac D=64", (9, 9, 130, -70, 64)),
        ("clip-sat: 0..255 then hold", (0, 0, 48, 0, 1)),     # saturates at x>=6
        ("clip-sat neg slope", (255, 0, -32, 0, 1)),           # 255..0 then hold 0
        ("clip 2-D diag", (0, 0, 40, 40, 1)),                  # corners clip both ways
        ("flat degenerate", (123, 0, 0, 0, 1)),
        ("D=1 tiny slope", (3, 0, 1, 1, 1)),
        ("off-lattice tau folds (D=1)", (100, 11, -40, 0, 1)),  # tau>D: folds
        # family boundary (NOT wire-representable): the generated origin
        # sits at the 255 clamp while the row drops — no (v00, tau) with
        # pred(0,0)=v00 can continue that row; the pair intervals go empty.
        ("clip-below-origin (decline)", (255, 10, -64, -8, 2)),
    ]
    expected_hit = {"clip-below-origin (decline)": False}
    ok = 0
    for name, params in fixtures:
        tile = gen_law_tile(params)
        sol, bl = solve_div_tile(tile)
        hit = sol is not None and sol["exact"]
        # independent decode-form recompute (the fit's verify is necessary,
        # not sufficient — same rule as ID1)
        if hit:
            for c in range(3):
                v00, tau = sol["wire"][c], sol["wire"][3 + c]
                vx = int.from_bytes(sol["wire"][6 + 4 * c:8 + 4 * c], "little", signed=True)
                vy = int.from_bytes(sol["wire"][8 + 4 * c:10 + 4 * c], "little", signed=True)
                D = sol["wire"][18 + c]
                assert np.array_equal(div_pred((v00, tau, vx, vy, D), 16, 16), tile[..., c]), name
        want = expected_hit.get(name, True)
        assert hit == want, f"{name}: {'unexpected MISS' if want else 'unexpected EXACT hit'}"
        ok += hit
        print(f"  {name:30} D={params[4]:<3} vx={params[2]:<4} vy={params[3]:<4} "
              f"-> {'EXACT' if hit else 'declined'}")
        rows.append({"stage": "A", "fixture": name, "params_gen": params,
                     "exact": bool(hit), "expected": want})
    print(f"  inversion: {ok}/{len(fixtures)} EXACT "
          f"(1 family-boundary decline as pre-registered)")
    assert ok == len(fixtures) - 1, "membership inversion must hit every member fixture"


def stage_b():
    print("== B: fp control vs the shipped mode-6 wire (i8 rate ceiling)")
    tile = gen_law_tile((40, 0, 9, 0, 1))  # pred = 40 + 9x, EXACT zero-residual
    p = tile[..., 0].astype(np.float64)
    xs, ys = np.meshgrid(np.arange(16.0), np.arange(16.0))
    A = np.stack([np.ones(xs.size), xs.ravel(), ys.ravel()], 1)
    (a_f, b_f, c_f), *_ = np.linalg.lstsq(A, p.ravel(), 1)
    # quantize to the mode-6 WIRE lattice: rates at 1/32 CLAMPED INTO i8
    # (the fp fit wants b=288 = 9/px, which mode 6 cannot transport; the
    # honest control clamps exactly what its wire would carry)
    a_q = int(np.clip(round(a_f), -128, 127))
    b_q, c_q = int(np.clip(round(b_f * 32), -128, 127)), int(np.clip(round(c_f * 32), -128, 127))
    pred6 = a_q + (b_q * xs.astype(np.int64) + c_q * ys.astype(np.int64)) // 32  # unclamped wire
    cost6 = 1 + 12 + len(zlib.compress(resid_bytes(pred6[..., None], tile), 9))
    sol = solve_div_channel(tile[..., 0], {"diff_guard": 0, "cand_cap": 0})
    cost9 = 1 + 21 + len(zlib.compress(resid_bytes(sol[1][..., None], tile), 9))
    print(f"  fp fit rate 9.0/px -> mode-6 wire i8 clamp {b_q}/32; record {cost6} B "
          f"(abs-err {int(np.abs(pred6 - tile[..., 0]).sum())}) "
          f"vs div-law (i16 + free D) {cost9} B (exact)")
    rows.append({"stage": "B", "cost6": cost6, "cost9": cost9,
                 "fp_abs_err": int(np.abs(pred6 - tile[..., 0]).sum()),
                 "note": "mode-6 decline is transport (i8), not fit quality — "
                         "fp fit landed on the D=32 lattice exactly; only the "
                         "div-form's i16 rate carries this class"})
    assert cost9 < cost6, "exact-generative integer must beat the wire-limited replay"


def stage_f(named):
    print("== F: firing census on named-class files")
    ts = 16
    for name, path, crop in named:
        img = np.asarray(Image.open(path).convert("RGB"), np.int32)[:crop, :crop]
        h, w = img.shape[:2]
        med_full = pred_map("med", img.astype(np.int64))[:h, :w]
        n_t = (h // ts) * (w // ts)
        wins, argmins, guards, exacts = 0, {}, {"diff_guard": 0, "cand_cap": 0}, 0
        for ty in range(0, h - ts + 1, ts):
            for tx in range(0, w - ts + 1, ts):
                t = img[ty:ty + ts, tx:tx + ts]
                m = med_full[ty:ty + ts, tx:tx + ts]
                a = census_arms(t, m)
                argmins[a["_argmin"]] = argmins.get(a["_argmin"], 0) + 1
                for k in guards:
                    guards[k] += a["_guard"].get(k, 0)
                exacts += bool(a["div_exact"])
                if a["div"] is not None and a["_argmin"] == "div":
                    wins += 1
        print(f"  {name}: tiles={n_t} div-wins={wins} argmin={argmins} "
              f"budget={guards} div-exact-tiles={exacts}")
        rows.append({"stage": "F", "file": name, "tiles": n_t, "div_wins": wins,
                     "argmin_counts": argmins, "budget": guards,
                     "div_exact_tiles": exacts})


def stage_c():
    print("== C: composition frame through the existing per-tile pick")
    rng = np.random.default_rng(14)
    ts = 16
    mix = [(40, 0, 9, 0, 1), (0, 0, 1, 0, 3), (123, 0, 0, 0, 1),
           (200, 0, 6, -11, 1), (0, 1, 9, 0, 2), (9, 9, 130, -70, 64)]
    img = np.zeros((256, 256, 3), np.int32)
    kinds = np.zeros((16, 16), np.int32)
    for ty in range(16):
        for tx in range(16):
            if rng.random() < 0.15:  # noise tiles: the pick must keep literals
                img[ty * ts:(ty + 1) * ts, tx * ts:(tx + 1) * ts] = rng.integers(
                    0, 256, (ts, ts, 3))
            else:
                k = int(rng.integers(0, len(mix)))
                kinds[ty, tx] = k + 1
                img[ty * ts:(ty + 1) * ts, tx * ts:(tx + 1) * ts] = gen_law_tile(mix[k])
    med_full = pred_map("med", img.astype(np.int64))
    tot_incl = tot_excl = 0
    picks = {}
    for ty in range(16):
        for tx in range(16):
            t = img[ty * ts:(ty + 1) * ts, tx * ts:(tx + 1) * ts]
            m = med_full[ty * ts:(ty + 1) * ts, tx * ts:(tx + 1) * ts]
            a = census_arms(t, m)
            picks[a["_argmin"]] = picks.get(a["_argmin"], 0) + 1
            tot_incl += a["_costs"][a["_argmin"]]
            tot_excl += min(v for k, v in a["_costs"].items() if k != "div")
    print(f"  composite 256x256: pick-with-div total {tot_incl} B vs without {tot_excl} B "
          f"= {tot_incl - tot_excl:+d} B; tile picks {picks}")
    cli = 1 << 62
    binp = ROOT / "target/release/manifold"
    if binp.exists():
        import subprocess as sp
        png = OUT / "id3_composite.png"
        Image.fromarray(img.astype(np.uint8)).save(png)
        out = OUT / "id3_composite.mnf8"
        r = sp.run([str(binp), "encode", str(png), str(out), "--mnf8", "--profile", "maximum"],
                   capture_output=True, text=True)
        if r.returncode == 0:
            cli = out.stat().st_size
    if cli >= 1 << 62:
        print("  CLI anchor: release binary absent — not run (recorded as absent)")
    else:
        print(f"  CLI anchor (current binary, no mode 9): {cli} B")
    rows.append({"stage": "C", "with_div": tot_incl, "without_div": tot_excl,
                 "cli_anchor": cli, "tile_picks": picks})
    Image.fromarray(kinds.astype(np.uint8) * 12).save(OUT / "id3_kinds.png")


def main():
    stages = sys.argv[1] if len(sys.argv) > 1 else "abfc"
    if "a" in stages:
        stage_a()
    if "b" in stages:
        stage_b()
    if "f" in stages:
        stage_f([
            ("gradient_320x240", ROOT / "testdata/gradient_320x240.png", 320),
            ("hubble_512 (CLI-decoded)", Path("/tmp/hf1_hubble_decoded.png"), 512),
        ])
    if "c" in stages:
        stage_c()
    (OUT / "id3_rows.json").write_text(json.dumps(rows, indent=1, default=int))
    print("wrote", OUT / "id3_rows.json")


if __name__ == "__main__":
    main()