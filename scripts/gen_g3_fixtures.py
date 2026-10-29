#!/usr/bin/env python3
"""MNF10-G3 (Track B shared-model discovery) controlled fixtures.

Jon's test design: (1) a real photograph with repeated structural
features; (2) a CONTROLLED variant where several patches are generated
from exactly the same mathematical rule (integer phase-lattice texture,
the CY01 mechanism), plus a 2%-corrupted variant (CY01's measured
robustness class). These separate implementation capability from
practical value on natural photography.
"""
import numpy as np
from PIL import Image
from pathlib import Path

ROOT = Path(__file__).parent.parent
OUT = Path("/tmp/g3")


def main():
    OUT.mkdir(exist_ok=True)
    photo = np.asarray(Image.open(ROOT / "testdata/photos/sand_160x120_s900000.png").convert("RGB"), dtype=np.uint8)
    Image.fromarray(photo, "RGB").save(OUT / "g3_photo.png")
    h, w = photo.shape[:2]
    # ONE 32x32 texture tile at a textural patch (mid-image), displaced to 5
    # disjoint sites at integer offsets (the exact shared rule: translation)
    ty, tx = 44, 64
    tile = photo[ty:ty + 32, tx:tx + 32].copy()
    sites = [(8, 8), (44, 8), (80, 8), (8, 120), (44, 120), (80, 120)]
    variant = photo.copy()
    for (sy, sx) in sites:
        variant[sy:sy + 32, sx:sx + 32] = tile
    Image.fromarray(variant, "RGB").save(OUT / "g3_shared_exact.png")
    rng = np.random.RandomState(21)
    corrupt = variant.copy().astype(np.int16)
    mask = rng.rand(h, w) < 0.02
    for c in range(3):
        corrupt[:, :, c][mask] = np.clip(corrupt[:, :, c][mask] + rng.randint(-3, 4, mask.sum()), 0, 255)
    Image.fromarray(corrupt.astype(np.uint8), "RGB").save(OUT / "g3_shared_2pct.png")
    print(f"fixtures: photo {w}x{h}; shared-exact {len(sites)} sites; corrupted {mask.mean():.3f}")


if __name__ == "__main__":
    main()
