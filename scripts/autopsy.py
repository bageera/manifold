#!/usr/bin/env python3
"""MNF10 Bit-Cost Autopsy (two-track partner of generative discovery).

Independent walk of SHIPPED MNF8 files from the release CLI: decompress the
payload, replay the record grammar (mode 0 literal / 1 exact-ref / 2 geom /
3 ycocg + per-record mode byte), and attribute every byte to a bucket:
header, mode bytes, literal bodies, reference payloads, container framing.
Output: per-file + corpus totals table (CSV) — the readout that tells
Discovery which structures to target with exact rules.
"""

import struct
import sys
import zlib
from pathlib import Path

try:
    import zstandard
except ImportError:
    zstandard = None

HEADER = 32


def autopsy_file(path):
    blob = Path(path).read_bytes()
    if len(blob) < HEADER:
        raise SystemExit(f"{path}: shorter than header")
    magic = blob[0:4].decode("latin1")
    w, h, tile, tile_count = struct.unpack("<IIII", blob[4:20])
    payload_len, flags, backend = struct.unpack("<III", blob[20:32])
    payload = blob[HEADER:HEADER + payload_len]
    if backend == 1:
        if zstandard is None:
            raise SystemExit("zstd payload but no zstandard module")
        d = zstandard.ZstdDecompressor()
        stream = d.stream_reader(__import__("io").BytesIO(payload)).read()
    else:
        stream = zlib.decompress(payload)
    counts = {0: 0, 1: 0, 2: 0, 3: 0}
    cols = (w + tile - 1) // tile
    counts.update({"tok_lit": 0, "tok_match": 0, "tok_near": 0})
    pos = 0
    if flags & 0x8:                       # FLAG_TOKENS: §3.1 token wire
        npx = w * h
        ppos = 0
        while ppos < npx and pos < len(stream):
            tag = stream[pos]
            pos += 1
            if tag == 0:
                counts["tok_lit"] += 1
                ppos += 1
                pos += 3
            elif tag == 1:
                counts["tok_match"] += 1
                dx, dy, ln = struct.unpack("<HHB", stream[pos:pos + 5])
                pos += 5
                ppos += ln
            elif tag == 2:
                counts["tok_near"] += 1
                dx, dy, ln = struct.unpack("<BBB", stream[pos:pos + 3])
                pos += 3
                ppos += ln
            else:
                raise SystemExit(f"unknown token tag {tag} at {pos} in {path} head={stream[:24].hex()} ppos={ppos}")
    else:
        for t in range(tile_count):
            gy, gx = divmod(t, cols)
            tw = min(tile, w - gx * tile)
            th = min(tile, h - gy * tile)
            nb = 3 * tw * th
            mode = stream[pos]
            pos += 1
            counts[mode] += 1
            if mode == 0:
                pos += nb
            elif mode == 1:
                pos += 4 + nb
            elif mode == 2:
                pos += 5 + nb
            elif mode == 3:
                pos += nb
            else:
                raise SystemExit(f"unknown mode {mode} in {path}")
    consumed = pos
    overhead = len(stream) - consumed
    return {
        "file": Path(path).name, "magic": magic, "w": w, "h": h,
        "tile_count": tile_count, "flags": flags, "backend": backend,
        "stream": len(stream), "consumed": consumed, "overhead": overhead,
        "modes": counts, "file_size": len(blob),
    }


def main(paths):
    rows = []
    for p in paths:
        rows.append(autopsy_file(p))
        r = rows[-1]
        print(f"{r['file']:40s} size={r['file_size']:>8d} stream={r['stream']:>9d} "
              f"consumed={r['consumed']:>9d} overhead={r['overhead']:>6d} "
              f"modes={r['modes']} flags=0x{r['flags']:x} backend={r['backend']}")
    tot_stream = sum(r["stream"] for r in rows)
    tot_size = sum(r["file_size"] for r in rows)
    print(f"TOTAL stream={tot_stream} file={tot_size}")
    import csv
    with open("/tmp/autopsy.csv", "w", newline="") as f:
        wr = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
        wr.writeheader()
        wr.writerows(rows)
    print("csv -> /tmp/autopsy.csv")


if __name__ == "__main__":
    main(sys.argv[1:])