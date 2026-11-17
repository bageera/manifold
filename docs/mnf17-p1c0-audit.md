# MNF17-P1-C0 — mathematical dependency audit of the probability-model loop

**Status**: C0 gate complete. No code changed. Every consumer of every
frequency entry identified; the exactness obstacles for C1 are located
line-precisely; the surviving optimization hypotheses are re-ranked.

Scope (all read line-precisely): `src/mix_coder.rs` — `MixCtx::probs`
(206–236), `probs_with` (242–252), `model` (254–269), `step`
(271–293), `share_step` (360–368), `mix_vecs` (300–322), the two encode
loops (568–604, 615–662), the two decode loops (666–715, 725–795);
constriction 0.5.0 `from_nonzero_fixed_point_probabilities` (contiguous.rs
471) and the `ArithmeticEncoder` interval consumption. Constants: SCALE
2^24, COUNT_INC 32, caps 4096/65536 (b4) or 32768/1048576 (b5/b6/b7),
WARM_N0 12, cold window 256, EWA_LR 0.15.

## 1. The measured problem, resolved to passes

Per coded symbol, the loop does:

1. `probs()` — **7 full passes over 256** (+ 2 scalar exp/sums):
   Σfast, Σslow, per-slot mix+float-quantize `⌊(wf·a_i + (1-wf)·b_i)·2^24⌋`,
   Σp (remainder), argmax (dense), zero-count, zero-clamp.
2. `probs_with` (b6/b7 while cold, ≤256 symbols/context) adds the parent's
   own 7 passes + `mix_vecs`' 7 more (Σ, per-slot mix, Σp, argmax,
   zero-count, zero-clamp).
3. `model()` — constriction cdf accumulation: **+1 pass** (strictly,
   `accumulate_nonzero_probabilities` = one pass building the cdf table).
4. `encode_symbols` consumes **only 3 cumulative quantities** (cdf(s),
   cdf(s+1)−cdf(s), total 2^24) from the model's cdf table.
5. `step(b)` — Σfast, Σslow (for the log-W update), +1 point update,
   **up to 4 halving passes** when a cap is exceeded (caps b4 4096/65536,
   b5+ 32768/1048576 — halvings fire at ~cap/32-symbol cadence per
   context; most contexts rarely halve).
6. `share_step` (b6/b7) — a full parent `step` again.

Total: **≈ 8 passes/symbol hot path (b4/b5), ≈ 15 (b6/b7 cold), with the
per-context parent chain multiplying step costs** — matching P1-B's
model-layer ≈99% attribution.

## 2. The dependency map (C0's deliverable)

Consumers by quantity:

| Quantity | Producer | Consumers |
| --- | --- | --- |
| `fast[i]`, `slow[i]` (single slot) | increment + halving | mix quantization of slot i only; log-W needs slot b's ratio |
| `Σfast`, `Σslow` | full reduction | mix quantization (all slots), log-W update, halving trigger |
| `wf = 1/(1+e^(−log_w))` | scalar | mix quantization (all slots) |
| `p[i] = ⌊(...)⌋` | per slot | cdf accumulation (through i), remainder, argmax, zero-count, zero-clamp |
| `Σp` over all slots | full reduction | **p[0] += SCALE − Σp** (remainder to slot 0) |
| argmax of p | full reduction | dense-slot bump `max(p[dense], 1+zeros)` |
| zero count | full reduction | dense bump + `p[i] == 0 → 1` flags |
| cdf[s] = Σ_{i<s} p_final | prefix reduction of the FINAL vector | the writer (2 entries) + constriction's interval math |
| decoder | same cdf | symbol search over the same table |

**Every one of the 256 entries feeds at least one per-symbol global
reduction.** The five global reductions (Σfast, Σslow, Σp, argmax,
zero-count) are what make the per-symbol work O(256), not the writer's
3-quantity demand.

## 3. C1 verdict — algebraic cumulative mixture: blocked, three obstacles

Each obstacle is a fixed-point pipeline stage whose output is a *global*
function of every quantized slot:

