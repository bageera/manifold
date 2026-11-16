#!/usr/bin/env python3
"""MNF16-T1 step 3 - the multi-arm runner on the frozen production trace.

Compares multiple probability-model arms against the SAME frozen stream
without a single encoder invocation: the trace npz supplies the stream,
the shipped-schedule baseline costs (L0 = stored 'a' bits), positions
and the lane-class array. Candidate arms each carry their OWN evolving
model state (rust audit calls on the stored stream - no shared
adaptation). Output = the four-level comparison report
(prequential model bytes, entropy-wire bytes incl framing, controls),
JSON to disk.

Usage:
  python scripts/mnf16_t1_runner.py                      # all trace files
  python scripts/mnf16_t1_runner.py --screening          # T2 subset
  python scripts/mnf16_t1_runner.py <file> [<file>...]   # subset

Default arms = the X1-L set (L0/L1/L2/L3 per docs/mnf16-x1l-record.md):
the frozen consumer contract says the runner must reproduce those
recorded numbers from the trace alone - reuse may not change
measurements.
"""

import hashlib
import json
import sys
import time
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

import mnf16_s2_bridge as s2
import mnf16_t1_contract as tc
import mnf16_t1_trace as tt
import mnf16_s3_coder as s3


def arms_for(meta, classes, seed=17):
    """Frozen X1-L arm set (docs/mnf16-x1l-record.md)."""
    base = {4: s2.B4, 5: s2.B5, 6: s2.B6}[meta["backend"]]
    cls_shuf = classes[np.random.default_rng(seed).permutation(len(classes))]
    return [
        ("L0", base, None),
        ("L1", base + "-lanechild", classes),
        ("L2", base + "-lanechild-laneparent", classes),
        ("L3", base + "-lanechild", cls_shuf),
    ]


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("-")]
    screening = "--screening" in sys.argv
    lock = json.loads(tt.LOCK.read_text())
    files = [m for m in lock["files"] if m["file"].split("__")[0] in
             [f.split("__")[0] for f in [m["file"]]]]          # placeholder
    files = lock["files"]
    if screening:
        scr = json.loads((ROOT / "research" / "manifests" / "screening.lock").read_text())
        keep = set(scr["files"])
        files = [m for m in files if m["file"] in keep]
    if args:
        keep = set(args)
        files = [m for m in files if m["file"] in keep]
    rows = []
    for m in files:
        t0 = time.perf_counter()
        tr = tc.load_trace(m["file"])
        N = m["n"]
        base_bits = float(tr["bits"].sum()) / 8.0     # stored baseline: L0
        classes = tr["classes"]
        stream_b = tr["stream"].tobytes()
        body_sha = hashlib.sha256(stream_b).hexdigest()[:16]
        cls_sha = hashlib.sha256(classes.tobytes()).hexdigest()[:16]
        geom = (m["w"], m["h"], m["tile_count"], m["flags"])
        row = {"file": m["file"], "backend": m["backend"], "n": N,
               "body_sha": body_sha, "classes_sha": cls_sha, "geom": geom,
               "L0_B": round(base_bits, 1), "arms": {}}
        for name, mode, feats in arms_for(m, classes):
            feats_b = classes.tobytes() if feats is None else feats.tobytes()
            out = s3.bridge_feat(mode, stream_b, feats_b, "a")
            nn = int.from_bytes(out[:4], "little")
            bits = np.frombuffer(out[4:4 + 8 * nn], np.float64)
            # candidate provenance (R6.2 lesson): construction id = mode
            # string + body hash + geometry — a flag string identifies an
            # arm FAMILY, never a unique input stream.
            wire = s3.bridge_feat(mode, stream_b, np.frombuffer(feats_b, np.uint8), "e")
            wire_sha = hashlib.sha256(wire).hexdigest()[:16]
            row["arms"][name] = {"L_B": round(float(bits.sum() / 8), 3),
                                 "wire_B": int(len(wire) - 4),
                                 "mode": mode,
                                 "wire_sha": wire_sha}
        row["gain_L1_B"] = round(row["L0_B"] - row["arms"]["L1"]["L_B"], 1)
        row["gain_L2_B"] = round(row["L0_B"] - row["arms"]["L2"]["L_B"], 1)
        row["gain_L3_B"] = round(row["L0_B"] - row["arms"]["L3"]["L_B"], 1)
        row["elapsed_s"] = round(time.perf_counter() - t0, 1)
        rows.append(row)
        print(json.dumps({k: v for k, v in row.items() if k != "arms"}),
              flush=True)
    if rows:
        g1 = sum(r["gain_L1_B"] for r in rows)
        g2 = sum(r["gain_L2_B"] for r in rows)
        g3 = sum(r["gain_L3_B"] for r in rows)
        print(f"== AGG == files {len(rows)}: L1 {g1:+,.1f} B | L2 {g2:+,.1f} B | "
              f"L3 {g3:+,.1f} B")
    out = ROOT / "research" / "results" / "last_run.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(rows, indent=1))
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
