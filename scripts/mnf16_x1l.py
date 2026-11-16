#!/usr/bin/env python3
"""MNF16-X1-L — lane-class conditioning coder experiment.

Authorized arms on each mix file's own shipped schedule (frozen spec):
  L0  the shipped adaptive schedule (identity anchor)
  L1  + lane children: context 20+k for difference-lane bytes (class 1),
      base key kept for value-lane/framing bytes (class 0) — children
      fresh (parent 0xFF, no propagation)
  L2  + `laneparent`: children parented to their base context and child
      updates propagate down the ancestor chain (the shipped
      hierarchical-refine pattern; base-key share semantics untouched)
  L3  L1 with the class array permuted WITHIN the file (class counts
      preserved exactly; assignment randomized; seed 17)
Class mapping = the exact X1 availability classification (class 1 =
residual byte at lane > 0; class 0 = lane-0 residual or framing byte).
Lane bytes are decoder-derivable from the record walk (X1 T0-T4 proof,
scripts/mnf16_x1.py PrefixView) and are NOT transmitted — the 'e' wire
accounting therefore excludes them. No magnitude/sign/spatial features;
no further context splits; this is the one bounded coder experiment.
"""

import json
import struct
import sys
import time
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "crates/mnf-mans/scripts"))
sys.path.insert(0, str(ROOT / "scripts"))

import mnf16_s2_bridge as s2
import mnf16_x1 as x1
from mnf13_r9a_gate import extract_stream

import mnf16_s3_coder as s3

SHUF_SEED = 17


def run(png_rel, s3log=None):
    t0 = time.perf_counter()
    backend, flags, blob, _ = extract_stream(ROOT / png_rel)
    if blob[:4] != b"MNF8" or (flags & 0x8) or backend not in (4, 5, 6):
        return {"file": png_rel, "skip": f"be{backend}/token"}
    w, h, _ts, tc = struct.unpack_from("<IIII", blob, 4)
    plen = struct.unpack_from("<I", blob, 20)[0]
    payload = bytes(blob[32:32 + plen])
    base = {4: s2.B4, 5: s2.B5, 6: s2.B6}[backend]
    stream = s3.bridge(base, payload, "p")
    N = len(stream)
    wire, pos, idx, modes = x1.walk_modes(stream, flags, w, h, tc)
    classes = np.zeros(N, np.uint8)
    classes[idx] = (pos[:, 2] > 0).astype(np.uint8)   # frozen mapping
    t_lane = time.perf_counter() - t0

    rng = np.random.default_rng(SHUF_SEED)
    cls_shuf = classes[rng.permutation(N)]

    rows = {}
    arms = {
        "L0": (base, classes),
        "L1": (base + "-lanechild", classes),
        "L2": (base + "-lanechild-laneparent", classes),
        "L3": (base + "-lanechild", cls_shuf),
    }
    t1 = time.perf_counter()
    for name, (mode, feats) in arms.items():
        out = s3.bridge_feat(mode, stream, feats.tobytes(), "a")
        nn = int.from_bytes(out[:4], "little")
        bits = np.frombuffer(out[4:4 + 8 * nn], np.float64)
        wire_b, n_e = s3.wire_len(mode, stream, feats)
        rows[name] = {"mode": mode, "L_B": round(float(bits.sum() / 8), 1),
                      "wire_B": int(wire_b), "n": int(nn)}
    t_audit = time.perf_counter() - t1
    L0, L1, L2, L3 = (rows[k]["L_B"] for k in ("L0", "L1", "L2", "L3"))
    return {"file": png_rel, "backend": int(backend), "n": N,
            "classes1": int(classes.sum()),
            "L0_B": L0, "L1_B": L1, "L2_B": L2, "L3_B": L3,
            "gain_L1_B": round(L0 - L1, 1), "gain_L2_B": round(L0 - L2, 1),
            "gain_L3_B": round(L0 - L3, 1),
            "wire0_B": rows["L0"]["wire_B"], "wire1_B": rows["L1"]["wire_B"],
            "wire2_B": rows["L2"]["wire_B"], "wire3_B": rows["L3"]["wire_B"],
            "lane_ms": round(t_lane * 1000, 1),
            "audit_ms": round(t_audit * 1000, 1)}


def main():
    paths = [a for a in sys.argv[1:] if not a.startswith("-")]
    if not paths:
        import mnf16_s1a_census as s1
        paths = [str(pp) for pp in s1.corpus()]
    rows = []
    for rel in paths:
        try:
            rows.append(run(rel))
        except Exception as e:
            rows.append({"file": rel, "skip": str(e)[:200]})
        print(json.dumps({k: v for k, v in rows[-1].items() if k != "mode"}),
              flush=True)
    ok = [r for r in rows if "skip" not in r]
    if ok:
        agg = {k: sum(r[k] for r in ok)
               for k in ("L0_B", "L1_B", "L2_B", "L3_B",
                         "gain_L1_B", "gain_L2_B", "gain_L3_B",
                         "wire0_B", "wire1_B", "wire2_B", "wire3_B")}
        print(f"== AGG == files {len(ok)}")
        print(f"L0 {agg['L0_B']:,.1f} B | L1 {agg['L1_B']:,.1f} "
              f"(gain {agg['gain_L1_B']:+,.1f}) | L2 {agg['L2_B']:,.1f} "
              f"(gain {agg['gain_L2_B']:+,.1f}) | L3 {agg['L3_B']:,.1f} "
              f"(gain {agg['gain_L3_B']:+,.1f})")
        print(f"WIRE: L0 {agg['wire0_B']:,} B | L1 {agg['wire1_B']:,} "
              f"(gain {agg['wire0_B'] - agg['wire1_B']:+,}) | "
              f"L2 {agg['wire2_B']:,} (gain {agg['wire0_B'] - agg['wire2_B']:+,}) | "
              f"L3 {agg['wire3_B']:,} (gain {agg['wire0_B'] - agg['wire3_B']:+,})")
    # identity gate: L0 must equal the recorded S3-0 corpus anchor
    okL0 = sum(r["L0_B"] for r in ok)
    print(f"IDENTITY: L0 total {okL0:,.1f} B vs S3-0 anchor 583,366.1 B "
          f"(delta {okL0 - 583366.1:+,.1f})")
    with open("/tmp/mnf16_x1l.json", "w") as fo:
        json.dump(rows, fo, indent=1)
    print("wrote /tmp/mnf16_x1l.json")


if __name__ == "__main__":
    main()
