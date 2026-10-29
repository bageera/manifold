#!/usr/bin/env python3
"""MNF10-G4 Arm-B/C isolation: identity-only vs D8 shared-template arms.

Jon's G4 directive: separate the three savings sources (template reuse /
D8 transforms / sparse residual coding) with everything else held fixed —
same template budget (one template per file on the aligned T-grid), same
profitability boundary (measured Arm-E: fires when a block's total
signed-residual magnitude < 1.1 B/px-equivalent of literal cost), same
containers (z19 and z9 both measured, smaller charged), same complete
serialization (template record + per-ref [coords, T-index, full int8
residual payload] + left-predicted complement), same decode discipline.

Savings-rate readout is REPORTED, never collapsed into a promotion score
(the metrics stay independent: bytes, time, refs).
"""
import struct
import sys
import time
import zlib
from collections import Counter
from pathlib import Path

import numpy as np
import zstandard
from PIL import Image

ROOT = Path(__file__).parent.parent.parent.parent  # repo root


def d8(b):
    return [
        b,
        np.flipud(b).copy(),
        np.fliplr(b).copy(),
        np.flip(b, (0, 1)).copy(),
        b.transpose(1, 0, 2).copy(),
        np.flipud(b).transpose(1, 0, 2).copy(),
        np.fliplr(b).transpose(1, 0, 2).copy(),
        np.flip(b, (0, 1)).transpose(1, 0, 2).copy(),
    ]


def left_res(img):
    h, w = img.shape[:2]
    a = img.astype(np.int64)
    res = np.zeros((h, w, 3), np.int64)
    res[:, 1:] = (a[:, 1:] - a[:, :-1]) % 256
    res[:, 0] = a[:, 0]
    return res % 256


def z19(b):
    return len(zstandard.ZstdCompressor(level=19, write_checksum=True).compress(b))


def z9(b):
    return len(zlib.compress(b, 9))


def arm(img, T=16, use_d8=True, boundary=1.1, template_hint=None):
    """The shared-template arm. use_d8=False = the identity-only control.
    template_hint = (row, col) of a KNOWN template (instrumented fixtures;
    real files discover by min-cost). Returns (bytes | None, (t_disc, t_ref))."""
    h, w = img.shape[:2]
    t0 = time.time()
    blocks = {}
    for by in range(0, h - T + 1, T):
        for bx in range(0, w - T + 1, T):
            blocks[(by, bx)] = img[by : by + T, bx : bx + T].astype(np.int64)
    keys = list(blocks)
    sig = {k: blocks[k].tobytes() for k in keys}
    cnt = Counter(sig.values())
    common, n = cnt.most_common(1)[0]
    if template_hint is not None and tuple(template_hint) in blocks:
        tkey = tuple(template_hint)
        template = blocks[tkey]
    elif n >= 3:
        tkey = [k for k in keys if sig[k] == common][0]
        template = blocks[tkey]
    else:
        best = None
        for cand in keys[:60]:
            tfs = [np.rot90(blocks[cand], 1), np.fliplr(blocks[cand]), blocks[cand]]
            tot = 0
            for k in keys:
                if k == cand:
                    continue
                tot += min(
                    int(np.abs(((blocks[k] - tf) % 256 + 128) % 256 - 128).sum())
                    for tf in tfs
                )
            if best is None or tot < best[0]:
                best = (tot, cand)
        tkey = best[1]
        template = blocks[tkey]
    t_discover = time.time() - t0

    t1 = time.time()
    tfs = list(d8(template)) if use_d8 else [template]
    ref_tis = {}
    for k in keys:
        if k == tkey:
            continue
        blk = blocks[k]
        bc = min(
            (int(np.abs(((blk - tb) % 256 + 128) % 256 - 128).sum()), ti)
            for ti, tb in enumerate(tfs)
        )
        if bc[0] < T * T * 3 * boundary:
            ref_tis[k] = bc
    t_refs = time.time() - t1
    if not ref_tis:
        return None, (t_discover, t_refs)

    out = bytearray()
    out.append(7)
    out += struct.pack("<HH", T, len(ref_tis) + 1)
    out += template.astype(np.uint8).tobytes()
    out += struct.pack("<2H", *tkey)
    m = np.zeros((h, w), bool)
    m[tkey[0] : tkey[0] + T, tkey[1] : tkey[1] + T] = True
    for k, (cost, ti) in ref_tis.items():
        tb = tfs[ti] if use_d8 else template
        rb = ((blocks[k] - tb) % 256 + 128) % 256 - 128
        out += struct.pack("<2HBB", k[0], k[1], ti if use_d8 else 0, 0)
        out += np.clip(rb, -127, 127).astype(np.int8).tobytes()
        m[k[0] : k[0] + T, k[1] : k[1] + T] = True
    lit = np.where(m[..., None], 0, left_res(img).astype(np.uint8))
    out += lit.tobytes()
    s = bytes(out)
    return min(32 + z19(s), 32 + z9(s)), (t_discover, t_refs)


