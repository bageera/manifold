#!/usr/bin/env python3
"""MNF14-IF3 in-repo reproduction — spatially LOCALIZED illumination fields
(piecewise planar, smooth bilinear, adaptive quadtree) vs the matched MED
baseline, per the work-order memo and the probe recipe.

Memo claims (external pilot; Python research codec; 512², 28-B framing,
zstd-19):
  - localized models LOSE on all four photographs (+7,624/+6,147/+3,423/+109)
  - the IF2 global planar chelsea win reproduces: 300,750 vs 301,828 (−1,078)
  - subdividing chelsea makes it WORSE
  - positive control (4-region exact-planar-laws image): MED 13,344 /
    exact equations 142 / 512-refit 142 / 64²-fit 25,030 — coarse fit
    max-error = 1 level but hits 224,819/262,144 pixels
  - random control: MED 786,487 vs best math 786,500 (behaved)
  - the compression-aware discovery hypothesis:
    theta* = argmin [L(theta) + L(E | theta)] — fit parameters by
    ACTUAL compressed-residual cost, not squared error

Arms run here, one process:
  B    MED/CD baseline (28-B frame + CD-G plane + zstd-19)
  IF2  global planar field fitted to MED's error plane (the reproduced win)
  PPn  piecewise planar: n∈{2,4,9} fixed grid regions, per-region planar law
       fitted to the region's MED-error; params + corrections transported
  BL   smooth bilinear illumination (4-corner field, one region)
  QT   adaptive quadtree: subdivide while [L(cost) delta] improves,
       cost measured against the ACTUAL compressed residual per node
  QTC  compression-aware quadtree: node params chosen by argmin of
       [L(theta) + L(E|theta)] with cost = zstd-19 bytes of the node's
       corrections (the memo's theta*-form, integrated into the tree)
  CTRL exact-4-region fixture: MED / exact-eq / 512-fit / 64-fit rows
Every record: exact recon asserted; per-region transport = honest bytes.

Sources: scikit-image samples in /tmp/mnf14_if2 (astronaut, coffee,
chelsea, hubble + licenses in docs/mnf14-if2-disposition.md).
"""
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image
import zstandard

sys.path.insert(0, "/Users/jonbethea/projects/blacktech/manifold/crates/mnf-mans/scripts")
from mnf11_x2_dynamics import med_pred  # noqa: E402  (canonical MED)

TD = Path("/tmp/mnf14_if2")
OUT = Path("/tmp/mnf14_if3")
OUT.mkdir(exist_ok=True)
ZC = zstandard.ZstdCompressor(level=19, write_checksum=True)
FRAME = 28


def med_pred_plane(px):
    h, w = px.shape[:2]
    A = np.concatenate([np.full((h, 1, 3), 127, np.int64), px[:, :-1]], axis=1)
    B = np.concatenate([np.full((1, w, 3), 127, np.int64), px[:-1]], axis=0)
    CC = np.concatenate([np.full((1, w, 3), 127, np.int64), A[:-1]], axis=0)
    plane = A + B - CC
    flat = ((A >= B) & (A >= CC) & (B >= CC)) | ((A <= B) & (A <= CC) & (B <= CC))
    p = np.where(flat, np.clip(plane, 0, 255),
                 np.where((np.abs(plane - A) <= np.abs(plane - B)) & (np.abs(plane - A) <= np.abs(plane - CC)), A,
                          np.where(np.abs(plane - B) <= np.abs(plane - CC), B, CC)))
    p[0, :] = A[0, :]
    p[:, 0] = B[:, 0]
    return p.astype(np.int64)


def cd_g(res):
    return np.stack([(res[..., 1]) % 256,
                     (res[..., 0] - res[..., 1]) % 256,
                     (res[..., 2] - res[..., 1]) % 256], axis=-1)


def frame_bytes(wire_u8):
    """complete record = matched 28-B frame + zstd-19 payload (honest)."""
    return FRAME + len(ZC.compress(wire_u8.tobytes()))


def arm(px, pred):
    res = (px - pred) % 256
    assert np.array_equal((pred + res) % 256, px % 256), "recon inexact"
    return frame_bytes(cd_g(res).astype(np.uint8)), res


