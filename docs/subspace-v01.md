# MNF Subspace v0.1 — Experiment Results: Exact Low-Rank Predictive Atlas

Status: research branch (`mnf-subspace`), NOT a format change.
MNF5/MNF6/MNF7 frozen formats untouched.

## Question (review, ADR)

*Can a shared low-dimensional model encode image variation more cheaply
than conventional prediction plus entropy coding?*

## Method

All experiments in `crates/mnf-subspace` (discovery/basis/predictor
layers per §7):

- Discovery: power-iteration SVD, deterministic initialization and
  sign-canonical directions (fp, encoder-only).
- Basis: i16 fixed-point quantization at scale s=14 (q14), integer mean.
- Synthesis: `xhat = clip(mu_c + (sum B_jc q_j + half) >> s)`, i64
  accumulators, deterministic rounding. No fp in the decoder path.
- Residual: `(x - xhat) mod 256`; reconstruction `(xhat + r) mod 256`.
- Every test asserts exact reconstruction byte-for-byte.
- Economics: both streams framed identically then zlib'd (same coder
  both sides, mirroring the shipped codec's final zlib pass).

## Results (screenshot_640x400, 1000 tiles)

| Experiment | Stream | zlib bytes | vs A |
|---|---|---|---|
| A | literal tiles | 3,830 | — |
| B | MED residual | **3,135** | −18.2% |
| E3 | fixed 5-dir basis (planes+grads), coord search | 3,742 | −2.3% |
| E4 | learned PCA basis k=1/2/4/8/16 (basis overhead excluded) | 8,863 / 11,096 / 11,153 / 10,623 / 10,717 | +131%..+191% |
| E5 | MED + learned-on-MED-residuals k=8 (overhead excluded) | 6,383 | +66.7% |
| — | MNF5-atlas shipped best | 3,947 | (worse than plain MED) |
| — | MNF7 shipped | 1,823 | (flags/zstd/planar — the real winner) |

## Findings

1. **The conventional baseline dominates everything measured.** A plain
   MED + zlib beats the entire shipped MNF5-atlas pipeline (3,135 vs
   3,947) — the codec's current contribution is its container/planner,
   not its predictor.

2. **Fixed analytic subspaces: small but real.** 5 quantized directions
   bought 88 bytes (2.3%) over literals with zero basis overhead; it
   loses to MED. As a complement to MED (not tested here — see finding
   4) it remains unproven.

3. **Learned quantized bases are not yet viable.** PCA directions
   quantized to i16 q14 + coarse coordinate search synthesize poorly:
   the residual (and its zlib stream) grew +131–191%. The gap between
   fp-PCA and integer synthesis eats the model's value at every rank.

4. **Residual subspaces on top of MED also lose** (+66%): residuals
   from a good predictor are small and high-entropy; quantized
   synthesis error then dominates the residual zlib stream.

5. **Basis economics disqualify global learned bases** at this corpus
   scale: a 768×8 i16 basis serialized costs ~6,144 bytes; the whole
   image's best stream is only 3,135. η = (L_baseline − L_subspace) /
   L_basis is decisively negative. Sharing across ≥3 images' worth of
   identical-content tiles is not available in these corpora.

## Retention-gate decision (per ADR: retain only if gains survive)

Subspace modes do NOT retain a slot in a next frozen format on the
strength of these experiments. The two priority strategies the review
named (residual subspace, clustered learned) were the two that lost.

## What the experiments do mandate

- **MED/Paeth enters all future experiment matrices as the mandatory
  spatial baseline** (review configuration B): any new predictive
  mechanism must beat 3,135-class numbers, not 3,830-class ones.
- Coordinate-quantization research (finer fixed-point scales, b16
  coords, projection-refined coordinate search) is the specific knob
  where learned subspaces failed; a rerun with improved quantization
  is the cheapest possible continuation before any basis design work.
- The shipped MNF7's win came from entropy coder + planar layout +
  rate-aware flags — infrastructure, not learned prediction. The
  geometry-aware hypothesis remains open: the testable next step is
  D8-aligned clustering feeding PCA (review strategy 4), which no
  experiment here has isolated yet.

## Reproducibility

    cargo test -p mnf-subspace --release -- --nocapture

Every experiment prints one line per configuration; all assertions
must stay green (exactness is enforced, not assumed).
