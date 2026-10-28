#!/usr/bin/env python3
"""MNF10 generative-function discovery probe (Jon's directive, 2026-10-28).

Hypothesis: discover the simplest decoder-reproducible integer rule that
GENERATES the observed quantized pixels exactly, as candidate arms against
the current shipping pick (v1.9.0 CLI --mnf8). Two modes per model:
  exact-fit    (zero residual payload; params + id + container only)
  residual-fit (model prediction + corrections, residual stream coded)
Retention metric per file: L_model = L_type + L_params + L_residuals + L_container.
A model is retained only if its complete file beats the shipping pick.

Canonical decoder arithmetic (explicit rounding, all integer):
  div-form   I(x)     = (N*x + T) // D          (floor division)
  div-2D     I(x,y)   = (Nx*x + Ny*y + T) // D
  quad       I(x)     = (N2*x*x + N1*x + T) // D
  rec-1      run: x_{n+1} = x_n + d
  rec-2      run: d_{n+1} = d_n + d2 (second difference constant)
  bilinear-J I(x,y)   = (v00*(W-1-x)*(H-1-y) + v10*x*(H-1-y)
                        + v01*(W-1-x)*y + v11*x*y) // ((W-1)*(H-1))
  bilinear-N nested round-lerp over x then over y (editor convention)
D comes from a bounded convention table (the ones real generators use);
scan orientation row-major / col-major is a charged shared bit.

All arms in ONE process; residual containers: zlib-9 AND zstd-19 (checksum)
both measured, smaller charged (same-coder parity vs itself; the baseline is
the release CLI --mnf8 wire — flate2/zstd deltas are harness-boundary noise,
noted in the doc). Params are encoded with a real varint writer — the bytes
are counted as the wire would carry them.

Floor rule (skill): the FIRST measurement is the constructed home-turf class
(Jon's 20->230 floor-lerp gradient). If the exact-fit model floor cannot beat
the pick there, the family is underwater regardless of fit quality.
"""

import io
import subprocess
import struct
import sys
import zlib
from pathlib import Path

import numpy as np
from PIL import Image

REPO = Path("/Users/jonbethea/projects/blacktech/manifold")
CLI = REPO / "target/release/manifold"
WORK = Path("/tmp/mnf10-gen")

# ---------------------------------------------------------------- generators

