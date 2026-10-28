# Riemann Array v0.5 exp.2 — shear-wrap (toroidal) MED probe: DENIED (2026-10-27)

The proposal (Riemann Array v0.5): connect opposite boundaries of the
raster (flat torus under a unit shear — the causal, derivability-safe
version), let the x=0 column's west/northwest come from the previous
row's tail, and measure MED residual cost. One flag bit, whole-file
pick, same zlib-9 both sides (the agreed scope after the torus/fold
probes).

## Results (30-file corpus, metadata counted)

| class | raster-MED | shear-wrap | delta |
|---|---|---|---|
| photos (14) | — | — | +0.01…+0.11% (all LOSE slightly) |
| heldout synthetics (5) | — | — | +0.00% |
| noise / gradient / screenshot / solid / repeat | — | — | 0.00% / 0.00% / 0.00% / 0.00% / **+0.49%** |
| **TOTAL** | 1,076,739 | 1,077,041 | **+0.03%** |
| files where wrap wins | — | — | **0/30** |

Best-case constructed class (diagonal-stripe textures, where each row's
head continues the previous row's tail — the strongest possible wrap
content): repeat/tileable P=16 → **−0.6%**; non-tileable P=17 → **−1.5%**.

## Why the ceiling is structural (1/W, not tunable)

The wrap only reassigns prediction context at column x=0 — 1/W of the
plane (≈0.4–0.8% of pixels). The mechanism cannot exceed ~±1% on ANY
image, and the direction depends on whether row-heads actually continue
row-tails (photos: they don't; MED's north context at x=0 is strictly
less noisy than a 191-px-distant tail: +0.0…+0.1% measured loss). On
plaid_192 the fixture's column x=0 is constant, so every context is
exact and no wrap can change anything (verified: 0 prediction diffs).

This confirms the pattern from the other six geometry probes with an
even sharper bound: **topology is a 1/W-sized lever on a W-px-wide
plane**; periodic connectivity of boundaries can't reorganize
information the coder already reaches. Torus-on-noise (fold probe),
metric-guided traversal, metric-warp predictor, conformal guidance, and
now shear-wrap prediction all deny the same family from different
angles.

## Verdict

NOT RETAINED — no format item, no flag. The Riemann Array v0.5
experiments close as:

1. **Cyclic seam optimization**: bounded analytically (bits(largest
   residual) − log2 N; only pays on "one big cut, otherwise flat" 1D
   signals — the fold probe's slice-rule finding on synthetics)
   + measured 0/30 on this corpus via the same mechanism → closed.
2. **Toroidal spatial prediction**: this probe → DENIED (0/30 files;
   ≤1.5% on constructed diagonal textures).
3. **Metric-weighted periodic prediction**: covered by the five prior
   geometry-family denials + the anisotropic orientation tiebreak
   record (single document-class effect) → closed.

The 4D/6D torus-embedding program (§6 of the proposal) inherits these
bounds: embeddings change descriptions, not information; every
measurable effect of the connectivity exists in the 2D case already
measured.