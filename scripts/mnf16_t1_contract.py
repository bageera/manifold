#!/usr/bin/env python3
"""MNF16-T1 step 2 - the shared causal-extractor contract, run as a gate.

Executes prefix-invariance checks (T1-T4) for a registered feature
against a loaded production trace, reusing the X1 instrument's machine:

  feature registry: name -> (ref_fn, fast_fn)
    ref_fn(view, y, x, lane) -> feature byte or None (fallback) — the
        T3 prefix-only reference path; ONLY sees PrefixView
    fast_fn(wire_res, pos3, idx, N, stream) -> (array over stream bytes,
        present-mask) — the vectorized twin; T4 must show exact parity

  T1 exhaustive 256-value target substitution (sampled symbols, all values)
  T2 future mutation (suffix overwrite at N/2, 3N/4 + window permutation)
  T4 reference == vectorized exactly
  (T0 = the documented operand audit; T3 = structural, the view itself)

Usage: python scripts/mnf16_t1_contract.py <file-rel> [feature]
A new information-domain feature registers here and must pass before
any census or coder arm consumes it.
"""

import hashlib
import json
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

import mnf16_t1_trace as tt
import mnf16_x1 as x1

SAMPLE = 1024          # frozen gate parameter
MUT_SEED = 17


def f_x1lane_ref(view, y, x, lane):
    """X1 lane class: 1 if a same-pixel earlier lane exists."""
    return 1 if view.earlier(y, x, lane) else None


def f_x1lane_fast(wire_res, pos3, idx, N, stream):
    classes = np.zeros(N, np.uint8)
    classes[idx] = (pos3[:, 2] > 0).astype(np.uint8)
    return classes.astype(np.int64), classes.astype(bool)


REGISTRY = {"f_x1lane": (f_x1lane_ref, f_x1lane_fast)}


def load_trace(png_rel):
    lock = json.loads(tt.LOCK.read_text())
    matches = [m for m in lock["files"] if m["file"] == png_rel]
    if not matches:
        raise SystemExit(f"{png_rel} not in {tt.LOCK} - materialize first")
    m = matches[0]
    npz_path = ROOT / m["trace_file"]
    data = dict(np.load(npz_path))
    if tt.sha(npz_path.read_bytes()) != m["trace_sha256"]:
        raise SystemExit(f"trace digest mismatch for {png_rel} - regenerate")
    data["meta"] = m
    return data


def run_checks(trace, ref_fn, fast_fn):
    stream = trace["stream"].tobytes()
    N = len(stream)
    idx = trace["idx"].astype(np.int64)
    pos3 = trace["pos"].astype(np.int64)          # residual-indexed by the walk
    wire_res = trace["stream"][idx].astype(np.int64)
    cand = idx.tolist()
    rng = np.random.default_rng(x1.SHUF_SEED)

    # T3/T4 reference path: the view; then the vectorized twin
    view = x1.PrefixView()
    F_ref = np.zeros(N, np.int64)
    H_ref = np.zeros(N, bool)
    for i in cand:
        r = int(np.searchsorted(idx, i))
        y, x, k = int(pos3[r, 0]), int(pos3[r, 1]), int(pos3[r, 2])
        fe = ref_fn(view, y, x, k)
        H_ref[i] = fe is not None
        if fe is not None:
            F_ref[i] = int(fe)
        view.expose(y, x, k, int(stream[i]))
    F_fast, H_fast = fast_fn(wire_res, pos3, idx, N, stream)

    # T1: exhaustive target substitution on sampled symbols
    sub = rng.choice(cand, size=min(SAMPLE, len(cand)), replace=False)
    t1_ok = 0
    for i in sub:
        r = int(np.searchsorted(idx, i))
        y, x, k = int(pos3[r, 0]), int(pos3[r, 1]), int(pos3[r, 2])
        before = ref_fn(view, y, x, k)
        prev = view.buf.get((y, x, k))
        for tv in range(256):
            if prev is not None:
                view.buf[(y, x, k)] = tv          # mutate only the target slot
            t1_ok += int(before == ref_fn(view, y, x, k))
        if prev is not None:
            view.buf[(y, x, k)] = prev

    # T2: future mutation
    t2 = True
    for i0 in (N // 2, 3 * N // 4):
        smut = bytearray(stream)
        for ip in cand:
            if ip >= i0:
                smut[ip] = 0xAA
        tr2 = dict(trace, stream=np.frombuffer(bytes(smut), np.uint8))
        F2, _ = fast_fn(np.frombuffer(bytes(smut), np.uint8)[idx].astype(np.int64),
                        pos3, idx, N, np.frombuffer(bytes(smut), np.uint8))
        t2 &= bool((F2[:i0] == F_fast[:i0]).all())
    j0 = N // 3
    smut = bytearray(stream)
    tail = [ip for ip in cand if ip >= j0]
    vals = [smut[ip] for ip in tail]
    for ip, pv in zip(tail, [vals[q] for q in np.random.default_rng(MUT_SEED).permutation(len(tail))]):
        smut[ip] = pv
    head = [i for i in cand if i < j0]
    F3, _ = fast_fn(np.frombuffer(bytes(smut), np.uint8)[idx].astype(np.int64),
                    pos3, idx, N, np.frombuffer(bytes(smut), np.uint8))
    t2 &= bool(np.array_equal(F3[head], F_fast[head]))

    t4 = bool(np.array_equal(F_fast, F_ref) and np.array_equal(H_fast, H_ref))
    return {"file": trace["meta"]["file"], "n": N,
            "t1_substitutions": int(t1_ok), "t1_total": int(len(sub) * 256),
            "t2_future_mutation": bool(t2), "t4_parity": t4,
            "pass": bool(t4 and t2 and t1_ok == len(sub) * 256)}


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("-")]
    png_rel = args[0]
    name = args[1] if len(args) > 1 else "f_x1lane"
    ref_fn, fast_fn = REGISTRY[name]
    trace = load_trace(png_rel)
    print(json.dumps(run_checks(trace, ref_fn, fast_fn), indent=1))


if __name__ == "__main__":
    main()
