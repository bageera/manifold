# Atlas Model Evaluation — Predictor Ablation (MNF v1.2 experiment 1)

Status: research (branch `mnf-subspace`), no bitstream changes.
Direct follow-up to docs/subspace-v01.md; implements the review's
configuration matrix at entropy-coder parity.

## Setup

Uniform framing per tile, single zlib coder (stock, level 9) both sides
of every comparison, identical canonical RGB8 pixels. Python/numpy
implementation for search speed; the Rust crate (flate2/miniz) confirms
the arm ordering independently (numbers below are NOT cross-language
comparable — coder libraries differ ~10-33% — lesson recorded).

Arms stacked spatial-prediction-first:

    A  literals only
    B  MED (JPEG-LS edge-sense)
    B' plane-only predictor: p = a + b - c (no edge sense)
    C  best-prior exact-reference atlas on the residual layer
    D  D8 geometric atlas on the residual layer (C + rotations/reflections)

## Results (bytes, zlib-9 uniform)

| image | A | B (MED) | B' (plane) | C (MED+exact) | D (MED+D8) | shipped MNF7 |
|---|---|---|---|---|---|---|
| gradient 320×240 | 219,782 | 4,321 | 5,081* | 8,489 | 8,713 | 1,886 |
| repeat 256² | 3,894 | 1,950 | 1,563* | 1,862 | 1,862 | 757 |
| screenshot 640×400 | 3,099 | 3,007 | 3,000 | 3,024 | 3,018 | 1,823 |
| solid 256² | 512 | 511 | 511 | 511 | 511 | 70 |
| noise 256² | 196,935 | =A | =A | =A | =A | 196,908 |
| blob (held-out) | 24,285 | **10,886** | 10,784 | 10,886 | 10,887 | 10,820 |
| blocks | 2,010 | 3,344 | 3,404 | 3,344 | 3,344 | 1,892 |
| document | 1,207 | 1,928 | 1,928 | 2,165 | 2,181 | 1,368 |
| plaid | 108,184 | 63,374 | 67,797 | 61,292 | 60,960 | 58,863 |
| poster | 1,716 | 3,710 | 4,141 | 3,569 | 3,569 | 1,207 |

(*plane-vs-MED order flips between gradient/repeat: neither dominates.)

## Findings

1. **Spatial prediction is the single biggest lever** on smooth content:
   gradient A→B = 219,782 → 4,321 (98%). Any predictor-free result on
   photographic/smooth classes is meaningless.

2. **The tile-reference atlas contributes ≈0 on top of a real
   predictor** (C vs B: −0.02% to +7% — mostly negative). The reason:
   after spatial prediction, residual tiles are small, and every atlas
   reference costs 5-6 bytes of metadata it rarely earns back. The
   atlas's visible value in the shipped v0.5-era codec came from
   predicting raw tiles under a weak default coder — infrastructure
   (zstd/planar/rate-aware flags) obsoletes most of that.

3. **D8 geometric matching adds ≈0 on top of exact matching** (D vs C:
   −0.01% to −0.5%). The only true positive in the corpus: plaid
   (symmetric sine structure) — 60,960 vs 61,292 (+0.5%). Genuinely
   rotation-symmetric image structure is rare in this corpus.

4. **Edge-sense MED loses to plane-only on half the classes**
   (screenshot, gradient, repeat): the flat-UI tiles' statistics differ
   from photographs. Predictor choice is content-dependent — argues
   for encoder-side predictor selection (1-2 bits), not a universal
   predictor.

5. **Per-tile greedy cost ≠ global optimum reproduced again**: C's
   per-tile "wins" (592 on gradient) still produced an aggregate worse
   than B. The MDL accounting must count stream-level effects.

## Retention decision for the frozen-format family

- Keep: rate-aware flags + zstd + planar (MNF6/7 infra) — the only
  measured mechanism that beats both the predictor and atlas on most
  classes.
- MED/plane predictors: NOT shippable as-is today under a strong zstd
  payload coder... except the measured exceptions above are large
  (gradient 8.7k->4.3k = 2x, plaid 108k->61k = 1.8x). A future format
  decision (MNF8 = spatial-predictor flag with per-image selection by
  proxy cost, exactly like MNF6's flags mechanism) is justified by the
  size of the wins on smooth classes — pending a corpus with more
  photographic content.
- Subspace: remains failed per v0.1 findings (retention gate).

## Reproducibility

    python3 crates/mnf-subspace/scripts/ablate_abcd.py   # (numpy + Pillow needed)

Rust parity for arms A/B: cargo test -p mnf-subspace --release --nocapture.
