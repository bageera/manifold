#!/usr/bin/env python3
"""MNF16-T1 step 1 - materialize the versioned production research trace.

One pass of the expensive computations the research program repeated
every experiment: per mix-payload file of the frozen corpus,
  - extract_stream (the encoder invocations - the dominant cost),
  - the bridge wire-decode ('p') -> the exact production symbol stream,
  - the audit per-symbol baseline costs ('a', the shipped schedule),
  - the record-grammar walk (stream-position mapping, modes, lane ids),
  - the X1 lane-class array (byte-level availability classification),
  - production context keys (prod_keys),
stored as ONE compressed npz per file. The DIGESTS are written to
research/manifests/corpus.lock (the committed versioned artifact, bound
to the v1.16.0 vintage + tool identities); the npz blobs live on disk
(gitignored) and are regenerated from the committed fixtures on demand.

Nothing here changes production code or the anchors. The trace is
metadata about frozen streams - every stored array is decoder-order
only (the walk produces prefix-valid annotations; features must still
pass the T0-T4 contract before use).
"""

import hashlib
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
from mnf13_r9a_gate import BRIDGE, bridge, extract_stream

VINTAGE = "v1.16.0"
TRACE_DIR = ROOT / "research" / "traces" / VINTAGE
LOCK = ROOT / "research" / "manifests" / "corpus.lock"


def sha(b):
    return hashlib.sha256(b).hexdigest()


def materialize(png_rel, bridge_ident):
    backend, flags, blob, _ = extract_stream(ROOT / png_rel)
    if blob[:4] != b"MNF8" or (flags & 0x8) or backend not in (4, 5, 6):
        return None
    w, h, _ts, tc = struct.unpack_from("<IIII", blob, 4)
    plen = struct.unpack_from("<I", blob, 20)[0]
    payload = bytes(blob[32:32 + plen])
    mode = {4: s2.B4, 5: s2.B5, 6: s2.B6}[backend]
    stream = bridge(mode, payload, "p")           # shipped-schedule decode
    out = bridge(mode, stream, "a")               # per-byte baseline costs
    nn = int.from_bytes(out[:4], "little")
    bits = np.frombuffer(out[4:4 + 8 * nn], np.float64)
    wire, pos, idx, modes = x1.walk_modes(stream, flags, w, h, tc)
    N = len(stream)
    assert len(bits) == N and len(wire) == len(idx)
    classes = np.zeros(N, np.uint8)               # the X1 mapping, frozen
    classes[idx] = (pos[:, 2] > 0).astype(np.uint8)
    keys = s2.prod_keys(stream, backend)
    npz = TRACE_DIR / (png_rel.replace("/", "__") + ".npz")
    np.savez_compressed(
        npz, stream=np.frombuffer(stream, np.uint8), bits=bits,
        pos=pos.astype(np.int32), idx=idx.astype(np.int64),
        modes=modes.astype(np.uint8), classes=classes, keys=keys.astype(np.int16))
    meta = {"file": png_rel, "backend": int(backend), "flags": int(flags),
            "w": w, "h": h, "tile_count": tc, "plen": plen, "n": N,
            "n_residuals": int(len(wire)),
            "ref": int((flags >> 6) & 3), "planar": bool(flags & 1),
            "bridge": bridge_ident,
            "stream_sha256": sha(stream),
            "trace_sha256": sha(npz.read_bytes()),
            "trace_file": str(npz.relative_to(ROOT))}
    print(json.dumps({k: meta[k] for k in ("file", "backend", "n")}), flush=True)
    return meta


def main():
    TRACE_DIR.mkdir(parents=True, exist_ok=True)
    (ROOT / "research" / "manifests").mkdir(parents=True, exist_ok=True)
    import mnf16_s1a_census as s1
    rels = sorted(str(p) for p in s1.corpus())
    bridge_ident = f"mnf16 bridge {VINTAGE} @ {BRIDGE}"
    rows, fails = [], []
    for rel in rels:
        t0 = time.perf_counter()
        try:
            m = materialize(rel, bridge_ident)
        except Exception as e:
            m = None
            fails.append({"file": rel, "error": str(e)[:160]})
        if m:
            m["materialize_s"] = round(time.perf_counter() - t0, 1)
            rows.append(m)
    # T2 screening population: frozen deterministic rule over the trace
    # manifest (documented, pre-registered; never cherry-picked by result):
    # the 5 mix files spanning content/size extremes = argmax n (the
    # production-scale stream), argmin n (the smallest), and the 3
    # median-n files (cost spread without size bias).
    ns = sorted(rows, key=lambda r: r["n"])
    picks = {ns[0]["file"], ns[-1]["file"]}
    mids = [r["file"] for r in ns[1:-1]]
    mids.sort(key=lambda f: abs(rows[[r["file"] for r in rows].index(f)]["n"]
                                - ns[len(ns) // 2]["n"]))
    picks.update(mids[:3])
    screening = {"rule": "argmin n, argmax n, 3 nearest-median-n files",
                 "seed_vintage": VINTAGE, "files": sorted(picks)}
    lock = {"vintage": VINTAGE, "bridge": bridge_ident,
            "generated_by": "mnf16_t1_trace.py",
            "n_traces": len(rows),
            "total_stream_bytes": sum(r["n"] for r in rows),
            "files": rows, "failures": fails}
    LOCK.write_text(json.dumps(lock, indent=1))
    (ROOT / "research" / "manifests" / "screening.lock").write_text(
        json.dumps(screening, indent=1))
    print(f"== {len(rows)} traces, {lock['total_stream_bytes']:,} stream bytes; "
          f"lock {LOCK} ({LOCK.stat().st_size:,} B); failures {len(fails)}; "
          f"screening {len(screening['files'])} files")


if __name__ == "__main__":
    main()
