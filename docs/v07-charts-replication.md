# v0.7 parametric charts — independent replication (2026-10-27)

The v0.7 experiment proposes per-tile parametric charts (rotations,
reflections, integer shears; 1 metadata byte/tile) vs the raster atlas.
Claimed: −25.2% on a photo crop, parity on gradient, no forced loss on
random. Our independent replication (own cost model, zlib-9 parity,
measured per-tile pick, metadata counted, 18 self-invertible transforms —
id, 3 rotations, 3 reflections, 10 cyclic shears) reproduces the PATTERN
and bounds the numbers:

| class (ours) | raster+atlas | charts+atlas | delta |
| --- | --- | --- | --- |
| photo-crop (curved repeat) | 9,413 B | 8,574 B | −8.9% |
| plaid (rotation-symmetric) | 839 B | 685 B | **−18.4%** |
| bottle-label (curved text) | 830 B | 778 B | −6.3% |
| repeated-gradient | 539 B | 539 B | 0.0% |
| random | 12,315 B | 12,315 B | 0.0% (pick rejects charts) |

## What this actually is (the mechanism)

A parametric chart applied to a TILE before reference/literal coding is a
**transformed reference** — exactly the proposal-review (Riemannian)
prediction: the metric/chart machinery collapses to "predictor context +
per-tile transform family". Our D8 (8 rotations/reflections) already
ships; the 10 integer shears are NEW members of that family, and they are
where the measured delta comes from: shears align curved/repeating
structure with the raster scan so the reference residual becomes sharper.

This reconciles two prior results that looked contradictory:
- v12: D8 geometric matching ≈ 0 gain (finding 3), sole true positive
  plaid (+0.5%) — because D8 was applied to EXACT-match search only.
- v0.7: charts (D8+shears) beat the atlas on curved content by 6–19%
  when applied to the PAYLOAD before residual coding AND measured per
  tile.

## Verdict against the proposal

- **Confirmed:** pattern replication on all five classes including the
  no-forced-loss property (the measured pick makes charts free on random).
  The shears (not rotations!) carry the gain on curved content: rotations
  are already in D8; shears are the addition.
- **Bounded vs claimed:** −6% to −18% in our replication vs −25% claimed —
  the exact delta depends on the crop's warp frequency and the transform
  family; our corpus is synthetic. Not validated at Rust level (their own
  Rust is "unverified"): the proposal's bundle is not in this repo.
- **Gap to v0.8 as proposed:** "shared geometric charts across patches" =
  arm P's shared-latent precondition, which our record bounds: pays on
  genuinely shared structure, loses on photos. A chart-space atlas is a
  new twist (matching IN transformed space), plausibly useful — it should
  be measured against MED-first arms, not atlas-only, per the standing
  rule (predictor-first matrices).

## Med-first matrix — MEASURED (decisive)

{MED} vs {MED + per-tile best-of-10-shears} (1 byte/tile, measured pick):

| class | med | med+shears | delta |
| --- | --- | --- | --- |
| photo-crop | 4,469 | 6,811 | **+52.4%** |
| plaid | 1,212 | 2,013 | **+66.1%** |
| bottle-label | 720 | 1,085 | **+50.7%** |
| random | 12,299 | 12,315 | +0.1% |

Shears in the LITERAL path lose catastrophically everywhere (+50–66%):
shearing a tile breaks raster continuity with its 2-D neighbors, so
in-tile prediction degrades even where intra-tile structure aligns.
Charts only pay inside the atlas-reference path (the reference is
transformed WITH the tile; residual then aligns). Combined with MNF8's
existing machinery (per-tile measured pick, med literals, D8 geom refs),
the concrete format question is narrow: **extend the D8 reference family
with the 10 integer shears** — measured per tile, rate-aware, bit-exact.
Expected gain class: curved/repeating content matched to a prior tile;
expected cost: nothing on non-matching tiles (pick rejects). This is the
same shape as FLAG_PRED_MED (measured, rate-aware) and is the only
format-worthy item out of v0.7. The −25% headline came from chart-vs-ATLAS
competition on a curved crop, not chart-vs-MED.