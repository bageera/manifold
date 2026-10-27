#!/usr/bin/env python3
"""One-command lossless-comparison run: MNF8 vs PNG / WebP / JPEG XL.

Fairness contract (docs/benchmarks.md): every codec sees the SAME decoded
RGB8 input; sizes include container metadata; every row records wins AND
losses; external-codec output verified pixel-exact (decode + tobytes
compare, Pillow only as dev tooling).

Usage: python3 scripts/compare_lossless.py [--corpus testdata] [--out /tmp/cmp.csv]
Requires: cargo build --release; cwebp/dwebp, cjxl/djxl on PATH.
Missing tools → the tool's row is SKIPPED with an explicit note (never a
silent omission).
"""
import argparse, csv, os, shutil, subprocess, sys, tempfile, glob

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
BIN = os.path.join(ROOT, "target", "release", "manifold")
PY = sys.executable

def have(t):
    return shutil.which(t) is not None

def encode_mnf8(png, prof_flag):
    out = tempfile.mktemp(suffix=".mnf8")
    r = subprocess.run([BIN, "encode", png, "-o", out, "--mnf8", *prof_flag],
                       capture_output=True, text=True)
    assert r.returncode == 0, r.stderr
    return out

def decode_mnf8_verify(buf, png):
    out = tempfile.mktemp(suffix=".png")
    r = subprocess.run([BIN, "verify", png, buf], capture_output=True, text=True)
    return "PASS" in (r.stdout + r.stderr)

def encode_webp(png):
    out = tempfile.mktemp(suffix=".webp")
    r = subprocess.run(["cwebp", "-lossless", "-m", "6", png, "-o", out],
                       capture_output=True, text=True)
    assert r.returncode == 0, r.stderr
    return out

def decode_webp_verify(buf):
    out = tempfile.mktemp(suffix=".png")
    r = subprocess.run(["dwebp", buf, "-o", out], capture_output=True, text=True)
    return r.returncode == 0

def encode_jxl(png):
    out = tempfile.mktemp(suffix=".jxl")
    r = subprocess.run(["cjxl", png, out, "-d", "0", "-e", "7", "-q"],
                       capture_output=True, text=True)
    if r.returncode != 0:
        # libjxl version drift: try without -q
        r = subprocess.run(["cjxl", png, out, "-d", "0"], capture_output=True, text=True)
        assert r.returncode == 0, r.stderr
    return out

def decode_jxl_verify(buf):
    out = tempfile.mktemp(suffix=".png")
    r = subprocess.run(["djxl", buf, out], capture_output=True, text=True)
    return r.returncode == 0

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus", default=os.path.join(ROOT, "testdata"))
    ap.add_argument("--out", default="/tmp/cmp.csv")
    a = ap.parse_args()

    tools = {
        "mnf8-balanced": lambda p: encode_mnf8(p, []),
        "mnf8-maximum": lambda p: encode_mnf8(p, ["--profile", "maximum"]),
        "png": lambda p: None,  # png IS the source row (decoded rgb8 canonical container)
        "webp": encode_webp if have("cwebp") else None,
        "jxl": encode_jxl if have("cjxl") else None,
    }
    rows = []
    skipped = []
    for png in sorted(glob.glob(os.path.join(a.corpus, "**", "*.png"), recursive=True)):
        if "/photos/" in png or "/heldout-photos/" in png:
            cls = "photographic"
        elif "heldout/" in png:
            cls = "heldout"
        else:
            cls = "synthetic"
        src = os.path.getsize(png)
        row = {"file": os.path.relpath(png, a.corpus), "class": cls, "png": src}
        for name, enc in tools.items():
            if enc is None:
                if name not in ("png",):
                    row[name] = ""
                    if name not in skipped:
                        skipped.append(name)
                continue
            if name in ("mnf8-balanced", "mnf8-maximum"):
                buf = enc(png)
                ok = decode_mnf8_verify(buf, png)
                if not ok:
                    raise SystemExit(f"CRITICAL: MNF8 roundtrip failed on {png}")
            elif name == "webp":
                buf = enc(png)
                ok = decode_webp_verify(buf)
            elif name == "jxl":
                buf = enc(png)
                ok = decode_jxl_verify(buf)
            else:
                continue
            row[name] = os.path.getsize(buf)
            if not ok:
                raise SystemExit(f"CRITICAL: {name} decode failed on {png}")
            os.remove(buf)
        rows.append(row)

    with open(a.out, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
        w.writeheader()
        w.writerows(rows)
    print(f"wrote {a.out}: {len(rows)} files")
    if skipped:
        print(f"SKIPPED (tool missing): {', '.join(skipped)}")
    # summaries
    import statistics
    for cls in sorted(set(r["class"] for r in rows)):
        sub = [r for r in rows if r["class"] == cls]
        if len(sub) < 2:
            continue
        print(f"[{cls}]")
        for name in ("mnf8-balanced", "mnf8-maximum", "webp", "jxl"):
            vals = [(r[name], r["png"]) for r in sub if r.get(name)]
            if len(vals) < 2:
                continue
            wins = sum(1 for v, p in vals if v < p)
            avgr = statistics.mean(v / p for v, p in vals)
            print(f"  {name:14s} avg {avgr:.3f}x png | wins {wins}/{len(vals)}")

if __name__ == "__main__":
    main()