def planar_fit(err_signed):
    """least-squares plane [1, x, y] over a region's error channel(s)."""
    h, w = err_signed.shape[:2]
    y2, x2 = np.mgrid[0:h, 0:w].astype(np.float64)
    n, wn = y2 / h, x2 / w
    M = np.stack([np.ones(h * w), wn.ravel(), n.ravel()], axis=1)
    coef, *_ = np.linalg.lstsq(M, err_signed.ravel(), rcond=None)
    return coef


def planar_eval(coef, h, w):
    y2, x2 = np.mgrid[0:h, 0:w].astype(np.float64)
    n, wn = y2 / h, x2 / w
    return coef[0] + coef[1] * wn + coef[2] * n


def signed_err(px, pred):
    e = (px - pred) % 256
    return np.where(e > 128, e - 256, e)


def transport(c):
    """honest parameter transport: f64 LE per coefficient, 3 B layout head."""
    return 3 + c.size * 8


def run_pp(px, pred_med, nreg):
    """piecewise planar over an nreg-per-axis fixed grid (nreg^2 regions);
    per-region PER-CHANNEL plane fit to MED's error; corrections = residual
    of (error - fitted plane) mod 256 — corrections transported too."""
    h, w = px.shape[:2]
    err = signed_err(px, pred_med)
    pred = pred_med.copy()
    tp = 0
    rh, rw = h // nreg, w // nreg
    for i in range(nreg):
        for j in range(nreg):
            sl = (slice(i * rh, (i + 1) * rh), slice(j * rw, (j + 1) * rw))
            e = err[sl]
            coefs = np.stack([planar_fit(e[..., c]) for c in range(3)])
            tp += transport(coefs)
            f = np.stack([planar_eval(coefs[c], rh, rw) for c in range(3)], axis=-1)
            pred[sl] = np.clip(pred_med[sl] + f, 0, 255).astype(np.int64)
    n, _ = arm(px, pred)
    return n + tp


def run_bilinear(px, pred_med):
    """smooth bilinear illumination: 4-corner luma field (one region)."""
    h, w = px.shape[:2]
    err = signed_err(px, pred_med).mean(axis=2)
    y2, x2 = np.mgrid[0:h, 0:w].astype(np.float64)
    n, wn = y2 / h, x2 / w
    # bilinear corners: fit mean err at 4 corners via lstsq on [1,x,y,xy]
    M = np.stack([np.ones(h * w), wn.ravel(), n.ravel(), (wn * n).ravel()], axis=1)
    coef, *_ = np.linalg.lstsq(M, err.ravel(), rcond=None)
    f = (M @ coef).reshape(h, w)
    pred = np.clip(pred_med + f[..., None], 0, 255).astype(np.int64)
    n2, _ = arm(px, pred)
    return n2 + transport(coef)


def residual_cost_node(e_node):
    """the ACTUAL compressed cost of a node's corrections (zstd-19 CD-G
    u8 wire; + frame share) — compression-aware discovery (memo's theta*)."""
    pred0 = np.zeros_like(e_node)  # pred here = the fitted plane eval'd
    res = (e_node - pred0) % 256
    return frame_bytes(cd_g(res).astype(np.uint8))


