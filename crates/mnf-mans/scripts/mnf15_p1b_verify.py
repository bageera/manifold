#!/usr/bin/env python3
"""MNF15-P1-B post-refactor verification (B4/B5/B6).

Re-encodes the 30-file production corpus through the rebuilt release CLI
(P1-B pick loop) at BOTH profiles with /usr/bin/time -l (peak RSS), then
asserts, against the committed pre-refactor manifests:

  B4 (maximum): docs/mnf15-p1b-baseline.sha  — every sha256 must match
  B6 (balanced): docs/mnf15-p1b-balanced.sha — every sha256 must match
  B5 readout: the summed maximum wall vs the baseline's s column
     (docs/mnf15-p1b-gate.log), and per-file peak RSS vs the recorded
     .research/r9/r9e_rows.csv new_rss_mb column.

Exits nonzero on any mismatch.
"""
import csv
import hashlib
import subprocess
import sys
import time as _t
from pathlib import Path

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")
BIN = ROOT / "target/release/manifold"
OUTDIR = ROOT / ".research/p1b"
GATES = ROOT / "docs/mnf15-p1b-gate.log"


def rss_of(stderr: str) -> float:
    for line in stderr.splitlines():
        if "maximum resident set size" in line:
            parts = line.split()
            digits = [t for t in parts if t.isdigit()]
            assert digits, f"unreadable RSS line: {line!r}"
            # macOS 26 time(1) prints the number BYTES-first (label after):
            # `true` measured 1,212,416 -> 1.16 MB with bytes; r9e's gate
            # (same machine) matched its MB values with the /1024^2 scale.
            return int(digits[-1]) / (1024.0 * 1024.0)
    return 0.0


def main() -> int:
    files = [r["file"] for r in csv.DictReader((ROOT / ".research/r9/r9e_rows.csv").open())]
    assert len(files) == 30
    base_max = {}
    for line in (ROOT / "docs/mnf15-p1b-baseline.sha").read_text().splitlines():
        if line.startswith("#"):
            continue
        f, _size, _b, _fl, sha = line.split()
        base_max[f] = sha
    base_bal = {}
    for line in (ROOT / "docs/mnf15-p1b-balanced.sha").read_text().splitlines():
        if line.startswith("#"):
            continue
        f, _size, sha = line.split()
        base_bal[f] = sha
    r9 = {r["file"]: r for r in csv.DictReader((ROOT / ".research/r9/r9e_rows.csv").open())}

    max_rows = []
    bal_bad = 0
    tot_max_bytes = 0
    tot_max_s = 0.0
    rss_out = []
    lines = []
    for f in files:
        png = ROOT / f
        out_m = OUTDIR / (Path(f).stem + ".mnf8-max-new")
        out_b = OUTDIR / (Path(f).stem + ".mnf8-bal-new")
        t0 = _t.monotonic()
        cmd = ["/usr/bin/time", "-l", str(BIN), "encode", str(png), "-o", str(out_m),
               "--mnf8", "--profile", "maximum"]
        r = subprocess.run(cmd, capture_output=True, text=True)
        assert r.returncode == 0, (f, r.stderr[-300:])
        wall = _t.monotonic() - t0
        rss = rss_of(r.stderr)
        v = subprocess.run([BIN, "verify", str(png), str(out_m)], capture_output=True, text=True)
        assert v.returncode == 0, (f, v.stderr[-200:])
        blob = out_m.read_bytes()
        sha = hashlib.sha256(blob).hexdigest()
        old_sha = base_max[f]
        ok_max = sha == old_sha
        flags = int.from_bytes(blob[24:28], "little")
        backend = int.from_bytes(blob[28:32], "little")
        r9_rss = float(r9[f]["new_rss_mb"])
        rss_out.append((f, rss, r9_rss))
        tot_max_bytes += len(blob)
        tot_max_s += wall
        state = "IDENTICAL" if ok_max else "DRIFT"
        lines.append(
            f"{f:52s} {len(blob):8d} B  {wall:7.1f} s  RSS {rss:5.1f} MB"
            f" (r9 {r9_rss:5.1f})  flags 0x{flags:x} b{backend}  B4:{state}")
        assert ok_max, f"B4 drift on {f}: {sha} != {old_sha}"
        # balanced
        r2 = subprocess.run([str(BIN), "encode", str(png), "-o", str(out_b),
                             "--mnf8", "--profile", "balanced"],
                            capture_output=True, text=True)
        assert r2.returncode == 0, (f, r2.stderr[-300:])
        blob_b = out_b.read_bytes()
        sha_b = hashlib.sha256(blob_b).hexdigest()
        v2 = subprocess.run([BIN, "verify", str(png), str(out_b)], capture_output=True, text=True)
        assert v2.returncode == 0, (f, v2.stderr[-200:])
        if sha_b != base_bal[f]:
            bal_bad += 1
        print(lines[-1], flush=True)
    assert tot_max_bytes == 810_816, tot_max_bytes
    assert bal_bad == 0, bal_bad
    base_time = 1127.9
    lines.append(
        f"\nB4: 30/30 byte-identical (total {tot_max_bytes} = the v1.15.0 anchor)")
    lines.append("B6: 30/30 balanced byte-identical")
    lines.append(
        f"B5: maximum wall {tot_max_s:.1f} s vs pre-refactor baseline {base_time} s"
        f" = {tot_max_s / base_time - 1:+.1%}")
    dmax = max(r - o for _, r, o in rss_out)
    lines.append(
        f"B6: max RSS {max(r for _, r, _ in rss_out):.1f} MB, worst delta vs"
        f" r9e new_rss_mb {dmax:+.1f} MB")
    GATES.write_text("\n".join(lines) + "\n")
    print("\n".join(lines[-4:]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
