#!/usr/bin/env python3
"""Bitstream-size report (the deterministic size rows for docs/benchmarks.md).

Byte costs are deterministic (variant search is data-driven); they belong in
a report, not a statistical harness. For every corpus file:

    cargo-run encode --mnf8 [--profile maximum] / encode --mnf6 (mnf7) / PNG source size

Asserts decode-exactness on every row (encode -> verify), so the report
doubles as a corpus conformance run. Losses recorded beside wins.

Usage: python3 scripts/report_sizes.py [--out /tmp/sizes.csv]
"""
import argparse, csv, glob, os, subprocess, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
BIN = os.path.join(ROOT, "target", "release", "manifold")

def sh(args):
    r = subprocess.run(args, capture_output=True, text=True)
    assert r.returncode == 0, (args, r.stderr)
    return r.stdout + r.stderr

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus", default=os.path.join(ROOT, "testdata"))
    ap.add_argument("--out", default="/tmp/sizes.csv")
    a = ap.parse_args()
    files = sorted(glob.glob(os.path.join(a.corpus, "**", "*.png"), recursive=True))
    rows = []
    for png in files:
        rel = os.path.relpath(png, a.corpus)
        cls = ("photographic" if "/photos/" in png else
               "heldout" if "heldout/" in png else "synthetic")
        row = {"file": rel, "class": cls, "png": os.path.getsize(png)}
        for name, extra in (("mnf8-balanced", ["--mnf8"]),
                            ("mnf8-maximum", ["--mnf8", "--profile", "maximum"]),
                            ("mnf7", ["--mnf6"])):
            out = tempfile.mktemp(suffix=".mnf")
            sh([BIN, "encode", png, "-o", out, *extra])
            sh([BIN, "verify", png, out])  # exactness assert
            row[name] = os.path.getsize(out)
            os.remove(out)
        rows.append(row)

    with open(a.out, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
        w.writeheader()
        w.writerows(rows)
    print(f"wrote {a.out}: {len(rows)} files, all roundtrips exact")
    import statistics
    for cls in sorted(set(r["class"] for r in rows)):
        sub = [r for r in rows if r["class"] == cls]
        if len(sub) < 2:
            continue
        print(f"[{cls}]")
        for name in ("mnf8-balanced", "mnf8-maximum", "mnf7"):
            vals = [(r[name], r["png"]) for r in sub if r.get(name)]
            if len(vals) < 2:
                continue
            wins = sum(1 for v, p in vals if v < p)
            avgr = statistics.mean(v / p for v, p in vals)
            print(f"  {name:14s} avg {avgr:.3f}x png | wins {wins}/{len(vals)}")

if __name__ == "__main__":
    main()
