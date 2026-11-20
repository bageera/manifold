# MNF-HP8-0 — Fresh production computational census

Program: MNF-HP8 (entropy representation research; initial authorization
§22: HP8-0, P1/P2, BT0–BT3, S). Repository revision 82ea70d (v1.17.1 line).

## 1. Production baseline verification (MEASURED)

- Frozen corpus pick re-verified in the HP0 gauntlet (docs/mnf-hp0-record.md):
  MNF8-maximum = 768,303 B, 30 files, byte-exact reproduction.
- Full release test gate green at the HP-gauntlet commit: 105 passed,
  0 failed, 21 ignored (env-gated research instruments).
- Historical numbers cross-checked against committed records: corpus
  768,303 ✓, precision 24-bit ✓ (2^24 fixed point, constriction
  ContiguousCategoricalEntropyModel<u32, Vec<u32>, 24>), HP2's Q16 delta
  −187.7 bits/360k rows ✓ (recorded −0.52/krow), HP2 flatness 1,056 ideal
  bits ✓, BZ16/HP2 NEGATIVE verdicts ✓.

## 2. Fresh stage timing (MEASURED, this session, release build, current tree)

Re-ran the P1-B decomposition instrument (mnf17_p1a_census::p1b_decomp) on
the screening cohort (solid/gradient/repeat flushed at analysis time;
instrument run continues per-file; full CSV
research/results/hp8-computational-census.csv, per-backend summary
hp8-computational-census-summary.csv). Record-stage per-symbol timings
(n = 20020452 symbols aggregated):

| backend | ns/symbol (mean) | min | max | twin-loop share |
|---|---|---|---|---|
| 4 (mix4) | 742.9 | 663 | 951 | 100.2% |
| 5 (mix5) | 743.3 | 655 | 950 | 100.4% |
| 6 (mix6) | 756.2 | 660 | 945 | 99.7% |
| 7 (mix7 lane) | 761.5 | 654 | 931 | 100.1% |

prep+seal ≈ 0; walk_excl separate. The model loop carries the whole
per-symbol cost — consistent with MNF17-P1-B (~99%) on the current C4-A-shipped
tree. Backend-7 lane-walk adds ≈ +2.5% over backend 6 in-stream (walk rows).

## 3. Static cost profile per backend 4–7 (from source at §1 revision, MEASURED structurally)

- Probability precision: 24 bits (2^24), u32 cdf cells, u64 state coder.
- Probability outcomes per decision: 256-symbol categorical (per byte).
- Active contexts: 16 / 20 / 20+16 lane / 40 (backends 4/5/6/7).
- Model-state bytes: MixCtx ≈ 4,104 B ⇒ 65.9 / 82.4 / 148.7 / 164.8 KB per
  encode (transient); counters FAST+u64 SLOW ×256, EWA weights, maintained
  Σfast/Σslow (C4-A).
- Per symbol: 1 context lookup (trigram key), probs() = 2×256-word fixed
  point + rebalance, EWA mix when parent live, u32 cast → constriction CDF
  build (256 cells) + 1 coder call; step = counter update + halving checks +
  maintained-sum update; share = parent-step. model_calls = symbols
  (1 model build per byte).
- Encoder/decoder throughput (uninstrumented): 105.2 MB/s in / 141.2 MB/s
  out record arms (1.28 µs/byte = 2× the twin's 743 ns walk — measurement
  overhead split per instrument).

## 4. Gate

PASSED — baseline verified, costs attributed on the current implementation,
research-only instrumentation (env-gated #[ignore]), production outputs
unchanged (no encoder/decoder semantic edits).