def lerp_floor_plane(w, h, endpoints):
    """I(x) = a + (x*(b-a))//(W-1) per channel, rows constant along y.
    endpoints: [(a_r,b_r),(a_g,b_g),(a_b,b_b)]."""
    x = np.arange(w, dtype=np.int64)
    chan = []
    for (a, b) in endpoints:
        chan.append(a + (x * (b - a)) // (w - 1))
    row = np.stack(chan, axis=-1).astype(np.uint8)          # (w,3)
    return np.repeat(row[None, :, :], h, axis=0)             # (h,w,3)


def lerp_round_plane(w, h, endpoints):
    """I(x) = round(a + (b-a)*x/(W-1)) — the round-lerp convention."""
    x = np.arange(w, dtype=np.int64)
    chan = []
    for (a, b) in endpoints:
        chan.append(a + ((x * (b - a)) + (w - 1) // 2) // (w - 1))
    row = np.stack(chan, axis=-1).astype(np.uint8)
    return np.repeat(row[None, :, :], h, axis=0)


def lerp_float_plane(w, h, endpoints):
    """float64 lerp + round — editor-style float interpolation."""
    x = np.arange(w, dtype=np.float64)
    chan = []
    for (a, b) in endpoints:
        chan.append(np.round(a + (b - a) * x / (w - 1)))
    row = np.stack(chan, axis=-1).astype(np.uint8)
    return np.repeat(row[None, :, :], h, axis=0)


def recurrence_plane(w, h, starts, step):
    """x_{n+1} = (x_n + d) mod 256 along x, rows identical (accumulation)."""
    x = np.arange(w, dtype=np.int64)
    chan = []
    for (a) in starts:
        chan.append((a + (x * step)) % 256)   # recurrence == exact arithmetic sequence mod 256
    row = np.stack(chan, axis=-1).astype(np.uint8)
    return np.repeat(row[None, :, :], h, axis=0)


def bilinear_joint_plane(w, h, corners):
    """Joint floor-bilinear over integer corners (canonical form above)."""
    (v00, v10, v01, v11) = corners  # per channel tuples handled by caller loop
    x = np.arange(w, dtype=np.int64)[None, :, None]
    y = np.arange(h, dtype=np.int64)[:, None, None]
    num = (v00[None, None, :] * (w - 1 - x) * (h - 1 - y)
           + v10[None, None, :] * x * (h - 1 - y)
           + v01[None, None, :] * (w - 1 - x) * y
           + v11[None, None, :] * x * y)
    return (num // ((w - 1) * (h - 1))).astype(np.uint8)


def bilinear_nested_plane(w, h, corners):
    """Editor-style nested round-lerp per channel: round(lerp over x) on the
    top and bottom corner rows, then round(lerp over y) between them."""
    (v00, v10, v01, v11) = corners
    x = np.arange(w, dtype=np.int64)[:, None]                     # (w,1) per-channel col
    top = v00[None, :] + ((x * (v10 - v00)[None, :] + (w - 1) // 2) // (w - 1))   # (w,3)
    bot = v01[None, :] + ((x * (v11 - v01)[None, :] + (w - 1) // 2) // (w - 1))   # (w,3)
    y = np.arange(h, dtype=np.int64)[:, None, None]
    inner = top[None, :, :] + ((y * (bot[None, :, :] - top[None, :, :])
                                + (h - 1) // 2) // (h - 1))        # (h,w,3)
    return inner.astype(np.uint8)


def piecewise_plane(w, h, segs):
    """Concatenated ramps; segs = [(a,b,wfrac)], rounding = floor-lerp."""
    x = np.arange(w, dtype=np.int64)
    out = np.zeros((w, 3), dtype=np.int64)
    wx = 0
    for i, (a, b, frac) in enumerate(segs):
        n = w if i == len(segs) - 1 else int(round(w * frac))
        seg_x = np.clip(x - wx, 0, None)
        seg_w = max(n - 1, 1)
        for c in range(3):
            out[wx:wx + n, c] = a[c] + (np.clip(seg_x[wx:wx + n], 0, seg_w) * (b[c] - a[c])) // seg_w
        wx += n
        if wx >= w:
            break
    row = out.astype(np.uint8)
    return np.repeat(row[None, :, :], h, axis=0)


def solid_plane(w, h, rgb):
    return np.zeros((h, w, 3), dtype=np.uint8) + np.array(rgb, dtype=np.uint8)[None, None, :]


# ------------------------------------------------------------------ corpus

def build_corpus():
    WORK.mkdir(parents=True, exist_ok=True)
    files = []  # (name, array, class, heldout)

    def add(name, arr, cls, heldout=False):
        files.append((name, arr, cls, heldout))

    E = [(20, 230), (230, 20), (60, 200)]
    add("exact_floor", lerp_floor_plane(160, 120, E), "exact-floor")           # Jon's home turf
    add("round_conv", lerp_round_plane(160, 120, E), "round-conv")
    add("float_conv", lerp_float_plane(160, 120, E), "float-conv")
    add("recurrence", recurrence_plane(160, 120, [30, 200, 90], 7), "recurrence")
    add("bilinear_j", bilinear_joint_plane(160, 120,
        (np.array([15, 200, 70]), np.array([225, 30, 180]),
         np.array([90, 90, 20]), np.array([245, 240, 240]))), "bilinear-joint")
    add("bilinear_n", bilinear_nested_plane(160, 120,
        (np.array([15, 200, 70]), np.array([225, 30, 180]),
         np.array([90, 90, 20]), np.array([245, 240, 240]))), "bilinear-nested")
    add("piecewise", piecewise_plane(160, 120,
        [((20, 10, 60), (130, 120, 150), 0.5), ((130, 120, 150), (230, 235, 200), 1.0)]), "piecewise")
    add("noisy", (lerp_floor_plane(160, 120, E).astype(np.int32)
                  + np.random.RandomState(11).randint(-1, 2, (120, 160, 3))) .clip(0, 255).astype(np.uint8), "noisy")
    add("solid", solid_plane(160, 120, (7, 7, 7)), "solid")
    # editor-generated: Pillow's own linear gradient (float, its convention)
    lg = Image.linear_gradient("L")  # 256x256
    rgblg = np.asarray(lg.convert("RGB"), dtype=np.uint8)
    try:
        RS = Image.Resampling.NEAREST
    except AttributeError:
        RS = Image.NEAREST
    rgblg = np.asarray(lg.convert("RGB").resize((160, 120), RS), dtype=np.uint8)
    add("pillow_gradient", rgblg, "editor")
    # held-out: same generator SET, different endpoints/step/dims, never tuned on
    E2 = [(5, 250), (100, 250), (1, 254)]
    add("h_exact_floor", lerp_floor_plane(144, 96, E2), "exact-floor", heldout=True)
    add("h_round_conv", lerp_round_plane(144, 96, E2), "round-conv", heldout=True)
    add("h_recurrence", recurrence_plane(144, 96, [3, 111, 222], 11), "recurrence", heldout=True)
    add("h_bilinear_j", bilinear_joint_plane(144, 96,
        (np.array([1, 2, 3]), np.array([250, 100, 200]),
         np.array([99, 250, 5]), np.array([200, 7, 245]))), "bilinear-joint", heldout=True)
    add("h_solid", solid_plane(144, 96, (221, 221, 221)), "solid", heldout=True)

    names = []
    for name, arr, cls, heldout in files:
        p = WORK / f"{name}.png"
        Image.fromarray(arr, "RGB").save(p)
        names.append((name, p, cls, heldout))
    return names


# ------------------------------------------------------------ wire plumbing

def varint(n):
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out.append(b | 0x80)
        else:
            out.append(b)
            return bytes(out)


def svarint(n):
    return varint((n << 1) ^ (n >> 63) if n >= 0 else ((-n - 1) << 1) | 1)


DEN_TABLE = None  # filled per-image inside arms (depends on W,H)


def den_id(D, extras):
    """Bounded convention table: index or None if not representable."""
    table = list(dict.fromkeys(extras))
    if D in table:
        return table.index(D), table
    return None, table


def z9(b):
    return len(zlib.compress(b, 9))


def z19(b):
    try:
        import zstandard
        c = zstandard.ZstdCompressor(level=19, write_checksum=True)
        return len(c.compress(b)) if hasattr(c, "compress") else len(c.compress(b))
    except Exception:
        return 1 << 30


# ---------------------------------------------------------------- the models
# Each model: solve(plane HxWx3) -> (exact_wire_bytes or None if not exact,
#                                 pred plane, params_bytes)
# Exact wire = 32(hdr) + 1(id) + params + 0 payload  (residual-free)
# Resid wire = 32 + 1 + params + payload(min over z9/z19, planar+interleaved)

def fit_const(P):
    h, w, _ = P.shape
    first = P[0, 0].astype(np.int64)
    ok = np.all(P == first[None, None, :])
    params = b"".join(varint(int(c)) for c in first)
    return ok, first[None, None, :].repeat(w, 1).repeat(h, 0), params


def solve_div_1d(rowvals, w, D):
    """rowvals int64 (w,) — find N,T with (N*x+T)//D == rowvals; return (N,T) or None.
    Interval intersection: for each x the constraint is T ∈ [v(x)*D - N*x,
    v(x)*D - N*x + D). Intersect over x=0 and x=1 (width-D constraint each);
    the intersection pins T to ≤ 1 value per N candidate. N candidates from
    the endpoint slope ±2 around each rounding of D*(v1-v0)/(w-1)."""
    if w < 2:
        return None
    v0, v1 = int(rowvals[0]), int(rowvals[-1])
    x = np.arange(w, dtype=np.int64)
    candidates = set()
    for N in range(round((v1 - v0) * D / (w - 1)) - 2, round((v1 - v0) * D / (w - 1)) + 3):
        lo = v0 * D - N * 0
        hi = lo + D - 1
        for xi in (1, w // 2, w - 1):
            tl = rowvals[xi] * D - N * xi
            th = tl + D - 1
            lo = max(lo, tl)
            hi = min(hi, th)
        if lo <= hi:
            for T in range(lo, hi + 1):
                candidates.add((N, T))
    if len(candidates) > 64:      # bounded verification budget
        candidates = set(sorted(candidates)[:64])
    for (N, T) in sorted(candidates):
        if np.array_equal((N * x + T) // D, rowvals):
            return (N, T)
    return None


def fit_div(P, scan_row, D, two_d=False):
    """Affine div-form fit, per channel, shared D. Returns (ok, params, pred)."""
    h, w, _ = P.shape
    plane = P.astype(np.int64)
    if scan_row:
        # rows constant? verify then fit I(x)
        if not np.array_equal(plane[:, 0, :], plane.max(axis=1)) or not np.array_equal(plane, np.repeat(plane[:1], h, axis=0)):
            if not two_d:
                return False, None, None
        x = np.arange(w, dtype=np.int64)
        row = plane[0, :, :]
        params_parts = []
        preds = []
        okc = True
        for c in range(3):
            sol = solve_div_1d(row[:, c], w, D)
            if sol is None:
                okc = False
                break
            N, T = sol
            params_parts.append(svarint(N) + svarint(T))
            preds.append(((N * x + T) // D))
        if not okc:
            return False, None, None
        pred = np.stack(preds, axis=-1)[None, :, :].repeat(h, 0)
        return True, b"".join(params_parts), pred
    return False, None, None


def fit_div2d(P, D):
    """I(x,y) = (Nx*x + Ny*y + T)//D per channel. Solve from row0/col0 + verify."""
    h, w, _ = P.shape
    plane = P.astype(np.int64)
    x = np.arange(w, dtype=np.int64)[None, :]
    y = np.arange(h, dtype=np.int64)[:, None]
    params_parts = []
    preds = []
    for c in range(3):
        # vertical: at x=0, (Ny*y + T)//D == col0
        sol_row = solve_div_1d(plane[0, :, c], w, D)
        if sol_row is None:
            return False, None, None
        Nx_try, T_row = sol_row[0], sol_row[1]
        # solve (Ny*y + T')//D for col0 with T' = T0 offset by row-0 fit? The
        # joint form needs one global (Nx,Ny,T): from (0,0): T in [v00*D, v00*D+D)
        # Nx from row0 fit range; Ny analogous; verify the full grid.
        found = None
        v00 = int(plane[0, 0, c])
        slope_row = (int(plane[0, w - 1, c]) - v00) * D / max(w - 1, 1)
        slope_col = (int(plane[h - 1, 0, c]) - v00) * D / max(h - 1, 1)
        for Nx in range(round(slope_row) - 2, round(slope_row) + 3):
            for Ny in range(round(slope_col) - 2, round(slope_col) + 3):
                # T interval: x=0,y=0 gives [v00*D, v00*D+D); tighter: x=1,y=0
                tl = int(plane[0, 1, c]) * D - Nx if w > 1 else v00 * D
                th = tl + D - 1
                lo = max(v00 * D, tl)
                hi = min(v00 * D + D - 1, th)
                if w > 2:
                    tl = int(plane[0, 2, c]) * D - 2 * Nx
                    lo = max(lo, tl); hi = min(hi, tl + D - 1)
                if h > 1:
                    tl = int(plane[1, 0, c]) * D - Ny
                    lo = max(lo, tl); hi = min(hi, tl + D - 1)
                if lo > hi:
                    continue
                for T in range(lo, hi + 1):   # ≤ D but usually 1-2 after intersection
                    num = Nx * x + Ny * y + T
                    if np.array_equal(num // D, plane[:, :, c]):
                        found = (Nx, Ny, T)
                        break
                if found:
                    break
            if found:
                break
        if not found:
            return False, None, None
        Nx, Ny, T = found
        params_parts.append(svarint(Nx) + svarint(Ny) + svarint(T))
        preds.append((Nx * x + Ny * y + T) // D)
    pred = np.stack(preds, axis=-1)
    return True, b"".join(params_parts), pred


def fit_rec(P, order, scan_row):
    """Rows/columns CONSTANT scan rule (the recurrence runs INSIDE one scan
    line; the cross-line jump is structural, not +d). All scan lines must be
    identical; the rule is discovered on line 0 and verified on the rest."""
    h, w, _ = P.shape
    plane = P.astype(np.int64)
    if scan_row:
        lines = plane                     # (h, w, 3): scan line = a row
    else:
        lines = plane.transpose(1, 0, 2)  # (w, h, 3): scan line = a column
    line0 = lines[0]
    if not np.array_equal(lines, np.repeat(line0[None], lines.shape[0], 0)):
        return False, None, None
    flat = line0.astype(np.int64)         # (n,3)
    d1 = np.diff(flat, axis=0)
    mod_d1 = (d1 + 128) % 256 - 128       # signed wrap diffs
    n = flat.shape[0]
    if order == 1:
        if n < 2 or not np.all(mod_d1 == mod_d1[0]):
            return False, None, None
        a0 = flat[0]
        d = mod_d1[0]
        k = np.arange(n)
        pred_line = (a0[None, :] + d[None, :] * k[:, None]) % 256
        params = b"".join(varint(int(v)) for v in a0) + b"".join(svarint(int(v)) for v in d)
    else:
        d2 = np.diff(mod_d1, axis=0)
        if n < 3 or not np.all(d2 == d2[0]):
            return False, None, None
        a0 = flat[0]
        d0 = mod_d1[0]
        dd = d2[0]
        k = np.arange(n)
        d_k = d0[None, :] + dd[None, :] * k[:, None]
        pred_line = (a0[None, :] + np.cumsum(d_k, axis=0) - d_k) % 256
        params = (b"".join(varint(int(v)) for v in a0)
                  + b"".join(svarint(int(v)) for v in d0)
                  + b"".join(svarint(int(v)) for v in dd))
    ok = np.array_equal(pred_line.astype(np.int64), line0)
    assert ok, "recurrence must generate line0 exactly"
    pred = np.repeat(pred_line[None], lines.shape[0], 0)
    if not scan_row:
        pred = pred.transpose(1, 0, 2)
    return True, params, pred


def fit_quad(P, D):
    h, w, _ = P.shape
    plane = P.astype(np.int64)
    x = np.arange(w, dtype=np.int64)
    params_parts = []
    preds = []
    for c in range(3):
        row = plane[0, :, c]
        if not np.array_equal(plane[:, :, c], np.repeat(row[None, :], h, 0)):
            return False, None, None
        v0 = int(row[0])
        found = None
        d1 = np.diff(row)
        d2v = np.diff(d1)
        n2 = round(float(np.median(d2v)))
        slope0 = int(d1[0]) if w > 1 else 0
        for N2 in (n2 - 1, n2, n2 + 1):
            # interval-intersect like solve_div_1d over x = 1, 2, mid, w-1
            for xi, target in [(1, int(row[1]) if w > 1 else v0), (2, int(row[2]) if w > 2 else v0)]:
                pass
            lo_ok = None
            for N1 in range(slope0 * D - 2 * D, slope0 * D + 2 * D + 1):
                lo = v0 * D
                hi = v0 * D + D - 1
                good = True
                for xi in (1, 2, w // 2, w - 1):
                    tl = int(row[xi]) * D - N2 * xi * xi - N1 * xi
                    th = tl + D - 1
                    lo = max(lo, tl)
                    hi = min(hi, th)
                    if lo > hi:
                        good = False
                        break
                if good and np.array_equal((N2 * x * x + N1 * x + lo) // D, row):
                    found = (N2, N1, lo)
                    break
            if found:
                break
        if not found:
            return False, None, None
        N2, N1, T = found
        params_parts.append(svarint(N2) + svarint(N1) + svarint(T))
        preds.append((N2 * x * x + N1 * x + T) // D)
    pred = np.stack(preds, axis=-1)[None, :, :].repeat(h, 0)
    return True, b"".join(params_parts), pred


def fit_bilinear(P, joint):
    h, w, _ = P.shape
    plane = P.astype(np.int64)
    c00, c10, c01, c11 = plane[0, 0], plane[0, w - 1], plane[h - 1, 0], plane[h - 1, w - 1]
    x = np.arange(w, dtype=np.int64)[None, :, None]
    y = np.arange(h, dtype=np.int64)[:, None, None]
    if joint:
        num = (c00[None, None] * (w - 1 - x) * (h - 1 - y)
               + c10[None, None] * x * (h - 1 - y)
               + c01[None, None] * (w - 1 - x) * y
               + c11[None, None] * x * y)
        pred = num // ((w - 1) * (h - 1))
        params = b"".join(varint(int(v)) for trip in (c00, c10, c01, c11) for v in trip) + b"\x00"
        ok = np.array_equal(pred, plane)
    else:
        xi = (w - 1) // 2
        yi = (h - 1) // 2
        top = c00[None, None] + ((x * (c10 - c00)[None, None] + xi) // (w - 1))
        bot = c01[None, None] + ((x * (c11 - c01)[None, None] + xi) // (w - 1))
        pred = top + ((y * (bot - top) + yi) // (h - 1))
        params = b"".join(varint(int(v)) for trip in (c00, c10, c01, c11) for v in trip) + b"\x01"
        ok = np.array_equal(pred, plane)
    return ok, params, pred


MODELS = [
    ("const",      lambda P, D: fit_const(P)),
    ("div1d",      None),   # per-D loop, filled below
    ("div2d",      None),
    ("rec1",       None),
    ("rec2",       None),
    ("quad",       None),
    ("bilin_joint", None),
    ("bilin_nested", None),
]


def evaluate(P, w, h):
    """Run every model; return rows (name, exact_bytes, resid_bytes, notes)."""
    rows = []
    dens = [1, 2, 4, w - 1, w, 2 * (w - 1), 256, 256 * 256]
    dens = [d for d in dens if d > 0]
    plane_u8 = P

    for name in ["const"]:
        ok, pred, params = fit_const(plane_u8)
        rows.append(model_row(name, ok, params, plane_u8, pred))

    for D in dens:
        ok, params, pred = fit_div(plane_u8, True, D, two_d=False)
        rows.append(model_row(f"div1d(D={D})", ok, params, plane_u8, pred))
    ok, params, pred = fit_div2d(plane_u8, 1)
    rows.append(model_row("div2d(D=1)", ok, params, plane_u8, pred))
    # 2D with other D values: bounded set
    for D in dens[1:]:
        ok, params, pred = fit_div2d(plane_u8, D)
        rows.append(model_row(f"div2d(D={D})", ok, params, plane_u8, pred))
    for order, scan in [(1, True), (1, False), (2, True), (2, False)]:
        ok, params, pred = fit_rec(plane_u8, order, scan)
        rows.append(model_row(f"rec{order}({'row' if scan else 'col'})", ok, params, plane_u8, pred))
    for D in [1, 2, (max(w - 1, 1))]:
        ok, params, pred = fit_quad(plane_u8, D)
        rows.append(model_row(f"quad(D={D})", ok, params, plane_u8, pred))
    ok, params, pred = fit_bilinear(plane_u8, joint=True)
    rows.append(model_row("bilin_joint", ok, params, plane_u8, pred))
    ok, params, pred = fit_bilinear(plane_u8, joint=False)
    rows.append(model_row("bilin_nested", ok, params, plane_u8, pred))
    return rows


def model_row(name, ok, params, P_true, P_pred):
    """L_model rows: exact-fit wire (params only) and residual-fit wire.
    !ok or P_pred None: only the residual-less identity applies — for a failed
    fit the residual mode is undefined, so report exact=None and resid=None."""
    idb = 1
    if not ok or P_pred is None:
        return (name, False, None, None, len(params or b""))
    exact = 32 + idb + len(params)
    res = (P_true.astype(np.int64) - P_pred.astype(np.int64)) % 256
    planar = np.concatenate([res[:, :, c].reshape(-1) for c in range(3)]).astype(np.uint8).tobytes()
    inter = res.astype(np.uint8).reshape(-1).tobytes()
    payload = min(z9(planar), z9(inter), z19(planar), z19(inter))
    resid = 32 + idb + len(params) + payload
    return (name, ok, exact, resid, len(params))


# ------------------------------------------------------------------ baseline

def cli_size(png):
    out = WORK / "t.mnf5"
    r = subprocess.run([str(CLI), "encode", str(png), "-o", str(out), "--mnf8"],
                       capture_output=True, timeout=300)
    if r.returncode != 0:
        raise RuntimeError(r.stderr.decode()[:300])
    v = subprocess.run([str(CLI), "verify", str(png), str(out)], capture_output=True, timeout=300)
    assert b"PASS" in v.stdout, v.stdout
    return out.stat().st_size


# -------------------------------------------------------------------- main

def main():
    print("building corpus...")
    corpus = build_corpus()
    print(f"corpus: {len(corpus)} files (heldout included)\n")
    print("FLOOR CHECK FIRST: exact floor-lerp home turf (Jon's 20->230)")
    total_baseline = 0
    total_best = 0
    beats = 0
    for name, png, cls, heldout in corpus:
        img = np.asarray(Image.open(png).convert("RGB"))
        h, w = img.shape[:2]
        rows = evaluate(img, w, h)
        base = cli_size(png)
        exact_rows = [(r for r in rows if r[2] is not None)]
        best_name, best_bytes = "pick", base
        for (n, ok, ex, res, npar) in rows:
            if ex is not None and ex < best_bytes:
                best_name, best_bytes = f"{n}[exact]", ex
            if res is not None and res < best_bytes:
                best_name, best_bytes = f"{n}[resid]", res
        total_baseline += base
        total_best += min(best_bytes, base)
        flag = "GEN-WIN" if best_bytes < base else "pick"
        if best_bytes < base:
            beats += 1
        ho = " HELOUT" if heldout else ""
        # top-2 exact rows for the table
        ex_rows = sorted([r for r in rows if r[2] is not None], key=lambda r: r[2])[:1]
        exs = f"{ex_rows[0][0]}={ex_rows[0][2]}" if ex_rows else "none-exact"
        print(f"{name:>18s} [{cls:>15s}]{ho:>7s}  pick={base:>7d}  exact[{exs}]  "
              f"best_resid={min((r[3] for r in rows if r[3] is not None), default=10**9):>7d}  -> {best_name}={best_bytes} {flag}")
    print(f"\nTOTAL pick={total_baseline}  with-gen={total_best}  delta={total_best - total_baseline:+d}  beats={beats}/{len(corpus)}")


if __name__ == "__main__":
    main()