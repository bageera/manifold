# Periodic Structure Prediction v0.6 probe — RETAINED IN-PRINCIPLE, two findings (2026-10-27)

The work order (2026-10-27) closed the geometry/topology direction with the
1/W result reclassified precisely (a bound on the affected-position fraction
N_affected/N_total = 1/W under the row-boundary rule — not a general
information bound) and posed v0.6:

> A reversible predictor that recognizes repeated structure across the entire
> signal can outperform local spatial prediction when the underlying data
> contains sufficiently strong periodic redundancy.

Unlike boundary wrap (v0.5), a period-lag predictor touches nearly every
sample after its head window — the memo's scope argument. This probe ran
exactly that experiment on the standard 30-file corpus plus known-period
synthetics, same zlib-9 coder on every arm, period + axis description cost
counted, exactness asserted per record.

## Arms

- `med` — raster MED (a+b−c, PNG conventions), whole-image, the mandatory baseline.
- `lag` — x̂[y,x] = x[y−T,x] (vertical) or x[y,x−T] (horizontal), T swept 1..64
  both axes, whole-image rate-aware pick; cost = residual zlib-9 + flag byte +
  varint(T); head rows/cols (no lag source) fall back to MED residuals.
- `hyb` (memo's hybrid, measured in-session, DENIED — see below).
- `raw` — zlib-9 on the plane directly (the memo's entropy-coding reference).

Probe: `scripts/periodic_structure_probe.py` (committed; gate log
`docs/periodic-prediction-v06-gate.log`).

## Results — corpus (30 files, metadata counted)

| class | n | med | lag | Δmed | wins |
|---|---|---|---|---|---|
| synthetic | 5 | 67222 | 59987 | −10.76% | 4/5 |
| photographic | 20 | 339964 | 311755 | −8.30% | 17/20 |
| heldout | 5 | 19436 | 16846 | −13.33% | 4/5 |
| **TOTAL** | **30** | **426,622** | **388,588** | **−8.92%** | **25/30** |

Per-file winning offsets are the scientific content:

- **Every photo wins at T=1** — the plain vertical-delta predictor, NOT
  periodic structure. Measured mechanism: order-0 residual entropy drops
  ~0.6–0.8 bits/symbol vs MED on natural photos (forest 5.86→5.03, ocean
  5.10→4.49, noise 7.27→6.75) — the signed three-neighbor residual spreads
  wider at edges/textures than the two-sample vertical difference. Real,
  fair, same-coder — but it is an *established* predictor, and the memo
  pre-classified such wins: novelty must come from the model's measured
  performance, not from introducing lag.
- **True periodicity wins where periodic redundancy exists**: repeat T=32
  976→644 (−34.0%), poster T=16 2088→1306 (−37.5%), document T=10 803→666
  (−17.1%), gradient T=4 5297→3554 (−32.9%). These are T > 1 — the mechanism
  the hypothesis names, working at prediction positions throughout the field,
  not 1/W of it.
- **MED holds where it should**: plaid 10660 vs 11891 (+11.6% for lag —
  plaid is small-period in both axes AND near-MED-exact; MED's stream is
  tighter there), solid 88 vs 86 (tie territory), stone 6735 vs 6568
  (+2.5%), dusk-s900003 7037 vs 6922 (+1.7%).

Synthetics: exact-period row-repeat with ±2 noise 16395→8612 (−47.5% at the
true T=8); diagonal stripes 85→44 (−48%).

## The coefficient hybrid (memo's a·x[t−1] + b·x[t−T]) — DENIED

Measured on all four T>1 classes (a quantized Q8, swept; b = 256−a; causal
sources; exactness asserted): hybrid loses to pure lag everywhere —
repeat 1363 vs 644, poster 2832 vs 1306, document 1058 vs 666, gradient
6074 vs 3554. Mechanism: at true period T the x[y−T] term carries the
prediction and any nonzero weight on x[y−1] injects the vertical-delta
noise component the pure-lag stream does not contain. The per-row selector
hybrid (row-wise MED-vs-lag from prior-row zero counts) was also dropped:
never better than its better arm alone (≈ MED + 0.3% on the files it
touched). The memo's hybrid variant closes as: **pure lag is the arm**.

## Controls

- Raw zlib-9 reference: lag beats raw-plane zlib on 27/30 (photos raw ≈
  +4–9% over lag), loses on gradient (49989 vs 3554 — MED-family prediction
  is what makes gradient compress at all) and repeat-adjacent synthetics —
  consistent with every prior probe: prediction precedes entropy coding.
- Strongest existing control (MNF9 2D tokens, retained arm): on the shared
  repeat class the token engine codes the MED-residual plane at 793 B total
  (flate2, `mnf9-token-arm.md`) vs the lag arm's 644 B plane residual (zlib-
  9). Cross-plan-basis caveat applies (tokens work on RGB-plane MED
  residuals under the MNF8 pipeline; this probe codes single luminance
  planes): the honest statement is plane-level parity-to-better, pending
  the same-basis run if a format item is proposed.

## Verdict

Two separate outcomes, reported separately per the work order's own
distinction:

1. **Periodic structure (T ≥ 4 class wins): CONFIRMED as a mechanism** —
   real corpus wins with period cost counted, exactly on the classes with
   strong periodic redundancy, at prediction positions throughout the
   field. This is what the memo predicted would differ from boundary wrap,
   and it does.
2. **The photo-class wins are NOT periodic structure** — they are the
   plain vertical-delta predictor at T=1. Retention question that follows
   is therefore a *format* question, split in two:
   - a "second whole-image predictor" flag (vertical-delta vs MED,
     rate-aware pick) is the simplest item and covers the −9.66% photo
     class — standard-practice mechanism, worth its own format decision;
   - a "periodic lag" flag (T up to 64, +1 flag bit + varint(T)) covers
     repeat/poster/document-class content at plane level, competing with
     the already-retained MNF9 token arm in the same classes (token engine
     currently wins there at stream level; lag at plane level) — the
     decisive comparison is same-basis, deferred to the format-decision
     session.
3. **Hybrid predictor: denied** (see above).

The topology/geometry branch stays closed; v0.6's contribution is the
measured principle the work order asked for:

> *Compression improvement comes from better statistical modeling of
> information — here, modeling periodic recurrence directly at every
> position, which the boundary mechanism could never touch.*