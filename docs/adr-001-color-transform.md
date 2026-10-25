# ADR-001: Color-transform strategy for MNF6/MNF7

Date: 2026-10-25 (proposed by external analysis; implemented same day)
Status: adopted

## Context

The v0.7 finding "YCoCg-R is not byte-packable in a byte-oriented format"
was correct for STANDARD signed YCoCg-R (Co/Cg need 9 bits for RGB8
inputs: |r-b| <= 255 => 511 values) but overgeneralized: the family of
reversible transforms includes a MODULAR variant that performs every
lifting step with u8 wrapping arithmetic — a bijection over RGB8
triples that never needs more than 8 bits.

## Decision

1. Keep RGB8 as the canonical representation everywhere in the atlas.
2. Adopt modular 8-bit lifting as an OPTIONAL per-tile residual-color
   domain (MNF7 modes 4/5), selected by the encoder's cost model.
3. Retain standard YCoCg-R as MNF6/7 mode 3 (proper 9-bit bit-packed
   chroma planes — never 8-bit truncated, which breaks recoverability
   and was rejected outright).
4. Defer any wider-precision serialization (26-bit packed, entropy-coded
   signed planes) to a research branch; MNF7 mode 3 already provides
   the entropy-coded variant measured as the better literal coder
   elsewhere in the corpus study.

## Consequences

- MNF5/MNF6 files stay valid; MNF7 is strictly a decoder extension.
- Color-domain selection is proven reversible (exhaustive test) and
  measured: 0 selections across the 10-image benchmark corpus (the RGB
  residual already encodes that correlation efficiently) — the cost
  model therefore never pays for it when it does not win. Mechanism
  proven on structured-chroma-delta workloads by dedicated test.
- Shipping-gate evidence recorded in docs/format.md §MNF7 and
  docs/benchmarks.md (v0.7 tables remain the throughput reference).

## Alternatives rejected

- Truncating standard Co/Cg to 8 bits: NOT reversible; rejected.
- Changing the canonical representation of atlas patches to a color
  space: breaks the "atlas stays RGB8" integration invariant; rejected.
- Silently adding modes to MNF5: violates the MNF5 freeze; rejected
  (hence MNF7 magic).
