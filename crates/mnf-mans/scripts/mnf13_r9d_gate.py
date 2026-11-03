#!/usr/bin/env python3
"""MNF13-R9-D — ctx-15/7 conditional refinement on the R9-A streams.

The refinement (only where population supports it): heavy-band keys
{7, 15} (prev bucket 7 = the >=97-pixel population) split into a child
per prev3's coarse band; the mid-flight merged design is carried as its
own named arm for provenance. Modes (the r9_audit bridge):

  C0_shipped     frozen backend 5 (anchor; byte-identity asserted)
  D1M_merged     keys 7+15 share ONE child pair on prev3's band
                 (registering the pre-fix mid-flight design verbatim)
  D1_refineA     D1: distinct children per heavy key (7->16+b3, 15->18+b3)
  D3a_refine15   D3: key 15 refined only (attribute separately)
  D3b_refine7    D3: key 7 refined only (attribute separately)
  D2_refA_w12s   D2: refineA + C2 warm2p12 + `share` (base heavy key
                 stays LIVE for its children to mix into while cold)
  C2_w2p12       C2 comparator re-run (the R9-C winner)

Per file: complete entropy-wire bytes; roundtrip asserted in-bridge;
C0 == shipped payload byte-identical; prequential audit attribution
split ctx7 / ctx15 / elsewhere (elsewhere == 0 asserted on no-warm
refine arms). Streams are extracted once through the committed anchor
path and cached under /tmp/r9d_streams (extract_stream asserts its
anchors on first extraction; cached bytes are the asserted bytes).

Usage:
  mnf13_r9d_gate.py              # the registered D-arms run
  mnf13_r9d_gate.py <modes...>   # composed run (D4 follow-up)
  mnf13_r9d_gate.py --static     # only the static pre-registration
"""
import json
import struct
import subprocess
import sys
from pathlib import Path

import numpy as np

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")
sys.path.insert(0, str(ROOT / "crates/mnf-mans/scripts"))
from mnf13_r9a_gate import extract_stream, bucket  # noqa: E402

BIN = ROOT / "target/release/examples/r9_audit"
C0 = "mix32768_1048576-lr15-inc32-ctx16"
D_ARMS = [
    ("D1M_merged", C0 + "-refineM"),
    ("D1_refineA", C0 + "-refineA"),
    ("D3a_refine15", C0 + "-refine15"),
    ("D3b_refine7", C0 + "-refine7"),
    ("D2_refA_w12s", C0 + "-refineA-warm2p12-share"),
    ("C2_w2p12", C0 + "-warm2p12"),
]
ARMS = [("C0_shipped", C0)] + D_ARMS

# `--static` runs ONLY the omniscient static-floor diagnostic (the
# pre-registration: the conditional-information gain the refinement can
# at best realize); `<mode tokens...>` replaces the registered D-arms
# with explicit refinements (the D4 composition after run 1).
STATIC_ONLY = "--static" in sys.argv
if STATIC_ONLY:
    ARMS = None
elif len(sys.argv) > 1:
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    ARMS = [("C0_shipped", C0)] + [(f"comp{m}", C0 + m) for m in args]

FILES = [
    ("document_200x280.png", ROOT / "testdata/heldout/document_200x280.png"),
    ("screenshot_640x400.png", ROOT / "testdata/screenshot_640x400.png"),
] + [(p.name, p) for p in sorted((ROOT / "testdata").glob("**/*.png"))
     if p.name not in ("document_200x280.png", "screenshot_640x400.png",
                       "gradient_320x240.png", "blob_224x160.png",
                       "blocks_192x128.png", "plaid_192.png",
                       "poster_208x144.png", "noise_256x256.png",
                       "repeat_256x256.png", "solid_256x256.png")]


def run_bridge(stream, mode, cmd):
    r = subprocess.run([str(BIN), cmd, mode],
                       input=f"{len(stream)} {mode}\n".encode() + stream,
                       capture_output=True, timeout=600)
    assert r.returncode == 0, (mode, cmd, r.stderr[:200])
    return r.stdout


