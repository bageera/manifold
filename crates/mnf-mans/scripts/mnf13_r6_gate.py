#!/usr/bin/env python3
"""MNF13-R6 — adaptation & quantization ablation on the v1.13 backend-4
coder (measurement-only; work-order memo 2026-10-30).

The representation is FROZEN: predictor, channel transforms, token grammar,
and framing untouched. Only the entropy model's adaptation machinery varies.

Arms (all zero-transport; every model recomputes decode-side from the coded
symbols; per-file exact roundtrip asserted by the bridge):
  B  shipped   : fast 4,096 / slow 65,536, EWA lr 0.15, inc 32, 2^24 scale,
                 8 magnitude-bucket contexts   [the v1.13 baseline]
  R1 caps      : (fast,slow) cap sweeps (1024/4k/16k x 16k/64k/256k)
  R2 lr sweep  : EWA learning rate 0.05 / 0.25 / 0.5 (log-space step)
  R3 inc sweep : per-symbol count increment 16 / 64 (adaptation speed)
  R4 context   : 16 keys (8 buckets x prev-2 buckets, decoder-causal)
  R5 quantize  : probability scale 2^20 instead of 2^24 (coarser fixed point)

Comparator = the SHIPPED backend-4 payload per file (plen, header + framing
identical), so every delta is a pure entropy-model delta on the pick's own
files. Backend-1/0 files keep their shipped bytes (no comparator row; the
pick's never-worse shape already handles them).

Anchor (instrument validity, asserted before any arm runs): the extracted
stream re-encoded through the UNCHANGED shipped mix coder reproduces the
shipped payload byte-for-byte on every backend-4 file.

Usage:
  python3 crates/mnf-mans/scripts/mnf13_r6_gate.py              # all files
  python3 crates/mnf-mans/scripts/mnf13_r6_gate.py --subset 3   # first N
Output: docs/mnf13-r6-gate.log (JSON rows) — the doc derives from it.
"""
import argparse
import json
import subprocess
import sys
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(Path(__file__).parent))
from mnf13_r6_extract import residual_wire, cli_encode  # noqa: E402

BRIDGE = ROOT / "target/release/examples/r6_mix"
OUT = ROOT / "docs" / "mnf13-r6-gate.log"

# v1.13 baseline params (src/mix_coder.rs constants)
SHIPPED = ("mix4096_65536", "lr15", "inc32", "s24", "ctx8")

# arm name -> bridge mode string (extended bridge, see r6_mix.rs)
# mode grammar: "mix<F>_<S>" caps; "lr<X>" EWA step; "inc<N>" count inc;
# "s<N>" prob scale (20/24); "ctx<N>" context rule (8 buckets | 16 joint)
# — the bridge parses hyphen-separated tokens: mix4096_65536-lr05-inc16-s20-ctx16
ARMS = [
    ("A_shipped",    "mix4096_65536-lr15-inc32-s24-ctx8"),
    ("B_caps_1024_16384", "mix1024_16384-lr15-inc32-s24-ctx8"),
    ("B_caps_1024_65536", "mix1024_65536-lr15-inc32-s24-ctx8"),
    ("B_caps_4096_16384", "mix4096_16384-lr15-inc32-s24-ctx8"),
    ("B_caps_4096_262144", "mix4096_262144-lr15-inc32-s24-ctx8"),
    ("B_caps_16384_65536", "mix16384_65536-lr15-inc32-s24-ctx8"),
    ("C_lr005",      "mix4096_65536-lr005-inc32-s24-ctx8"),
    ("C_lr025",      "mix4096_65536-lr025-inc32-s24-ctx8"),
    ("C_lr050",      "mix4096_65536-lr050-inc32-s24-ctx8"),
    ("D_inc16",      "mix4096_65536-lr15-inc16-s24-ctx8"),
    ("D_inc64",      "mix4096_65536-lr15-inc64-s24-ctx8"),
    ("E_s20",        "mix4096_65536-lr15-inc32-s20-ctx8"),
    ("F_ctx16",      "mix4096_65536-lr15-inc32-s24-ctx16"),
]


def bridge(mode: str, syms: bytes, ctx: bytes) -> bytes:
    """Full wire bytes from the extended bridge (exact roundtrip asserted
    inside; stderr 'ERR' -> RuntimeError)."""
    hdr = f"{len(syms)} {mode}\n".encode()
    r = subprocess.run([str(BRIDGE), "c", mode], input=hdr + syms + ctx,
                       capture_output=True)
    if r.returncode != 0:
        raise RuntimeError(f"bridge {mode}: {r.stderr[:200]!r}")
    return r.stdout  # raw wire bytes


