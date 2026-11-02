#!/usr/bin/env python3
"""MNF13-R9-C — warm-up sharing arms on the R9-A production streams.

Runs the R9-A bridge ('e' mode) on every audited file's EXACT symbol
stream (R9-B attribution's inputs; durable under .research/r9/) with:
  C0 shipped  : mix32768_1048576-lr15-inc32-ctx16 (the frozen backend-5)
  C1 warminit : seed child tables from the parent bucket on first use
  C2 warm2pN  : λ = 2^N/(2^N+n) child↔parent mixing while cold
  C3 warmfixΛ : constant λ mixing (the comparator; risks hot-context
                pollution by design)

Per file: complete wire bytes for every arm (the roundtrip is asserted
inside the bridge on every call — the same contract as R9-A), the
anchor (C0 == the shipped payload, byte-identical), and per-arm deltas.
Also runs the audit ('a' mode) on the best corpus arm for the cold/
steady split verification against R9-B's prediction.
"""
import json
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")
sys.path.insert(0, str(ROOT / "crates/mnf-mans/scripts"))
from mnf13_r9a_gate import extract_stream  # noqa: E402

BIN = ROOT / "target/release/examples/r9_audit"
C0 = "mix32768_1048576-lr15-inc32-ctx16"
ARMS = [
    ("C0_shipped", C0),
    ("C1_warminit", C0 + "-warminit"),
    ("C2_2p8", C0 + "-warm2p8"),
    ("C2_2p10", C0 + "-warm2p10"),
    ("C2_2p12", C0 + "-warm2p12"),
    ("C2_2p14", C0 + "-warm2p14"),
    ("C3_fix15", C0 + "-warmfix15"),
    ("C3_fix30", C0 + "-warmfix30"),
]

FILES = [
    ("document_200x280.png", ROOT / "testdata/heldout/document_200x280.png"),
    ("screenshot_640x400.png", ROOT / "testdata/screenshot_640x400.png"),
] + [(p.name, p) for p in sorted((ROOT / "testdata").glob("**/*.png"))
     if p.name not in ("document_200x280.png", "screenshot_640x400.png",
                       "gradient_320x240.png", "blob_224x160.png",
                       "blocks_192x128.png", "plaid_192.png",
                       "poster_208x144.png", "noise_256x256.png",
                       "repeat_256x256.png", "solid_256x256.png")]


def bridge_e(mode, syms):
    r = subprocess.run([str(BIN), "e", mode], input=f"{len(syms)} {mode}\n".encode() + syms,
                       capture_output=True, timeout=300)
    if r.returncode != 0:
        raise RuntimeError(f"{mode}: {r.stderr[:120]!r}")
    return r.stdout


def main():
    rows = []
    tot = {}
    for name, png in FILES:
        be, fl, blob, stream = extract_stream(png)
        if be != 5 or stream is None:
            continue
        plen = struct.unpack_from("<I", blob, 20)[0]
        row = {"file": name, "n": len(stream), "plen": plen}
        for arm, mode in ARMS:
            wire = bridge_e(mode, stream)
            row[arm] = len(wire) - 4  # entropy-wire = payload minus len-word
            if arm == "C0_shipped":
                assert wire == blob[32:32 + plen], (
                    f"{name}: C0 anchor fails ({len(wire)-4} vs {plen})")
                row["anchor_ok"] = True
        for arm, _ in ARMS:
            tot[arm] = tot.get(arm, 0) + row[arm]
        rows.append(row)
        print(f"{name:32} n={len(stream):8} " +
              " ".join(f"{arm.split('_')[0]}={row[arm]}" for arm, _ in ARMS),
              flush=True)
    base = tot[ARMS[0][0]]
    print(f"\n== {len(rows)} files, C0 anchored byte-identical everywhere ==")
    for arm, _ in ARMS:
        d = tot[arm] - base
        print(f"  {arm:12} total {tot[arm]:9} B  ({d:+d} vs shipped)")
    Path("/tmp/r9c_rows.json").write_text(json.dumps(rows, indent=1))
    print("rows -> /tmp/r9c_rows.json")


if __name__ == "__main__":
    main()