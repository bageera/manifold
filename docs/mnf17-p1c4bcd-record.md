# MNF17-P1-C4-B/C/D — fused reductions, direct CDF, combined cells

**Status**: all four cells driven through the differential harness
(exactness clean) and the factorial cohort (measurements final).
Result: **only C4-A's counter sums pay**; C4-B's pass fusion and C4-C's
direct-CDF hypothesis are measured non-improvements. Zero wire
divergences in any cell anywhere (576 timed encodes + all fixtures).

## Factorial cohort (192 arms × 4 cells × 3 reps, medians)

| Cell | Counter sums | Fused reductions | Direct CDF | Total | Ratio vs control |
| --- | --- | --- | --- | --- | --- |
| Control | — | — | — | 58.03 s | 1.000× |
| A | yes | — | — | 50.26 s | **1.155×** |
| B | — | yes | — | 58.20 s | 0.997× (neutral) |
| AB (= D-minus-C) | yes | yes | — | 50.36 s | 1.152× |

Per-backend A ratios 1.147–1.161×, B ≈ 1.00× (−1.1% to +1.0%, noise),
AB 1.142–1.173× — no interaction benefit: **AB ≈ A within measurement
noise; B contributes nothing on top of A.**

Exactness evidence for B and AB (the strongest proof obligation):
all 9 §4 fixtures × 4 backends: per-symbol **256/256 quantized-vector
parity**, 512-counter state parity, EWA bit-parity, plus **whole-wire
byte parity** (plain, and backend-7 lane streams). The fused algebra
(corrected argmax `q'0 ≥ M_¬0 ⇒ 0 else argmax_¬0`; corrected zero count
`Z − [q0=0] + [q'0=0]`; dense bump + clamp in one write-back) is exact —
verified per symbol including tie-breaking and slot-0 status flips.

## Why B does not pay (recorded conclusion)

C4-B fused the *sweeps* (7 → 3 array passes) but the per-slot workload
is dominated by per-slot float arithmetic — the convention recorded
here is **divisions**: `pf=cf/tf` and `ps=cs/ts` = **2 divisions/slot =
512 divisions per symbol**; counting the mix multiplies (`wf*mix +
(1-wf)*mix`) and the integer-scale quantize multiply raises the full
per-symbol float-op tally to ~1,024. Either convention leaves fusion
unchanged (and unchanged in cost): reassociation or reciprocal
substitution would change the fixed-point vector and the wire. Array passes operate on an L1-resident 2 KB working set; their
cost is orders below the float arithmetic. Pass fusion was an Amdahl
model over a component that is not the bottleneck inside the model
layer — the honest correction to the §3 projection.

## C4-C — constructor path measured (probe, 2000 builds)

- `from_nonzero_fixed_point_probabilities`: **4 µs / 2000 = ~2 ns per
  model build**; single-symbol `encode_symbols`: below µs resolution.
- The model-build + writer consume < 1% of the per-symbol budget; the
  direct-cdf lever has nothing to attack.
- Interface constraint recorded: constriction 0.5.0's cdf-direct
  constructor is private; a hand-built cdf cannot be injected without
  an upstream change.
- `infer_last_probability` (255-prefix + inferred last) produces an
  identical model (0/2,000 mismatches by compressed-output equality)
  since our total is exactly 2^24 — but saves nothing measurable
  (~0 µs) and is therefore moot.

## Dispositions (ledger update)

| Gate | Disposition |
| --- | --- |
| C4-A — counter sums | CLOSED · PASS · 1.155× |
| C4-B — fused reductions | CLOSED · exact but net-neutral (0.997×) |
| C4-C — direct CDF | CLOSED · bottleneck-free (~2 ns/build) |
| C4-D — combined | CLOSED ≡ A (1.152–1.155×, no interaction) |
| C5–C7 | ride C4-A's combined result if production integration is later authorized (HOLD) |

The C4 program's measured winner is C4-A alone; B/C are honest
negative results with their exactness proofs retained. The natural
next lever inside the model layer (reciprocal-multiply per-slot
quantization) is **blocked by the zero-regret wire contract** — it
reassociates the fixed-point arithmetic and would change the wire.

Instruments: `#[cfg(test)] mod p1c4a` / `mod p1c4b` (differential
fixtures in both), cohort drivers `p1c4a_cohort` / `p1c4bcd_cohort`,
probe `p1c4c_constructor_probe` — all env-gated, `#[ignore]`-gated,
production untouched.
