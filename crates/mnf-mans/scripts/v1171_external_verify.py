#!/usr/bin/env python3
"""MNF17-P1 v1.17.1 release verification: external fixtures + wire-vs-v1.17.0.

For each generated external fixture (independent of the research corpus):
  1. encode with the current release build,
  2. decode the produced container and compare pixels exactly,
  3. encode with a v1.17.0 reference binary (if provided) and require
     byte-identical wires (zero-regret across releases).

Usage:
  env -u PYTHONPATH python3 crates/mnf-mans/scripts/v1171_external_verify.py \
      [--v1170 PATH_TO_v1.17.0_BINARY] [--bin PATH_TO_v1.17.1_BINARY]

Exit 0 and a JSON line "EXTERNAL_VERIFY": "PASS" on success.
"""
import argparse
import hashlib
import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
FIX = os.path.join(ROOT, "research", "external-fixtures", "v1.17.1")


def sha16(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()[:16]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", default=os.path.join(ROOT, "target", "release", "manifold"))
    ap.add_argument("--v1170", default="")
    args = ap.parse_args()
    if not os.path.isdir(FIX) or not os.listdir(FIX):
        subprocess.run(
            [sys.executable, os.path.join(HERE, "v1171_external_fixtures.py")], check=True
        )
    fixtures = sorted(
        f for f in os.listdir(FIX) if f.endswith(".png")
    )
    assert len(fixtures) == 6, f"expected 6 external fixtures, got {len(fixtures)}"
    report = {"bin": args.bin, "fixtures": [], "recon_fail": [], "wire_fail": []}
    with tempfile.TemporaryDirectory() as td:
        for name in fixtures:
            src = os.path.join(FIX, name)
            enc = os.path.join(td, name + ".mnf")
            dec = os.path.join(td, name + ".dec.png")
            subprocess.run(
                [args.bin, "encode", src, "-o", enc, "--mnf8=maximum"],
                check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            )
            r = subprocess.run(
                [args.bin, "decode", enc, "-o", dec],
                capture_output=True, text=True,
            )
            assert r.returncode == 0, r.stderr
            # exact reconstruction: raster-compare the decoded pixels with
            # the source (lossless BMP conversion via sips, then bytes)
            bmp_src = os.path.join(td, name + ".src.bmp")
            bmp_dec = os.path.join(td, name + ".dec.bmp")
            for a in ((src, bmp_src), (dec, bmp_dec)):
                subprocess.run(["sips", "-s", "format", "bmp", a[0], "--out", a[1]],
                               check=True, capture_output=True)
            if open(bmp_src, "rb").read() != open(bmp_dec, "rb").read():
                report["recon_fail"].append(name)
            # and a wire-level re-encode round trip (bonus exactness)
            reenc = os.path.join(td, name + ".reenc.mnf")
            subprocess.run(
                [args.bin, "encode", dec, "-o", reenc, "--mnf8=maximum"],
                check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            )
            if sha16(reenc) != sha16(enc):
                report["recon_fail"].append(name)
            row = {"fixture": name, "size": os.path.getsize(enc), "sha16": sha16(enc)}
            if args.v1170:
                legacy = os.path.join(td, name + ".v1170.mnf")
                subprocess.run(
                    [args.v1170, "encode", src, "-o", legacy, "--mnf8=maximum"],
                    check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                )
                row["v1170_sha16"] = sha16(legacy)
                if sha16(legacy) != sha16(enc):
                    report["wire_fail"].append(name)
            report["fixtures"].append(row)
    report["EXTERNAL_VERIFY"] = (
        "PASS" if not report["recon_fail"] and not report["wire_fail"] else "FAIL"
    )
    print(json.dumps(report, indent=1))
    sys.exit(0 if report["EXTERNAL_VERIFY"] == "PASS" else 1)


if __name__ == "__main__":
    main()
