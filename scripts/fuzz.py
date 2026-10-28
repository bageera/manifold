#!/usr/bin/env python3
"""Mutation fuzzer for the MNF5 decoder (CI-friendly, bounded).

Encodes every corpus image (with and without --geom), applies deterministic
mutations (bit flips / truncation / appends), and asserts the decoder never
panics (exit 101). Structurally valid results are allowed but counted.
"""
import argparse
import random
import subprocess
import sys
from pathlib import Path

BIN = Path(__file__).parent.parent / "target" / "release" / "manifold"
TD = Path(__file__).parent.parent / "testdata"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--trials", type=int, default=600)
    ap.add_argument("--seed", type=int, default=2026)
    args = ap.parse_args()
    if not BIN.exists():
        print(f"missing {BIN}; run cargo build --release first", file=sys.stderr)
        return 2

    images = sorted(TD.glob("*.png"))
    if not images:
        print("no testdata images", file=sys.stderr)
        return 2

    rng = random.Random(args.seed)
    panics = accepted = rejected = 0
    total = 0
    for img in images:
        for extra in ([], ["--geom"], ["--mnf6"], ["--mnf8"], ["--mnf8=maximum"]):
            subprocess.run(
                [BIN, "encode", str(img), "-o", "/tmp/fz_base.mnf5", *extra],
                check=True, capture_output=True,
            )
            base = Path("/tmp/fz_base.mnf5").read_bytes()
            for trial in range(args.trials // (len(images) * 3) + 1):
                b = bytearray(base)
                kind = trial % 3
                if kind == 0:
                    for _ in range(rng.randint(1, 4)):
                        b[rng.randrange(len(b))] = rng.randrange(256)
                elif kind == 1:
                    b = b[: rng.randrange(1, len(b))]
                else:
                    b += bytes(rng.randrange(256) for _ in range(rng.randint(1, 50)))
                Path("/tmp/fz.mnf5").write_bytes(b)
                r = subprocess.run(
                    [BIN, "decode", "/tmp/fz.mnf5", "-o", "/tmp/fz_out.png"],
                    capture_output=True, text=True,
                )
                total += 1
                if r.returncode == 0:
                    accepted += 1
                elif r.returncode == 101:
                    panics += 1
                    print(f"PANIC {img.name} {'geom' if '--geom' in extra else 'mnf6' if '--mnf6' in extra else 'mnf8max' if '--mnf8=maximum' in extra else ''} trial {trial}: {r.stderr[-200:]}", file=sys.stderr)
                else:
                    rejected += 1

    print(f"fuzz: {total} trials, rejected={rejected} accepted={accepted} panics={panics}")
    return 1 if panics else 0


if __name__ == "__main__":
    sys.exit(main())