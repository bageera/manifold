#!/usr/bin/env python3
"""MNF10-G4 revised acceptance gate (the review's correction list).

Baseline = the RELEASED 1.10.0 CLI's --mnf8 complete output (the real
portfolio, incl. tokens/near/branch predictors + the MNF9 arm) - not an
internal whole-stream proxy. Arm A (4-stream internal control) retained
for diagnosis only.

Shared-model arms: identity+sparse and D8+sparse (the review's missing
comparison), one data-derived template per file on the aligned grid, the
same frozen budget/boundary, complete serialized cost: template record +
per-ref [coords, T-id, FULL int8 residual payload] + left-predicted
complement; container = min(z19, z9).

Corpus: guard/hero sprite frames (the natural sprite-class fixtures),
document, poster, screenshot (mixed-content control), gradient/solid
(known owned by other arms - negative control).

Encoding-time accounting per arm; savings-rate reported independently,
never a promotion score.
"""
import os
import struct
import subprocess
import time
import zlib
from collections import Counter
from pathlib import Path

import numpy as np
from PIL import Image

try:
    import zstandard
except ImportError:
    zstandard = None

ROOT = Path(__file__).resolve().parent.parent.parent.parent


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


def left_res_full(img):
    h, w = img.shape[:2]
    a = img.astype(np.int64)
    res = np.zeros((h, w, 3), np.int64)
    res[:, 1:] = (a[:, 1:] - a[:, :-1]) % 256
    res[:, 0] = a[:, 0]
    return res % 256


def container(s):
    a = len(zlib.compress(s, 9))
    if zstandard is not None:
        a = min(a, len(zstandard.ZstdCompressor(level=19, write_checksum=True).compress(s)))
    return a


def cli_write(path):
    env = os.environ.copy()
    env["PATH"] = ("/opt/homebrew/opt/rustup/bin:"
                   + str(Path.home() / ".rustup/toolchains/stable-aarch64-apple-darwin/bin")
                   + ":" + env["PATH"])
    env["RUSTUP_HOME"] = str(Path.home() / ".rustup")
    out = Path("/tmp/g4rev.mnf5")
    t0 = time.time()
    subprocess.run([str(ROOT / "target/release/manifold"), "encode", str(path),
                    "-o", str(out), "--mnf8"], capture_output=True, env=env, check=True)
    dt = time.time() - t0
    v = subprocess.run([str(ROOT / "target/release/manifold"), "verify", str(path), str(out)],
                       capture_output=True, env=env)
    assert "PASS" in v.stdout.decode("latin1"), "CLI baseline must verify"
    return out.stat().st_size, dt


def shared_arm(img, T=16, use_d8=False, boundary=1.1, hint=None):
    """Identity-or-D8 + sparse: (bytes | None), (t_discover, t_refs), nrefs.
    Same budget in both arms; the toggled axis = the transform table."""
    h, w = img.shape[:2]
    t0 = time.time()
    blocks = {}
    for by in range(0, h - T + 1, T):
        for bx in range(0, w - T + 1, T):
            blocks[(by, bx)] = img[by:by + T, bx:bx + T].astype(np.int64)
    keys = list(blocks)
    sig = {k: blocks[k].tobytes() for k in keys}
    cnt = Counter(sig.values())
    common, n = cnt.most_common(1)[0]
    if hint is not None and tuple(hint) in blocks:
        tkey = tuple(hint)
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
                tot += min(int(np.abs(((blocks[k] - tf) % 256 + 128) % 256 - 128).sum())
                           for tf in tfs)
            if best is None or tot < best[0]:
                best = (tot, cand)
        tkey = best[1]
        template = blocks[tkey]
    t_dis = time.time() - t0
    t1 = time.time()
    tfs = list(d8(template)) if use_d8 else [template]
    ref_tis = {}
    for k in keys:
        if k == tkey:
            continue
        blk = blocks[k]
        bc = min((int(np.abs(((blk - tb) % 256 + 128) % 256 - 128).sum()), ti)
                 for ti, tb in enumerate(tfs))
        if bc[0] < T * T * 3 * boundary:
            ref_tis[k] = bc
    t_ref = time.time() - t1
    if not ref_tis:
        return None, (t_dis, t_ref), 0
    out = bytearray()
    out.append(7)
    out += struct.pack("<HH", T, len(ref_tis) + 1)
    out += template.astype(np.uint8).tobytes()
    out += struct.pack("<2H", *tkey)
    m = np.zeros((h, w), bool)
    m[tkey[0]:tkey[0] + T, tkey[1]:tkey[1] + T] = True
    for k, (cost, ti) in ref_tis.items():
        tb = tfs[ti]
        rb = ((blocks[k] - tb) % 256 + 128) % 256 - 128
        out += struct.pack("<2HB", k[0], k[1], ti if use_d8 else 0)
        out += np.clip(rb, -127, 127).astype(np.int8).tobytes()
        m[k[0]:k[0] + T, k[1]:k[1] + T] = True
    out += np.where(m[..., None], 0, left_res_full(img).astype(np.uint8)).tobytes()
    s = bytes(out)
    return 32 + container(s), (t_dis, t_ref), len(ref_tis)


