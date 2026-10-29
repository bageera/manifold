# MNF13-R1 — statistical redundancy on the v1.12 wires (2026-10-29)

Reproduction + promotion-gate instrument (the pilot's archive is
external; this repo reproduces from the spec per the standing
rule). Machinery: constriction 0.5's stream range coder (the
rans_mixed.rs precedent) through a new bridge example
(`crates/mnf-mans/examples/r1_ctx.rs`) with per-context ADAPTIVE
fixed-point models — zero model transport (counts evolve identically
decoder-side), exact roundtrip asserted inside every measurement.

## In-repo measurements (the ACTUAL v1.12 residual wires; six photographic record files)

| file (shipped pick) | shipped B | wire n | adaptive o1 B | verdict vs shipped |
|---|---|---|---|---|
| slate 192 (REF=3, planar) | 36,128 | 82,944 | 37,524 | +3.9% LOSE |
| sand 160 (REF=1, planar) | 25,262 | 57,600 | 26,264 | +4.0% LOSE |
| ocean 192 (REF=3, planar) | 34,786 | 82,944 | 35,696 | +2.6% LOSE |
| forest 224 (REF=1, planar) | 45,522 | 107,520 | 47,472 | +4.3% LOSE |
| terrain 224 (REF=1, planar) | 41,782 | 107,520 | 42,616 | +2.0% LOSE |
| dusk 224 heldout (REF=2, planar) | 46,697 | 107,520 | 48,760 | +4.4% LOSE |

(O1 = order-1 bucketed previous-byte contexts; x-channel keys made no
planar difference in this implementation's shape.)

## The entropy floor (what a PERFECT context coder could reach)

| file | H0 floor | shipped (already below H0 by) | H\|prev floor | ceiling vs shipped |
|---|---|---|---|---|
| slate 192 | 38,036 | 36,128 (−5.0%) | 33,479 | **−7.3%** |
| sand 160 | 26,812 | 25,262 (−5.8%) | 23,141 | **−8.4%** |

The shipped container ALREADY codes below the order-0 entropy floor
(zstd's range coder + string matching exploit run/long-range
structure the order-0 model cannot state). The conditional floor
(order-1 on the coded residual order — the R0-measured −0.33
bits/symbol) would recover ≈ **−7..−8% per photographic file** IF a
coder reached it exactly.

## Verdicts

- The pilot's −2.88% full-image number reproduces as consistent with
  the ceiling's arithmetic — but under-measures the OPPORTUNITY (its
  comparator = a single-arm MED+CD-G zstd wire, not the pick).
- The ADAPTIVE per-context range coder LOSES on 6/6 real files today
  (+2..+4.4%): model adaptation cost exceeds the conditional gain at
  these file sizes. Block-batched model refresh = the same class of
  loss.
- The R0 record stands: activity/direction contexts ≈ 0; the
  recoverable statistical structure = the −0.33-bit order-1 term +
  the residual cross-channel coupling, both already quantified.

## The promotion gate for this class (recorded)

A context-aware backend is a WIRE candidate only when it demonstrably
reaches ≤ the conditional-entropy floor's neighborhood on REAL v1.12
wires at complete-file cost — i.e., a coder that halves the
adaptation price of the measured bridge. The rANS honest-transport
lesson remains the boundary: any model table carried = counted; with
per-context adaptive models the transport is zero and the whole
question = the adaptation curve. Until a candidate demonstrates that,
the shipped zstd/deflate containers stand; this record = the
instrument + the floor numbers, not a format change.