def pick_size(path):
    import os
    import subprocess

    env = os.environ.copy()
    env["PATH"] = (
        "/opt/homebrew/opt/rustup/bin:"
        + str(Path.home() / ".rustup/toolchains/stable-aarch64-apple-darwin/bin")
        + ":"
        + env["PATH"]
    )
    env["RUSTUP_HOME"] = str(Path.home() / ".rustup")
    out = Path("/tmp/g4.mnf5")
    subprocess.run(
        [str(ROOT / "target/release/manifold"), "encode", path, "-o", str(out), "--mnf8"],
        capture_output=True, env=env, check=True,
    )
    return out.stat().st_size


def main():
    fixtures = [
        ("document", ROOT / "testdata/heldout/document_200x280.png", None),
        ("poster", ROOT / "testdata/heldout/poster_208x144.png", None),
        # instrumented capability fixture: the template is the KNOWN planted
        # block (Arm E's class — a planted fixture's honest readout holds the
        # template fixed; a blind min-cost search on a corrupted fixture is
        # noise-dominated, itself a measured finding)
        ("corrupt-d8-2pct", Path("/tmp/g3/g3_shared_d8_2pct.png"), (32, 32)),
        ("shared-d8-clean", Path("/tmp/g3/g3_shared_d8.png"), (32, 32)),
    ]
    for (name, path, hint) in fixtures:
        if not Path(path).exists():
            print(f"{name}: fixture missing ({path}) — skipped")
            continue
        img = np.asarray(Image.open(path).convert("RGB"), dtype=np.uint8)
        A = pick_size(str(path))
        Bs = [arm(img, T=T, use_d8=False, template_hint=hint) for T in (16, 32)]
        Cs = [arm(img, T=T, use_d8=True, template_hint=hint) for T in (16, 32)]
        B, tb = min((x for x in Bs if x[0]), key=lambda x: x[0], default=(None, (0, 0)))
        C, tc = min((x for x in Cs if x[0]), key=lambda x: x[0], default=(None, (0, 0)))
        bT = next((T for T, (b, _) in zip((16, 32), Bs) if b == B), 0)
        cT = next((T for T, (c, _) in zip((16, 32), Cs) if c == C), 0)
        b_show = f"{B:d} ({tb[0]+tb[1]:.2f}s)" if B else "no-fire"
        c_show = f"{C:d} ({tc[0]+tc[1]:.2f}s)" if C else "no-fire"
        d8_value = f"{B - C:+d} B" if (B and C) else "-"
        t_tot = tc[0] + tc[1]
        rate = f"{(A - C) / t_tot:.0f} B/s" if C and t_tot > 0 and A > C else "n/a"
        print(f"{name:16s} {A:>8d} {b_show:>18s} {c_show:>18s} {d8_value:>9s} {rate:>12s}"
              f"  (B@T={bT}, C@T={cT})")


if __name__ == "__main__":
    main()