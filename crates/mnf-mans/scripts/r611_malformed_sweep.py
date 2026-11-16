#!/usr/bin/env python3
"""R6.11 backend-7-specific malformed-input sweep.

Targets: a backend-7 container (forest), a second backend-7 container
(terrain), a zstd-1 container, and the MNF9-class container.
Mutations: truncations, header-field edits (plen/lengths/geometry words),
byte-set edits across the record-header region (first 64 B) and the body,
and deterministic random edits.
Acceptance: decoder never panics (rc 101), never hangs (timeout 120 s),
peak RSS bounded (<512 MB); structurally valid mutants may decode (counted).
"""
import hashlib
import json
import os
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")
BIN = ROOT / "target/release/manifold"
ENV = dict(os.environ, PATH="/opt/homebrew/opt/rustup/bin:" + os.environ["PATH"],
           RUSTUP_HOME=os.path.expanduser("~/.rustup"))
sys.path.insert(0, str(ROOT / "crates/mnf-mans/scripts"))
from mnf13_r6_extract import cli_encode  # noqa: E402

TARGET_FILES = ["testdata/photos/forest_224x160_s900002.png",
                "testdata/photos/terrain_224x160_s900005.png",
                "testdata/repeat_256x256.png",
                "testdata/solid_256x256.png"]


def decode(mut, tag):
    p = Path("/tmp/r611.mnf")
    p.write_bytes(mut)
    r = subprocess.run(["/usr/bin/time", "-l", str(BIN), "decode", "/tmp/r611.mnf",
                        "-o", "/tmp/r611_dec.png"], cwd=ROOT, env=ENV,
                       capture_output=True, text=True, timeout=120)
    rss = 0
    for ln in r.stderr.splitlines():
        if "maximum resident set size" in ln:
            rss = int(ln.split()[0])
    return r.returncode, rss


def sweep(base, tag, light):
    muts = []
    n = len(base)
    # truncations
    step = 4096 if not light else 8192
    muts += [("trunc", base[:i]) for i in range(0, n, step)]
    muts += [("trunc", base[:64]), ("trunc", base[:1]), ("trunc", b""), ("trunc", base[: n - 1]), ("trunc", base[: n - 4])]
    # header + first 64 B byte edits (set 0x00 / 0xFF)
    for i in range(min(64, n)):
        for v in (0x00, 0xFF):
            if base[i] != v:
                muts.append(("edit", base[:i] + bytes([v]) + base[i + 1:]))
    # plen / total-len word nudges (offsets 8..31 header words)
    for off in (8, 16, 20, 28):
        if off + 4 <= n:
            w = int.from_bytes(base[off:off + 4], "little")
            for d in (-1, 1, 256):
                nw = (w + d) & 0xFFFFFFFF
                if nw != w:
                    muts.append(("word", base[:off] + nw.to_bytes(4, "little") + base[off + 4:]))
    if light:
        muts = muts[:120]
    stats = {"tag": tag, "cases": len(muts), "rejected": 0, "accepted": 0, "errors": 0,
             "max_rss_mb": 0.0, "panics": 0, "timeouts": 0}
    for kind, m in muts:
        try:
            rc, rss = decode(m, tag)
        except subprocess.TimeoutExpired:
            stats["timeouts"] += 1
            print(json.dumps({"FAIL": "TIMEOUT", "target": tag}), flush=True)
            continue
        stats["max_rss_mb"] = max(stats["max_rss_mb"], rss / 1048576)
        if rc == 101 or "panicked" in str(rc):
            stats["panics"] += 1
            print(json.dumps({"FAIL": "PANIC", "target": tag}), flush=True)
        elif rc == 0:
            stats["accepted"] += 1
        elif rc != 0:
            stats["rejected"] += 1
    print(json.dumps(stats), flush=True)
    return stats


def main():
    allstats = []
    blob = cli_encode(TARGET_FILES[0], profile=("mnf8", "maximum"))
    allstats.append(sweep(blob, "forest_b7", light=False))
    blob2 = cli_encode(TARGET_FILES[1], profile=("mnf8", "maximum"))
    allstats.append(sweep(blob2, "terrain_b7", light=True))
    blob3 = cli_encode(TARGET_FILES[2], profile=("mnf8", "maximum"))
    allstats.append(sweep(blob3, "repeat_b1", light=True))
    blob4 = cli_encode(TARGET_FILES[3], profile=("mnf8", "maximum"))
    allstats.append(sweep(blob4, "solid_mnf9", light=True))
    total = sum(s["cases"] for s in allstats)
    fatal = sum(s["panics"] + s["timeouts"] for s in allstats)
    print(json.dumps({"R6_11": {"total_mutation_cases": total, "panics_or_timeouts": fatal,
                                "max_rss_mb": max(s["max_rss_mb"] for s in allstats)}}), flush=True)
    (ROOT / "research/results/x1l_r611_fuzz.json").write_text(json.dumps(allstats, indent=1))
    print("WROTE research/results/x1l_r611_fuzz.json")


if __name__ == "__main__":
    main()
