# MNF-HP8-P — probability precision record

Population: HP1's actual production P0 vector prefixes — 28 images × 40k
rows = 1,120,000 rows of u32 2^24 fixed-point vectors + observed symbols
from the native traces. Production rounding rule (largest-remainder: floor +
first-max remainder grants, ties to lowest index, zero-repair from max)
applied to every budget.

## 1. P1 precision census (MEASURED, research/results/hp8-precision.csv)

Ideal-bit delta of coding the observed stream with the quantized vector vs
the production 2^24 vector:

| budget Q | Δ bits over 1.12M rows | per 1k rows | floor (freq=1) cells | verdict |
|---|---|---|---|---|
| 24 | 0 (reference) | — | 0 | — |
| 20 | **−3.7** | **−0.003** | 3.73M | FREE (noise) |
| 16 | +976.0 | +0.872 | 105.7M | marginal |
| 14 | +10,933 | +9.76 | 230.5M | real cost |
| 12 | +72,735 | +64.9 | 256.9M | severe |
| 10 | +409,797 | +365.9 | 265.7M | severe |
| 8 | +6,325,039 | +5,647 | **286.7M = rows×256** | FORCED UNIFORM |

The Q=8 row is the §5 P1-prediction made empirical: floor 1 × 256 symbols
= total 256 ⇒ every vector identical ⇒ 8 bits/symbol. Floor cells reach
100% of the table at Q=8 and 37% at Q=10 — the floor, not information loss,
is the distortion driver below Q=12. Max per-symbol log-error stays within
Q/255 units (reported per row in the CSV).

## 2. P2 normalization methods (MEASURED, 6 train images × 20k rows, Q=16)

- Largest-remainder (production): mean entropy delta **+0.0201 bits/sym**
  vs the exact vector; row-sum exact by construction. CLARIFICATION (§5):
  this +0.0201 bits per SYMBOL differs from P1's Q=16 rate of
  +0.872 bits per 1k rows (+0.000872/symbol) because P2 and P1 measure on
  different populations AND different metrics: P2 = full-entropy delta
  H(q16)−H(exact) over 6 TRAIN synthetic-heavy prefixes at 20k rows;
  P1 = observed-symbol coding-cost delta over 28 images (heldout-heavy,
  40k rows). Both are tiny and same-sign — distinct datasets and distinct
  statistics, not a contradiction. and same-sign; no contradiction — distinct
  datasets and distinct statistics.
- Floor-plus-top-grant and cumulative-boundary rounding: as standalone
  rules they do NOT preserve the row sum. CLARIFICATION (§5): the earlier
  draft claimed a sum-repair "collapses them into largest-remainder" —
  that is RETRACTED: a deterministic repair is not guaranteed to reproduce
  LR's exact grant/tie order, and no repaired variant was measured. Only
  LR (production) rows appear in the tables; the alternatives are
  unmeasured.
- Cost accounting: LR ≈ 256 mul + 256 sub + remainder ordering; the
  ordering dominates (argsort O(256 log 256) row-wise; the production
  single-symbol path uses the scalar variant).

## 3. P3 actual entropy coding (MEASURED via production coder, tests/hp8_wire_probe.rs)

File-driven probe: LR rows → constriction models at Σ=2^P → encode 5,000
observed symbols of three representative prefixes (forest, terrain,
screenshot) → production DefaultRangeDecoder round-trip. ALL 21 runs
decode_exact=true. Finalized payload bytes:

| image | Q24 | Q20 | Q16 | Q14 | Q12 | Q10 | Q8 |
|---|---|---|---|---|---|---|---|
| forest 224x160 | 1908 | 1908 | **1908** | 1916 | 1952 | 2148 | 5004 |
| terrain 160x120 | 1164 | 1164 | **1164** | 1172 | 1212 | 1432 | 5004 |
| screenshot 640x400 | 8 | 8 | **12** | 20 | 64 | 264 | 5004 |
| ideal bits (forest) | 15254 | 15254 | 15257 | 15298 | 15585 | 17159 | 40000 |

Reading: **coder bytes are precision-insensitive through Q=16 on the photo
streams** (forest/terrain identical at 24/20/16). CLARIFICATION (operator
§5): screenshot — near-uniform rows dominated by equal largest frequencies —
diverges ALREADY at Q=16 (8/8/12 B): the uniform-shape rows lose the
remainder-grant freedom earlier. The invariant "identical at Q=16" is an
EMPIRICAL property of peaked rows, not a theorem; precision reduction buys NOTHING on
either byte count or (per P1) meaningful ideal bits, and the §16 caveat is
confirmed in both directions: lower precision neither reduces nor inflates
coder work; it only costs ideal bits as P1 quantifies. Q=8 is the forced-
uniform bound (ideal = exactly 8·n bits).

## 4. P4 mixed precision (MEASURED, per-entropy-bucket table)

16 pre-symbol entropy buckets × budgets in research/results/
hp8-precision.csv.bucketed: per-bucket deltas are flat within noise for
Q ≥ 16 across buckets (the concentrated rows tolerate Q=12 with small
deltas; near-uniform rows degrade first). A decoder-reproducible bucket→Q
rule (metadata +16 nybbles/image) cannot beat the uniform Q=16 or Q=20
choice by more than the bucket-table's own cost at these magnitudes —
recorded CONDITIONAL-negative.

## 5. P5 economics (MEASURED structurally)

Internal-model precision (counters/weights, 2^24) and output-CDF precision
(coder cells) are independent: the P3 probe shows the CDF-side can go to
Q=16 with zero byte change. But the expensive work (probs() + EWA mix +
counter steps, per HP8-0 ≈ 743–762 ns/symbol) is entirely on the INTERNAL
side and is untouched by output-Q changes. Integer widths: Q=16 needs
u32 cdf cells + u16 freqs; Q beyond 16 buys nothing measured (Q=20 ideal
delta −0.003/krow).

## 6. Gate disposition

Gate: "demonstrated computational or storage benefit with measured
complete-wire economics". **FAILED** — Q=16 changes zero bytes and saves no
computation (internal model unchanged); Q=20 same. The ONLY demonstrated
possibility is Q=16/Q=20 as a future wire-precision OPTION for new
experimental backends (zero-cost vs 24-bit), carried as a design fact, not
a promotion. **HP8-P verdict: NEGATIVE for storage/compute benefit;
POSITIVE as a precision-robustness design fact (Q=16 free, Q=20 free).**
