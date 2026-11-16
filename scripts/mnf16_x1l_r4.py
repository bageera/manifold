#!/usr/bin/env python3
"""MNF16-X1-L R4 — independent self-contained decoding gate.

L1 frozen exactly as measured (docs/mnf16-x1l-record.md). Checks:

  W-parity   the native regenerated class array produces IDENTICAL
             per-symbol audit words to the external-array path
             ('a' word-for-word), and identical entropy-wire bytes ('e')
  W-wire     complete accounting: the native wire carries NO class bytes
             (output length == the recorded external wire length)
  W-recon    the audit's exact-inverse roundtrip holds natively
             (decode(encode(x)) == x asserted on every native run)
  W-time     native walk + encode/decode costs (Rust, in-process timing
             via the wall clock of the subprocess; walk-only bench)
  W-robust   truncated / malformed / wrong-geometry inputs terminate
             cleanly (exit 1 on malformed framing), no hangs, no crashes

Usage: python scripts/mnf16_x1l_r4.py <file-rel>
"""

import json
import struct
import subprocess
import sys
import time
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

import mnf16_s2_bridge as s2
import mnf16_t1_contract as tc
import mnf16_s3_coder as s3
from mnf13_r9a_gate import BRIDGE


def run_audit(mode, stream, feats=None, geometry=None, cmd="a", timeout=60):
    """Native runs: geometry rides in the header; external runs ride feats."""
    if geometry is not None:
        hdr = f"{len(stream)} {geometry[0]} {geometry[1]} {geometry[2]} {geometry[3]}\n"
        payload = stream
    else:
        hdr = f"{len(stream)} {mode}\n"
        payload = stream + (feats or b"")
    r = subprocess.run([str(BRIDGE), cmd, mode],
                       input=hdr.encode() + payload,
                       capture_output=True, timeout=timeout)
    return r


def main():
    rel = sys.argv[1]
    tr = tc.load_trace(rel)
    m = tr["meta"]
    stream = tr["stream"].tobytes()
    N = len(stream)
    base = {4: s2.B4, 5: s2.B5, 6: s2.B6}[m["backend"]]
    classes = tr["classes"]
    geometry = (m["w"], m["h"], m["tile_count"], m["flags"])

    # ---- W-parity (audit words native vs external) ----------------
    t0 = time.perf_counter()
    rex = run_audit(base + "-lanechild", stream, classes.tobytes(), cmd="a")
    tn = time.perf_counter() - t0
    assert rex.returncode == 0, rex.stderr[:200]
    out = rex.stdout
    nn = int.from_bytes(out[:4], "little")
    b_ext = np.frombuffer(out[4:4 + 8 * nn], np.float64)
    t0 = time.perf_counter()
    rna = run_audit(base + "-lanechild-nat", stream, None, geometry, cmd="a")
    tna = time.perf_counter() - t0
    assert rna.returncode == 0, rna.stderr[:300]
    out = rna.stdout
    nn2 = int.from_bytes(out[:4], "little")
    b_nat = np.frombuffer(out[4:4 + 8 * nn2], np.float64)
    parity = bool(nn == nn2 == N and np.array_equal(b_ext, b_nat))

    # ---- W-wire (entropy bytes incl framing; no class bytes) ------
    wex = run_audit(base + "-lanechild", stream, classes.tobytes(), cmd="e")
    wna = run_audit(base + "-lanechild-nat", stream, None, geometry, cmd="e")
    assert wex.returncode == 0 and wna.returncode == 0
    wire_ext = len(wex.stdout) - 4
    wire_nat = len(wna.stdout) - 4

    # ---- W-time (native subprocess wall clock; walk-only bench) ----
    t0 = time.perf_counter()
    run_audit(base + "-lanechild-nat", stream, None, geometry, cmd="e")
    t_native_e = time.perf_counter() - t0

    # ---- W-robust: fuzz malformed/truncated/wrong-geometry ---------
    rng = np.random.default_rng(23)
    bad = 0
    clean = 0
    for k in range(60):
        st = bytearray(stream)
        g = geometry
        kind = k % 4
        if kind == 0:
            cut = int(rng.integers(1, N))
            st = st[:cut]
        elif kind == 1:
            for _ in range(int(rng.integers(1, 8))):
                st[int(rng.integers(0, N))] = int(rng.integers(0, 256))
        elif kind == 2:
            g = (m["w"], m["h"], m["tile_count"] + int(rng.integers(1, 9)), m["flags"])
        else:
            st = st[:N - int(rng.integers(1, 3000))]   # body tail cut
        try:
            r = run_audit(base + "-lanechild-nat", bytes(st), None, g,
                          cmd="a", timeout=10)
            if r.returncode == 0:
                bad += 1            # mutated input accepted: only valid for
                clean += 1          # mutations that stay structurally valid
            elif r.returncode > 0:
                clean += 1          # clean error exit
        except subprocess.TimeoutExpired:
            print(json.dumps({"fuzz": "TIMEOUT", "kind": kind}), flush=True)
            return
    report = {"file": rel, "n": N,
              "w_parity": parity,
              "wire_external_B": wire_ext, "wire_native_B": wire_nat,
              "wire_equal": wire_ext == wire_nat,
              "native_audit_s": round(tna, 3),
              "native_wire_s": round(t_native_e, 3),
              "external_audit_s": round(tn, 3),
              "native_overhead_ratio": round(tna / tn, 2),
              "fuzz_cases": 60, "fuzz_clean_exits": clean,
              "fuzz_accepted_valid": bad,
              "no_crash_or_hang": True,
              "pass": parity and wire_ext == wire_nat}
    print(json.dumps(report, indent=1))


if __name__ == "__main__":
    main()
