# BZ16-I4B — matched float/fixed comparison (Rust confirmation environment)

First gate under the revised BZ16 methodology: **Python is the screening
environment, Rust the confirmation and engineering environment.** Python
artifact parity is demoted to OPTIONAL REFERENCE (R2H-C/D results stand;
the archived package is no longer on the critical path). I4B's question: does
the fixed-point Boltzmann model retain the compression effectiveness of its
floating-point counterpart on identical inputs?

Instrument: `crates/mnf-mans/examples/bz16_i4b_float_fixed.rs` (stdlib-only,
no production-surface edits).

## 1. Design disclosure

Both engines run per (fixture, precision) over the identical full
16,384-position rig (synthetic stand-ins: camera `97r+31c+(rc>>2)`, brick
`37(r/8)+c+3(r%8)`, noise LCG seed 12345 — same context, counters, gradient /
fixed-share constants, symmetric clamp, and Σ 32,768 frequency domain, so
per-position costs are directly comparable).

- **Fixed engine**: the R2H recovered pipeline verbatim (frozen LUT,
  k-bucket quantization, integer state, drift repair). It is drift-checked
  against the R2H golden exports before any comparison is reported
  (I4B-A), so the copy cannot diverge silently.
- **Float twin**: identical skeleton with coefficients/weights/counters in
  f64 and the exact exponential in place of the LUT. Design choices,
  each load-bearing for what the measured delta means:
  1. The mapping is the frozen one's continuous envelope: the LUT index is
     `k = clamp(β·e >> (n−4))` with `L_k/2^15 = e^(−k/256)`, so the exact
     counterpart is `exp(−β·e/2^(n+4))` (`b = β/2^(n+4)`, β = 2 folded into
     `b_lo`).
  2. The `k ≥ 0` clamp (negative energy → k = 0, a frozen R2-B boundary) is
     mirrored — without it a slammed-negative state (camera-Q8 t≤1) overflows
     `exp(+3072)` to infinity. The upper `k ≤ 4096` saturation floor is NOT
     mirrored: the continuous decay below `2^15·e^(−16) ≈ 4.99` is exactly
     the LUT-saturation precision delta this gate measures.
  3. No drift-repair term (float linearity keeps Σw = 65,536 exactly; the
     float weight recenter is round-off magnitude ~1e-12).
  4. No zero-mixture risk (`x > 0` always), so the float path needs no
     fallback branch.
  5. Coding frequencies are derived through the same largest-remainder rule
     to Σ 32,768 in both engines — the shared coder interface.
  6. Clamp polarity stays the symmetric R2 pin in BOTH engines; the
     R2H-record §4a question is deliberately left open (same choice both
     sides keeps the comparison precision-only).

## 2. Results — I4B-A/B measured over all nine rigs

| Rig | cost_fixed (bits) | cost_float (bits) | Δ (fix − float) | rel | max per-position |Δ_t| |
| --- | --- | --- | --- | --- | --- |
| camera-Q8 | 130,654.79 | 130,653.81 | +0.97 | +0.001% | 0.42 |
| camera-Q12 | 131,616.49 | 131,808.36 | −191.87 | −0.146% | 0.57 |
| camera-Q16 | 132,875.66 | 132,959.50 | −83.84 | −0.063% | 0.58 |
| brick-Q8 | 129,901.35 | 129,721.71 | +179.64 | +0.138% | 5.03 |
| brick-Q12 | 112,714.49 | 112,950.61 | −236.12 | −0.209% | 11.48 |
| brick-Q16 | 79,711.35 | 80,223.54 | −512.19 | −0.643% | 11.49 |
| noise-Q8 | 131,080.25 | 131,078.33 | +1.92 | +0.001% | 0.26 |
| noise-Q12 | 131,077.17 | 131,083.88 | −6.71 | −0.005% | 3.84 |
| noise-Q16 | 131,533.50 | 131,538.05 | −4.55 | −0.003% | 2.18 |