def file_ctx_keys(stream: bytes, ctx_mode: str) -> bytes:
    """Context keys per symbol, decoder-causal (recomputed from the wire's
    own bytes).
    ctx8  = v1.13's rule: 8 magnitude buckets of the PREVIOUS coded byte.
    ctx16 = prev bucket (8) x4 magnitude buckets of the byte BEFORE it —
    computed on the same wire bytes; the bridge receives them precomputed.
    The mode byte per tile is part of the stream (coded order)."""
    def bucket(b: int) -> int:
        if b == 0:
            return 0
        if b <= 2:
            return 1
        if b <= 5:
            return 2
        if b <= 11:
            return 3
        if b <= 24:
            return 4
        if b <= 48:
            return 5
        if b <= 96:
            return 6
        return 7

    keys = bytearray(len(stream))
    if ctx_mode == "ctx8":
        prev = 0
        for i, b in enumerate(stream):
            keys[i] = bucket(prev)
            prev = b
    else:  # ctx16: prev bucket + 2 bits of the pre-prev bucket's band
        p2 = 0
        prev = 0
        for i, b in enumerate(stream):
            keys[i] = (bucket(prev)) | (bucket(p2) >> 1 << 3) if False else (
                bucket(prev) + 8 * min(1, bucket(p2)) if bucket(p2) >= 6
                else bucket(prev) + 8 * (1 if bucket(p2) >= 3 else 0))
            p2 = prev
            prev = b
    return bytes(keys)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--subset", type=int, default=0)
    args = ap.parse_args()

    if not BRIDGE.exists():
        print(f"missing {BRIDGE}; build: cargo build --release -p mnf-mans "
              "--example r6_mix", file=sys.stderr)
        return 2

    files = sorted((ROOT / "testdata").glob("**/*.png"))
    rows = []
    for png in files:
        rel = str(png.relative_to(ROOT / "testdata"))
        if args.subset and len(rows) >= args.subset:
            break
        backend, flags, kind, ref, planar, stream, blob = residual_wire(
            png, profile=("mnf8", "maximum"))
        plen = struct.unpack_from("<I", blob, 20)[0]
        payload = blob[32:32 + plen]
        row = {"file": rel, "class": ("photographic" if "/photos/" in rel
                                      else "heldout" if "heldout-" in rel or "/heldout/" in rel
                                      else "synthetic"),
               "backend": backend, "flags": flags, "plen": plen}
        if blob[:4] != b"MNF8" or stream is None or backend != 4:
            # MNF9-magic whole-frame records, token wires, and backend-0/1
            # files: the mix arm never competes there (the pick declined);
            # the file's shipped wire stands. Record and skip.
            row["skip"] = (f"magic{blob[:4].decode(errors='replace')}"
                           if blob[:4] != b"MNF8"
                           else "token-wire" if (flags & 8)
                           else f"backend{backend}")
            rows.append(row)
            print(f"{rel}: SKIP {row['skip']}", flush=True)
            continue
        # ANCHOR: unchanged shipped params must reproduce the shipped payload
        a_keys = file_ctx_keys(stream, "ctx8")
        anchor = bridge("mix4096_65536-lr15-inc32-s24-ctx8-anchor", stream, a_keys)
        if anchor != payload:
            row["anchor_fail"] = True
            rows.append(row)
            print(f"{rel}: ANCHOR FAIL (wire {len(stream)} -> {len(anchor)} "
                  f"vs {plen})", flush=True)
            continue
        row["anchor"] = True
        # per-arm complete wire sizes (payload-only delta; framing shared)
        for (name, mode) in ARMS:
            if name == "A_shipped":
                row[name] = plen
                continue
            ctx_mode = "ctx16" if "-ctx16" in mode else "ctx8"
            keys = a_keys if ctx_mode == "ctx8" else file_ctx_keys(stream, "ctx16")
            out = bridge(mode, stream, keys)
            row[name] = len(out)
        best = min(row[n] for n, _ in ARMS)
        rows.append(row)
        print(f"{rel}: plen {plen} best {best} "
              f"({best - plen:+d} vs shipped)", flush=True)

    total_ship = sum(r.get("A_shipped") or r.get("plen") for r in rows
                     if r["backend"] == 4)
    print(f"\nbackend-4 files: {sum(1 for r in rows if r['backend'] == 4)}")
    for (name, _mode) in ARMS[1:]:
        tot = sum(r[name] for r in rows if r["backend"] == 4)
        fires = sum(1 for r in rows if r.get(name) is not None
                    and r[name] < r["A_shipped"])
        print(f"{name:22s} total {tot} ({tot - total_ship:+d} vs shipped) fires {fires}")
    OUT.write_text("# MNF13-R6 gate log (payload bytes per arm)\n"
                   + json.dumps(rows, indent=1) + "\n")
    print("wrote", OUT)


if __name__ == "__main__":
    main()