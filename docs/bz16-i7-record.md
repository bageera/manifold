# BZ16-I7 — bounded arithmetic and streaming for the recovered model

Third gate under the revised methodology
([docs/bz16-i4b-record.md](bz16-i4b-record.md),
[docs/bz16-i5-record.md](bz16-i5-record.md)). Question: can the model
operate with bounded arithmetic and bounded working memory while streaming?

Instrument: `crates/mnf-mans/examples/bz16_i7_bounds_streaming.rs`
(stdlib-only, no production-surface edits).

## 1. I7-A — the bounds table (asserted identities + live checks)

| Quantity | Bound | Where |
| --- | --- | --- |
| Energy magnitude | 12,582,912 / 201,326,592 / 3,221,225,472 (Q8/Q12/Q16, memo §5) | asserted per position |
| Energy difference (LUT-index input) | ≤ 2·e_lim under the symmetric clamp (live per R2H record §4a) | per position |
| Gradient numerator | ≤ FREQ_SUM·4096 per feature; ×16 envelope 26,388,279,066,624 < 2^62 | asserted |
| Coefficient interval | [−c_lim, +c_lim], c_lim = 4<<n (clamp-polarity question open, R2H §4a) | clamp |
| LUT index | k ∈ [0, 4096] (frozen R2-B boundary table) | clamp |
| Normalization products (largest remainder) | n·sum_to = total·sum_to: counter branch ≤ 2^31 (post-policy), mixture ≤ 2^38, weight ≤ 2^49 — all live-asserted < 2^62 | per call |
| Weight/probability products | W_SUM·FREQ_SUM = 2^31 exactly | asserted |
| Coder products (18-bit instrument) | rng·FREQ_SUM ≤ 2^33 − 2^15 | asserted |
| Counter domain | Σ ≤ 65,536 under the policy (live) | per position |

## 2. I7-B — the bounded-counter policy (the production candidate)

The recovered semantics carry "counter rescaling: none in the reference" —
unbounded growth, unbounded per-symbol counts, and a per-position cost that
grows with Σcounters (the counter-branch products). I7 introduces the
minimal streaming policy candidate: **halve every counter (round half up,
floor 1) whenever Σ exceeds 2^16** and measures it against the unbounded
reference on extended streams (each base fixture tiled ×4 = 65,536
positions; Σ crosses the cap at t ≈ 65,280, so the policy fires on every
extended rig — asserted).

| Rig (×4 tiled, 65,536 pos) | cost_unbounded | cost_bounded | delta bits | rel |
| --- | --- | --- | --- | --- |
| camera-Q12 | 527,288.84 | 527,291.96 | +3.12 | +0.001% |
| camera-Q8 | 522,579.78 | 522,580.11 | +0.34 | 0.000% |
| camera-Q16 | 533,312.49 | 533,312.77 | +0.28 | 0.000% |
| brick-Q12 | 451,089.71 | 451,075.05 | −14.66 | −0.003% |
| brick-Q8 | 521,873.01 | 521,873.37 | +0.36 | 0.000% |
| brick-Q16 | 325,233.43 | 325,233.43 | +0.00 | 0.000% |
| noise-Q12 | 523,952.52 | 523,952.54 | +0.02 | 0.000% |
| noise-Q8 | 523,950.17 | 523,950.18 | +0.02 | 0.000% |
| noise-Q16 | 525,760.84 | 525,760.89 | +0.05 | 0.000% |

The bounded-counter policy is effectively free: |Δ| ≤ 15 bits over
≥ 325 k-bit streams (≤ 0.003%), in either direction (brick-Q12 is slightly
cheaper under the policy — the rescale refreshes the unigram domain's
granularity). The halving rule keeps Σ' ≤ 2^15 + 256 after each fire, so
the policy refires roughly once per 32 k positions, indefinitely — the cost
delta is length-independent by construction.

## 3. I7-C — streaming discipline

- Working memory is O(256) i64 counters + 6 coefficients + 4 weights +
  the constant 16,388 B LUT; no history, no per-position vectors retained.
- The probability trace is consumed incrementally (512 B/position × 65,536 = 33.5 MB
  consumed per extended rig; consumption asserted). The I7 instrument uses
  a cheap internal FNV-chain for the check (not the R2H-D SHA-256 contract —
  the digests' cross-language identity is I4B's drift-check domain).
- Stream length is unbounded under the policy (the counter domain never
  grows past 2^16 + 256; every other domain is precision-bounded).

## 4. Disposition

| Gate | Disposition |
| --- | --- |
| I7-A bounds table | PASS (identities + live per-position checks) |
| I7-B bounded-counter policy | PASS (fires on all extended rigs; |Δ| ≤ 0.003%) |
| I7-C streaming discipline | PASS (O(256) memory, incremental consumption, unbounded length) |
| CACM87 regression fixtures | inherited (R2H's bounded guards + nine-stream coverage; I4B/I5 runs under them) |
| I4B / I5 | CLOSED (retention + change-point) |
| Native replay / portfolio evaluation | CONDITIONAL (next: ΔL vs Manifold's production entropy models under complete serialisation accounting) |
| Production integration | HOLD |
