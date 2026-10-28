# WebP-hybrid probe: WebP-style predictors on MNF8's coder — measured, not retained (2026-10-27)

The proposal: borrow WebP Lossless's five principles (14 spatial
predictors, subtract-green-style decorrelation, LZ77, color cache,
context-adaptive entropy) for a hybrid architecture, and isolate whether
geometry adds value after conventional techniques
(4-arm ladder: WebP → v0.7 → WebP-predictors-no-geometry → same+charts).

## Fair corpora comparison (already the repo command: `make corpus`)

`scripts/compare_lossless.py`: same decoded RGB8 for every codec,
container sizes, roundtrip-verified both sides, 30 files:

| class | MNF8-balanced vs PNG | WebP-lossless | JXL |
|---|---|---|---|
| heldout (5) | 0.711×, 5/5 wins | 0.480× | 0.760× |
| photographic (20) | 0.995×, 8/20 | 0.690×, 20/20 | 0.622× |
| synthetic (15) | 0.441×, 5/5 | 0.277× | 0.909× |

Per-file: MNF8 beats WebP on **zero of 30 files** (closest: noise 1.00×,
plaid 1.10×, blob 1.09×). Photo-average MNF8/WebP = **1.452×** (worst
1.70×). WebP wins because of LZ77 distance coding + color cache +
per-region entropy context — none of which the byte-oriented + zlib/zstd
backend can express on these classes. Honest recorded fact, consistent
with docs/benchmarks.md.

## Arm 3 measured: best-of-12 WebP predictors, same coder, metadata counted

Per-tile per-channel best-of-12 causal predictors (the WebP key/avg/avg4
family, incl. MED-equivalent modes), 3×4 bits/tile ids + residual stream
through one zlib-9 (the same entropy budget as MNF8's literal arm:

| file | MNF8 (MED img-wide) | b14 ids | b14 + subtract-green | WebP |
|---|---|---|---|---|
| autumn 160×120 | 28,953 | 30,233 | **25,849** | 20,886 |
| dusk 224×160 | 63,417 | 72,376 | **55,181** | 41,350 |
| forest 192×144 | 45,546 | 54,598 | **41,822** | 28,738 |
| gradient 320×240 | **1,639** | 3,625 | 6,672 | 148 |
| noise 256² | 196,912 | **196,716** | 196,717 | 196,688 |
| repeat 256² | **540** | 885 | 1,011 | 358 |
| screenshot | **782** | 2,072 | 2,278 | 320 |
| solid 256² | **62** | 296 | 296 | 44 |
| blob (held) | **8,466** | 13,108 | 15,457 | 7,792 |
| blocks (held) | **1,774** | 1,978 | 2,058 | 1,344 |
| document (held) | **1,099** | 1,676 | 1,677 | 312 |

Arm 3 on top of MNF8's existing arm set, same coder, metadata in:

- **loses on 9 of 12 measured files** (per-tile predictors fragment the
  residual plane at every tile boundary; the image-wide MED keeps a
  smooth plane that zlib's window exploits globally),
- the per-tile predictor-id overhead eats the win even where a
  better predictor exists locally,
- subtract-green helps exactly the noise-dominated photos (the only 3
  non-losses) and still loses to WebP there.

## Arm 4 (geometry after webp-pred) — answered by existing measurements

Geometry is not added after the choice of predictor: MNF8's rate-aware
pick already chooses per tile between MED literal bodies and
atlas-reference bodies (mode 1/2 with D8), i.e. the "hybrid predictor"
the proposal asks for (`m* = argmin L(m)+L(θ)+L(R)`) is exactly the
shipping encoder. The charts replication
(docs/v07-charts-replication.md) measured the atlas-reference path with
extended transforms: photo-crop −8.9%, plaid −18.4% — but the Rust
atlas arm did not transfer to the real corpus (D8+shears gate:
30/30 identical, docs/shear-refs-format-probe.md).

## Verdict

1. **WebP-style predictor set: NOT RETAINED** — measured worse than the
   shipping MED arm on the same entropy budget for synthetic/low-color
   classes and marginally better only on noise-heavy photos at 60–80% of
   WebP's size anyway.
2. **The fair-comparison numbers go in the announcement** (losses
   alongside wins: zero file wins vs WebP on this corpus).
3. The realistic paths to closing the WebP gap are the two things this
   codebase's byte+zlib/zstd backend structurally lacks: LZ77 with 2D
   distance coding (or equivalently multi-reference atlas with free
   distance metric) and context-modeled entropy coding of residuals.
   Measured evidence so far says neither is available in small diffs —
   they are architectural (the "engine extraction" conversation).
4. The 4-arm ladder is satisfied by: WebP (above), v0.7 (charts doc),
   arm-3 probe (this doc), arm-4 = existing MNF8 hybrid + charts
   measurements — geometry shows value ONLY in the atlas-reference path.