def main():
    zan = Path.home() / "projects/nocturnal/zanset/zanset/art/sprites"
    fixtures = []
    if (zan / "guard_t1.png").exists():
        f4 = [np.asarray(Image.open(zan / f"guard_t{i}.png").convert("RGB"))
              for i in range(1, 5)]
        strip = np.concatenate(f4, axis=1)
        p = Path("/tmp/g4_guard_strip.png")
        Image.fromarray(strip, "RGB").save(p)
        fixtures.append(("guard-strip", p))
        h0 = np.asarray(Image.open(zan / "hero_idle.png").convert("RGB"))
        h1 = np.asarray(Image.open(zan / "hero_run.png").convert("RGB"))
        if np.array_equal(h0, h1):
            p2 = Path("/tmp/g4_hero_pair.png")
            Image.fromarray(np.concatenate([h0, h1], axis=1), "RGB").save(p2)
            fixtures.append(("hero-pair-identical", p2))
    fixtures += [
        ("document", ROOT / "testdata/heldout/document_200x280.png"),
        ("poster", ROOT / "testdata/heldout/poster_208x144.png"),
        ("screenshot", ROOT / "testdata/screenshot_640x400.png"),
        ("gradient", ROOT / "testdata/gradient_320x240.png"),
        ("solid", ROOT / "testdata/solid_256x256.png"),
    ]
    hdr = (
        f"{'fixture':22s} {'A=CLI1.10':>10s} {'id+sparse':>20s} "
        f"{'D8+sparse':>20s} {'geom-value':>10s} {'rate(id)':>10s}"
    )
    print(hdr)
    for name, path in fixtures:
        if not path.exists():
            print(f"{name}: missing")
            continue
        img = np.asarray(Image.open(path).convert("RGB"), dtype=np.uint8)
        (A, tA) = cli_write(path)
        rows = []
        for use_d8 in (False, True):
            best = None
            for T in (16, 32):
                sz, tt, nr = shared_arm(img, T=T, use_d8=use_d8)
                if sz and (best is None or sz < best[0]):
                    best = (sz, sum(tt), nr, T)
            rows.append(best)
        bB, tB, nB, TB = rows[0] if rows[0] else (A, tA, 0, 16)
        bC, tC, nC, TC = rows[1] if rows[1] else (A, tA, 0, 16)
        geom = (bB - bC) if (rows[0] and rows[1]) else None
        showB = f"{bB} (n={nB},T={TB})" if rows[0] else "no-fire"
        showC = f"{bC} (n={nC},T={TC})" if rows[1] else "no-fire"
        gshow = f"{geom:+d} B" if geom is not None else "n/a"
        rate = (f"{(A - bB) / max(tB, 1e-9):.0f} B/s"
                if rows[0] and A > bB and tB > 0 else "n/a")
        print(f"{name:22s} {A:>10d} {showB:>20s} {showC:>20s} {gshow:>10s} {rate:>10s}")


if __name__ == "__main__":
    main()