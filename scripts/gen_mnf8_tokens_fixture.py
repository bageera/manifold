#!/usr/bin/env python3
"""Generate the MNF8 token-arm conformance fixtures (ADR-003).

Golden vector: a deterministic 64x64 gradient whose MED residual plane
repeats (the token-winning class), encoded with the FLAG_TOKENS pick
forced by construction (encode_mnf8 measures and picks tokens on this
body). Writes:
  tests/conformance/mnf8_tokens_gradient_64x64.mnf8
  tests/conformance/mnf8_tokens_gradient_64x64.expected.pixels
  tests/conformance/rej_mnf8_tokens_planar.mnf8      (PLANAR|TOKENS exclusive)
  tests/conformance/rej_mnf8_tokens_flag.mnf8        (bit 4 reserved)
Rejection files carry a valid header but an impossible flags word.
"""
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).parent.parent
CONF = ROOT / "tests/conformance"
OUT = ROOT / "tests/conformance"
W = H = 64


def gradient_64x64():
    px = bytearray(W * H * 3)
    for y in range(H):
        for x in range(W):
            i = (y * W + x) * 3
            px[i] = (y * 3) % 256
            px[i + 1] = (y // 4) % 256
            px[i + 2] = 200
    return bytes(px)


def main():
    px = gradient_64x64()
    png = "/tmp/mnf8_tokens_src.png"
    # write a PNG via manifold? No: use PIL-free route — the repo testdata
    # generator uses PIL. PIL is a dev dep in the venv; fall back to raw
    # encode via the CLI, which needs a PNG input.
    from PIL import Image

    Image.frombytes("RGB", (W, H), px).save(png)

    r = subprocess.run(
        [str(ROOT / "target/release/manifold"), "encode", png, "-o", "/tmp/mnf8_tokens.mnf5", "--mnf8"],
        capture_output=True, text=True,
    )
    print(r.stdout.strip(), r.stderr.strip())
    if "flags=0xc" not in r.stdout and "flags=0x0c" not in r.stdout:
        # flags hex is printed as {:#04x} → 0x0c
        sys.exit(f"expected the token pick (flags 0xc) on this body; got {r.stdout!r}")
    buf = Path("/tmp/mnf8_tokens.mnf5").read_bytes()
    (OUT / "mnf8_tokens_gradient_64x64.mnf8").write_bytes(buf)
    (OUT / "mnf8_tokens_gradient_64x64.expected.pixels").write_bytes(px)

    # rejection: PLANAR|TOKENS (bits0|3) is exclusive by ADR-003
    rej = bytearray(buf)
    flags = struct.unpack_from("<I", rej, 24)[0] | 1  # add PLANAR
    struct.pack_into("<I", rej, 24, flags)
    (OUT / "rej_mnf8_tokens_planar.mnf8").write_bytes(bytes(rej))

    # rejection: reserved bit 4 set
    rej2 = bytearray(buf)
    flags2 = struct.unpack_from("<I", rej2, 24)[0] | 0b10000
    struct.pack_into("<I", rej2, 24, flags2)
    (OUT / "rej_mnf8_tokens_flag.mnf8").write_bytes(bytes(rej2))

    print("fixtures written")


if __name__ == "__main__":
    main()