def load_stream(name, png):
    """Backend-5 stream via the committed anchor path, cached once."""
    cpath = Path("/tmp/r9d_streams") / f"{name}.bin"
    cpath.parent.mkdir(exist_ok=True)
    if cpath.exists():
        return cpath.read_bytes()
    be, fl, blob, stream = extract_stream(png)
    if be != 5 or stream is None:
        return None
    cpath.write_bytes(stream)
    return stream


def base_keys(stream):
    """Base backend-5 key stream + band3 vector, in the mix stream's
    coded order (the same causal schedule the bridge recomputes)."""
    bs = np.frombuffer(stream, np.uint8).astype(np.intp)
    ks = np.empty(len(bs), np.intp)
    b3 = np.empty(len(bs), np.intp)
    prev = p2 = p3 = 0
    for i, b in enumerate(bs):
        ks[i] = bucket(prev) | ((bucket(p2) >= 3) << 3)
        b3[i] = 1 if bucket(p3) >= 3 else 0
        p3, p2, prev = p2, prev, b
    return ks, b3


def refined_keys_variant(ks, b3, variant):
    """Mirror of r9_audit's key_of under the given refinement scope."""
    k7, k15 = ks == 7, ks == 15
    kr = ks.copy()
    if variant == "refineM":
        kr[k7 | k15] = 16 + b3[k7 | k15]
    elif variant == "refine7":
        kr[k7] = 16 + b3[k7]
    elif variant == "refine15":
        kr[k15] = 18 + b3[k15]
    elif variant == "refineA":
        kr[k7] = 16 + b3[k7]
        kr[k15] = 18 + b3[k15]
    return kr


def read_audit(stdout, n):
    assert len(stdout) == 4 + n * 8, "audit arity"
    return np.frombuffer(stdout[4:], np.float64).copy()


def run_static():
    """Omniscient static floor per refinement scope (DIAGNOSTIC only —
    whole-file visibility; the verdict rests on the causal run)."""
    rows = []
    for name, png in FILES:
        stream = load_stream(name, png)
        if stream is None:
            continue
        ks, b3 = base_keys(stream)
        ss = np.frombuffer(stream, np.uint8).astype(np.intp)
        cnt_base = np.zeros((16, 256), np.int64)
        np.add.at(cnt_base, (ks, ss), 1)
        p_base = cnt_base / np.maximum(cnt_base.sum(axis=1, keepdims=True), 1)
        cb = -np.log2(np.maximum(p_base[ks, ss], 1e-12)) / 8.0
        out = {"file": name, "n": len(stream)}
        for variant, kcount in (("refineA", 20), ("refineM", 18),
                                ("refine7", 20), ("refine15", 20)):
            kr = refined_keys_variant(ks, b3, variant)
            cnt = np.zeros((kcount, 256), np.int64)
            np.add.at(cnt, (kr, ss), 1)
            p_r = cnt / np.maximum(cnt.sum(axis=1, keepdims=True), 1)
            cr = -np.log2(np.maximum(p_r[kr, ss], 1e-12)) / 8.0
            heavy = (ks == 7) | (ks == 15)
            if variant == "refine7":
                heavy = ks == 7
            elif variant == "refine15":
                heavy = ks == 15
            out[f"d_{variant}"] = round(float((cr - cb)[heavy].sum() * 8), 1)
        k7m, k15m = ks == 7, ks == 15
        out["heavy_share"] = round(float((k7m | k15m).mean()), 4)
        out["pop_k7"] = [int((k7m & (b3 == 0)).sum()), int((k7m & (b3 == 1)).sum())]
        out["pop_k15"] = [int((k15m & (b3 == 0)).sum()), int((k15m & (b3 == 1)).sum())]
        rows.append(out)
        print(f"{name:32} heavy {out['heavy_share']:4.0%} "
              f"popK7 {out['pop_k7']} popK15 {out['pop_k15']} "
              f"d_refineA {out['d_refineA']:8.1f} d_refine15 {out['d_refine15']:8.1f} "
              f"d_refine7 {out['d_refine7']:8.1f} d_refineM {out['d_refineM']:8.1f}",
              flush=True)
    print("\nTOTAL static deltas (positive = refinement loses bits):")
    for k in ("d_refineA", "d_refine15", "d_refine7", "d_refineM"):
        print(f"  {k:12} {round(sum(r.get(k, 0) for r in rows), 1):10.1f}")
    Path("/tmp/r9d_static.json").write_text(json.dumps(rows, indent=1))
    print("-> /tmp/r9d_static.json")


