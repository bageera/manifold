#!/usr/bin/env python3
"""MNF17-P1 v1.17.1 external-fixture set (independent of corpus.lock).

Generates six deterministic RGB8 PNGs that are NOT members of the
research corpus, spanning the structure classes relevant to P1-D6:
degenerate, structured gradient, pseudo-random texture, photo-like
smooth blend, document-like sparse, and one odd-size case.
Stdlib only (zlib + struct). Deterministic seeds; regenerate with:
    env -u PYTHONPATH python3 crates/mnf-mans/scripts/v1171_external_fixtures.py
"""
import os
import struct
import zlib

OUT = os.path.join(os.path.dirname(__file__), "..", "..", "..", "research", "external-fixtures", "v1.17.1")


def png(path, w, h, rgb):
    def chunk(tag, data):
        c = struct.pack(">I", len(data)) + tag + data
        return c + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    raw = b"".join(
        b"\x00" + b"".join(bytes(px) for px in row) for row in rgb
    )
    data = b"\x89PNG\r\n\x1a\n"
    data += chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
    data += chunk(b"IDAT", zlib.compress(raw, 9))
    data += chunk(b"IEND", b"")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(data)
    print(path, os.path.getsize(path), "bytes")


def st(seed):
    s = seed & 0xFFFFFFFF
    while True:
        s ^= (s << 13) & 0xFFFFFFFF
        s ^= s >> 17
        s ^= (s << 5) & 0xFFFFFFFF
        yield s & 0xFF


def main():
    # 1. flat degenerate 96x96
    png(
        os.path.join(OUT, "flat_external_96.png"),
        96, 96,
        [[(33, 66, 99)] * 96 for _ in range(96)],
    )
    # 2. horizontal gradient 320x201 (odd height)
    png(
        os.path.join(OUT, "grad_external_320x201.png"),
        320, 201,
        [[(x * 255 // 319, y * 255 // 200, (x + y) * 255 // 519) for x in range(320)] for y in range(201)],
    )
    # 3. pseudo-random texture 128x128
    g = st(0x1F123BB5)
    png(
        os.path.join(OUT, "noise_external_128.png"),
        128, 128,
        [[(next(g), next(g), next(g)) for _ in range(128)] for _ in range(128)],
    )
    # 4. photo-like smooth blend 208x181 (photo regime: late cert firings)
    g = st(0xDEADBEEF)
    png(
        os.path.join(OUT, "photo_external_208x181.png"),
        208, 181,
        [
            [
                (
                    (x * 200 // 207 + (next(g) & 7) - 3) & 255,
                    (y * 180 // 180 + (next(g) & 7) - 3) & 255,
                    ((x + y) * 90 // 388 + (next(g) & 15) - 7) & 255,
                )
                for x in range(208)
            ]
            for y in range(181)
        ],
    )
    # 5. document-like sparse 176x220
    png(
        os.path.join(OUT, "document_external_176x220.png"),
        176, 220,
        [[(250, 250, 244) if (x + y) % 97 else (20, 20, 20) for x in range(176)] for y in range(220)],
    )
    # 6. checkerboard 101x101 (odd-size)
    png(
        os.path.join(OUT, "checker_external_101.png"),
        101, 101,
        [[(240, 240, 240) if (x // 4 + y // 4) % 2 else (15, 15, 15) for x in range(101)] for y in range(101)],
    )


if __name__ == "__main__":
    main()
