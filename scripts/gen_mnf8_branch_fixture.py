#!/usr/bin/env python3
"""ADR-004 branch-predictor conformance fixtures.

Golden: a deterministic 200x120 poster-like body (flat regions + hard
vertical edges — the measured LEFT-winning class), encoded with the
balanced pick that MUST select a branch arm here.
Writes tests/conformance/:
  mnf8_branch_left_poster.mnf8 / .expected.pixels
  rej_mnf8_two_predictors.mnf8 (MED|PLANE both set)
  rej_mnf8_tokens_left.mnf8    (TOKENS|LEFT)
"""
import struct
import subprocess
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).parent.parent
OUT = ROOT / "tests/conformance"
# The fixture body IS the gate's measured LEFT winner (poster heldout:
# left-z19 1142 < all shipping arms) — real content, real pick.


def body_and_dims():
    img = Image.open(ROOT / "testdata/heldout/poster_208x144.png").convert("RGB")
    return img.tobytes(), img.width, img.height


def main():
    px, W, H = body_and_dims()
    png = "/tmp/mnf8_branch_src.png"
    Image.frombytes("RGB", (W, H), px).save(png)
    r = subprocess.run(
        [str(ROOT / "target/release/manifold"), "encode", png, "-o", "/tmp/mnf8_branch.mnf5", "--mnf8"],
        capture_output=True, text=True,
    )
    m = [l for l in r.stdout.splitlines() if "flags=" in l]
    print(m[0] if m else r.stdout)
    if "flags=0x20" not in (m[0] if m else ""):  # PRED_LEFT = 32
        sys.exit(f"expected the LEFT pick (0x20) on this body")
    buf = Path("/tmp/mnf8_branch.mnf5").read_bytes()
    (OUT / "mnf8_branch_left_poster.mnf8").write_bytes(buf)
    (OUT / "mnf8_branch_left_poster.expected.pixels").write_bytes(px)
    # rejections: flip flag bits on the valid file (payload untouched — flags
    # validation fires before any payload walk)
    rej = bytearray(buf)
    flags = struct.unpack_from("<I", rej, 24)[0]
    struct.pack_into("<I", rej, 24, flags | 4)  # LEFT|MED both set
    (OUT / "rej_mnf8_two_predictors.mnf8").write_bytes(bytes(rej))
    rej2 = bytearray(buf)
    struct.pack_into("<I", rej2, 24, flags | 8)  # LEFT|TOKENS
    (OUT / "rej_mnf8_tokens_left.mnf8").write_bytes(bytes(rej2))
    print("fixtures written")


if __name__ == "__main__":
    main()