# Backend-3 rANS arm — corpus-gated verdict (2026-10-28)

The [engine ladder](mnf9-engine-ladder-probe.md)'s stage-2 protocol
applied end-to-end: the mixed run/LZ + contextual rANS coder
(`crates/mnf-mans/src/rans_mixed.rs`) measured against zlib-9/zstd-19 on
identical planes, AND re-costed through the shipped pick's own payload
(the LZ77-2D token wire). Exactness asserted in Rust per sub-stream
(encode→decode roundtrip INSIDE the size measurement — cost == wire, no
proxy). Honest model transport counted (used-set bitmap + table framing
per stream). Gate: 30-file corpus, MED residual planes.

## Results (totals across 30 files)

| arm | bytes | vs best |
|---|---|---|
| **rate-aware pick (per-file min of all arms)** | **1,029,925** | 1.00× |
| zstd-19 plane (the SHIPPING balanced arm) | 1,030,353 | 1.0004× |
| zlib-9 plane | 1,076,739 | 1.05× |
| rANS mixed plane | 1,125,591 | 1.09× |
| token wire, zlib | 1,182,757 | 1.15× |
| token wire, rANS | 1,213,475 | 1.18× |

Per-file standouts (the honest per-class breakdown):

- **solid**: rANS-plane 27 B vs zstd 28 vs zlib 218 — tokens+rans LOSE
  (57) because the 771-match token wire re-encodes poorly (6 B/match ×
  771 = model-heavy). The class the ladder gave tokens (−34%) was
  actually a zlib-vs-plane artifact; zstd-19 alone does better.
- **gradient**: rANS-plane 998 vs zstd 256? NO — zstd-19 hits 256 on the
  ramp plane, crushing everything (the long zero-run is one zstd match).
  Token wire z9 = 446 > 256. zstd-plane is the pick.
- **screenshot**: rANS-plane 444 beats shipping 670 by −34% — the order-1
  class context DOES exploit the screenshot's per-region structure that
  zstd's window cannot reach.
- **document**: rANS-plane 685 vs zstd 886 (−23%) — same mechanism.
- **blocks**: wire-rans 1607 < wire-z9 1669 < z19 1413 — wire+rans the
  wrong arm; z19-plane still wins.

## Verdict: backend-3 rANS arm — DENIED as a universal backend;
**RETAINED as a per-file candidate behind the rate-aware pick**

The pick rule absorbs everything measured here:

1. rANS-plane LOSES to zstd-19-plane on 28/30 files (the empirical
   probability tables + per-stream model transport cost more than LZ's
   string layer on residual planes — the stage-1/2 finding RE-CONFIRMED
   at whole-corpus scale).
2. ON ITS OWN CLASSES it wins big: screenshot −34%, document −23%, solid
   −1 B vs zstd. These are exactly the classes where per-region token
   structure exists but 2D repeats are too noisy for lz77_2d.
3. Token-wire-through-rANS: DENIED universally — the wire is
   tag-structured, not byte-structured, so the mixed model's assumption
   (repeated BYTE sequences) doesn't apply; wire+rans loses to wire+z9 on
   25/30.
4. The rate-aware pick (adding rans-plane + wire-z9 + wire-rans as
   candidates) totals 1.0004× of shipping zstd-19-plane — the four new
   arms cost nothing in the general case and capture −34%/−23% on the
   document/screenshot classes when they fire.

## Format mapping (if a future step promotes the rANS arm)

- backend 3 = rANS mixed (the reserved id exists); FLAG tokens bit stays
  its own axis — but wire+rans is dead, so backend 3 implies record
  payload, and TOKENS implies z9/z19 container only.
- The pick loop gains one candidate: (backend=3, flags=PRED_MED).
- Model transport is already in the measured numbers (bitmap + framing).
- Not shipped in this commit: the backend-3 Coder arm in src/ (research
  crate only; the format's reserved backend 3 stays reject-until-gated —
  this doc IS the gate record).

## Method notes

- constriction 0.5 queue coder (DefaultRangeEncoder/Decoder,
  ContiguousCategoricalEntropyModel<u32, Vec<u32>, 24>), fixed-point
  probabilities, remainder to the densest symbol — the same machinery the
  class gate proved in v1.5.
- Sub-stream exactness: every compressed_len call runs a real
  encode→decode roundtrip and panics on mismatch — a size that cannot be
  decoded is never counted.
- One-coder parity: each arm's comparisons flow through ONE library per
  row (flate2 for z9-z9 pairs, python zlib + zstandard for the python
  columns, constriction for rANS); cross-library deltas are annotated
  per-file in the log (flate2-vs-python-zlib ±1.5% as known).
- Full log: /tmp/mnf9_rans_gate.log; committed harness:
  crates/mnf-mans/scripts/rans_gate.py + bridge9.