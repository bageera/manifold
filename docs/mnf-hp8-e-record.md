# MNF-HP8-E — exact probability-construction equivalence record

Authorization: E0–E4 (E5 conditional). Repository revision c6416b3 (v1.17.1
line). All instruments env-gated `#[ignore]` (MNF_HP8_E, MNF_HP8_E_TWIN);
no production code path changed.

## E0 — exclusive hot-path decomposition (GATE PASSED, MEASURED)

Instrument `src/mix_coder.rs::mnf_hp8_e` (`hp8_e_decomp`): walks the
PRODUCTION backend-4 form over forest / dusk / screenshot / gradient,
captures full MixCtx snapshots every 97 symbols (12,036 states), benches
isolated operations on real states. CSV research/results/hp8-e-decomp.csv;
log hp8-e-decomp.log. Bench-overhead disclosure: per-stage absolute ns are
inflated by harness overhead (per-state call + black_box + fresh
allocations); the attribution below uses WITHIN-HARNESS arm differences and
the encoder-twin cross-check.

Isolated arms (mean across fixtures, ns/symbol-equivalent):
ctx_key ≈ 0.7–3.7 (negligible); state clone (2×256×8B) ≈ 64–164;
mixture map (256 × 2 f64 divisions + mul/add/cast) ≈ 332–353;
rebalance passes as written (sum + argmax + zero) ≈ 786–825 including the
mixture recomputed for input (pure rebalance ≈ 480 within-harness);
u32 collect + constriction from_nonzero_fixed_point_probabilities ≈ 190–200
beyond probs; step() ≈ 202–205; share_step for backend 4 = zero work
(bench shows 0 — the visible number is the instrument's own vec alloc).

In-loop cross-check (encoder twin, §E5): aggregate 725–751 ns/symbol for
the COMPLETE walk — consistent with the p1b 743–762 census. The construction
side (mixture + rebalance + u32 + constriction) dominates the walk; step and
key are minor.

## E1 — frequency-equivalence origin census (GATE PASSED, MEASURED)

Per-state distinct-value counts and adjacent-equal origins (same instrument,
census section of the CSV):

| fixture | distinct (fast,slow) pairs | distinct fast / slow | runs (contiguous equal freq) | adj-equal pairs /255 | origin: IDENTICAL COUNTERS | origin: same-f64-diff-counters | origin: rounding collapse |
|---|---|---|---|---|---|---|---|
| forest 224x160 | 38.6 | 24.4 / 37.4 | 46.4 | 209.7 (82.2%) | 99.6% | 0.0% | 0.4% |
| dusk 160x120 | 28.7 | 19.8 / 27.9 | 38.3 | 217.8 (85.4%) | 99.9% | 0.0% | 0.1% |
| screenshot 640x400 | 2.5 | 2.1 / 2.5 | 3.0 | 253.0 (99.2%) | 100.0% | 0.0% | 0.0% |
| gradient 320x240 | 11.5 | 4.2 / 11.4 | 7.8 | 248.2 (97.3%) | 95.4% | 0.0% | 4.6% |

Findings: 82–99% of the 256-entry vector is adjacent-equal runs; the runs
originate (≥99.6% on the photo/screenshot fixtures) from IDENTICAL raw
counter pairs — upstream arithmetic that literally repeats the same
subexpressions — not from integer-quantization collapse (0–4.6%). Max run
reaches 256 (whole-vector). u64→u32 truncation cannot merge anything (all
values ≤ 2^24 < 2^32); cast-equality is not an origin. E2's premise is
CONFIRMED: the redundancy exists and is upstream-identical.

## E2 — exact run-based construction (IMPLEMENTED, EXACT, TIMING NEGATIVE)

Four wire-exact candidates, all verified BIT-EXACT against MixCtx::probs on
all 12,036 captured states:

| arm | structure | isolated ns (forest/dusk/screenshot/gradient) | dense baseline |
|---|---|---|---|
| E2a full-pair dedupe (sort) | one mixture per distinct pair | 2315–3178 | 872–900 |
| E2b per-value + pair memo (linear maps) | divisions per distinct VALUE | 1687–5096 | 872–900 |
| E2c run-fused, dense-order tail | scan adjacent counter runs; 2 divs per run; sum/zeros fused; argmax separate | 460–744 | 872–900 |
| E2d E2c + u32 emission fused | direct u32 vector | 444–752 | 872–900 |
| E2e dense map + fused sum/zero chain | no run scan, serial sum | 761–829 | 872–900 |

Exactness discipline: an earlier E2c draft fused the argmax INTO the scan —
RETRACTED (the dense argmax runs AFTER the p[0] remainder fixup and must
observe the updated p[0]); the shipped candidates place the argmax pass in
exactly the dense order, making exactness STRUCTURAL (not coincidental).

Encoder-twin gate (`hp8_e_encoder_twin`, p1b cohort, 8 fixtures,
1,869,738 symbols): wires BYTE-IDENTICAL to production mix_encode_with in
every arm and fixture — the exactness proof at full-encoder scale. Timing
honestly NEGATIVE:

| arm | aggregate ns/sym (prod → candidate) |
|---|---|
| E2c run-fused | 725 → 779 (+8.9%) |
| E2e fused-sum | 750 → 815 (+13.4%) |

Per-fixture losses, E2c: solid +1.3%, gradient +3.5%, repeat +4.3%, plaid
+7.9%, **noise +23.4%**, forest +11.7%, dusk +9.9%, screenshot +4.6% —
monotonically slower on high-entropy streams (runs degenerate toward
length 1) and never faster even on the most concentrated fixture.

Instrumentation-defect disclosure: the clone-carrying twin first measured
+30.9% — that was the instrument's own per-symbol 4 KB state clone, not the
candidate; removed via borrowed-parameter form. The remaining +8.9% is the
candidate's true cost: the data-dependent run scan defeats the
auto-vectorization the dense map gets, and the E2e serial-fused sum creates
a dependency chain that blocks the SIMD reduction the separate sum pass gets.

## E3 — direct cumulative-frequency construction (GATE CLOSED, interface)

constriction 0.5.0 source reading: `from_nonzero_fixed_point_probabilities`
performs one validated pass (256 wrap-adds + comparisons + cdf pushes);
there is NO public constructor accepting a precomputed cumulative
distribution (`from_fixed_point_cdf` is private). A compact run-wise CDF
representation therefore DEFERS the same dense work inside the coder's
model build, and cannot bypass it via the public API. P1's previously
rejected direct-CDF approaches are not repeated. Disposition: BLOCKED BY
INTERFACE for the constriction-path candidates; recorded as an upstream
API constraint.

## E4 — exact integer arithmetic (GATE CLOSED, candidates dispositioned)

| candidate | requirement | disposition |
|---|---|---|
| Reciprocal-multiply division | exact quotient on proven domain | BLOCKED — wire-diverging per MNF17-P1-B record (different f64 rounding); memoizing the division per DISTINCT value is the only exact form and is what E2c implements |
| Common-denominator reuse | identical rounding | covered by E2b/c memoization — slower in situ |
| SIMD probability mixing | exact integer lanes | the dense map already auto-vectorizes (E2c/E2e losses are evidence); hand-SIMD unresolved, and the E0 profile shows the map is NOT the dominant remaining term once rebalance is included |
| Fused normalization | identical frequencies + remainder allocation | E2c/E2e implement it exactly; in-situ slower (serial dependency) |
| Shared multiplication terms | proven equivalent arithmetic | implemented in E2c (per-run wf/rf reuse) — the in-loop gain does not materialize |
| Reduced-width intermediates | formal overflow bounds | E2d (u32 direct) measured ≈ E2c (wash); values ≤ 2^24 are u32-safe by the Σf = 2^24 invariant |

## E5 — native equivalence and performance gate (CONDITIONAL — NOT MET)

Acceptance matrix (§6):
- Probability vectors: EXACT ✓ (12,036/12,036 states, per candidate)
- Cumulative distributions / models: EXACT ✓ (same constriction build from
  identical vectors)
- Entropy payload + complete wires: BYTE-IDENTICAL ✓ (8/8 fixtures, 1.87M
  symbols × {E2c, E2e} arms)
- Candidate selector / adaptive state: unchanged ✓ (twin walks share the
  production update logic)
- Encoding/decoding: exact ✓ (identical wires)
- Memory: ≈ equal (same allocations; u32 variant equal) ✓
- **Runtime: NO measurable improvement — FAILS the gate** (+8.9% / +13.4%
  in situ; no fixture faster).

## Ledger

| gate | verdict |
|---|---|
| E0 exclusive decomposition | PASS (positive evidence preserved) |
| E1 equivalence-origin census | PASS (redundancy confirmed, origins identified) |
| E2 run-based construction | wire-exactness PROVEN; runtime NEGATIVE in situ |
| E3 direct cumulative construction | CLOSED (interface-blocked, documented) |
| E4 exact integer optimization | CLOSED NEGATIVE within authorization |
| E5 native performance gate | CONDITIONAL — NOT MET (no production candidate; integration HOLD) |

## Conclusion

The 82–99% run redundancy is real, upstream-identical, and exploitable
MATHEMATICALLY — every candidate reproduces production outputs bit-exactly
at full-encoder scale. But on this hardware the dense formulation is what
the optimizer executes well: hand-exploiting the structure (sort, memoize,
scan, fuse) costs more than the arithmetic it saves, and the loss grows
with entropy. The redundancy does not convert into runtime without an
algebraic reformulation that stays SIMD-shaped — a candidate class left
OPEN for future work, with nothing promoted.

Research-only artifacts: the mnf_hp8_e instrument, e2a–e2e candidate
implementations, twin harness (all env-gated `#[ignore]`, zero production
impact). Production wire, backends, and portfolio: unchanged.
