# Dimensional Folding — first measured probe (2026-10-27)

Status: research; no format item; gate applies. Companion to
docs/cryoem-plan.md (arm P/S/F). This probes the proposal's own §7 contract:
**reversible 3D→2D reindexing is not compression; the question is whether
fold-space model+residual beats both the plain coder and in-plane prediction
on the same bytes, WITH path metadata counted.**

## Arms measured (zlib-9 parity, exact recon asserted everywhere)

- base: raw depth-last 3D stack → zlib-9 (the honest plain-coder baseline;
  NOT raw+no-coder: every claim must survive "zlib on the same bytes").
- naive fold `F(x,y,z)=(x+Wz, y)` → zlib-9 (the proposal's §1 bijection),
  0 path bits (deterministic).
- Morton (z-order) fold → zlib-9 + path id bits.
- depth-chain: slice k coded as (slice k-1) + mod-256 residual → zlib-9
  (v12-equivalent delta chain along z; 0 path bits).
- slice-atlas: per-slice measured best prior + packed index bytes (CY01
  machinery along depth).
- slice-rule: one slice literal + repeat count (the §5 '256 identical
  copies' structural mode; discovery = byte-equality detector, honest).

## Results (32×32×D and 16³ volumes)

| class | base | fold | morton | depth-chain | slice-atlas | slice-rule |
| --- | --- | --- | --- | --- | --- | --- |
| identical slices (32³) | 2178 | 1302 (0.60) | — | 2183 (1.00) | 1200 (0.55) | **1041 (0.48)** |
| identical (16³) | 376 | — | 1582 (4.21) | — | — | **170 (0.45)** |
| smooth depth-drift (32×32×64) | **5773 (0.088 B/px)** | 39997 (6.93) | 39997-class → worse at 16³ (4.08) | 10280 (1.78) | 13593 (2.35) | n/a (not identical) |
| depth-correlated 16³ drift | **545** | — | 2223 (4.08) | 457 (0.84) | — | — |
| photo-like (independent smooth slices ×64) | 62837 | 46252 (0.74)* | — | 65274 (1.04) | — (plane-dy 34619 = 0.55*) | n/a |
| random bytes | 32784 | 32784 (1.00) | — | 32784 (1.00) | 32808 (1.00) | n/a |

\* on photo-like, the honest comparison is plane-dy (in-plane vertical
delta) 0.55× base vs fold 0.74× — **fold loses to the plain in-plane
baseline on the same data**. Fold's apparent ratio vs base is a coder
artifact (stacking slices as rows changes zlib's window contents), not a
representation win.

## Verdicts (honest, measured)

1. **Reindexing folds (naive, Morton) never win.** Best case = no-op on
   random/solid; on correlated depth content they are 4–7× WORSE than base.
   Mechanism: a bijection preserves bytes; the coder sees the same
   information re-ordered, and z-order destroys the byte-locality the LZ
   window exploits. Path bits add cost for nothing. This confirms the
   proposal's own §1 and the repo's standing lesson: representation
   *reordering* is not a compression mechanism. NOT RETAINED (no format
   item, no further arm).

2. **Structural repeat-rule (slice-rule) is the real win where it applies:**
   −52% (0.48×) vs the best conventional arm on identical slices at 32³,
   −55% on 16³. The mechanism is NOT folding: it is a measured repetition
   detector + count-vs-byte-stream coding (zlib codes 64 copies as ~254
   ×258-byte matches ≈ 2 kB; the rule codes the *count*, not the stream).
   Generalizes the record-grammar repeat concept from tiles to slices.
   **Candidate format item for the MNF voxel research track** — pending
   noisy-repetition robustness (identical-with-±1 noise) and real MRC
   corpus confirmation.

3. **Depth-chain ≈ no-op on honest depth-correlated content** (0.84× of
   base at 16³ = the small-volume framing penalty; 1.78× at 32×32×64 where
   zlib's own window already reaches 32 slices back). The repo's v12 lesson
   repeats at the third axis: delta chains lose to LZ's string layer
   wherever the bytes are LZ-friendly. NOT RETAINED.

4. **Slice-atlas loses on every non-identical depth class** (2.35× on
   scaled drift; 7.74× on cyclic-phase without the phase machinery).
   The atlas only pays when residuals are near-zero — same boundary as v12.
   NOT RETAINED as a general arm; subsumed by the slice-rule detector +
   CY01-style phase machinery (torus-on-slices: **0.26× base on cyclic-phase
   slices, phases recovered exactly, roundtrip exact** — the one clean win
   beyond the repeat-rule, same mechanism class as CY01, retained as
   research evidence for arm P's multi-view claim).

## The honest bottom line

The fold transform itself is not a compression mechanism — measured
across every data class, reindexing either ties base or loses catastrophically
(up to 7× worse), and path metadata only adds cost. The wins that DO exist
(repeat-rule −52%, phase-atlas−74%) are structural-redundancy modes that
happen to be describable through a fold — the redundancy is the mechanism,
the fold is incidental. This lands the same verdict as v12's atlas study
but at a third axis, and it sharpens arm P: the multi-view case should be
attacked with SHARED-MODEL + MEASURED-COORDINATE machinery (phase/repeat
discovery), not geometric re-layout.