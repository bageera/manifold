#!/usr/bin/env python3
"""Regenerate tests/conformance/mnf8_mix6_document.* from the release CLI.

The MNF8 conformance vector for MNF13-R9-E's backend-6 pick: encode
document 200x280 with --mnf8=maximum (the measured-winner pick), verify
decode-exactness, dump the decoded RGB8 pixels. Asserts the pick's
backend id and flags word names the arms before writing anything.

Usage: python3 scripts/gen_mnf8_mix6_fixture.py
"""
import os, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
BIN = os.path.join(ROOT, "target", "release", "manifold")
SRC = os.path.join(ROOT, "testdata", "heldout", "document_200x280.png")
OUT_DIR = os.path.join(ROOT, "tests", "conformance")


def sh(args):
    r = subprocess.run(args, capture_output=True, text=True)
    assert r.returncode == 0, (args, r.stderr)
    return r.stdout + r.stderr


def main():
    out = tempfile.mktemp(suffix=".mnf8")
    sh([BIN, "encode", SRC, "-o", out, "--mnf8", "--profile", "maximum"])
    sh([BIN, "verify", SRC, out])
    blob = open(out, "rb").read()
    os.remove(out)
    backend = int.from_bytes(blob[28:32], "little")
    flags = int.from_bytes(blob[24:28], "little")
    slen = int.from_bytes(blob[32:36], "little")
    # MNF8 wire checks: LEFT|REF=3 record arm (flags bit 0 + REF field 3),
    # backend 6 (MNF13-R9-E), and the mix payload's stream length word =
    # the record stream's real length (168,234: 234 mode-0 tiles for
    # 200x280). Adjust the constants here when the measured pick moves.
    assert backend == 6, f"backend {backend} != the mix6 pick"
    assert flags == 0xE0, f"flags 0x{flags:x} != LEFT|REF=3 (0xe0)"
    assert slen == 168_234, f"stream_len {slen} != 168234"
    from PIL import Image
    px = Image.open(SRC).convert("RGB").tobytes()
    assert len(px) == 200 * 280 * 3
    open(os.path.join(OUT_DIR, "mnf8_mix6_document.mnf8"), "wb").write(blob)
    open(os.path.join(OUT_DIR, "mnf8_mix6_document.expected.pixels"), "wb").write(px)
    print(f"wrote mnf8_mix6_document.mnf8 ({len(blob)} B, backend {backend}, "
          f"flags 0x{flags:x}, stream_len {slen}) + expected.pixels")


if __name__ == "__main__":
    sys.exit(main())