1. **Per-slot floor** — `p[i] = ⌊(wf·a_i + (1−wf)·b_i)·2^24⌋`; floors do
   not distribute over the mixture the way `Q_k(s)` needs.
2. **Remainder-to-slot-0** — `p[0] += SCALE − Σ_i pQuant[i]`; the
   correction is a function of all 256 rounded entries, so every
   cumulative sum F_s depends on Σ over the whole alphabet. Exactness
   would require reproducing the SAME floor pattern, which requires the
   per-slot pass anyway.
3. **Dense bump + zero set** — post-normalization corrections keyed by
   the argmax and the zero count; both global.

Conclusion: `Σ_{i<s} p_final[i] ≠ Σ_k w_k·Q_k(s)` under any
reconstruction that skips the global quantization passes. **C1 is
recorded as blocked by exactness**, per the C0 instruction ("if all
entries are mathematically necessary, record that result"). The wire is
NOT touched.

### 3a. C2 verdict — hierarchical Σ maintenance: viable for the counters

The two counter sums are maintainable incrementally: `Σfast += 32` per
child step (and parent `share_step`), re-derived exactly on halving
(halvings are O(256) already — the recount is free inside them). A
Fenwick tree over `fast` would also buy the per-slot ratio (log-W), but
halvings are mass updates — a plain maintained accumulator with
recount-on-halving is the same O(), less machinery. **Subsumed by C4.**

### 3b. C3 verdict — lazy evaluation: the consumers table is real, but the globals remain

The writer needs 3 quantities; adaptation needs 2 slots + 2 sums. But
both sums, the remainder, the argmax and the zero count are global, and
wf moves every symbol (EWA 0.15 update per symbol), re-rolling **every**
slot's floor. No lazy subset escapes the O(256) per-symbol work on the
hot path. **C3 as written is blocked; its useful content (fusing the
writer's demand into the model build) = C4.**

### 3c. C4 verdict — fused fixed-point reduction: the surviving lever

Today's 7+1 passes can become **3 exact passes** without changing any
intermediate value:
- pass A: Σfast/Σslow fused with the per-slot quantization
  (the sums are needed only for the mix, computable in the same sweep
  since slots are independent — but Σ arrives only at the end; the
  mix term needs the SUM first → two sub-sweeps, or keep the sums
  incrementally maintained (C2-lite) so pass A = quantize only);
- pass B: Σp remainder + argmax + zero-count in one sweep (single
  accumulator tuple — no intermediate vector round-trips);
- pass C: zero-clamp fused with the cdf accumulation into the model
  table (constriction's from_fixed_point_cdf accepts a ready cdf —
  build the cdf directly during the clamp; the model object then
  wraps the exact same table).

The values are bit-identical (same arithmetic, fewer passes and no
intermediate allocations); the `Vec<u64>→Vec<u32>→cdf` triple
materialization disappears. Expected: the 7+1 passes → 2–3, i.e.
**1.5–2.5× on the model layer, ≈1.4–2.2× on mix evaluation** (Amdahl at
p=0.99), a *measured* question for C6 — not algebraically guaranteed.

## 4. Exactness oracle (unchanged, per R6.2-B)

The C4 rewrite must hold F_t, f_t, T and θ_{t+1} identical for every
symbol — trivially satisfiable under pass-fusion (identical values,
different pass order) — and then pass C5 byte-replay, C6 cost, C7
corpus: 768,303 B / 1,438,848 B / 60/60 recon. The cdf-fed
`from_fixed_point_cdf` path must be pinned to constriction's exact
`wrapping_pow2(PRECISION)` total semantics (the same constant SCALE =
2^24 as the shipped path).

## 5. C0 conclusion

The writer demands 3 cumulative quantities; the pipeline demands 5
global reductions over all 256 — **the full alphabet is consumed per
symbol by the reductions, not by the coder**. C1: recorded blocked
(three fixed-point obstacles). C3: blocked (globals). The minimum exact
restructure = **C4 fused passes with C2-lite maintained counter sums**.
Estimated ceiling 2–3× on the model layer; the next measured question
(C1–C4 gates as experiments, C5–C7 as validation) follows only on the
separate authorization the ledger requires (P1-C1–C4 CONDITIONAL).