def main():
    if STATIC_ONLY:
        run_static()
        return
    rows, attrib_rows = [], []
    tot = {a: 0 for a, _ in ARMS}
    for name, png in FILES:
        stream = load_stream(name, png)
        if stream is None:
            continue
        row = {"file": name, "n": len(stream)}
        for arm, mode in ARMS:
            out = run_bridge(stream, mode, "e")
            row[arm] = len(out) - 4
            if arm == "C0_shipped":
                # anchor: extracted recompute == shipped payload bytes
                be, fl, blob, s = extract_stream(png)
                plen = struct.unpack_from("<I", blob, 20)[0]
                assert s == stream and be == 5, (name, "stream drift")
                assert out == blob[32:32 + plen], (name, "anchor fail")
                row["anchor_ok"] = True
        for arm, _ in ARMS:
            tot[arm] += row[arm]
        rows.append(row)
        print(f"{name:32} n={len(stream):8} " +
              " ".join(f"{a[:13]}={row[a]}" for a, _ in ARMS[1:]), flush=True)
        # --- attribution: prequential bits per arm, split by base key ---
        ks, b3 = base_keys(stream)
        k7, k15 = ks == 7, ks == 15
        heavy = k7 | k15
        arow = {"file": name, "n_children_k7": [int((k7 & (b3 == 0)).sum()),
                                                int((k7 & (b3 == 1)).sum())],
                "n_children_k15": [int((k15 & (b3 == 0)).sum()),
                                   int((k15 & (b3 == 1)).sum())]}
        c0_bits = None  # set by the anchor arm below; every delta needs it
        for arm, mode in ARMS:
            bits = read_audit(run_bridge(stream, mode, "a"), len(stream))
            L = float(bits.sum())
            if arm == "C0_shipped":
                c0_bits = bits
                arow["L_prequential_C0"] = round(L, 2)
                continue
            d = bits - c0_bits
            split = {"ctx7_B": round(float(d[k7].sum()) / 8.0, 2),
                     "ctx15_B": round(float(d[k15].sum()) / 8.0, 2),
                     "elsewhere_B": round(float(d[~heavy].sum()) / 8.0, 2)}
            assert abs(split["ctx7_B"] + split["ctx15_B"] + split["elsewhere_B"]
                       - round((L - float(c0_bits.sum())) / 8.0, 2)) < 1.0, \
                "attribution does not close"
            arow[arm] = {"L_prequential": round(L, 2), "split": split}
            # no-warm refine arms must be exact there: non-heavy contexts
            # untouched (same keys, never-stepped models)
            if any(mode.endswith(s) for s in ("refineM", "refineA", "refine15", "refine7")):
                assert abs(split["elsewhere_B"]) == 0.0, (name, arm, split)
        attrib_rows.append(arow)
        print(f"{' ':32} attrib " +
              " ".join(f"{a[:13]}={arow[a]['split']}" for a, _ in ARMS[1:]),
              flush=True)
    base = tot["C0_shipped"]
    suffix = "_run1" if len(sys.argv) <= 1 else "_comp"
    print(f"\n== {len(rows)} files, C0 anchored byte-identical everywhere ==")
    for arm, _ in ARMS[1:]:
        d = tot[arm] - base
        wins = sum(1 for r in rows if r[arm] < r["C0_shipped"])
        losses = sum(1 for r in rows if r[arm] > r["C0_shipped"])
        print(f"  {arm:14} total {tot[arm]:9} B  ({d:+5})  wins {wins:2}  losses {losses:2}")
    Path(f"/tmp/r9d_rows{suffix}.json").write_text(json.dumps(rows, indent=1))
    Path(f"/tmp/r9d_attrib{suffix}.json").write_text(json.dumps(attrib_rows, indent=1))
    print(f"rows -> /tmp/r9d_rows{suffix}.json, attribution -> /tmp/r9d_attrib{suffix}.json")


if __name__ == "__main__":
    main()