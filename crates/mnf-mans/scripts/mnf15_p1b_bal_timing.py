#!/usr/bin/env python3
"""MNF15-P1-B acceptance — matched-environment Balanced timing (B6 gap).

Sweeps the 30-file production corpus at the BALANCED profile with
/usr/bin/time -l (wall + RSS), asserts 30 rows, writes file,size,sha,s,rss
CSV. Runs against whatever BIN it is pointed at — the pre-refactor
worktree build (746483f vintage) and the post-refactor binary are both
swept so the comparison is same-machine, back-to-back.

The byte comparator is docs/mnf15-p1b-balanced.sha (the committed
pre-refactor digests); both sweeps must assert EVERY sha identical to it.
"""
import csv
import hashlib
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path("/Users/jonbethea/projects/blacktech/manifold")
ROWS_IN = ROOT / ".research/r9/r9e_rows.csv"
BASE = ROOT / "docs/mnf15-p1b-balanced.sha"


def rss_of(stderr: str) -> float:
    for line in stderr.splitlines():
        if "maximum resident set size" in line:
            parts = line.split()
            digits = [t for t in parts if t.isdigit()]
            assert digits, line
            return int(digits[-1]) / (1024.0 * 1024.0)  # bytes -> MB (macOS 26)
    return 0.0


def main() -> int:
    binp = Path(sys.argv[1])
    out_csv = Path(sys.argv[2])
    outdir = Path(sys.argv[3])
    assert binp.exists(), binp
    outdir.mkdir(parents=True, exist_ok=True)
    files = [r["file"] for r in csv.DictReader(ROWS_IN.open())]
    assert len(files) == 30
    base = {}
    for line in BASE.read_text().splitlines():
        if not line.startswith("#"):
            f, _sz, sha = line.split()
            base[f] = sha
    rows = []
    tot_s = 0.0
    for f in files:
        png = ROOT / f
        out = outdir / (Path(f).stem + ".bal.mnf8")
        t0 = time.monotonic()
        r = subprocess.run(
            ["/usr/bin/time", "-l", str(binp), "encode", str(png),
             "-o", str(out), "--mnf8", "--profile", "balanced"],
            capture_output=True, text=True)
        assert r.returncode == 0, (f, r.stderr[-200:])
        s = time.monotonic() - t0
        v = subprocess.run([str(binp), "verify", str(png), str(out)],
                           capture_output=True, text=True)
        assert v.returncode == 0, (f, v.stderr[-200:])
        blob = out.read_bytes()
        sha = hashlib.sha256(blob).hexdigest()
        assert sha == base[f], f"balanced drift on {f}: {sha} != {base[f]}"
        rows.append((f, len(blob), sha, s, rss_of(r.stderr)))
        tot_s += s
        print(f"{f:52s} {len(blob):7d} B  {s:6.2f} s  RSS {rows[-1][4]:5.1f}",
              flush=True)
    assert len(rows) == 30
    with out_csv.open("w", newline="") as fh:
        w = csv.writer(fh)
        w.writerow(["file", "size", "sha", "s", "rss_mb"])
        w.writerows(rows)
    rmax = max(r[4] for r in rows)
    print(f"TOTAL balanced 30 files: {tot_s:.2f} s, peak RSS {rmax:.1f} MB,"
          f" all 30 digests == committed baseline", flush=True)
    out_csv.with_suffix(".total").write_text(f"{tot_s:.2f}\n{rmax:.2f}\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
