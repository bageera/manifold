# MNF13-R5 — dual-timescale adaptive coding on the v1.12 wires (2026-10-29)

R4's recommendation executed: per-context FAST and SLOW adaptive
estimators (halving caps 4,096 and 65,536) mixed by an
exponentiated-weight rule updated per symbol from the DECODED symbol
(log-space EWA, lr 0.15 — deterministic, zero transport, identical
evolution both sides = exact inverse). Machinery = constriction
0.5's stream range coder through the new bridge
`crates/mnf-mans/examples/r4_ctx.rs` (exact roundtrip asserted inside
every measurement; r1_ctx.rs = the R1 instrument, kept).

## Corpus gate (all 30 files, complete serialized cost, never-worse pick)

The arm replaces ONLY the record-mode payload's entropy backend
(backend id 4 = the mix coder; contexts = the previous coded
residual byte bucketed to 8 keys — decoder-side causal, zero
metadata); token-wire files and the MNF9 solid keep their shipped
pick.

- Shipped 832,036 → mix-pick **817,057 = −14,979 B (−1.80%)**
- The mix arm FIRES AND WINS on **19/19** photographic record files;
  every other file keeps its shipped arm (no regressions anywhere —
  never-worse by construction)
- Largest per-file wins: forest224b −1,511 (−5.9%), autumn-h −1,472
  (−6.2%), autumn −1,352 (−6.7%), terrain224 −1,346 (−3.2%),
  slate160 −1,101 (−4.9%)
- Encode cost of the mix coder: 0.2 s for a 107 kB wire (vs the ~5 s
  CLI encode total today) — decode = the same inverse walk

## Mechanism attribution (R4's table, measured)

- The R1 adaptive coder (single per-context estimator, cap ~4096)
  LOST +2..+4.4% on the same six files (the R1 record).
- The ONLY change = the second (slow) estimator + the per-symbol
  EWA latch: 6/6 flips to WIN. The adaptation-rate compromise R4
  named is exactly the recovered value: log_w clamps to pure-slow on
  stable stretches and follows fast after shifts (its curve is
  decoder-visible — the weight's evolution is itself compressible
  context, not transmitted).
- The corpus-coder remains ~1.5-2% above the R0/R1 conditional-entropy
  floor (−7..-8%/file) — the remaining adaptation price; the
  quantization (8 ctx keys, fixed-point 2^24 scale) = the next
  measured knobs.

## Disposition by the standing gates

- Complete-file improvement over the shipping portfolio's actual
  output: **−1.80% corpus, wins concentrated on the byte-heavy
  photographic class, zero regressions** — the biggest validated rung
  since ADR-008 (7× the v1.12 selector's −0.26%).
- Standalone exact decode: asserted per file by the bridge; the
  format promotion (backend id 4 dispatch + validation + conformance
  vectors + fuzz variant + docs surface) = the next session's
  numbered rung, per the freeze protocol.
- The honest-transport rule: zero model bytes carried (adaptive
  models); the wire = the 32-B MNF8 header + the mix-coded payload;
  every piece the decoder needs recomputes from decoded symbols.