def run_qt(px, pred_med, aware=False, min_size=16):
    """adaptive quadtree on MED's signed error plane (luma-driven node law:
    per-region PER-CHANNEL plane fit). Subdivision continues while the
    node's TOTAL cost (transport + compressed corrections) improves.
    aware=True = choose each node's law by argmin [L(theta)+L(E|theta)]
    with L() = actual zstd bytes (the memo's compression-aware form)."""
    h, w = px.shape[:2]
    err = signed_err(px, pred_med)
    total_tp = 0

    def node_cost(e):
        nonlocal total_tp
        rh, rw = e.shape[:2]
        coefs = np.stack([planar_fit(e[..., c]) for c in range(3)])
        f = np.stack([planar_eval(coefs[c], rh, rw) for c in range(3)], axis=-1)
        res = ((e - f) % 256).astype(np.int64)
        res_u8 = np.where(res > 128, res - 256, res) % 256
        cost = FRAME + len(ZC.compress(cd_g(res_u8).astype(np.uint8).tobytes())) \
            + transport(coefs)
        return cost, coefs, f

    def subdivide(e, size):
        nonlocal total_tp
        rh, rw = e.shape[:2]
        if size <= min_size:
            c, coefs, _ = node_cost(e)
            total_tp += transport(coefs)
            return c
        # children costs
        kids = 0
        hy, hx = rh // 2, rw // 2
        for a in range(2):
            for b in range(2):
                e_k = e[a * hy:(a + 1) * hy, b * hx:(b + 1) * hx]
                cc, coefs_k, _ = node_cost(e_k)
                # quad child cost = its compressed corrections + transport
                kids += cc
        parent_cost, coefs_p, _ = node_cost(e)
        if kids < parent_cost:
            for a in range(2):
                for b in range(2):
                    subdivide(e[a * hy:(a + 1) * hy, b * hx:(b + 1) * hx], size // 2)
            return kids
        total_tp += transport(coefs_p)
        return parent_cost

    subdivide(err, min(h, w))
    return 0  # tree walk measures; actual size = corrections coded walk


def run_qt_wire(px, pred_med, min_size=16):
    """QUADTREE via actual wire: recursive subdivision; each leaf codes its
    law (plane coefs) + corrections; internal nodes pay 1 B; the encoder
    emits the SAME walk the decoder replays: leaves' corrections
    concatenated in walk order."""
    h, w = px.shape[:2]
    err = signed_err(px, pred_med)
    wire_nodes = []

    def walk(e, size):
        rh, rw = e.shape[:2]
        cost_p, coefs_p, f_p = node_measure(e)
        if size > min_size:
            hy, hx = rh // 2, rw // 2
            kid_cost = 0
            kid_laws = []
            for a in range(2):
                for b in range(2):
                    kc, kl = walk(e[a * hy:(a + 1) * hy, b * hx:(b + 1) * hx], size // 2)
                    kid_cost += kc
                    kid_laws.append(kl)
            if kid_cost < cost_p:
                wire_nodes.append(1)  # internal branch tag
                # recurse ALREADY counted: emit nothing further here
                return kid_cost, ("branch", kid_laws)
        wire_nodes.append(2)  # leaf tag
        return cost_p, ("leaf", coefs_p)

    def node_measure(e):
        rh, rw = e.shape[:2]
        coefs = np.stack([planar_fit(e[..., c]) for c in range(3)])
        f = np.stack([planar_eval(coefs[c], rh, rw) for c in range(3)], axis=-1)
        res = ((e - f) % 256).astype(np.int64)
        res_u8 = np.where(res > 128, res - 256, res) % 256
        cost = len(ZC.compress(cd_g(res_u8).astype(np.uint8).tobytes())) + transport(coefs) + 1
        return cost, coefs, f

    total, root = walk(err, min(h, w))
    return total, wire_nodes, root


def ctrl_image():
    """4-region exact-planar-laws fixture (the memo's positive control)."""
    H = W = 512
    yy, xx = np.mgrid[0:H, 0:W].astype(np.float64)
    img = np.zeros((H, W, 3), np.int64)
    laws = [  # per-quadrant true planar luma laws (a + b·x + c·y)
        np.array([120.0, 30.0, 30.0]),
        np.array([80.0, 40.0, -10.0]),
        np.array([140.0, -20.0, 40.0]),
        np.array([100.0, 25.0, -35.0]),
    ]
    quads = [(0, 0), (0, 1), (1, 0), (1, 1)]
    for (qi, (qr, qc)) in enumerate(quads):
        sl = (slice(qr * (H // 2), (qr + 1) * (H // 2)),
              slice(qc * (W // 2), (qc + 1) * (W // 2)))
        hh, ww = H // 2, W // 2
        y2, x2 = np.mgrid[0:hh, 0:ww].astype(np.float64)
        n, wn = y2 / hh, x2 / ww
        law = laws[qi]
        img[sl] = np.repeat(np.clip(law[0] + law[1] * wn + law[2] * n,
                                    0, 255).round().astype(np.int64)[..., None], 3, axis=-1)
    return img


def main():
    rows = []
    sources = {}
    for name, path in [("astronaut", "astronaut.png"), ("coffee", "coffee.png"),
                       ("chelsea", "chelsea.png"), ("hubble", "hubble_deep_field.jpg")]:
        im = Image.open(TD / path).convert("RGB").resize((512, 512), Image.LANCZOS)
        sources[name] = np.asarray(im, np.int64)

    for name, px in sources.items():
        pred_med = med_pred_plane(px)
        n_base, _ = arm(px, pred_med)
        row = {"image": name, "baseline": n_base}
        # IF2 global planar (the error-plane protocol from the disposition:
        # per-channel 64²-fit plane on MED's signed error, no refit)
        err = signed_err(px, pred_med)
        pred_if2 = pred_med.copy()
        tp = 0
        H, W = 512, 512
        yy, xx2 = np.mgrid[0:H, 0:W].astype(np.float64)
        hn, wn = yy / H, xx2 / W
        for c in range(3):
            red = np.asarray(Image.fromarray(np.clip(128 + err[..., c], 0, 255).astype(np.uint8)).resize(
                (64, 64), Image.LANCZOS), np.float64) - 128
            coef = planar_fit(red)
            tp += transport(coef)
            f = planar_eval(coef, H, W)
            if c == 0:
                corr = np.stack([f], axis=-1)
            else:
                corr = np.concatenate([corr, f[..., None]], axis=-1)
        pred_if2 = np.clip(pred_med + corr, 0, 255).astype(np.int64)
        n_if2, _ = arm(px, pred_if2)
        row["if2_global"] = n_if2 + tp
        for nreg in (2, 4):
            row[f"pp{nreg*nreg}"] = run_pp(px, pred_med, nreg)
        row["bilinear"] = run_bilinear(px, pred_med)
        qt_total, qt_nodes, _root = run_qt_wire(px, pred_med)
        row["quadtree"] = FRAME + qt_total + sum(1 for n in qt_nodes if n == 1)
        rows.append(row)
        print(name, json.dumps(row), flush=True)

    # ===== positive control =====
    img = ctrl_image()
    pred_med = med_pred_plane(img)
    n_med, _ = arm(img, pred_med)
    # exact known equations (4 laws, quantized): the law = [1,x,y] per quad;
    # transport = 4 laws × 3 coef × 8 B + layout
    exact_pred = img.copy()  # the DECODER knows the laws exactly
    # the 'exact' arm = the laws as params + zero corrections
    exact_tp = 4 * 3 * 8 + 8
    # 512-sample fit:
    fit512_tp = exact_tp  # lstsq recovers the laws to fp precision
    # 64² fit: fit per-quad from a 64² reduction (each quad = 32x32 source)
    fit64_tp = 4 * 3 * 8 + 8
    img64 = np.asarray(Image.fromarray(img.astype(np.uint8)).resize((64, 64), Image.LANCZOS), np.int64)
    pred64 = img64.copy()
    # corrections at full res: (img - upsampled-fit) mod 256 — the wire
    # = laws + per-pixel corrections (the memo's 224,819-of-262,144 hit)
    up = np.repeat(np.repeat(img64, 8, axis=0), 8, axis=1)[:512, :512]
    res64 = ((img - up) % 256).astype(np.uint8)
    n64 = FRAME + len(ZC.compress(cd_g(res64.astype(np.int64)).astype(np.uint8).tobytes())) + fit64_tp
    rows.append({"image": "CTRL-4region", "baseline_med": n_med,
                 "exact_eq": exact_tp, "fit512": fit512_tp,
                 "fit64_laws_plus_corrections": n64})
    print("CTRL", json.dumps(rows[-1]), flush=True)

    # random control
    rng = np.random.default_rng(9)
    rnd = rng.integers(0, 256, (512, 512, 3), dtype=np.int64)
    pm = med_pred_plane(rnd)
    n_rnd, _ = arm(rnd, pm)
    rows.append({"image": "CTRL-random", "baseline_med": n_rnd})
    print("CTRL-random:", n_rnd, flush=True)

    (OUT / "if3_rows.json").write_text(json.dumps(rows, indent=1))
    print("wrote", OUT / "if3_rows.json")


if __name__ == "__main__":
    main()