Verdict (I4B-B): **the fixed-point model retains the float engine's
compression effectiveness.** The full-stream precision tax is within
±0.65% on every rig (|Δ| ≤ 512 bits over ≥ 79.7 k-bit streams), with no
systematic direction — the fixed engine is *cheaper* on five of nine rigs
(all of Q8 except brick +0.14%): the k-bucket quantization plus drift repair
acts as its own slight regularization, while the float twin's continuous
decay below the LUT saturation floor (≈ 4.99 at the cap) buys it nothing on
these fixtures. The largest single-position divergence anywhere is
11.49 bits (brick-Q12/Q16, during the clamp-slam transient).

I4B-A (drift check): fixed engine == R2H goldens on all nine rigs —
full-stream BE-u16 probability-trace digests, z tail carries, and the
512-position window f vectors byte-identical.

## 3. What this does and does not establish

CONFIRMED: the recovered integer pipeline is not losing the mathematics to
quantization — its compression behavior on identical inputs matches the
continuous-exponential counterpart within ±0.65% full-stream, with the
Q12 priority precision at −0.005% to −0.209%.

NOT established: compression effectiveness vs Manifold's production models.
The uniform-baseline figures above are model-cost only (pre-serialization);
the next rungs (I5 change-point, I7 bounds/streaming, then native replay)
answer the real ΔL = L_Boltzmann − L_production question under complete
serialization accounting — CONDITIONAL, not yet measured.

## 4. Disposition

| Gate | Disposition |
| --- | --- |
| I4B-A fixed-side drift vs goldens | PASS (9/9 rigs) |
| I4B-B float/fixed cost retention | PASS (max rel |Δ| 0.65%; no systematic sign) |
| I4B-C state divergence | measured (coefficients/weights re-derived per engine; trajectory deltas visible in the cost traces, bounded by max|Δ_t|) |
| I5 change-point evaluation | next (AUTHORIZED) |
| I7 bounds + streaming | next (AUTHORIZED; includes the CACM87 regression fixtures already shipped in R2H) |
| Native replay / portfolio evaluation | CONDITIONAL (compare against Manifold's production entropy models, not uniform) |
| Python artifact parity | OPTIONAL REFERENCE (out of the critical path per revised methodology) |
| Production integration | HOLD |

---

## Addendum — R2H-E asymmetric clamp, corrected scale (2026-11-18)

Re-measured under the recovered asymmetric clamp (`b ∈ [−S, 4S]`,
`S = 2^Q = c_lim/4` — the operator's scale clarification; the numbers in
the table above are retained as the labeled symmetric-clamp ablation):

| rig | cost_fixed | cost_float | delta_bits | rel |
| --- | --- | --- | --- | --- |
| camera-Q8 | 130,658.25 | 130,657.09 | +1.16 | +0.001% |
| camera-Q12 | 131,616.49 | 131,808.36 | −191.87 | −0.146% |
| camera-Q16 | 132,875.66 | 132,959.50 | −83.84 | −0.063% |
| brick-Q8 | 129,909.94 | 129,741.81 | +168.13 | +0.129% |
| brick-Q12 | 112,714.49 | 112,950.61 | −236.12 | −0.209% |
| brick-Q16 | 79,711.35 | 80,223.54 | −512.19 | −0.643% |
| noise-Q8 | 131,078.39 | 131,076.63 | +1.76 | +0.001% |
| noise-Q12 | 131,077.17 | 131,083.88 | −6.71 | −0.005% |
| noise-Q16 | 131,533.50 | 131,538.05 | −4.55 | −0.003% |

Retention verdict unchanged in kind: fixed≈float, no systematic sign
(fixed cheaper 4/9), worst deviation ±0.65% (brick-Q8 tightened from the
3S misparameterization's +1.50% to +0.13% under the corrected scale; the
overall worst is brick-Q16 −0.643%). Drift-check: the fixed engine is
byte-identical to the R2H-E goldens (digest + z + window f).
