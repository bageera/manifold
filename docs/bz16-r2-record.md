# BZ16-R2 — in-repo numerical parity prototype (scalar reference + Rust twin)

Companion to the two BZ16 memos and [docs/bz16-r1-record.md](bz16-r1-record.md).
R2's acceptance contract is exact Python–Rust integer parity
(`E_t^Rust = E_t^Python`, `k`, `f`, `θ_{t+1}`, compared per position over full
256-entry frequency vectors). The accepted research-environment reference is
still not on this box, so R2 is executed **in-repo**: a scalar Python
reference engine (pure ints/lists, no NumPy — the BZ16-I6 dual-implementation
methodology reproduced here) and an independent Rust twin, both consuming the
frozen LUT, compared position-by-position.

- Reference: `crates/mnf-mans/scripts/bz16_r2_reference.py` (stdlib-only)
- Twin: `crates/mnf-mans/examples/bz16_r2_parity.rs` (stdlib-only)
- Golden export: `research/results/bz16-r2-golden-{camera,brick,noise}.txt`
  (gitignored local logs: `bz16-r2-python.log`, `bz16-r2-parity.log`)

## 1. Pinned model specification (provenance split)

The memos freeze the mathematics but not every free constant; R2 pins the
remainder explicitly so the parity rig is reproducible, with the proviso that
**if the accepted research-env reference differs on any pinned choice, the
golden contract re-points and the diffs are re-run** — that is the pending
artifact list, unchanged from R1.

FROZEN by memo (verified numerics):

| Item | Value | Source |
| --- | --- | --- |
| LUT | `L_k = max(1, round(2^24·e^{−k/256}))`, k ∈ [0, 4096], u32 | memo §3 |
| Energy resolution | 1/256, max represented energy 16 → k = clamp(E/16, 0, 4096) in Q12 | memo §3 |
| Symbols | 256 full frequency vectors | memo §2 |
| Features | three circular-distance features | memo §5 |
| Coefficients | Q12, `i32` | memo §5 |
| Experts | two inverse-temperature experts | memo §5 |
| Mixture | four integer fixed-share weights | memo §2 |
| Rounding | half-to-even everywhere (Python `round()` parity) | memo §3 |
| Types | freq/cumulative `u32`, products `u64` | memo §5 |
| Families | none added; no new temperature schedules | both memos |

PINNED BY R2 (underdetermined by the memos; each affects the golden contract):

