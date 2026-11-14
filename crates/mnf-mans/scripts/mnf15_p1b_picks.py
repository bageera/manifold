#!/usr/bin/env python3
"""MNF15-P1-B baseline capture — pre-refactor vintage (746483f).

Encodes the 30 production-corpus files at the maximum profile through the
release CLI (pre-P1-B binary), timing each pick and hashing the output
byte-exactly. This is the B4 comparator: the post-refactor re-run must
reproduce EVERY sha256 byte-identically; the s column is the same-machine
timing baseline for the B5 readout (re-swept from the CURRENT build as of
this run, per the same-harness comparator rule).

Reads the corpus list from .research/r9/r9e_rows.csv (the recorded R9-E
30-file corpus). Asserts count == 30 and the v1.15.0 byte anchor 810,816.
Rows land in .research/p1b/picks_old.csv (gitignored workdir) and the
committed manifest copies the sha columns to docs/mnf15-p1b-baseline.sha.
"""
import csv
import hashlib
import subprocess
import time
from pathlib import Path

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")
BIN = ROOT / "target/release/manifold"
ROWS_IN = ROOT / ".research/r9/r9e_rows.csv"
OUTDIR = ROOT / ".research/p1b"


def main() -> int:
    files = [r["file"] for r in csv.DictReader(ROWS_IN.open())]
    assert len(files) == 30, f"corpus count: expected 30, got {len(files)}"
    OUTDIR.mkdir(parents=True, exist_ok=True)
    rows = []
    for f in files:
        png = ROOT / f
        out_m = OUTDIR / (Path(f).stem + ".mnf8-max")
        t0 = time.monotonic()
        r = subprocess.run(
            [BIN, "encode", str(png), "-o", str(out_m), "--mnf8",
             "--profile", "maximum"],
            capture_output=True, text=True,
        )
        dt = time.monotonic() - t0
        assert r.returncode == 0, (f, r.stderr[-300:])
        v = subprocess.run(
            [BIN, "verify", str(png), str(out_m)], capture_output=True, text=True
        )
        assert v.returncode == 0, (f, v.stderr[-200:])
        blob = out_m.read_bytes()
        flags = int.from_bytes(blob[24:28], "little")
        backend = int.from_bytes(blob[28:32], "little")
        rows.append({
            "file": f, "s": f"{dt:.3f}",
            "sha256": hashlib.sha256(blob).hexdigest(),
            "size": len(blob), "flags": f"0x{flags:x}", "backend": backend,
        })
        print(f"{f:52s} {len(blob):8d} B  {dt:7.1f} s  flags 0x{flags:x} b{backend}",
              flush=True)
    total = sum(r["size"] for r in rows)
    assert total == 810_816, f"anchor: {total}"
    ts = sum(float(r["s"]) for r in rows)
    with (OUTDIR / "picks_old.csv").open("w", newline="") as fh:
        w = csv.DictWriter(
            fh, fieldnames=["file", "s", "sha256", "size", "flags", "backend"]
        )
        w.writeheader()
        w.writerows(rows)
    manifest = ROOT / "docs/mnf15-p1b-baseline.sha"
    with manifest.open("w") as fh:
        fh.write("# MNF15-P1-B B4 baseline — pre-refactor pick digests (746483f vintage)\n")
        fh.write("# file size backend flags sha256  (assert: post-refactor re-run reproduces every line)\n")
        for r in rows:
            fh.write(f"{r['file']} {r['size']} b{r['backend']} {r['flags']} {r['sha256']}\n")
    print(f"\nfiles: {len(rows)}  total: {total}  time: {ts:.1f} s")
    print("manifest:", manifest)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
