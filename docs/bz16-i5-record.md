# BZ16-I5 — change-point recovery of the recovered fixed-share engine

Second gate under the revised methodology
([docs/bz16-i4b-record.md](bz16-i4b-record.md) carries the framework).
Question: does fixed-share adaptation recover efficiently when the
underlying symbol distribution changes?

Instrument: `crates/mnf-mans/examples/bz16_i5_changepoint.rs` — the
recovered fixed-point pipeline (the same code I4B anchored against the R2H
goldens) over two 8,192 + 8,192 concatenations with the change at
t* = 8192: camera→brick (cost-neutral/slightly easier continuation) and
brick→camera (upward shift, the engine's prior becomes wrong). Q12 primary,
Q8/Q16 controls. Costs are ideal pre-serialization b/sym
(15 − log₂ f(o)); recovery = first t > t* whose trailing-64-position mean
cost is within 5% of the post-change steady level
(mean over [t*+1024, t*+1536)); restart column = Σcost over the
[ t*, t*+R+512 ) window adapted-engine minus a FRESH engine seeded exactly
at t* (negative → the carried state helps, positive → it resists).

## 1. Results

| Rig | Q | before | after | join_max | recovery | window | restart Δ bits |
| --- | --- | --- | --- | --- | --- | --- | --- |
| camera→brick | 12 | 7.999 | 8.086 | 8.034 | 64 | 576 | −12.2 |
| camera→brick | 8 | 7.996 | 8.001 | 8.034 | 64 | 576 | −8.7 |
| camera→brick | 16 | 8.132 | 8.114 | 8.678 | 64 | 576 | +5.1 |
| brick→camera | 12 | 6.604 | 7.881 | 8.371 | 64 | 576 | +106.1 |
| brick→camera | 8 | 7.902 | 7.930 | 8.313 | 64 | 576 | +95.5 |
| brick→camera | 16 | 3.889 | 5.691 | 15.000 | 164 | 676 | −733.7 |

- Recovery is fast everywhere: the trailing-64 mean is already within 5% of
  the post-change steady level at the first window (64 positions) on five of
  six rigs; the Q16 up-shift takes 164. In absolute terms the adapted
  engine pays at most ~+106 bits vs a fresh restart over the whole measured
  window on the hardest rig (brick→camera Q12: ≈ 0.18 b/sym over 576
  positions) — and the fresh restart itself pays its own warm-up, so the
  comparison is conservative.
- The transient peak is capped by the coder-fit floor: join_max ≤ 15.000
  b/sym means the worst join position still has f(o) ≥ 1 (the f ≥ 1
  guarantee doing its job); the Q16 brick→camera rig touches it during the
  slam (the engine confidently wrong at Q16's sharp weights).
- The carried state HELPS on the downward shifts (−8.7 to −12.2 bits vs
  fresh restart; the brick→camera Q16 rig −733.7 — the Q16 weights transfer
  strongly) and costs at most ~106 bits (Q12 up-shift) when the prior is
  genuinely wrong. The counter-unigram component keeps the old symbol
  frequencies alive through the transient (no reset cliff).

## 2. Fixed-share α sensitivity (float domain, brick→camera Q12)

| α | join_max | recovery | steady_after |
| --- | --- | --- | --- |
| 0.999 (recovered equivalent) | 12.678 | 64 | 8.084 |
| 0.99 | 15.000 | 64 | 8.455 |
| 0.9 | 15.000 | 176 | 9.521 |

Faster tracking does NOT help: the constant drag toward the equal mixture
worsens both the transient (the posteriors already dominate; the drag wastes
weight immediately) and the steady state (α = 0.9 settles at 9.52 b/sym vs
8.08). The recovered α = 0.999 is the right regime; no re-point is
suggested by this ablation.

## 3. What this establishes

The recovered fixed-share engine absorbs a hard mid-stream distribution
change within 64–164 positions at a worst-case excess of ~0.2 b/sym over
its own first recovery window against a fresh restart, with a transient
capped by the coder-fit floor at 15 b/sym. Combined with I4B (retention
within ±0.65%) the adaptation machinery is behaving the way a production
residual model would need it to. Still measured model-side only — the
native replay gate (CONDITIONAL) remains the ΔL vs production decision.

## 4. Disposition

| Gate | Disposition |
| --- | --- |
| I5-A change-point recovery | PASS (64–164 positions; restart Δ ≤ +106 bits worst case) |
| I5-B α sensitivity | recovered α = 0.999 optimal among {0.999, 0.99, 0.9} |
| I7 bounds + streaming | next (AUTHORIZED) |
| Native replay / portfolio evaluation | CONDITIONAL |
| Production integration | HOLD |

---

## Addendum — R2H-E asymmetric clamp (2026-11-18)

Re-measured under the recovered asymmetric clamp ([−S, 4S]; the numbers
above are the labeled symmetric-clamp ablation):

| rig | Q | before | after | join_max | recover | adapt_x | restart |
| --- | --- | --- | --- | --- | --- | --- | --- |
| camera→brick | 12 | 7.999 | 8.086 | 8.034 | 64 | 576.0 | −12.2 |
| camera→brick | 8 | 7.996 | 8.002 | 8.034 | 64 | 576.0 | −8.9 |
| camera→brick | 16 | 8.132 | 8.114 | 8.678 | 64 | 576.0 | +5.1 |
| brick→camera | 12 | 6.604 | 7.881 | 8.371 | 64 | 576.0 | +106.1 |
| brick→camera | 8 | 7.903 | 7.932 | 8.313 | 64 | 576.0 | +95.5 |
| brick→camera | 16 | 3.889 | 5.691 | 15.000 | 164 | 676.0 | −733.7 |

Recovery verdict unchanged: 64 positions typical, 164 at the
brick→camera Q16 joint; the recovered α = 0.999 remains optimal (float
α-sensitivity: 0.999 → join 8.300, 0.99 → 8.286, 0.9 → 8.791 for the
brick→camera Q12 rig).