| Choice | Pinned value |
| --- | --- |
| Feature definition | `d_i(v)` = circular distance on the 256-byte cycle from candidate symbol v to each of s_{t−1}, s_{t−2}, s_{t−3}; scaled `D_i = d_i << 5` ∈ [0, 4096] |
| Temperature mapping | k_hi = clamp(E >> 4), k_lo = clamp(E >> 5) (inverse-temperature ratio 2:1); negative energy clamps to k = 0 |
| Coefficient bound | \|c\| ≤ 4·4096 = 16,384 (the memo-conservative bound) |
| Init state | coefficients 0; weights [1024; 4] (Σ 4096, Q12); f_prev = uniform 16 |
| Expert vectors | `x_e(s) = L[k_e(s)]` in the 2^24 domain |
| Mixture components | {hi expert, lo expert, uniform (2^24/256 = 65536), freeze (f_prev·4096)}; `m(s) = (Σ w_j·x_j) >> 12`, state carried |
| Normalization | largest remainder to Σ f = 4096 (Q12; ties: remainder desc, then lower symbol index); zero-frequency repair pulls 1 from the current argmax (ties: lowest index) — guarantees coder-fit f ≥ 1 |
| Gradient update | err = f(o) − (x_e(o) >> 12) (Q12); Δc_i = −((err·D_i) >> 15); clamp to ±16,384 |
| Fixed share | α = 64/4096 toward argmin \|target − value\| over the four components at o (tie: lowest index); sum drift restored to the argmax (drift>0) / argmin (drift<0), ties lowest index; Σ w = 4096 invariant |
| R2-G instrument | CACM87 carry-less range coder: 16-bit window, FREQ_SUM scale, E0/E1/E3 underflow counting; bit-exact both languages |
| Fixtures | synthetic stand-ins (the memo's camera/brick crops remain pending artifacts): 128×128 each — camera = 2D ramp `97r+31c+(rc>>2)`, brick = `37(r/8)+c+3(r%8)` bands, noise = LCG (seed 12345) |
| Padding | pre-stream prev3 = symbol 0 |
| Audit window | first 512 positions per fixture (memo §7 audit depth) |

## 2. Results — R2-A..R2-G all PASS in-repo

Golden digests are stable across reruns (deterministic export), and the Rust
twin reproduces every exported field bit-for-bit:

| Fixture | Parity (R2-A..F) | z carries (R2-E) | Window bits (Py = Rs) | Full-stream ac (R2-G) | Repairs (Py = Rs) |
| --- | --- | --- | --- | --- | --- |
| camera | 512/512 PASS | PASS | 4257 = 4257 | PASS, 161,562 bits | 616,305 = 616,305 |
| brick | 512/512 PASS | PASS | 2978 = 2978 | PASS, 93,163 bits | 3,858,791 = 3,858,791 |
| noise | 512/512 PASS | PASS | 4097 = 4097 | PASS, 131,079 bits | 0 = 0 |

- Per-position fields compared: observed symbol, `E_hi`/`E_lo` at o (R2-A),
  `k_hi`/`k_lo` (R2-B exercised in-stream), coefficient carries (R2-D),
  weights (R2-E), full 256-entry f (R2-C) and 257-entry F (R2-F); a
  one-frequency-unit mismatch aborts the run.
- The z line records the engine state after the complete 16,384-position run —
  the two independent implementations reach identical carries after the full
  adaptation history, with no encoder-side export needed by the decoder.
- R2-G: the range-coder instrument reproduces the stream exactly with the
  decoder recomputing every F_t from the decoded prefix alone. Its wire size
  sits within ~4.5% of the ideal-model entropy cost (instrument overhead, not
  the model's).
- Ideal cost (reporting only): camera 161,558.46 bits (9.86 b/sym — the pinned
  model LOSES to the 8.0 b/sym uniform baseline on this synthetic smooth
  fixture), brick 93,163.28 (5.68 b/sym — wins), noise 131,078.57
  (≈ 8.00005 b/sym — neutral, as expected on incompressible data). Honesty
  note for the frontier: the concentrated fixtures pay heavily in
  zero-repairs (brick ≈ 235/position); the model's economy question is
  BZ16-N's business, pending the accepted reference and native integration.

## 3. R2-B — LUT index + clamp boundary table (both languages, identical)

| E (Q12) | k_hi = clamp(E>>4) | k_lo = clamp(E>>5) |
| --- | --- | --- |
| 0 | 0 | 0 |
| 1 | 0 | 0 |
| 4,095 | 255 | 127 |
| 4,096 | 256 | 128 |
| 65,535 | 4,095 | 2,047 |
| 65,536 | 4,096 | 2,048 |
| −1 | 0 (clamp) | 0 (clamp) |
| 201,326,592 | 4,096 (clamp) | 4,096 (clamp) |

## 4. Bounds re-verified on the live typed path

- Energy accumulator `|E| ≤ 3·16384·4096 = 201,326,592` asserted every
  audited position — equal to the memo's frozen conservative value (the
  pinned D-scale and coefficient bound reproduce the memo's bound algebra
  exactly); fits `i32` at 9.375% of range.
- Gradient numerator `|err·D| ≤ 4096·4096` asserted; with the memo's frozen
  ×32768 conservative factor over three features this is
  1,649,267,441,664 < `i64` — matching memo §5.
- Weighted probability products bounded by 2^38 in `u64` (memo's proposed
  type); range-coder intermediates ≤ 2^28 in `u32`.
- Debug-mode overflow panics are live on the typed path for every streamed
  position (both languages run with bounds asserts active).

## 5. What this does and does not confirm

CONFIRMED: determinism and portability of the pinned integer Boltzmann model
across two independent implementations (pure-scalar Python and safe Rust),
including every clamped/edge path the frozen bound algebra names.

NOT YET: parity with the accepted research-environment engine. The memo's 36
golden probability vectors, the nine full-stream SHA-256 digests, the scalar
engine source, the frozen LUT binary export, and the three fixture crops
remain the acceptance contract for that re-pointing; the contract format is
already pinned above (h/p/f/F/z line protocol), so a matching or mismatching
reference drops in as a diff, not a rewrite.

## 6. Disposition

| Gate | Disposition |
| --- | --- |
| R2-A..R2-F in-repo twin parity | PASS |
| R2-G independent end-to-end decode | PASS (instrument; not a production backend) |
| R2 vs accepted research-env reference | PENDING artifacts (list unchanged) |
| Native replay and ablation (BZ16-N) | CONDITIONAL, per memo |
| Production promotion / new wire format | HOLD, per memo |
| P1 computational frontier | PRESERVED (no production-surface edits) |
